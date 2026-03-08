use serde::{Deserialize, Serialize};

/// Explicit durable memory state for one text object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryState {
    Pending,
    Integrated,
    Quarantined,
    Deferred,
}

/// Durable integrity snapshot produced after ECGL evaluation runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntegritySnapshot {
    pub id: String,
    pub ts_unix_ms: i64,
    pub ci_value: f32,
    pub tau: f32,
    pub integrated_count: u64,
    pub quarantined_count: u64,
    pub pending_count: u64,
    pub deferred_count: u64,
}
