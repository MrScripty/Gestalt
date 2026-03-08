use crate::error::EmilyError;
use crate::model::{
    AppendAuditRecordRequest, AppendSovereignAuditRecordRequest, AuditRecord, ContextPacket,
    ContextQuery, CreateEpisodeRequest, DatabaseLocator, EarlEvaluationRecord,
    EarlEvaluationRequest, EpisodeRecord, EpisodeTraceLink, HealthSnapshot, HistoryPage,
    HistoryPageRequest, IngestTextRequest, IntegritySnapshot, MemoryPolicy, OutcomeRecord,
    RecordOutcomeRequest, RemoteEpisodeRecord, RemoteEpisodeRequest, RoutingDecision, TextObject,
    TraceLinkRequest, UpdateRemoteEpisodeStateRequest, ValidationOutcome, VectorizationConfig,
    VectorizationConfigPatch, VectorizationJobSnapshot, VectorizationRunRequest,
    VectorizationStatus,
};
use async_trait::async_trait;

/// Public Emily API exposed to host systems.
#[async_trait]
pub trait EmilyApi: Send + Sync {
    /// Open an embedded database target for this runtime.
    async fn open_db(&self, locator: DatabaseLocator) -> Result<(), EmilyError>;

    /// Switch to another embedded database target.
    async fn switch_db(&self, locator: DatabaseLocator) -> Result<(), EmilyError>;

    /// Close the currently active database target.
    async fn close_db(&self) -> Result<(), EmilyError>;

    /// Ingest one text object into memory.
    async fn ingest_text(&self, request: IngestTextRequest) -> Result<TextObject, EmilyError>;

    /// Create one host-agnostic episode anchor record.
    async fn create_episode(
        &self,
        request: CreateEpisodeRequest,
    ) -> Result<EpisodeRecord, EmilyError>;

    /// Read one durable episode by id.
    async fn episode(&self, episode_id: &str) -> Result<Option<EpisodeRecord>, EmilyError>;

    /// Link one persisted text object into an episode trace.
    async fn link_text_to_episode(
        &self,
        request: TraceLinkRequest,
    ) -> Result<EpisodeTraceLink, EmilyError>;

    /// Record one durable outcome for an episode.
    async fn record_outcome(
        &self,
        request: RecordOutcomeRequest,
    ) -> Result<OutcomeRecord, EmilyError>;

    /// Append one immutable audit record.
    async fn append_audit_record(
        &self,
        request: AppendAuditRecordRequest,
    ) -> Result<AuditRecord, EmilyError>;

    /// Record one durable routing decision for a host episode.
    async fn record_routing_decision(
        &self,
        decision: RoutingDecision,
    ) -> Result<RoutingDecision, EmilyError>;

    /// Record one durable remote episode reference under a host episode.
    async fn create_remote_episode(
        &self,
        request: RemoteEpisodeRequest,
    ) -> Result<RemoteEpisodeRecord, EmilyError>;

    /// Transition one durable remote episode through an explicit host-observed
    /// lifecycle change.
    async fn update_remote_episode_state(
        &self,
        request: UpdateRemoteEpisodeStateRequest,
    ) -> Result<RemoteEpisodeRecord, EmilyError>;

    /// Record one durable validation outcome for local or remote outputs.
    async fn record_validation_outcome(
        &self,
        outcome: ValidationOutcome,
    ) -> Result<ValidationOutcome, EmilyError>;

    /// Append one immutable audit record with structured sovereign metadata.
    async fn append_sovereign_audit_record(
        &self,
        request: AppendSovereignAuditRecordRequest,
    ) -> Result<AuditRecord, EmilyError>;

    /// Read one durable routing decision by id.
    async fn routing_decision(
        &self,
        decision_id: &str,
    ) -> Result<Option<RoutingDecision>, EmilyError>;

    /// List durable routing decisions for one host episode.
    async fn routing_decisions_for_episode(
        &self,
        episode_id: &str,
    ) -> Result<Vec<RoutingDecision>, EmilyError>;

    /// Read one durable remote episode by id.
    async fn remote_episode(
        &self,
        remote_episode_id: &str,
    ) -> Result<Option<RemoteEpisodeRecord>, EmilyError>;

    /// List durable remote episodes for one host episode.
    async fn remote_episodes_for_episode(
        &self,
        episode_id: &str,
    ) -> Result<Vec<RemoteEpisodeRecord>, EmilyError>;

    /// Read one durable validation outcome by id.
    async fn validation_outcome(
        &self,
        validation_id: &str,
    ) -> Result<Option<ValidationOutcome>, EmilyError>;

    /// List durable validation outcomes for one host episode.
    async fn validation_outcomes_for_episode(
        &self,
        episode_id: &str,
    ) -> Result<Vec<ValidationOutcome>, EmilyError>;

    /// List sovereign audit records for one host episode.
    async fn sovereign_audit_records_for_episode(
        &self,
        episode_id: &str,
    ) -> Result<Vec<AuditRecord>, EmilyError>;

    /// Evaluate one episode with the current EARL gate.
    async fn evaluate_episode_risk(
        &self,
        request: EarlEvaluationRequest,
    ) -> Result<EarlEvaluationRecord, EmilyError>;

    /// Return the latest durable cognitive integrity snapshot.
    async fn latest_integrity_snapshot(&self) -> Result<Option<IntegritySnapshot>, EmilyError>;

    /// Retrieve ranked context items for the given query.
    async fn query_context(&self, query: ContextQuery) -> Result<ContextPacket, EmilyError>;

    /// Fetch one page of historical objects before a cursor sequence.
    async fn page_history_before(
        &self,
        request: HistoryPageRequest,
    ) -> Result<HistoryPage, EmilyError>;

    /// Read the current mutable memory policy.
    async fn memory_policy(&self) -> Result<MemoryPolicy, EmilyError>;

    /// Update policy used for retrieval/ranking.
    async fn set_memory_policy(&self, policy: MemoryPolicy) -> Result<(), EmilyError>;

    /// Report runtime and queue health.
    async fn health(&self) -> Result<HealthSnapshot, EmilyError>;

    /// Read current vectorization settings and job status.
    async fn vectorization_status(&self) -> Result<VectorizationStatus, EmilyError>;

    /// Persist a partial vectorization config update.
    async fn update_vectorization_config(
        &self,
        patch: VectorizationConfigPatch,
    ) -> Result<VectorizationConfig, EmilyError>;

    /// Start a background backfill run for missing vectors.
    async fn start_backfill(
        &self,
        request: VectorizationRunRequest,
    ) -> Result<VectorizationJobSnapshot, EmilyError>;

    /// Start a background revectorize run for existing objects.
    async fn start_revectorize(
        &self,
        request: VectorizationRunRequest,
    ) -> Result<VectorizationJobSnapshot, EmilyError>;

    /// Request cancellation for the currently active job.
    async fn cancel_vectorization_job(&self, job_id: &str) -> Result<(), EmilyError>;
}
