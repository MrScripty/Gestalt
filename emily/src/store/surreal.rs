use crate::error::EmilyError;
use crate::model::{
    ContextItem, ContextPacket, ContextQuery, DatabaseLocator, HistoryPage, HistoryPageRequest,
    TextEdge, TextEdgeType, TextObject, TextVector, VectorizationConfig,
};
use crate::store::EmilyStore;
use async_trait::async_trait;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use surrealdb::Surreal;
use surrealdb::engine::local::{Db, SurrealKv};
use tokio::sync::RwLock;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{HistoryPageRequest, TextObjectKind};
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn locator() -> DatabaseLocator {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch");
        DatabaseLocator {
            storage_path: std::env::temp_dir()
                .join(format!("emily-surreal-test-{}", now.as_millis())),
            namespace: "ns".to_string(),
            database: "db".to_string(),
        }
    }

    fn sample_object(sequence: u64, text: &str) -> TextObject {
        TextObject {
            id: format!("stream-a:{sequence}"),
            stream_id: "stream-a".to_string(),
            source_kind: "terminal".to_string(),
            object_kind: TextObjectKind::SystemOutput,
            sequence,
            ts_unix_ms: sequence as i64,
            text: text.to_string(),
            metadata: json!({}),
            epsilon: None,
            confidence: 1.0,
            outcome_factor: 0.5,
            novelty_factor: 0.5,
            stability_factor: 1.0,
            learning_weight: 1.0,
            gate_score: None,
            integrated: true,
            quarantine_score: 0.0,
        }
    }

    #[tokio::test]
    async fn open_insert_and_page_history_roundtrip() {
        let store = SurrealEmilyStore::new();
        let locator = locator();
        store.open(&locator).await.expect("open store");
        store
            .insert_text_object(&sample_object(1, "hello world"))
            .await
            .expect("insert 1");
        store
            .insert_text_object(&sample_object(2, "second line"))
            .await
            .expect("insert 2");

        store
            .upsert_text_vector(&TextVector {
                id: "vec:stream-a:2".to_string(),
                object_id: "stream-a:2".to_string(),
                stream_id: "stream-a".to_string(),
                sequence: 2,
                ts_unix_ms: 2,
                dimensions: 1024,
                profile_id: "qwen3-0.6b".to_string(),
                vector: vec![0.0; 1024],
            })
            .await
            .expect("upsert vector");

        let page = store
            .page_history_before(&HistoryPageRequest {
                stream_id: "stream-a".to_string(),
                before_sequence: None,
                limit: 1,
            })
            .await
            .expect("page history");
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].sequence, 2);
        assert_eq!(page.next_before_sequence, Some(2));

        store.close().await.expect("close store");
        let _ = std::fs::remove_dir_all(locator.storage_path);
    }
}
