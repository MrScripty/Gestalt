use crate::error::EmilyError;
use crate::model::{
    ContextItem, ContextPacket, ContextQuery, DatabaseLocator, HistoryPage, HistoryPageRequest,
    TextEdge, TextEdgeType, TextObject, TextVector, VectorizationConfig,
};
use crate::store::EmilyStore;
use async_trait::async_trait;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use surrealdb::Surreal;
use surrealdb::engine::local::{Db, SurrealKv};
use tokio::sync::RwLock;

#[cfg(test)]
mod tests;

#[derive(Debug, Default)]
struct StoreState {
    active_locator: Option<DatabaseLocator>,
    active_client: Option<Surreal<Db>>,
}

/// Embedded SurrealDB-backed store implementation.
#[derive(Debug, Default)]
pub struct SurrealEmilyStore {
    state: RwLock<StoreState>,
}

impl SurrealEmilyStore {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(StoreState::default()),
        }
    }

    async fn active_client(&self) -> Result<Surreal<Db>, EmilyError> {
        let state = self.state.read().await;
        state
            .active_client
            .clone()
            .ok_or(EmilyError::DatabaseNotOpen)
    }

    fn parse_query_tokens(value: &str) -> HashMap<String, usize> {
        let mut freq = HashMap::<String, usize>::new();
        for token in value
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|token| !token.is_empty())
        {
            *freq.entry(token.to_ascii_lowercase()).or_insert(0) += 1;
        }
        freq
    }

    fn lexical_similarity(query: &str, text: &str) -> f32 {
        let query_tokens = Self::parse_query_tokens(query);
        let text_tokens = Self::parse_query_tokens(text);
        if query_tokens.is_empty() || text_tokens.is_empty() {
            return 0.0;
        }

        let overlap = query_tokens
            .iter()
            .map(|(token, q_count)| {
                let t_count = text_tokens.get(token).copied().unwrap_or(0);
                usize::min(*q_count, t_count)
            })
            .sum::<usize>();
        let query_norm = query_tokens.values().copied().sum::<usize>();
        let text_norm = text_tokens.values().copied().sum::<usize>();
        if query_norm == 0 || text_norm == 0 {
            return 0.0;
        }
        (2.0 * overlap as f32) / (query_norm + text_norm) as f32
    }

    fn rank_score(similarity: f32, object: &TextObject) -> f32 {
        let recency_decay = 1.0;
        similarity
            * (0.4 + 0.6 * object.confidence)
            * (0.5 + 0.5 * object.learning_weight)
            * recency_decay
    }

    fn text_object_projection() -> &'static str {
        "type::string(id) AS id, stream_id, source_kind, object_kind, sequence, ts_unix_ms, text, metadata, epsilon, confidence, outcome_factor, novelty_factor, stability_factor, learning_weight, gate_score, integrated, quarantine_score"
    }

    async fn append_linear_edge(
        &self,
        client: &Surreal<Db>,
        object: &TextObject,
    ) -> Result<(), EmilyError> {
        if object.sequence == 0 {
            return Ok(());
        }

        let previous_sequence = object.sequence.saturating_sub(1);
        let mut response = client
            .query(format!(
                "SELECT {} FROM text_objects WHERE stream_id = $stream_id AND sequence = $sequence LIMIT 1",
                Self::text_object_projection()
            ))
            .bind(("stream_id", object.stream_id.clone()))
            .bind(("sequence", previous_sequence))
            .await
            .map_err(|error| EmilyError::Store(format!("surreal query failed: {error}")))?;

        let previous: Vec<TextObject> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (previous object): {error}"
            ))
        })?;

        let Some(previous) = previous.into_iter().next() else {
            return Ok(());
        };

        let edge = TextEdge {
            id: format!("edge:{}:{}", previous.id, object.id),
            from_id: previous.id,
            to_id: object.id.clone(),
            edge_type: TextEdgeType::LinearNext,
            weight: 1.0,
            ts_unix_ms: object.ts_unix_ms,
        };

        client
            .query("UPSERT type::thing('text_edges', $id) CONTENT $edge")
            .bind(("id", edge.id.clone()))
            .bind(("edge", edge))
            .await
            .map_err(|error| EmilyError::Store(format!("surreal edge upsert failed: {error}")))?;

        Ok(())
    }
}

