mod error;
mod migrate;
mod paths;
mod schema;
mod store;

use crate::state::AppState;
use crate::terminal::{PersistedTerminalState, TerminalManager};

/// Typed errors produced by workspace persistence operations.
pub use error::PersistenceError;
/// Versioned persisted workspace payload.
pub use schema::PersistedWorkspaceV1;

/// Builds a persistable workspace snapshot from current runtime state.
pub fn build_workspace_snapshot(
    app_state: &AppState,
    terminal_manager: &TerminalManager,
) -> PersistedWorkspaceV1 {
    let terminals = app_state
        .sessions
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
    Ok(Some(migrated))
}

/// Saves a workspace snapshot atomically to the persistence path.
pub fn save_workspace(workspace: &PersistedWorkspaceV1) -> Result<(), PersistenceError> {
    store::save_workspace(workspace)
}
