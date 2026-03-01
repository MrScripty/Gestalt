use crate::error::EmilyError;
use crate::model::{
    ContextItem, ContextPacket, ContextQuery, DatabaseLocator, HistoryPage, HistoryPageRequest,
    TextEdge, TextEdgeType, TextObject,
};
use crate::store::EmilyStore;
use async_trait::async_trait;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tokio::sync::RwLock;

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct MemoryDb {
    objects: Vec<TextObject>,
    edges: Vec<TextEdge>,
}

#[derive(Debug, Default)]
struct StoreState {
    active_locator: Option<DatabaseLocator>,
    active_db: MemoryDb,
}

/// Embedded store implementation behind the Surreal-facing contract.
///
/// Note: this persists to a local JSON file at the selected locator path.
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

    fn db_file_path(locator: &DatabaseLocator) -> PathBuf {
        locator
            .storage_path
            .join(&locator.namespace)
            .join(format!("{}.json", locator.database))
    }

    fn save_db(locator: &DatabaseLocator, db: &MemoryDb) -> Result<(), EmilyError> {
        let path = Self::db_file_path(locator);
        let parent = path.parent().ok_or_else(|| {
            EmilyError::Store("failed to derive parent directory for database file".to_string())
        })?;
        fs::create_dir_all(parent).map_err(|error| {
            EmilyError::Store(format!(
                "failed creating database directory {}: {error}",
                parent.display()
            ))
        })?;
        let payload = serde_json::to_vec_pretty(db)
            .map_err(|error| EmilyError::Store(format!("failed serializing database: {error}")))?;
        fs::write(&path, payload).map_err(|error| {
            EmilyError::Store(format!("failed writing {}: {error}", path.display()))
        })
    }

    fn load_db(locator: &DatabaseLocator) -> Result<MemoryDb, EmilyError> {
        let path = Self::db_file_path(locator);
        if !path.exists() {
            return Ok(MemoryDb::default());
        }
        let bytes = fs::read(&path).map_err(|error| {
            EmilyError::Store(format!("failed reading {}: {error}", path.display()))
        })?;
        serde_json::from_slice::<MemoryDb>(&bytes).map_err(|error| {
            EmilyError::Store(format!(
                "failed parsing embedded database payload {}: {error}",
                path.display()
            ))
        })
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

    fn append_linear_edge(db: &mut MemoryDb, object: &TextObject) {
        let previous = db
            .objects
            .iter()
            .filter(|candidate| candidate.stream_id == object.stream_id)
            .filter(|candidate| candidate.sequence + 1 == object.sequence)
            .max_by_key(|candidate| candidate.sequence);
        let Some(previous) = previous else {
            return;
        };

        db.edges.push(TextEdge {
            id: format!("edge:{}:{}", previous.id, object.id),
            from_id: previous.id.clone(),
            to_id: object.id.clone(),
            edge_type: TextEdgeType::LinearNext,
            weight: 1.0,
            ts_unix_ms: object.ts_unix_ms,
        });
    }
}

#[async_trait]
impl EmilyStore for SurrealEmilyStore {
    async fn open(&self, locator: &DatabaseLocator) -> Result<(), EmilyError> {
        let mut state = self.state.write().await;
        state.active_db = Self::load_db(locator)?;
        state.active_locator = Some(locator.clone());
        Ok(())
    }

    async fn close(&self) -> Result<(), EmilyError> {
        let mut state = self.state.write().await;
        if let Some(locator) = state.active_locator.clone() {
            Self::save_db(&locator, &state.active_db)?;
        }
        state.active_locator = None;
        state.active_db = MemoryDb::default();
        Ok(())
    }

    async fn insert_text_object(&self, object: &TextObject) -> Result<(), EmilyError> {
        let mut state = self.state.write().await;
        let locator = state
            .active_locator
            .clone()
            .ok_or(EmilyError::DatabaseNotOpen)?;

        Self::append_linear_edge(&mut state.active_db, object);
        state.active_db.objects.push(object.clone());
        Self::save_db(&locator, &state.active_db)
    }

    async fn query_context(&self, query: &ContextQuery) -> Result<ContextPacket, EmilyError> {
        let state = self.state.read().await;
        if state.active_locator.is_none() {
            return Err(EmilyError::DatabaseNotOpen);
        }

        let mut ranked = state
            .active_db
            .objects
            .iter()
            .filter(|object| {
                query
                    .stream_id
                    .as_ref()
                    .is_none_or(|stream_id| &object.stream_id == stream_id)
            })
            .map(|object| {
                let similarity = Self::lexical_similarity(&query.query_text, &object.text);
                let rank = Self::rank_score(similarity, object);
                ContextItem {
                    object: object.clone(),
                    similarity,
                    rank,
                    provenance: vec![object.id.clone()],
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

        let state = self.state.read().await;
        if state.active_locator.is_none() {
            return Err(EmilyError::DatabaseNotOpen);
        }

        let mut items = state
            .active_db
            .objects
            .iter()
            .filter(|object| object.stream_id == request.stream_id)
            .filter(|object| {
                request
                    .before_sequence
                    .is_none_or(|before| object.sequence < before)
            })
            .cloned()
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
                .join(format!("emily-store-test-{}", now.as_millis())),
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
            embedding: None,
            confidence: 1.0,
            learning_weight: 1.0,
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
    }
}
