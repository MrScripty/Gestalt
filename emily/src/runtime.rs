use crate::api::EmilyApi;
use crate::error::EmilyError;
use crate::inference::EmbeddingProvider;
use crate::model::{
    ContextPacket, ContextQuery, DatabaseLocator, HealthSnapshot, HistoryPage, HistoryPageRequest,
    IngestTextRequest, MemoryPolicy, TextObject, TextVector, VectorizationConfig,
    VectorizationConfigPatch, VectorizationJobKind, VectorizationJobSnapshot,
    VectorizationJobState, VectorizationRunRequest, VectorizationStatus,
};
use crate::store::EmilyStore;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{Mutex, RwLock, broadcast};

#[derive(Debug)]
struct RuntimeState {
    db_locator: Option<DatabaseLocator>,
    dropped_ingest_events: u64,
}

#[derive(Debug, Clone)]
struct VectorizationRuntimeState {
    config: VectorizationConfig,
    active_job: Option<VectorizationJobSnapshot>,
    last_job: Option<VectorizationJobSnapshot>,
}

impl Default for VectorizationRuntimeState {
    fn default() -> Self {
        Self {
            config: VectorizationConfig::default(),
            active_job: None,
            last_job: None,
        }
    }
}

#[derive(Debug, Clone)]
struct ActiveJobControl {
    job_id: String,
    cancel: Arc<AtomicBool>,
}

