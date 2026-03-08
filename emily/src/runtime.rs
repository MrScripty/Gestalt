use crate::api::EmilyApi;
use crate::error::EmilyError;
use crate::inference::EmbeddingProvider;
use crate::model::{
    AppendAuditRecordRequest, AppendSovereignAuditRecordRequest, AuditRecord, ContextPacket,
    ContextQuery, CreateEpisodeRequest, DatabaseLocator, EarlEvaluationRecord,
    EarlEvaluationRequest, EpisodeRecord, EpisodeTraceLink, HealthSnapshot, HistoryPage,
    HistoryPageRequest, IngestTextRequest, IntegritySnapshot, MemoryPolicy, OutcomeRecord,
    RecordOutcomeRequest, RemoteEpisodeRecord, RemoteEpisodeRequest, RoutingDecision, TextObject,
    TraceLinkRequest, UpdateRemoteEpisodeStateRequest, ValidationOutcome, VectorizationConfig,
    VectorizationConfigPatch, VectorizationJobKind, VectorizationJobSnapshot,
    VectorizationRunRequest, VectorizationStatus,
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
mod lifecycle;
mod retrieval;
mod sovereign;
#[cfg(test)]
mod sovereign_tests;
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

    async fn episode(&self, episode_id: &str) -> Result<Option<EpisodeRecord>, EmilyError> {
        self.episode_internal(episode_id).await
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

    async fn record_routing_decision(
        &self,
        decision: RoutingDecision,
    ) -> Result<RoutingDecision, EmilyError> {
        self.record_routing_decision_internal(decision).await
    }

    async fn create_remote_episode(
        &self,
        request: RemoteEpisodeRequest,
    ) -> Result<RemoteEpisodeRecord, EmilyError> {
        self.create_remote_episode_internal(request).await
    }

    async fn update_remote_episode_state(
        &self,
        request: UpdateRemoteEpisodeStateRequest,
    ) -> Result<RemoteEpisodeRecord, EmilyError> {
        self.update_remote_episode_state_internal(request).await
    }

    async fn record_validation_outcome(
        &self,
        outcome: ValidationOutcome,
    ) -> Result<ValidationOutcome, EmilyError> {
        self.record_validation_outcome_internal(outcome).await
    }

    async fn append_sovereign_audit_record(
        &self,
        request: AppendSovereignAuditRecordRequest,
    ) -> Result<AuditRecord, EmilyError> {
        self.append_sovereign_audit_record_internal(request).await
    }

    async fn routing_decision(
        &self,
        decision_id: &str,
    ) -> Result<Option<RoutingDecision>, EmilyError> {
        self.routing_decision_internal(decision_id).await
    }

    async fn routing_decisions_for_episode(
        &self,
        episode_id: &str,
    ) -> Result<Vec<RoutingDecision>, EmilyError> {
        self.routing_decisions_for_episode_internal(episode_id)
            .await
    }

    async fn remote_episode(
        &self,
        remote_episode_id: &str,
    ) -> Result<Option<RemoteEpisodeRecord>, EmilyError> {
        self.remote_episode_internal(remote_episode_id).await
    }

    async fn remote_episodes_for_episode(
        &self,
        episode_id: &str,
    ) -> Result<Vec<RemoteEpisodeRecord>, EmilyError> {
        self.remote_episodes_for_episode_internal(episode_id).await
    }

    async fn validation_outcome(
        &self,
        validation_id: &str,
    ) -> Result<Option<ValidationOutcome>, EmilyError> {
        self.validation_outcome_internal(validation_id).await
    }

    async fn validation_outcomes_for_episode(
        &self,
        episode_id: &str,
    ) -> Result<Vec<ValidationOutcome>, EmilyError> {
        self.validation_outcomes_for_episode_internal(episode_id)
            .await
    }

    async fn sovereign_audit_records_for_episode(
        &self,
        episode_id: &str,
    ) -> Result<Vec<AuditRecord>, EmilyError> {
        self.sovereign_audit_records_for_episode_internal(episode_id)
            .await
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
