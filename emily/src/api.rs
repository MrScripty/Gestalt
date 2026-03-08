use crate::error::EmilyError;
use crate::model::{
    AppendAuditRecordRequest, AuditRecord, ContextPacket, ContextQuery, CreateEpisodeRequest,
    DatabaseLocator, EarlEvaluationRecord, EarlEvaluationRequest, EpisodeRecord, EpisodeTraceLink,
    HealthSnapshot, HistoryPage, HistoryPageRequest, IngestTextRequest, MemoryPolicy,
    OutcomeRecord, RecordOutcomeRequest, TextObject, TraceLinkRequest, VectorizationConfig,
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

    /// Evaluate one episode with the current EARL gate.
    async fn evaluate_episode_risk(
        &self,
        request: EarlEvaluationRequest,
    ) -> Result<EarlEvaluationRecord, EmilyError>;

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
