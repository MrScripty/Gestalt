use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

mod earl;
mod episode;

pub use earl::{
    EarlDecision, EarlEvaluationRecord, EarlEvaluationRequest, EarlHostAction, EarlSignalVector,
};
pub use episode::{
    AppendAuditRecordRequest, AuditRecord, AuditRecordKind, CreateEpisodeRequest, EpisodeRecord,
    EpisodeState, EpisodeTraceKind, EpisodeTraceLink, OutcomeRecord, OutcomeStatus,
    RecordOutcomeRequest, TraceLinkRequest,
};

/// Address of an embedded database instance that can be opened or switched at runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseLocator {
    pub storage_path: PathBuf,
    pub namespace: String,
    pub database: String,
}

/// Generic input payload accepted by Emily from any host system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestTextRequest {
    pub stream_id: String,
    pub source_kind: String,
    pub object_kind: TextObjectKind,
    pub sequence: u64,
    pub ts_unix_ms: i64,
    pub text: String,
    pub metadata: Value,
}

/// Canonical text object stored by Emily.
///
/// The policy-related fields are storage slots for Emily's future policy runtime.
/// Until active `EARL` / `ECGL` behavior exists, hosts should treat these values
/// as provisional metadata rather than authoritative integration decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextObject {
    pub id: String,
    pub stream_id: String,
    pub source_kind: String,
    pub object_kind: TextObjectKind,
    pub sequence: u64,
    pub ts_unix_ms: i64,
    pub text: String,
    pub metadata: Value,
    pub epsilon: Option<f32>,
    pub confidence: f32,
    pub outcome_factor: f32,
    pub novelty_factor: f32,
    pub stability_factor: f32,
    pub learning_weight: f32,
    pub gate_score: Option<f32>,
    /// Whether the object has been admitted into integrated memory.
    ///
    /// New objects should remain `false` until an explicit integration policy
    /// decides otherwise.
    pub integrated: bool,
    pub quarantine_score: f32,
}

/// Vector record stored separately from text objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextVector {
    pub id: String,
    pub object_id: String,
    pub stream_id: String,
    pub sequence: u64,
    pub ts_unix_ms: i64,
    pub dimensions: usize,
    pub profile_id: String,
    pub vector: Vec<f32>,
}

/// Object category is intentionally generic and host-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextObjectKind {
    UserInput,
    SystemOutput,
    Summary,
    Note,
    Other,
}

/// Directed relationship between text objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEdge {
    pub id: String,
    pub from_id: String,
    pub to_id: String,
    pub edge_type: TextEdgeType,
    pub weight: f32,
    pub ts_unix_ms: i64,
}

/// Edge semantics for linear and semantic memory graph traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextEdgeType {
    LinearNext,
    SemanticSimilar,
    Related,
}

/// Query for ranked context packets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextQuery {
    pub stream_id: Option<String>,
    pub query_text: String,
    pub top_k: usize,
    pub neighbor_depth: u8,
}

/// One context item with provenance and scores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    pub object: TextObject,
    pub similarity: f32,
    pub rank: f32,
    pub provenance: Vec<String>,
}

/// Context response consumed by host orchestrators.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPacket {
    pub items: Vec<ContextItem>,
}

/// Cursor-based history page query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryPageRequest {
    pub stream_id: String,
    pub before_sequence: Option<u64>,
    pub limit: usize,
}

/// History response optimized for incremental backfill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryPage {
    pub items: Vec<TextObject>,
    pub next_before_sequence: Option<u64>,
}

/// Mutable policy values for retrieval/scoring behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPolicy {
    pub semantic_top_k: usize,
    pub semantic_min_similarity: f32,
    pub recency_decay_half_life_s: u64,
}

impl Default for MemoryPolicy {
    fn default() -> Self {
        Self {
            semantic_top_k: 12,
            semantic_min_similarity: 0.78,
            recency_decay_half_life_s: 3_600,
        }
    }
}

/// Health and pending-work snapshot for diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSnapshot {
    pub db_open: bool,
    pub db_locator: Option<DatabaseLocator>,
    /// Number of ingest operations currently in progress.
    ///
    /// Emily currently performs direct ingest rather than enqueueing work on a
    /// durable ingest queue, so this reflects in-flight ingest operations.
    pub queued_ingest_events: usize,
    pub dropped_ingest_events: u64,
}

/// Persistent runtime settings that control embedding behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorizationConfig {
    pub enabled: bool,
    pub expected_dimensions: usize,
    pub profile_id: String,
}

impl Default for VectorizationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            expected_dimensions: 1024,
            profile_id: "qwen3-0.6b".to_string(),
        }
    }
}

/// Partial update for vectorization config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VectorizationConfigPatch {
    pub enabled: Option<bool>,
    pub expected_dimensions: Option<usize>,
    pub profile_id: Option<String>,
}

/// User request to start a background vectorization run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorizationRunRequest {
    pub stream_id: Option<String>,
}

/// Background job kind for vectorization maintenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VectorizationJobKind {
    Backfill,
    Revectorize,
}

/// Runtime lifecycle status for one vectorization job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VectorizationJobState {
    Running,
    Completed,
    Cancelled,
}

/// Snapshot describing one vectorization job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorizationJobSnapshot {
    pub job_id: String,
    pub kind: VectorizationJobKind,
    pub state: VectorizationJobState,
    pub stream_id: Option<String>,
    pub processed: u64,
    pub vectorized: u64,
    pub skipped: u64,
    pub failed: u64,
    pub last_error: Option<String>,
}

/// Full vectorization runtime status for UI and orchestration consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorizationStatus {
    pub config: VectorizationConfig,
    pub provider_available: bool,
    pub provider_status: Option<EmbeddingProviderStatus>,
    pub active_job: Option<VectorizationJobSnapshot>,
    pub last_job: Option<VectorizationJobSnapshot>,
}

/// Embedding provider runtime status surfaced for operational diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingProviderStatus {
    pub state: String,
    pub session_id: Option<String>,
    pub queued_runs: Option<usize>,
    pub queue_items: Option<usize>,
    pub keep_alive: Option<bool>,
    pub last_error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locator_roundtrips_json() {
        let locator = DatabaseLocator {
            storage_path: PathBuf::from("/tmp/emily/demo"),
            namespace: "main".to_string(),
            database: "default".to_string(),
        };
        let text = serde_json::to_string(&locator).expect("serialize locator");
        let restored: DatabaseLocator = serde_json::from_str(&text).expect("deserialize locator");
        assert_eq!(locator, restored);
    }
}
