use gestalt::persistence::{self, PersistedWorkspaceV1};
use gestalt::state::AppState;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn insert_commands_persist_across_save_load() {
    with_workspace_path("insert-commands-roundtrip", |_workspace_path| {
        let mut state = AppState::default();
        let first_id = state.create_insert_command(
            "Build".to_string(),
            "cargo build".to_string(),
            "Build debug".to_string(),
            vec!["cargo".to_string(), "build".to_string()],
        );
        let second_id = state.create_insert_command(
            "Test".to_string(),
            "cargo test -q".to_string(),
            "Run tests".to_string(),
            vec!["cargo".to_string(), "test".to_string()],
        );
        let _ = state.update_insert_command(
            first_id,
            "Build Release".to_string(),
            "cargo build --release".to_string(),
            "Build release artifacts".to_string(),
            vec![
                "cargo".to_string(),
                "build".to_string(),
                "release".to_string(),
            ],
        );
        let _ = state.delete_insert_command(second_id);

        let workspace = PersistedWorkspaceV1::new(state.clone(), Vec::new());
        persistence::save_workspace(&workspace).expect("workspace save should succeed");

        let loaded = persistence::load_workspace()
            .expect("workspace load should succeed")
            .expect("workspace should exist");
        let restored = loaded.app_state.into_restored();

        assert_eq!(restored.commands().len(), 1);
        let command = restored
            .command_by_id(first_id)
            .expect("first command should exist");
        assert_eq!(command.name, "Build Release");
        assert_eq!(command.prompt, "cargo build --release");
    });
}

#[test]
fn insert_command_large_library_roundtrip() {
    with_workspace_path("insert-commands-large-roundtrip", |_workspace_path| {
        let mut state = AppState::default();
        for index in 0..1_500_u32 {
            let _ = state.create_insert_command(
                format!("Command {index:04}"),
                format!("echo command-{index:04}"),
                format!("Description {index:04}"),
                vec!["bulk".to_string(), "roundtrip".to_string()],
            );
        }

        let workspace = PersistedWorkspaceV1::new(state, Vec::new());
        persistence::save_workspace(&workspace).expect("workspace save should succeed");

        let loaded = persistence::load_workspace()
            .expect("workspace load should succeed")
            .expect("workspace should exist");
        let restored = loaded.app_state.into_restored();

        assert_eq!(
            restored.commands().len(),
            1_500,
            "all commands should survive save/load"
        );
        assert_eq!(
            restored.commands()[1_499].name,
            "Command 1499",
            "last command should be preserved"
        );
    });
}

#[test]
fn restore_repairs_invalid_command_id_counter() {
    with_workspace_path("insert-commands-repair", |_workspace_path| {
        let mut state = AppState::default();
        state.create_insert_command(
            "Deploy".to_string(),
            "cargo run --release".to_string(),
            String::new(),
            vec![],
        );
        let workspace = PersistedWorkspaceV1::new(state, Vec::new());
        let mut payload =
            serde_json::to_value(workspace).expect("workspace json value should serialize");
        payload["app_state"]["next_command_id"] = serde_json::Value::from(0_u32);
        let workspace: PersistedWorkspaceV1 =
            serde_json::from_value(payload).expect("workspace should deserialize");
        persistence::save_workspace(&workspace).expect("workspace save should succeed");

        let loaded = persistence::load_workspace()
            .expect("workspace load should succeed")
            .expect("workspace should exist");
        let mut restored = loaded.app_state.into_restored();
        let created_id = restored.create_insert_command(
            "Deploy 2".to_string(),
            "echo ok".to_string(),
            String::new(),
            vec![],
        );
        assert!(
            created_id >= 2,
            "created id should be allocated from repaired counter"
        );
    });
}

#[test]
fn legacy_workspace_without_command_library_loads_with_defaults() {
    with_workspace_path("insert-commands-legacy", |workspace_path| {
        let legacy_workspace = PersistedWorkspaceV1::new(AppState::default(), Vec::new());
        let mut payload =
            serde_json::to_value(legacy_workspace).expect("workspace json value should serialize");

        let Some(app_state) = payload
            .get_mut("app_state")
            .and_then(|value| value.as_object_mut())
        else {
            panic!("app_state object should exist");
        };
        app_state.remove("command_library");

        let json =
            serde_json::to_string_pretty(&payload).expect("legacy workspace json should encode");
        std::fs::write(&workspace_path, json).expect("legacy workspace should be written");

        let loaded = persistence::load_workspace()
            .expect("workspace load should succeed")
            .expect("workspace should exist");
        let restored = loaded.app_state.into_restored();
        assert!(
            restored.commands().is_empty(),
            "missing command library should default to empty"
        );
        let mut restored = restored;
        let created_id = restored.create_insert_command(
            "New Command".to_string(),
            "echo hello".to_string(),
            String::new(),
            vec![],
        );
        assert_eq!(created_id, 1, "id allocator should be repaired");
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