#[async_trait]
impl EmilyStore for SurrealEmilyStore {
    async fn open(&self, locator: &DatabaseLocator) -> Result<(), EmilyError> {
        fs::create_dir_all(&locator.storage_path).map_err(|error| {
            EmilyError::Store(format!(
                "failed creating surreal storage path {}: {error}",
                locator.storage_path.display()
            ))
        })?;

        let storage_path = locator.storage_path.to_string_lossy().to_string();
        let client = Surreal::new::<SurrealKv>(storage_path)
            .await
            .map_err(|error| EmilyError::Store(format!("surreal open failed: {error}")))?;
        client
            .use_ns(&locator.namespace)
            .use_db(&locator.database)
            .await
            .map_err(|error| EmilyError::Store(format!("surreal use ns/db failed: {error}")))?;

        let mut state = self.state.write().await;
        state.active_locator = Some(locator.clone());
        state.active_client = Some(client);
        Ok(())
    }

    async fn close(&self) -> Result<(), EmilyError> {
        let mut state = self.state.write().await;
        state.active_locator = None;
        state.active_client = None;
        Ok(())
    }

    async fn insert_text_object(&self, object: &TextObject) -> Result<(), EmilyError> {
        let client = self.active_client().await?;

        self.append_linear_edge(&client, object).await?;

        client
            .query("UPSERT type::thing('text_objects', $id) CONTENT $object")
            .bind(("id", object.id.clone()))
            .bind(("object", object.clone()))
            .await
            .map_err(|error| EmilyError::Store(format!("surreal object upsert failed: {error}")))?;

        Ok(())
    }

    async fn upsert_text_edge(&self, edge: &TextEdge) -> Result<(), EmilyError> {
        let client = self.active_client().await?;
        client
            .query("UPSERT type::thing('text_edges', $id) CONTENT $edge")
            .bind(("id", edge.id.clone()))
            .bind(("edge", edge.clone()))
            .await
            .map_err(|error| EmilyError::Store(format!("surreal edge upsert failed: {error}")))?;
        Ok(())
    }

    async fn upsert_text_vector(&self, vector: &TextVector) -> Result<(), EmilyError> {
        let client = self.active_client().await?;
        client
            .query("UPSERT type::thing('text_vectors', $id) CONTENT $vector")
            .bind(("id", vector.id.clone()))
            .bind(("vector", vector.clone()))
            .await
            .map_err(|error| EmilyError::Store(format!("surreal vector upsert failed: {error}")))?;
        Ok(())
    }