/// Default in-process Emily runtime.
pub struct EmilyRuntime<S: EmilyStore> {
    store: Arc<S>,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    state: Arc<RwLock<RuntimeState>>,
    policy: Arc<RwLock<MemoryPolicy>>,
    ingest_queue_depth: Arc<Mutex<usize>>,
    vectorization: Arc<RwLock<VectorizationRuntimeState>>,
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
            ingest_queue_depth: Arc::new(Mutex::new(0)),
            vectorization: Arc::new(RwLock::new(VectorizationRuntimeState::default())),
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
        if let Some(expected_dimensions) = patch.expected_dimensions {
            if expected_dimensions == 0 {
                return Err(EmilyError::InvalidRequest(
                    "expected_dimensions must be greater than zero".to_string(),
                ));
            }
        }
        if let Some(profile_id) = patch.profile_id.as_ref() {
            if profile_id.trim().is_empty() {
                return Err(EmilyError::InvalidRequest(
                    "profile_id cannot be empty".to_string(),
                ));
            }
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

    async fn emit_vectorization_status(&self) {
        let snapshot = {
            let vectorization = self.vectorization.read().await;
            VectorizationStatus {
                config: vectorization.config.clone(),
                provider_available: self.embedding_provider.is_some(),
                active_job: vectorization.active_job.clone(),
                last_job: vectorization.last_job.clone(),
            }
        };
        let _ = self.vectorization_events.send(snapshot);
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
        Ok(())
    }

    async fn update_active_job(&self, job: VectorizationJobSnapshot) {
        {
            let mut vectorization = self.vectorization.write().await;
            vectorization.active_job = Some(job);
        }
        self.emit_vectorization_status().await;
    }

    async fn spawn_vectorization_job(
        &self,
        kind: VectorizationJobKind,
        request: VectorizationRunRequest,
    ) -> Result<VectorizationJobSnapshot, EmilyError> {
        if request
            .stream_id
            .as_ref()
            .is_some_and(|stream_id| stream_id.trim().is_empty())
        {
            return Err(EmilyError::InvalidRequest(
                "stream_id cannot be empty when provided".to_string(),
            ));
        }

        let Some(provider) = self.embedding_provider.clone() else {
            return Err(EmilyError::InvalidRequest(
                "embedding provider is not available".to_string(),
            ));
        };

        let config = { self.vectorization.read().await.config.clone() };
        if !config.enabled {
            return Err(EmilyError::InvalidRequest(
                "vectorization is disabled".to_string(),
            ));
        }

        let job_id = format!(
            "vectorization-job-{}",
            self.job_counter.fetch_add(1, Ordering::Relaxed) + 1
        );
        let cancel = Arc::new(AtomicBool::new(false));

        {
            let mut active_control = self.active_job_control.lock().await;
            if active_control.is_some() {
                return Err(EmilyError::InvalidRequest(
                    "a vectorization job is already running".to_string(),
                ));
            }
            *active_control = Some(ActiveJobControl {
                job_id: job_id.clone(),
                cancel: cancel.clone(),
            });
        }

        let job = VectorizationJobSnapshot {
            job_id: job_id.clone(),
            kind,
            state: VectorizationJobState::Running,
            stream_id: request.stream_id.clone(),
            processed: 0,
            vectorized: 0,
            skipped: 0,
            failed: 0,
            last_error: None,
        };
        let stream_id_for_response = job.stream_id.clone();
        self.update_active_job(job.clone()).await;

        let store = Arc::clone(&self.store);
        let vectorization = Arc::clone(&self.vectorization);
        let active_job_control = Arc::clone(&self.active_job_control);
        let vectorization_events = self.vectorization_events.clone();

        tokio::spawn(async move {
            let mut running = job;
            let objects = match store.list_text_objects(request.stream_id.as_deref()).await {
                Ok(objects) => objects,
                Err(error) => {
                    running.last_error = Some(format!("failed listing text objects: {error}"));
                    running.failed = running.failed.saturating_add(1);
                    finalize_job(
                        &vectorization,
                        &active_job_control,
                        &vectorization_events,
                        running,
                        VectorizationJobState::Completed,
                    )
                    .await;
                    return;
                }
            };

            for object in objects {
                if cancel.load(Ordering::Relaxed) {
                    finalize_job(
                        &vectorization,
                        &active_job_control,
                        &vectorization_events,
                        running,
                        VectorizationJobState::Cancelled,
                    )
                    .await;
                    return;
                }

                running.processed = running.processed.saturating_add(1);
                let should_vectorize = match kind {
                    VectorizationJobKind::Backfill => match store.get_text_vector(&object.id).await
                    {
                        Ok(Some(existing)) => {
                            existing.dimensions != config.expected_dimensions
                                || existing.profile_id != config.profile_id
                        }
                        Ok(None) => true,
                        Err(error) => {
                            running.failed = running.failed.saturating_add(1);
                            running.last_error = Some(format!(
                                "failed loading vector for object {}: {error}",
                                object.id
                            ));
                            publish_running_job(&vectorization, &vectorization_events, &running)
                                .await;
                            continue;
                        }
                    },
                    VectorizationJobKind::Revectorize => true,
                };

                if !should_vectorize {
                    running.skipped = running.skipped.saturating_add(1);
                    publish_running_job(&vectorization, &vectorization_events, &running).await;
                    continue;
                }

                match provider.embed_text(&object.text).await {
                    Ok(vector) => {
                        if vector.is_empty() {
                            running.skipped = running.skipped.saturating_add(1);
                            publish_running_job(&vectorization, &vectorization_events, &running)
                                .await;
                            continue;
                        }
                        match EmilyRuntime::<S>::validate_embedding_vector(
                            &vector,
                            config.expected_dimensions,
                        ) {
                            Ok(()) => {
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
                                if let Err(error) = store.upsert_text_vector(&record).await {
                                    running.failed = running.failed.saturating_add(1);
                                    running.last_error =
                                        Some(format!("failed upserting vector: {error}"));
                                } else {
                                    running.vectorized = running.vectorized.saturating_add(1);
                                }
                            }
                            Err(error) => {
                                running.failed = running.failed.saturating_add(1);
                                running.last_error = Some(error.to_string());
                            }
                        }
                    }
                    Err(error) => {
                        running.failed = running.failed.saturating_add(1);
                        running.last_error = Some(format!("embedding call failed: {error}"));
                    }
                }
                publish_running_job(&vectorization, &vectorization_events, &running).await;
            }

            finalize_job(
                &vectorization,
                &active_job_control,
                &vectorization_events,
                running,
                VectorizationJobState::Completed,
            )
            .await;
        });

        Ok(VectorizationJobSnapshot {
            job_id,
            kind,
            state: VectorizationJobState::Running,
            stream_id: stream_id_for_response,
            processed: 0,
            vectorized: 0,
            skipped: 0,
            failed: 0,
            last_error: None,
        })
    }
}

async fn publish_running_job(
    vectorization: &Arc<RwLock<VectorizationRuntimeState>>,
    vectorization_events: &broadcast::Sender<VectorizationStatus>,
    running: &VectorizationJobSnapshot,
) {
    let snapshot = {
        let mut state = vectorization.write().await;
        state.active_job = Some(running.clone());
        VectorizationStatus {
            config: state.config.clone(),
            provider_available: true,
            active_job: state.active_job.clone(),
            last_job: state.last_job.clone(),
        }
    };
    let _ = vectorization_events.send(snapshot);
}

async fn finalize_job(
    vectorization: &Arc<RwLock<VectorizationRuntimeState>>,
    active_job_control: &Arc<Mutex<Option<ActiveJobControl>>>,
    vectorization_events: &broadcast::Sender<VectorizationStatus>,
    mut running: VectorizationJobSnapshot,
    state: VectorizationJobState,
) {
    running.state = state;

    {
        let mut control = active_job_control.lock().await;
        if control
            .as_ref()
            .is_some_and(|active| active.job_id == running.job_id)
        {
            *control = None;
        }
    }

    let snapshot = {
        let mut vectorization_state = vectorization.write().await;
        vectorization_state.active_job = None;
        vectorization_state.last_job = Some(running);
        VectorizationStatus {
            config: vectorization_state.config.clone(),
            provider_available: true,
            active_job: None,
            last_job: vectorization_state.last_job.clone(),
        }
    };
    let _ = vectorization_events.send(snapshot);
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
        self.load_vectorization_config().await
    }

