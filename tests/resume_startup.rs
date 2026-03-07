use gestalt::persistence::{self, PersistedWorkspaceV1};
use gestalt::state::AppState;
use gestalt::terminal::{PersistedTerminalState, TerminalManager};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_resume_startup_restores_groups_sessions_without_workspace_terminal_history() {
    with_workspace_path("resume", |_workspace_path| {
        let state = AppState::default();
        let session_id = state.sessions()[0].id;
        let group_id = state.sessions()[0].group_id;
        let cwd = state.group_path(group_id).unwrap_or(".").to_string();

        let terminal = PersistedTerminalState {
            session_id,
            cwd: cwd.clone(),
            rows: 24,
            cols: 80,
            cursor_row: 2,
            cursor_col: 3,
            hide_cursor: false,
            bracketed_paste: false,
            lines: vec!["restored one".to_string(), "restored two".to_string()],
        };

        let workspace = PersistedWorkspaceV1::new(state.clone(), vec![terminal]);
        persistence::save_workspace(&workspace).expect("save should succeed");

        let loaded = persistence::load_workspace()
            .expect("load should succeed")
            .expect("workspace should exist");
        let restored_state = loaded.app_state.clone().into_restored();
        assert_eq!(restored_state.groups().len(), state.groups().len());
        assert_eq!(restored_state.sessions().len(), state.sessions().len());

        let terminal_manager = TerminalManager::new();
        for restored in loaded.terminals {
            terminal_manager.seed_restored_terminal(restored);
        }
        let restored_snapshot = terminal_manager
            .snapshot_for_persist(session_id)
            .expect("snapshot should be available");
        assert_eq!(restored_snapshot.cwd, cwd);
        assert!(
            restored_snapshot.lines.is_empty(),
            "terminal history restoration now comes from Emily only"
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
