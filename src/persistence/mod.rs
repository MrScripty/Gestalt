mod error;
mod migrate;
mod paths;
mod schema;
mod store;

use crate::state::AppState;
use crate::terminal::{PersistedTerminalState, TerminalManager};

pub use error::PersistenceError;
pub use schema::PersistedWorkspaceV1;

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

pub fn load_workspace() -> Result<Option<PersistedWorkspaceV1>, PersistenceError> {
    let Some(workspace) = store::load_workspace()? else {
        return Ok(None);
    };

    let migrated = migrate::migrate_to_latest(workspace)?;
    Ok(Some(migrated))
}

pub fn save_workspace(workspace: &PersistedWorkspaceV1) -> Result<(), PersistenceError> {
    store::save_workspace(workspace)
}
