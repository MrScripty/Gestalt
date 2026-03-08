use crate::api::EmilyApi;
use crate::error::EmilyError;
use crate::inference::EmbeddingProvider;
use crate::model::{
    AppendAuditRecordRequest, AuditRecord, ContextPacket, ContextQuery, CreateEpisodeRequest,
    DatabaseLocator, EarlEvaluationRecord, EarlEvaluationRequest, EpisodeRecord, EpisodeTraceLink,
    HealthSnapshot, HistoryPage, HistoryPageRequest, IngestTextRequest, IntegritySnapshot,
    MemoryPolicy, MemoryState, OutcomeRecord, RecordOutcomeRequest, TextObject, TextVector,
    TraceLinkRequest, VectorizationConfig, VectorizationConfigPatch, VectorizationJobKind,
    VectorizationJobSnapshot, VectorizationRunRequest, VectorizationStatus,
};
use crate::store::EmilyStore;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use tokio::sync::{Mutex, RwLock, broadcast};

mod earl;
#[cfg(test)]
mod earl_tests;
mod ecgl;
#[cfg(test)]
mod ecgl_tests;
#[cfg(test)]
mod episode_tests;
mod episodes;
mod retrieval;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
mod vectorization;

#[derive(Debug)]
struct RuntimeState {
    db_locator: Option<DatabaseLocator>,
    dropped_ingest_events: u64,
}

#[derive(Debug, Clone, Default)]
struct VectorizationRuntimeState {
    config: VectorizationConfig,
    active_job: Option<VectorizationJobSnapshot>,
    last_job: Option<VectorizationJobSnapshot>,
}

#[derive(Debug, Clone)]
struct EcglRuntimeState {
    tau: f32,
    last_snapshot: Option<IntegritySnapshot>,
}

impl Default for EcglRuntimeState {
    fn default() -> Self {
        Self {
            tau: ecgl::TAU_INITIAL,
            last_snapshot: None,
        }
    }
}

#[derive(Debug, Clone)]
struct ActiveJobControl {
    job_id: String,
    cancel: Arc<AtomicBool>,
}

#[derive(Debug)]
struct InFlightIngestGuard<'a> {
    counter: &'a AtomicUsize,
}

impl<'a> InFlightIngestGuard<'a> {
    fn new(counter: &'a AtomicUsize) -> Self {
        counter.fetch_add(1, Ordering::Relaxed);
        Self { counter }
    }
}

