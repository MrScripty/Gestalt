use crate::api::EmilyApi;
use crate::error::EmilyError;
use crate::inference::EmbeddingProvider;
use crate::model::{
    ContextPacket, ContextQuery, DatabaseLocator, HealthSnapshot, HistoryPage, HistoryPageRequest,
    IngestTextRequest, MemoryPolicy, TextObject, TextVector,
};
use crate::store::EmilyStore;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

#[derive(Debug)]
struct RuntimeState {
    db_locator: Option<DatabaseLocator>,
    dropped_ingest_events: u64,
}

/// Default in-process Emily runtime.
pub struct EmilyRuntime<S: EmilyStore> {
    store: Arc<S>,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    state: RwLock<RuntimeState>,
    policy: RwLock<MemoryPolicy>,
    ingest_queue_depth: Mutex<usize>,
}

impl<S: EmilyStore> EmilyRuntime<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self::with_embedding_provider(store, None)
    }

    pub fn with_embedding_provider(
        store: Arc<S>,
        embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    ) -> Self {
        Self {
            store,
            embedding_provider,
            state: RwLock::new(RuntimeState {
                db_locator: None,
                dropped_ingest_events: 0,
            }),
            policy: RwLock::new(MemoryPolicy::default()),
            ingest_queue_depth: Mutex::new(0),
        }
    }

    fn validate_locator(locator: &DatabaseLocator) -> Result<(), EmilyError> {
        if locator.namespace.trim().is_empty() {
            return Err(EmilyError::InvalidDatabaseLocator(
                "namespace cannot be empty".to_string(),
            ));
        }
        if locator.database.trim().is_empty() {
            return Err(EmilyError::InvalidDatabaseLocator(
                "database cannot be empty".to_string(),
            ));
        }
        if locator.storage_path.as_os_str().is_empty() {
            return Err(EmilyError::InvalidDatabaseLocator(
                "storage_path cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    fn build_text_object(request: IngestTextRequest) -> TextObject {
        let object_id = format!("{}:{}", request.stream_id, request.sequence);
        TextObject {
            id: object_id,
            stream_id: request.stream_id,
            source_kind: request.source_kind,
            object_kind: request.object_kind,
            sequence: request.sequence,
            ts_unix_ms: request.ts_unix_ms,
            text: request.text,
            metadata: request.metadata,
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
}

#[async_trait]
impl<S: EmilyStore> EmilyApi for EmilyRuntime<S> {
    async fn open_db(&self, locator: DatabaseLocator) -> Result<(), EmilyError> {
        Self::validate_locator(&locator)?;
        self.store.open(&locator).await?;
        let mut state = self.state.write().await;
        state.db_locator = Some(locator);
        Ok(())
    }

    async fn switch_db(&self, locator: DatabaseLocator) -> Result<(), EmilyError> {
        Self::validate_locator(&locator)?;
        self.store.close().await?;
        self.store.open(&locator).await?;
        let mut state = self.state.write().await;
        state.db_locator = Some(locator);
        Ok(())
    }

    async fn close_db(&self) -> Result<(), EmilyError> {
        if let Some(provider) = &self.embedding_provider {
            provider.shutdown().await?;
        }
        self.store.close().await?;
        let mut state = self.state.write().await;
        state.db_locator = None;
        Ok(())
    }

    async fn ingest_text(&self, request: IngestTextRequest) -> Result<TextObject, EmilyError> {
        if request.stream_id.trim().is_empty() {
            return Err(EmilyError::InvalidRequest(
                "stream_id cannot be empty".to_string(),
            ));
        }
        if request.text.is_empty() {
            return Err(EmilyError::InvalidRequest(
                "text cannot be empty".to_string(),
            ));
        }

        let object = Self::build_text_object(request);
        self.store.insert_text_object(&object).await?;
        if let Some(provider) = &self.embedding_provider {
            let vector = provider.embed_text(&object.text).await?;
            if !vector.is_empty() {
                if vector.len() != 1024 {
                    return Err(EmilyError::Embedding(format!(
                        "embedding dimension mismatch: expected 1024, received {}",
                        vector.len()
                    )));
                }
                if vector.iter().any(|value| !value.is_finite()) {
                    return Err(EmilyError::Embedding(
                        "embedding vector contains non-finite values".to_string(),
                    ));
                }

                let record = TextVector {
                    id: format!("vec:{}", object.id),
                    object_id: object.id.clone(),
                    stream_id: object.stream_id.clone(),
                    sequence: object.sequence,
                    ts_unix_ms: object.ts_unix_ms,
                    dimensions: vector.len(),
                    vector,
                };
                self.store.upsert_text_vector(&record).await?;
            }
        }
        Ok(object)
    }

    async fn query_context(&self, query: ContextQuery) -> Result<ContextPacket, EmilyError> {
        self.store.query_context(&query).await
    }

    async fn page_history_before(
        &self,
        request: HistoryPageRequest,
    ) -> Result<HistoryPage, EmilyError> {
        self.store.page_history_before(&request).await
    }

    async fn memory_policy(&self) -> Result<MemoryPolicy, EmilyError> {
        Ok(self.policy.read().await.clone())
    }

    async fn set_memory_policy(&self, policy: MemoryPolicy) -> Result<(), EmilyError> {
        if policy.semantic_top_k == 0 {
            return Err(EmilyError::InvalidRequest(
                "semantic_top_k must be greater than zero".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&policy.semantic_min_similarity) {
            return Err(EmilyError::InvalidRequest(
                "semantic_min_similarity must be between 0 and 1".to_string(),
            ));
        }

        let mut current = self.policy.write().await;
        *current = policy;
        Ok(())
    }

    async fn health(&self) -> Result<HealthSnapshot, EmilyError> {
        let state = self.state.read().await;
        let queued_ingest_events = *self.ingest_queue_depth.lock().await;
        Ok(HealthSnapshot {
            db_open: state.db_locator.is_some(),
            db_locator: state.db_locator.clone(),
            queued_ingest_events,
            dropped_ingest_events: state.dropped_ingest_events,
        })
    }
}
