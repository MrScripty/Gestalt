use gestalt::persistence::{self, PersistedWorkspaceV1, PersistenceError};
use gestalt::state::AppState;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_load_workspace_with_corrupt_primary_uses_backup() {
    with_workspace_path("recovery", |workspace_path| {
        let state = AppState::default();
        let workspace = PersistedWorkspaceV1::new(state, Vec::new());
        persistence::save_workspace(&workspace).expect("initial save should succeed");

        let backup_path = workspace_path.with_file_name(format!(
            "{}.bak",
            workspace_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("workspace.json")
        ));
        std::fs::copy(&workspace_path, &backup_path).expect("backup copy should succeed");

        std::fs::write(&workspace_path, "{invalid json").expect("corrupt write should succeed");

        let loaded = persistence::load_workspace()
            .expect("load should not fail when primary is corrupt")
            .expect("backup should be used");

        assert_eq!(loaded.app_state.groups().len(), 1);
        assert!(
            !workspace_path.exists(),
            "corrupt primary should be quarantined"
        );
        let quarantined_path = workspace_path.with_file_name(format!(
            "{}.corrupt",
            workspace_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("workspace.json")
        ));
        assert!(
            quarantined_path.exists(),
            "corrupt workspace should be renamed to .corrupt"
        );
    });
}

#[test]
fn test_load_workspace_rejects_unsupported_schema_version() {
    with_workspace_path("unsupported-schema", |workspace_path| {
        let mut workspace = PersistedWorkspaceV1::new(AppState::default(), Vec::new());
        workspace.schema_version += 1;
        let payload = serde_json::to_string(&workspace).expect("payload should serialize");
        std::fs::write(&workspace_path, payload).expect("workspace write should succeed");

        let error = persistence::load_workspace().expect_err("load should reject schema");
        assert!(matches!(
            error,
            PersistenceError::UnsupportedSchemaVersion { version } if version == workspace.schema_version
        ));
    });
}

fn with_workspace_path(test_name: &str, run: impl FnOnce(PathBuf)) {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let root = std::env::temp_dir().join(format!("gestalt-{test_name}-{nonce}"));
    let workspace_path = root.join("workspace.json");
    std::fs::create_dir_all(&root).expect("temp root should be created");

    unsafe {
        std::env::set_var("GESTALT_WORKSPACE_PATH", &workspace_path);
    }

    run(workspace_path.clone());

    unsafe {
        std::env::remove_var("GESTALT_WORKSPACE_PATH");
    }
    let _ = std::fs::remove_dir_all(root);
}