impl Drop for InFlightIngestGuard<'_> {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Default in-process Emily runtime.
pub struct EmilyRuntime<S: EmilyStore> {
    store: Arc<S>,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    state: Arc<RwLock<RuntimeState>>,
    policy: Arc<RwLock<MemoryPolicy>>,
    in_flight_ingest_events: AtomicUsize,
    vectorization: Arc<RwLock<VectorizationRuntimeState>>,
    ecgl: Arc<RwLock<EcglRuntimeState>>,
    active_job_control: Arc<Mutex<Option<ActiveJobControl>>>,
    vectorization_events: broadcast::Sender<VectorizationStatus>,
    job_counter: AtomicU64,
}

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

    fn validate_vectorization_patch(patch: &VectorizationConfigPatch) -> Result<(), EmilyError> {
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
            memory_state: MemoryState::Pending,
            integrated: false,
            quarantine_score: 0.0,
        }
    }

    fn validate_embedding_vector(
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

    async fn load_vectorization_config(&self) -> Result<(), EmilyError> {
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
        // Persist defaults on first open so settings are explicit in storage.
        self.store.upsert_vectorization_config(&loaded).await?;
        self.emit_vectorization_status().await;
        Ok(())
    }

    async fn load_integrity_snapshot(&self) -> Result<(), EmilyError> {
        let snapshot = self.store.latest_integrity_snapshot().await?;
        let mut ecgl = self.ecgl.write().await;
        ecgl.tau = snapshot
            .as_ref()
            .map(|snapshot| snapshot.tau)
            .unwrap_or(ecgl::TAU_INITIAL);
        ecgl.last_snapshot = snapshot;
        Ok(())
    }

    async fn emit_vectorization_status(&self) {
        let snapshot = self.snapshot_vectorization_status().await;
        let _ = self.vectorization_events.send(snapshot);
    }

    async fn snapshot_vectorization_status(&self) -> VectorizationStatus {
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

    async fn maybe_embed_object(
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

    async fn update_active_job(&self, job: VectorizationJobSnapshot) {
        {
            let mut vectorization = self.vectorization.write().await;
            vectorization.active_job = Some(job);
        }
        self.emit_vectorization_status().await;
    }
}

#[async_trait]
impl<S: EmilyStore + 'static> EmilyApi for EmilyRuntime<S> {
    async fn open_db(&self, locator: DatabaseLocator) -> Result<(), EmilyError> {
        Self::validate_locator(&locator)?;
        self.store.open(&locator).await?;
        {
            let mut state = self.state.write().await;
            state.db_locator = Some(locator);
        }
        self.load_vectorization_config().await?;
        self.load_integrity_snapshot().await
    }

    async fn switch_db(&self, locator: DatabaseLocator) -> Result<(), EmilyError> {
        Self::validate_locator(&locator)?;
        self.store.close().await?;
        self.store.open(&locator).await?;
        {
            let mut state = self.state.write().await;
            state.db_locator = Some(locator);
        }
        self.load_vectorization_config().await?;
        self.load_integrity_snapshot().await
    }

    async fn close_db(&self) -> Result<(), EmilyError> {
        {
            let active = self.active_job_control.lock().await;
            if let Some(active) = active.as_ref() {
                active.cancel.store(true, Ordering::Relaxed);
            }
        }
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

        let _in_flight_ingest = InFlightIngestGuard::new(&self.in_flight_ingest_events);
        let object = Self::build_text_object(request);
        self.store.insert_text_object(&object).await?;

        let config = { self.vectorization.read().await.config.clone() };
        self.maybe_embed_object(&object, &config).await?;

        Ok(object)
    }

    async fn create_episode(
        &self,
        request: CreateEpisodeRequest,
    ) -> Result<EpisodeRecord, EmilyError> {
        self.create_episode_internal(request).await
    }

    async fn link_text_to_episode(
        &self,
        request: TraceLinkRequest,
    ) -> Result<EpisodeTraceLink, EmilyError> {
        self.link_text_to_episode_internal(request).await
    }

    async fn record_outcome(
        &self,
        request: RecordOutcomeRequest,
    ) -> Result<OutcomeRecord, EmilyError> {
        self.record_outcome_internal(request).await
    }

    async fn append_audit_record(
        &self,
        request: AppendAuditRecordRequest,
    ) -> Result<AuditRecord, EmilyError> {
        self.append_audit_record_internal(request).await
    }

    async fn evaluate_episode_risk(
        &self,
        request: EarlEvaluationRequest,
    ) -> Result<EarlEvaluationRecord, EmilyError> {
        self.evaluate_episode_risk_internal(request).await
    }

    async fn latest_integrity_snapshot(&self) -> Result<Option<IntegritySnapshot>, EmilyError> {
        Ok(self.ecgl.read().await.last_snapshot.clone())
    }

    async fn query_context(&self, query: ContextQuery) -> Result<ContextPacket, EmilyError> {
        self.query_context_internal(query).await
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
        Ok(HealthSnapshot {
            db_open: state.db_locator.is_some(),
            db_locator: state.db_locator.clone(),
            queued_ingest_events: self.in_flight_ingest_events.load(Ordering::Relaxed),
            dropped_ingest_events: state.dropped_ingest_events,
        })
    }

    async fn vectorization_status(&self) -> Result<VectorizationStatus, EmilyError> {
        Ok(self.snapshot_vectorization_status().await)
    }

    async fn update_vectorization_config(
        &self,
        patch: VectorizationConfigPatch,
    ) -> Result<VectorizationConfig, EmilyError> {
        Self::validate_vectorization_patch(&patch)?;

        let updated = {
            let mut vectorization = self.vectorization.write().await;
            if let Some(enabled) = patch.enabled {
                vectorization.config.enabled = enabled;
            }
            if let Some(expected_dimensions) = patch.expected_dimensions {
                vectorization.config.expected_dimensions = expected_dimensions;
            }
            if let Some(profile_id) = patch.profile_id {
                vectorization.config.profile_id = profile_id;
            }
            vectorization.config.clone()
        };
        self.store.upsert_vectorization_config(&updated).await?;
        self.emit_vectorization_status().await;
        Ok(updated)
    }

    async fn start_backfill(
        &self,
        request: VectorizationRunRequest,
    ) -> Result<VectorizationJobSnapshot, EmilyError> {
        self.spawn_vectorization_job(VectorizationJobKind::Backfill, request)
            .await
    }

    async fn start_revectorize(
        &self,
        request: VectorizationRunRequest,
    ) -> Result<VectorizationJobSnapshot, EmilyError> {
        self.spawn_vectorization_job(VectorizationJobKind::Revectorize, request)
            .await
    }

    async fn cancel_vectorization_job(&self, job_id: &str) -> Result<(), EmilyError> {
        let active = self.active_job_control.lock().await;
        let Some(active) = active.as_ref() else {
            return Err(EmilyError::InvalidRequest(
                "no active vectorization job".to_string(),
            ));
        };

        if active.job_id != job_id {
            return Err(EmilyError::InvalidRequest(format!(
                "active job id is '{}', received '{}'",
                active.job_id, job_id
            )));
        }

        active.cancel.store(true, Ordering::Relaxed);
        Ok(())
    }
}
