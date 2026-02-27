use crate::persistence::error::PersistenceError;
use crate::persistence::schema::{PersistedWorkspaceV1, WORKSPACE_SCHEMA_VERSION};

pub fn migrate_to_latest(
    workspace: PersistedWorkspaceV1,
) -> Result<PersistedWorkspaceV1, PersistenceError> {
    match workspace.schema_version {
        WORKSPACE_SCHEMA_VERSION => Ok(workspace),
        version => Err(PersistenceError::UnsupportedSchemaVersion { version }),
    }
}