    async fn get_text_vector(&self, object_id: &str) -> Result<Option<TextVector>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(
                "SELECT type::string(id) AS id, object_id, stream_id, sequence, ts_unix_ms, dimensions, profile_id, vector FROM text_vectors WHERE object_id = $object_id LIMIT 1",
            )
            .bind(("object_id", object_id.to_string()))
            .await
            .map_err(|error| EmilyError::Store(format!("surreal select text_vector failed: {error}")))?;
        let vectors: Vec<TextVector> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (text_vectors): {error}"
            ))
        })?;
        Ok(vectors.into_iter().next())
    }

    async fn list_text_vectors(
        &self,
        stream_id: Option<&str>,
    ) -> Result<Vec<TextVector>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(
                "SELECT type::string(id) AS id, object_id, stream_id, sequence, ts_unix_ms, dimensions, profile_id, vector FROM text_vectors",
            )
            .await
            .map_err(|error| EmilyError::Store(format!("surreal select text_vectors failed: {error}")))?;
        let mut vectors: Vec<TextVector> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (text_vectors): {error}"
            ))
        })?;
        if let Some(stream_id) = stream_id {
            vectors.retain(|vector| vector.stream_id == stream_id);
        }
        vectors.sort_by(|left, right| left.sequence.cmp(&right.sequence));
        Ok(vectors)
    }

    async fn list_text_objects(
        &self,
        stream_id: Option<&str>,
    ) -> Result<Vec<TextObject>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM text_objects",
                Self::text_object_projection()
            ))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal select text_objects failed: {error}"))
            })?;
        let mut objects: Vec<TextObject> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (text_objects): {error}"
            ))
        })?;
        if let Some(stream_id) = stream_id {
            objects.retain(|object| object.stream_id == stream_id);
        }
        objects.sort_by(|left, right| left.sequence.cmp(&right.sequence));
        Ok(objects)
    }

    async fn list_text_edges(
        &self,
        object_ids: &[String],
        max_depth: u8,
    ) -> Result<Vec<TextEdge>, EmilyError> {
        if object_ids.is_empty() || max_depth == 0 {
            return Ok(Vec::new());
        }

        let client = self.active_client().await?;
        let mut response = client
            .query(
                "SELECT type::string(id) AS id, from_id, to_id, edge_type, weight, ts_unix_ms FROM text_edges",
            )
            .await
            .map_err(|error| EmilyError::Store(format!("surreal select text_edges failed: {error}")))?;
        let all_edges: Vec<TextEdge> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (text_edges): {error}"
            ))
        })?;

        let mut seen_edges = HashSet::<String>::new();
        let mut visited_nodes = object_ids.iter().cloned().collect::<HashSet<_>>();
        let mut frontier = object_ids
            .iter()
            .cloned()
            .map(|id| (id, 0_u8))
            .collect::<VecDeque<_>>();
        let mut collected = Vec::<TextEdge>::new();

        while let Some((node_id, depth)) = frontier.pop_front() {
            if depth >= max_depth {
                continue;
            }

            for edge in &all_edges {
                let next_node = if edge.from_id == node_id {
                    Some(edge.to_id.clone())
                } else if edge.to_id == node_id {
                    Some(edge.from_id.clone())
                } else {
                    None
                };

                let Some(next_node) = next_node else {
                    continue;
                };

                if seen_edges.insert(edge.id.clone()) {
                    collected.push(edge.clone());
                }

                if visited_nodes.insert(next_node.clone()) {
                    frontier.push_back((next_node, depth.saturating_add(1)));
                }
            }
        }

        Ok(collected)
    }

    async fn get_vectorization_config(&self) -> Result<Option<VectorizationConfig>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query("SELECT enabled, expected_dimensions, profile_id FROM type::thing('runtime_config', $id)")
            .bind(("id", "vectorization"))
            .await
            .map_err(|error| EmilyError::Store(format!("surreal select config failed: {error}")))?;
        let configs: Vec<VectorizationConfig> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (vectorization config): {error}"
            ))
        })?;
        Ok(configs.into_iter().next())
    }

    async fn upsert_vectorization_config(
        &self,
        config: &VectorizationConfig,
    ) -> Result<(), EmilyError> {
        let client = self.active_client().await?;
        client
            .query("UPSERT type::thing('runtime_config', $id) CONTENT $config")
            .bind(("id", "vectorization"))
            .bind(("config", config.clone()))
            .await
            .map_err(|error| EmilyError::Store(format!("surreal upsert config failed: {error}")))?;
        Ok(())
    }

    async fn query_context(&self, query: &ContextQuery) -> Result<ContextPacket, EmilyError> {
        let client = self.active_client().await?;

        let mut response = client
            .query(format!(
                "SELECT {} FROM text_objects",
                Self::text_object_projection()
            ))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal select text_objects failed: {error}"))
            })?;
        let objects: Vec<TextObject> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (text_objects): {error}"
            ))
        })?;

        let mut ranked = objects
            .into_iter()
            .filter(|object| {
                query
                    .stream_id
                    .as_ref()
                    .is_none_or(|stream_id| &object.stream_id == stream_id)
            })
            .map(|object| {
                let similarity = Self::lexical_similarity(&query.query_text, &object.text);
                let rank = Self::rank_score(similarity, &object);
                ContextItem {
                    provenance: vec![object.id.clone()],
                    object,
                    similarity,
                    rank,
                }
            })
            .collect::<Vec<_>>();

        ranked.sort_by(|left, right| {
            right
                .rank
                .partial_cmp(&left.rank)
                .unwrap_or(Ordering::Equal)
                .then_with(|| right.object.sequence.cmp(&left.object.sequence))
        });
        ranked.truncate(query.top_k.max(1));

        Ok(ContextPacket { items: ranked })
    }

    async fn page_history_before(
        &self,
        request: &HistoryPageRequest,
    ) -> Result<HistoryPage, EmilyError> {
        if request.limit == 0 {
            return Err(EmilyError::InvalidRequest(
                "history page limit must be greater than zero".to_string(),
            ));
        }

        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM text_objects",
                Self::text_object_projection()
            ))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal select text_objects failed: {error}"))
            })?;
        let objects: Vec<TextObject> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (text_objects): {error}"
            ))
        })?;

        let mut items = objects
            .into_iter()
            .filter(|object| object.stream_id == request.stream_id)
            .filter(|object| {
                request
                    .before_sequence
                    .is_none_or(|before| object.sequence < before)
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.sequence.cmp(&left.sequence));
        items.truncate(request.limit);

        let next_before_sequence = items.last().map(|object| object.sequence);
        Ok(HistoryPage {
            items,
            next_before_sequence,
        })
    }
}
