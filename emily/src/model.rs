use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

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
    pub embedding: Option<Vec<f32>>,
    pub confidence: f32,
    pub learning_weight: f32,
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

/// Health and queue snapshot for diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSnapshot {
    pub db_open: bool,
    pub db_locator: Option<DatabaseLocator>,
    pub queued_ingest_events: usize,
    pub dropped_ingest_events: u64,
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
