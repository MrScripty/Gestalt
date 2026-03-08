use crate::persistence::error::PersistenceError;
use crate::persistence::schema::{PersistedWorkspaceV1, WORKSPACE_SCHEMA_VERSION};

/// Validates and migrates persisted workspace payloads to the latest schema.
pub fn migrate_to_latest(
    workspace: PersistedWorkspaceV1,
) -> Result<PersistedWorkspaceV1, PersistenceError> {
    match workspace.schema_version {
        WORKSPACE_SCHEMA_VERSION => Ok(workspace.without_terminal_history()),
        version => Err(PersistenceError::UnsupportedSchemaVersion { version }),
    }
}

#[cfg(test)]
mod tests {
    use super::migrate_to_latest;
    use crate::persistence::error::PersistenceError;
    use crate::persistence::schema::{PersistedWorkspaceV1, WORKSPACE_SCHEMA_VERSION};
    use crate::state::AppState;
    use crate::terminal::PersistedTerminalState;

    #[test]
    fn migrate_to_latest_strips_terminal_history_from_v1_payloads() {
        let state = AppState::default();
        let session_id = state.sessions()[0].id;
        let workspace = PersistedWorkspaceV1 {
            schema_version: WORKSPACE_SCHEMA_VERSION,
            saved_at_utc: "123".to_string(),
            app_state: state,
            terminals: vec![PersistedTerminalState {
                session_id,
                cwd: "/tmp/test".to_string(),
                rows: 24,
                cols: 80,
                cursor_row: 2,
                cursor_col: 3,
                hide_cursor: false,
                bracketed_paste: false,
                lines: vec!["prompt".to_string(), "output".to_string()],
            }],
        };

        let migrated = migrate_to_latest(workspace).expect("migration should succeed");
        assert_eq!(migrated.terminals.len(), 1);
        assert!(migrated.terminals[0].lines.is_empty());
    }

    #[test]
    fn migrate_to_latest_rejects_future_schema_versions() {
        let workspace = PersistedWorkspaceV1 {
            schema_version: WORKSPACE_SCHEMA_VERSION + 1,
            saved_at_utc: String::new(),
            app_state: AppState::default(),
            terminals: Vec::new(),
        };

        let error = migrate_to_latest(workspace).expect_err("migration should reject schema");
        assert!(matches!(
            error,
            PersistenceError::UnsupportedSchemaVersion { version }
                if version == WORKSPACE_SCHEMA_VERSION + 1
        ));
    }
}
