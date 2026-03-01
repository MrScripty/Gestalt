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
fn restore_repairs_invalid_command_id_counter() {
    with_workspace_path("insert-commands-repair", |_workspace_path| {
        let mut state = AppState::default();
        state.create_insert_command(
            "Deploy".to_string(),
            "cargo run --release".to_string(),
            String::new(),
            vec![],
        );
        state.command_library.next_command_id = 0;

        let workspace = PersistedWorkspaceV1::new(state, Vec::new());
        persistence::save_workspace(&workspace).expect("workspace save should succeed");

        let loaded = persistence::load_workspace()
            .expect("workspace load should succeed")
            .expect("workspace should exist");
        let mut restored = loaded.app_state.into_restored();
        let next_id_before = restored.command_library.next_command_id;
        let created_id = restored.create_insert_command(
            "Deploy 2".to_string(),
            "echo ok".to_string(),
            String::new(),
            vec![],
        );
        assert!(
            created_id >= next_id_before,
            "created id should be allocated from repaired counter"
        );
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
