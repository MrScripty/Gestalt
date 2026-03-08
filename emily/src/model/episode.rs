use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Request to create a host-agnostic episode record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateEpisodeRequest {
    pub episode_id: String,
    pub stream_id: Option<String>,
    pub source_kind: String,
    pub episode_kind: String,
    pub started_at_unix_ms: i64,
    pub intent: Option<String>,
    pub metadata: Value,
}

/// Persisted episode record used as the anchor for future policy runtimes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpisodeRecord {
    pub id: String,
    pub stream_id: Option<String>,
    pub source_kind: String,
    pub episode_kind: String,
    pub state: EpisodeState,
    pub started_at_unix_ms: i64,
    pub closed_at_unix_ms: Option<i64>,
    pub intent: Option<String>,
    pub metadata: Value,
    pub last_outcome_id: Option<String>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

/// Lifecycle state for one episode record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EpisodeState {
    Open,
    Cautioned,
    Blocked,
    Completed,
    Cancelled,
}

/// Request to link one text object into an episode trace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceLinkRequest {
    pub episode_id: String,
    pub object_id: String,
    pub trace_kind: EpisodeTraceKind,
    pub linked_at_unix_ms: i64,
    pub metadata: Value,
}

/// Persisted episode-to-text linkage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpisodeTraceLink {
    pub id: String,
    pub episode_id: String,
    pub object_id: String,
    pub trace_kind: EpisodeTraceKind,
    pub linked_at_unix_ms: i64,
    pub metadata: Value,
}

/// Typed role for one text object inside an episode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EpisodeTraceKind {
    Input,
    Output,
    Context,
    Summary,
    Note,
    Other,
}

/// Request to append one outcome record to an episode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordOutcomeRequest {
    pub outcome_id: String,
    pub episode_id: String,
    pub status: OutcomeStatus,
    pub recorded_at_unix_ms: i64,
    pub summary: Option<String>,
    pub metadata: Value,
}

/// Persisted outcome for one episode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutcomeRecord {
    pub id: String,
    pub episode_id: String,
    pub status: OutcomeStatus,
    pub recorded_at_unix_ms: i64,
    pub summary: Option<String>,
    pub metadata: Value,
}

/// Typed status for one episode outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutcomeStatus {
    Succeeded,
    Failed,
    Partial,
    Cancelled,
    Unknown,
}

/// Request to append one durable audit record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppendAuditRecordRequest {
    pub audit_id: String,
    pub episode_id: String,
    pub kind: AuditRecordKind,
    pub ts_unix_ms: i64,
    pub summary: String,
    pub metadata: Value,
}

/// Immutable audit trail entry for Emily runtime decisions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditRecord {
    pub id: String,
    pub episode_id: String,
    pub kind: AuditRecordKind,
    pub ts_unix_ms: i64,
    pub summary: String,
    pub metadata: Value,
}

/// Audit event category for episode and outcome flows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditRecordKind {
    EpisodeCreated,
    TraceLinked,
    EarlEvaluated,
    OutcomeRecorded,
    RoutingDecided,
    RemoteEpisodeRecorded,
    ValidationRecorded,
    BoundaryEvent,
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn episode_contracts_roundtrip_json() {
        let request = CreateEpisodeRequest {
            episode_id: "ep-1".to_string(),
            stream_id: Some("stream-a".to_string()),
            source_kind: "terminal".to_string(),
            episode_kind: "command_round".to_string(),
            started_at_unix_ms: 1,
            intent: Some("inspect state".to_string()),
            metadata: json!({"cwd": "/tmp"}),
        };
        let text = serde_json::to_string(&request).expect("serialize episode request");
        let restored: CreateEpisodeRequest =
            serde_json::from_str(&text).expect("deserialize episode request");
        assert_eq!(request, restored);

        let outcome = OutcomeRecord {
            id: "out-1".to_string(),
            episode_id: "ep-1".to_string(),
            status: OutcomeStatus::Succeeded,
            recorded_at_unix_ms: 2,
            summary: Some("command completed".to_string()),
            metadata: json!({"exit_code": 0}),
        };
        let text = serde_json::to_string(&outcome).expect("serialize outcome");
        let restored: OutcomeRecord = serde_json::from_str(&text).expect("deserialize outcome");
        assert_eq!(outcome, restored);
    }
}
