mod error;
mod migrate;
mod paths;
mod schema;
mod store;

use crate::state::AppState;
use crate::terminal::{PersistedTerminalState, TerminalManager};
use paths::{default_workspace_path, workspace_path};
use serde_json::Value;
use std::path::Path;

/// Typed errors produced by workspace persistence operations.
pub use error::PersistenceError;
/// Versioned persisted workspace payload.
pub use schema::PersistedWorkspaceV1;

/// Builds a persistable workspace snapshot from current runtime state.
pub fn build_workspace_snapshot(
    app_state: &AppState,
    terminal_manager: &TerminalManager,
) -> PersistedWorkspaceV1 {
    build_workspace_snapshot_with_history_limit(app_state, terminal_manager, usize::MAX)
}

/// Builds a persistable workspace snapshot with a per-terminal history cap.
pub fn build_workspace_snapshot_with_history_limit(
    app_state: &AppState,
    terminal_manager: &TerminalManager,
    _max_history_lines: usize,
) -> PersistedWorkspaceV1 {
    let terminals = app_state
        .sessions()
        .iter()
        .filter_map(|session| terminal_manager.snapshot_for_persist(session.id))
        .collect::<Vec<PersistedTerminalState>>();

    PersistedWorkspaceV1::new(app_state.clone(), terminals)
}

/// Loads the latest valid workspace snapshot if available.
pub fn load_workspace() -> Result<Option<PersistedWorkspaceV1>, PersistenceError> {
    let Some(workspace) = store::load_workspace()? else {
        return Ok(None);
    };

    let migrated = migrate::migrate_to_latest(workspace)?;
    Ok(Some(seed_commands_from_host_workspace_if_needed(migrated)?))
}

/// Saves a workspace snapshot atomically to the persistence path.
pub fn save_workspace(workspace: &PersistedWorkspaceV1) -> Result<(), PersistenceError> {
    store::save_workspace(workspace)
}

fn seed_commands_from_host_workspace_if_needed(
    mut workspace: PersistedWorkspaceV1,
) -> Result<PersistedWorkspaceV1, PersistenceError> {
    if workspace.app_state.has_commands() {
        return Ok(workspace);
    }

    let current_path = workspace_path();
    let host_path = default_workspace_path();
    if current_path == host_path {
        return Ok(workspace);
    }
    if !workspace_file_declares_command_library(&current_path)? {
        return Ok(workspace);
    }

    let Some(host_workspace) = store::load_workspace_from_path(&host_path)? else {
        return Ok(workspace);
    };
    let host_workspace = migrate::migrate_to_latest(host_workspace)?;
    workspace.app_state.seed_commands_from(&host_workspace.app_state);
    Ok(workspace)
}

fn workspace_file_declares_command_library(path: &Path) -> Result<bool, PersistenceError> {
    if !path.exists() {
        return Ok(false);
    }

    let contents = std::fs::read_to_string(path).map_err(|source| PersistenceError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let Ok(payload) = serde_json::from_str::<Value>(&contents) else {
        return Ok(false);
    };

    Ok(payload
        .get("app_state")
        .and_then(Value::as_object)
        .is_some_and(|app_state| app_state.contains_key("command_library")))
}
