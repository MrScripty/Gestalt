use crate::state::AppState;
use crate::terminal::PersistedTerminalState;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

pub const WORKSPACE_SCHEMA_VERSION: u32 = 1;

/// Serialized workspace envelope with schema metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedWorkspaceV1 {
    pub schema_version: u32,
    pub saved_at_utc: String,
    pub app_state: AppState,
    pub terminals: Vec<PersistedTerminalState>,
}

impl PersistedWorkspaceV1 {
    /// Creates a new schema-v1 workspace payload.
    pub fn new(app_state: AppState, terminals: Vec<PersistedTerminalState>) -> Self {
        Self {
            schema_version: WORKSPACE_SCHEMA_VERSION,
            saved_at_utc: String::new(),
            app_state,
            terminals,
        }
    }

    /// Returns a copy with the save timestamp set.
    pub fn with_saved_at_unix(mut self, unix_seconds: u64) -> Self {
        self.saved_at_utc = unix_seconds.to_string();
        self
    }

    /// Computes a stable hash used to avoid redundant autosaves.
    pub fn stable_fingerprint(&self) -> Result<u64, serde_json::Error> {
        let bytes = serde_json::to_vec(&StablePayloadRef {
            schema_version: self.schema_version,
            app_state: &self.app_state,
            terminals: &self.terminals,
        })?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        Ok(hasher.finish())
    }
}

#[derive(Serialize)]
struct StablePayloadRef<'a> {
    schema_version: u32,
    app_state: &'a AppState,
    terminals: &'a [PersistedTerminalState],
}
