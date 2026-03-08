use super::{EcglRuntimeState, EmilyRuntime, RuntimeState, VectorizationRuntimeState};
use crate::error::EmilyError;
use crate::inference::EmbeddingProvider;
use crate::model::{
    DatabaseLocator, IngestTextRequest, MemoryPolicy, MemoryState, TextObject, TextVector,
    VectorizationConfig, VectorizationConfigPatch, VectorizationJobSnapshot, VectorizationStatus,
};
use crate::runtime::ecgl;
use crate::store::EmilyStore;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use tokio::sync::{Mutex, RwLock, broadcast};

impl<S: EmilyStore + 'static> EmilyRuntime<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self::with_embedding_provider(store, None)
    }

    pub fn with_embedding_provider(
        store: Arc<S>,
        embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    ) -> Self {
        let (vectorization_events, _) = broadcast::channel(32);
        Self {
            store,
            embedding_provider,
            state: Arc::new(RwLock::new(RuntimeState {
                db_locator: None,
                dropped_ingest_events: 0,
            })),
            policy: Arc::new(RwLock::new(MemoryPolicy::default())),
            in_flight_ingest_events: AtomicUsize::new(0),
            vectorization: Arc::new(RwLock::new(VectorizationRuntimeState::default())),
            ecgl: Arc::new(RwLock::new(EcglRuntimeState::default())),
            active_job_control: Arc::new(Mutex::new(None)),
            vectorization_events,
            job_counter: AtomicU64::new(0),
        }
    }

    pub fn subscribe_vectorization_status(&self) -> broadcast::Receiver<VectorizationStatus> {
        self.vectorization_events.subscribe()
    }

    pub(super) fn validate_locator(locator: &DatabaseLocator) -> Result<(), EmilyError> {
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

    pub(super) fn validate_vectorization_patch(
        patch: &VectorizationConfigPatch,
    ) -> Result<(), EmilyError> {
        if let Some(expected_dimensions) = patch.expected_dimensions
            && expected_dimensions == 0
        {
            return Err(EmilyError::InvalidRequest(
                "expected_dimensions must be greater than zero".to_string(),
            ));
        }
        if let Some(profile_id) = patch.profile_id.as_ref()
            && profile_id.trim().is_empty()
        {
            return Err(EmilyError::InvalidRequest(
                "profile_id cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    pub(super) fn build_text_object(request: IngestTextRequest) -> TextObject {
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
            memory_state: MemoryState::Pending,
            integrated: false,
            quarantine_score: 0.0,
        }
    }

    pub(super) fn validate_embedding_vector(
        vector: &[f32],
        expected_dimensions: usize,
    ) -> Result<(), EmilyError> {
        if vector.len() != expected_dimensions {
            return Err(EmilyError::Embedding(format!(
                "embedding dimension mismatch: expected {}, received {}",
                expected_dimensions,
                vector.len()
            )));
        }
        if vector.iter().any(|value| !value.is_finite()) {
            return Err(EmilyError::Embedding(
                "embedding vector contains non-finite values".to_string(),
            ));
        }
        Ok(())
    }

    pub(super) async fn load_vectorization_config(&self) -> Result<(), EmilyError> {
        let loaded = self
            .store
            .get_vectorization_config()
            .await?
            .unwrap_or_default();
        {
            let mut vectorization = self.vectorization.write().await;
            vectorization.config = loaded.clone();
            vectorization.active_job = None;
            vectorization.last_job = None;
        }
        self.store.upsert_vectorization_config(&loaded).await?;
        self.emit_vectorization_status().await;
        Ok(())
    }

    pub(super) async fn load_integrity_snapshot(&self) -> Result<(), EmilyError> {
        let snapshot = self.store.latest_integrity_snapshot().await?;
        let mut ecgl = self.ecgl.write().await;
        ecgl.tau = snapshot
            .as_ref()
            .map(|item| item.tau)
            .unwrap_or(ecgl::TAU_INITIAL);
        ecgl.last_snapshot = snapshot;
        Ok(())
    }

    pub(super) async fn emit_vectorization_status(&self) {
        let snapshot = self.snapshot_vectorization_status().await;
        let _ = self.vectorization_events.send(snapshot);
    }

    pub(super) async fn snapshot_vectorization_status(&self) -> VectorizationStatus {
        let provider_status = match self.embedding_provider.as_ref() {
            Some(provider) => provider.status().await,
            None => None,
        };
        let vectorization = self.vectorization.read().await;
        VectorizationStatus {
            config: vectorization.config.clone(),
            provider_available: self.embedding_provider.is_some(),
            provider_status,
            active_job: vectorization.active_job.clone(),
            last_job: vectorization.last_job.clone(),
        }
    }

    pub(super) async fn maybe_embed_object(
        &self,
        object: &TextObject,
        config: &VectorizationConfig,
    ) -> Result<(), EmilyError> {
        if !config.enabled {
            return Ok(());
        }

        let Some(provider) = &self.embedding_provider else {
            return Ok(());
        };

        let vector = provider.embed_text(&object.text).await?;
        if vector.is_empty() {
            return Ok(());
        }

        Self::validate_embedding_vector(&vector, config.expected_dimensions)?;

        let record = TextVector {
            id: format!("vec:{}", object.id),
            object_id: object.id.clone(),
            stream_id: object.stream_id.clone(),
            sequence: object.sequence,
            ts_unix_ms: object.ts_unix_ms,
            dimensions: vector.len(),
            profile_id: config.profile_id.clone(),
            vector,
        };
        self.store.upsert_text_vector(&record).await?;
        self.maybe_link_semantic_edges(object, &record).await?;
        Ok(())
    }

    pub(super) async fn update_active_job(&self, job: VectorizationJobSnapshot) {
        {
            let mut vectorization = self.vectorization.write().await;
            vectorization.active_job = Some(job);
        }
        self.emit_vectorization_status().await;
    }
}
