use gestalt::persistence::{self, PersistedWorkspaceV1};
use gestalt::state::AppState;
use gestalt::terminal::PersistedTerminalState;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_save_workspace_roundtrip_restores_app_state() {
    with_workspace_path("roundtrip", |workspace_path| {
        let mut state = AppState::default();
        let first_session = state.sessions[0].id;
        state.rename_session(first_session, "Restored Agent".to_string());

        let terminal = PersistedTerminalState {
            session_id: first_session,
            cwd: state
                .group_path(state.sessions[0].group_id)
                .unwrap_or(".")
                .to_string(),
            rows: 42,
            cols: 140,
            cursor_row: 3,
            cursor_col: 2,
            hide_cursor: false,
            bracketed_paste: true,
            lines: vec!["history line".to_string(), "next line".to_string()],
        };
        let workspace = PersistedWorkspaceV1::new(state.clone(), vec![terminal]);

        persistence::save_workspace(&workspace).expect("workspace save should succeed");
        assert!(workspace_path.exists(), "workspace file should be created");

        let loaded = persistence::load_workspace()
            .expect("workspace load should succeed")
            .expect("workspace should exist");
        assert_eq!(loaded.app_state.sessions.len(), state.sessions.len());
        assert_eq!(loaded.app_state.groups.len(), state.groups.len());
        assert_eq!(loaded.terminals.len(), 1);
        assert!(
            loaded.terminals[0].lines.is_empty(),
            "terminal history should not be persisted in workspace snapshots"
        );

        let raw =
            std::fs::read_to_string(&workspace_path).expect("workspace file should be readable");
        assert!(
            !raw.contains("history line"),
            "workspace payload should not contain terminal history text"
        );
    });
}

#[test]
fn test_save_workspace_roundtrip_preserves_group_specific_layouts() {
    with_workspace_path("group-layout-roundtrip", |_workspace_path| {
        let mut state = AppState::default();
        let first_group = state.groups[0].id;
        let (second_group, _) = state.create_group_with_defaults("/tmp/layout-b".to_string());

        state.set_group_runner_width_px(first_group, 620);
        state.set_group_agent_top_ratio(first_group, 0.61);
        state.set_group_runner_top_ratio(first_group, 0.39);
        state.set_group_runner_width_px(second_group, 300);
        state.set_group_agent_top_ratio(second_group, 0.31);
        state.set_group_runner_top_ratio(second_group, 0.69);

        let workspace = PersistedWorkspaceV1::new(state, Vec::new());
        persistence::save_workspace(&workspace).expect("workspace save should succeed");

        let loaded = persistence::load_workspace()
            .expect("workspace load should succeed")
            .expect("workspace should exist");
        let first_layout = loaded.app_state.group_layout(first_group);
        let second_layout = loaded.app_state.group_layout(second_group);

        assert_eq!(first_layout.runner_width_px, 620);
        assert_eq!(first_layout.agent_top_ratio, 0.61);
        assert_eq!(first_layout.runner_top_ratio, 0.39);
        assert_eq!(second_layout.runner_width_px, 300);
        assert_eq!(second_layout.agent_top_ratio, 0.31);
        assert_eq!(second_layout.runner_top_ratio, 0.69);
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