    async fn switch_db(&self, locator: DatabaseLocator) -> Result<(), EmilyError> {
        Self::validate_locator(&locator)?;
        self.store.close().await?;
        self.store.open(&locator).await?;
        {
            let mut state = self.state.write().await;
            state.db_locator = Some(locator);
        }
        self.load_vectorization_config().await
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

        let object = Self::build_text_object(request);
        self.store.insert_text_object(&object).await?;

        let config = { self.vectorization.read().await.config.clone() };
        self.maybe_embed_object(&object, &config).await?;

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

    async fn vectorization_status(&self) -> Result<VectorizationStatus, EmilyError> {
        let vectorization = self.vectorization.read().await;
        Ok(VectorizationStatus {
            config: vectorization.config.clone(),
            provider_available: self.embedding_provider.is_some(),
            active_job: vectorization.active_job.clone(),
            last_job: vectorization.last_job.clone(),
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TextObjectKind, VectorizationConfig};
    use crate::store::EmilyStore;
    use serde_json::json;
    use tokio::time::{Duration, sleep};

    #[derive(Default)]
    struct MockStore {
        objects: Mutex<Vec<TextObject>>,
        vectors: Mutex<Vec<TextVector>>,
        config: Mutex<Option<VectorizationConfig>>,
    }

    #[async_trait]
    impl EmilyStore for MockStore {
        async fn open(&self, _locator: &DatabaseLocator) -> Result<(), EmilyError> {
            Ok(())
        }

        async fn close(&self) -> Result<(), EmilyError> {
            Ok(())
        }

        async fn insert_text_object(&self, object: &TextObject) -> Result<(), EmilyError> {
            self.objects.lock().await.push(object.clone());
            Ok(())
        }

        async fn upsert_text_vector(&self, vector: &TextVector) -> Result<(), EmilyError> {
            let mut vectors = self.vectors.lock().await;
            if let Some(index) = vectors.iter().position(|item| item.id == vector.id) {
                vectors[index] = vector.clone();
            } else {
                vectors.push(vector.clone());
            }
            Ok(())
        }

        async fn get_text_vector(&self, object_id: &str) -> Result<Option<TextVector>, EmilyError> {
            let vectors = self.vectors.lock().await;
            Ok(vectors
                .iter()
                .find(|item| item.object_id == object_id)
                .cloned())
        }

        async fn list_text_objects(
            &self,
            stream_id: Option<&str>,
        ) -> Result<Vec<TextObject>, EmilyError> {
            let mut objects = self.objects.lock().await.clone();
            if let Some(stream_id) = stream_id {
                objects.retain(|item| item.stream_id == stream_id);
            }
            objects.sort_by(|left, right| left.sequence.cmp(&right.sequence));
            Ok(objects)
        }

        async fn get_vectorization_config(
            &self,
        ) -> Result<Option<VectorizationConfig>, EmilyError> {
            Ok(self.config.lock().await.clone())
        }

        async fn upsert_vectorization_config(
            &self,
            config: &VectorizationConfig,
        ) -> Result<(), EmilyError> {
            *self.config.lock().await = Some(config.clone());
            Ok(())
        }

        async fn query_context(&self, _query: &ContextQuery) -> Result<ContextPacket, EmilyError> {
            Ok(ContextPacket { items: Vec::new() })
        }

        async fn page_history_before(
            &self,
            _request: &HistoryPageRequest,
        ) -> Result<HistoryPage, EmilyError> {
            Ok(HistoryPage {
                items: Vec::new(),
                next_before_sequence: None,
            })
        }
    }

    struct FixedEmbeddingProvider {
        vector: Vec<f32>,
        shutdown_calls: Mutex<u64>,
    }

    #[async_trait]
    impl EmbeddingProvider for FixedEmbeddingProvider {
        async fn embed_text(&self, _text: &str) -> Result<Vec<f32>, EmilyError> {
            Ok(self.vector.clone())
        }

        async fn shutdown(&self) -> Result<(), EmilyError> {
            let mut calls = self.shutdown_calls.lock().await;
            *calls += 1;
            Ok(())
        }
    }

    fn locator() -> DatabaseLocator {
        DatabaseLocator {
            storage_path: std::env::temp_dir().join("emily-runtime-tests"),
            namespace: "ns".to_string(),
            database: "db".to_string(),
        }
    }

    fn ingest_request(sequence: u64) -> IngestTextRequest {
        IngestTextRequest {
            stream_id: "stream-a".to_string(),
            source_kind: "terminal".to_string(),
            object_kind: TextObjectKind::SystemOutput,
            sequence,
            ts_unix_ms: sequence as i64,
            text: format!("line {sequence}"),
            metadata: json!({"cwd": "/tmp"}),
        }
    }

    #[tokio::test]
    async fn ingest_text_persists_vector_record_when_enabled_and_embedding_is_1024() {
        let store = Arc::new(MockStore::default());
        let provider = Arc::new(FixedEmbeddingProvider {
            vector: vec![0.25; 1024],
            shutdown_calls: Mutex::new(0),
        });
        let runtime = EmilyRuntime::with_embedding_provider(store.clone(), Some(provider));
        runtime.open_db(locator()).await.expect("open");
        runtime
            .update_vectorization_config(VectorizationConfigPatch {
                enabled: Some(true),
                ..VectorizationConfigPatch::default()
            })
            .await
            .expect("enable vectorization");

        runtime
            .ingest_text(ingest_request(1))
            .await
            .expect("ingest should succeed");

        let vectors = store.vectors.lock().await;
        assert_eq!(vectors.len(), 1);
        assert_eq!(vectors[0].dimensions, 1024);
        assert_eq!(vectors[0].profile_id, "qwen3-0.6b");
    }

    #[tokio::test]
    async fn ingest_text_skips_embedding_when_vectorization_disabled() {
        let store = Arc::new(MockStore::default());
        let provider = Arc::new(FixedEmbeddingProvider {
            vector: vec![0.25; 1024],
            shutdown_calls: Mutex::new(0),
        });
        let runtime = EmilyRuntime::with_embedding_provider(store.clone(), Some(provider));
        runtime.open_db(locator()).await.expect("open");

        runtime
            .ingest_text(ingest_request(1))
            .await
            .expect("ingest should succeed");

        let vectors = store.vectors.lock().await;
        assert!(vectors.is_empty());
    }

    #[tokio::test]
    async fn backfill_job_vectors_missing_rows() {
        let store = Arc::new(MockStore::default());
        store
            .insert_text_object(&EmilyRuntime::<MockStore>::build_text_object(
                ingest_request(1),
            ))
            .await
            .expect("insert 1");
        store
            .insert_text_object(&EmilyRuntime::<MockStore>::build_text_object(
                ingest_request(2),
            ))
            .await
            .expect("insert 2");

        let provider = Arc::new(FixedEmbeddingProvider {
            vector: vec![0.4; 1024],
            shutdown_calls: Mutex::new(0),
        });
        let runtime = EmilyRuntime::with_embedding_provider(store.clone(), Some(provider));
        runtime.open_db(locator()).await.expect("open");
        runtime
            .update_vectorization_config(VectorizationConfigPatch {
                enabled: Some(true),
                ..VectorizationConfigPatch::default()
            })
            .await
            .expect("enable");

        let job = runtime
            .start_backfill(VectorizationRunRequest { stream_id: None })
            .await
            .expect("start backfill");

        for _ in 0..50 {
            let status = runtime.vectorization_status().await.expect("status");
            if status
                .last_job
                .as_ref()
                .is_some_and(|last| last.job_id == job.job_id)
            {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }

        let status = runtime.vectorization_status().await.expect("status");
        let last = status.last_job.expect("last job");
        assert_eq!(last.job_id, job.job_id);
        assert_eq!(last.state, VectorizationJobState::Completed);
        assert_eq!(last.vectorized, 2);
    }

    #[tokio::test]
    async fn close_db_invokes_provider_shutdown() {
        let store = Arc::new(MockStore::default());
        let provider = Arc::new(FixedEmbeddingProvider {
            vector: vec![0.25; 1024],
            shutdown_calls: Mutex::new(0),
        });
        let runtime = EmilyRuntime::with_embedding_provider(store, Some(provider.clone()));
        runtime.open_db(locator()).await.expect("open");
        runtime.close_db().await.expect("close");
        assert_eq!(*provider.shutdown_calls.lock().await, 1);
    }
}
