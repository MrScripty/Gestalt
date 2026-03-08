use emily::api::EmilyApi;
use emily::model::DatabaseLocator;
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use gestalt::emily_bridge::EmilyBridge;
use gestalt::emily_seed::{SYNTHETIC_TERMINAL_DATASET, seed_builtin_corpus};
use gestalt::local_agent_context::{LocalAgentContextStatus, prepare_local_agent_command};
use gestalt::orchestrator::{GroupOrchestratorSnapshot, GroupTerminalState, TerminalRound};
use gestalt::state::{SessionRole, SessionStatus, WorkspaceState};
use gestalt::terminal::TerminalManager;
use gestalt::{orchestration_log::CommandPayload, orchestrator};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct DbGuards {
    _lock: std::sync::MutexGuard<'static, ()>,
    orchestration_path: PathBuf,
    checkpoint_path: PathBuf,
}

impl DbGuards {
    fn new(name: &str) -> Self {
        let lock = env_lock().lock().expect("env lock");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let orchestration_path =
            std::env::temp_dir().join(format!("gestalt-{name}-orchestration-{nonce}.sqlite3"));
        let checkpoint_path =
            std::env::temp_dir().join(format!("gestalt-{name}-checkpoints-{nonce}.sqlite3"));
        unsafe {
            std::env::set_var("GESTALT_ORCHESTRATION_DB_PATH", &orchestration_path);
            std::env::set_var("GESTALT_RUN_CHECKPOINT_DB_PATH", &checkpoint_path);
        }
        Self {
            _lock: lock,
            orchestration_path,
            checkpoint_path,
        }
    }
}

impl Drop for DbGuards {
    fn drop(&mut self) {
        unsafe {
            std::env::remove_var("GESTALT_ORCHESTRATION_DB_PATH");
            std::env::remove_var("GESTALT_RUN_CHECKPOINT_DB_PATH");
        }
        let _ = std::fs::remove_file(&self.orchestration_path);
        let _ = std::fs::remove_file(&self.checkpoint_path);
    }
}

struct TestRepo {
    root: PathBuf,
}

impl TestRepo {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let root = std::env::temp_dir().join(format!("gestalt-{name}-{nonce}"));
        std::fs::create_dir_all(&root).expect("test repo root should be created");

        run_git(&root, &["init"]);
        run_git(&root, &["config", "user.email", "gestalt-test@example.com"]);
        run_git(&root, &["config", "user.name", "Gestalt Test"]);
        write_file(&root.join("README.md"), "# test\n");
        run_git(&root, &["add", "README.md"]);
        run_git(&root, &["commit", "-m", "chore: initial"]);

        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn local_agent_command_uses_seeded_emily_context_and_preserves_display_text() {
    let _guards = DbGuards::new("emily-local-agent-context");
    let locator = unique_locator("emily-local-agent-context");
    let storage_path = locator.storage_path.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    runtime.block_on(async {
        let emily_runtime = EmilyRuntime::new(Arc::new(SurrealEmilyStore::new()));
        emily_runtime
            .open_db(locator.clone())
            .await
            .expect("db should open");
        let _ = seed_builtin_corpus(&emily_runtime, SYNTHETIC_TERMINAL_DATASET)
            .await
            .expect("terminal dataset should seed");
        emily_runtime
            .close_db()
            .await
            .expect("db should close cleanly");
    });

    let bridge = Arc::new(EmilyBridge::new(locator));
    let group_orchestrator = seeded_group_orchestrator();
    let command = "Summarize recent terminal context".to_string();

    let prepared = runtime.block_on(prepare_local_agent_command(
        bridge.clone(),
        group_orchestrator,
        command.clone(),
    ));

    assert_eq!(prepared.display_command, command);
    assert!(
        prepared
            .dispatched_command
            .contains("Emily context from session 1:")
    );
    assert!(
        prepared
            .dispatched_command
            .contains("repository clean and tests green")
    );
    assert!(matches!(
        prepared.context_status,
        LocalAgentContextStatus::Attached { session_id: 1, .. }
    ));

    drop(bridge);
    thread::sleep(Duration::from_millis(50));
    let _ = std::fs::remove_dir_all(storage_path);
}

#[test]
fn prepared_local_agent_run_logs_display_command_and_dispatches_context_payload() {
    let _guards = DbGuards::new("emily-local-agent-run");
    let repo = TestRepo::new("emily-local-agent-run");
    let repo_path = repo.path().to_string_lossy().to_string();
    let mut workspace = WorkspaceState::default();
    let (group_id, _) = workspace.create_group_with_defaults(repo_path);
    let terminal_manager = TerminalManager::new();

    let dispatch = orchestrator::start_local_agent_run_prepared(
        &workspace,
        &terminal_manager,
        group_id,
        "cargo check",
        "cargo check\n\nEmily context from session 1:\n- [note #5] repository clean and tests green",
    )
    .expect("prepared run start should succeed");
    assert_eq!(dispatch.results.len(), 3);

    let recent = gestalt::orchestration_log::OrchestrationLogStore::default()
        .load_recent_commands(1)
        .expect("recent command should load");
    assert_eq!(recent.len(), 1);
    match &recent[0].payload {
        CommandPayload::LocalAgentSendLine {
            line,
            display_line,
            run_id,
            ..
        } => {
            assert!(line.contains("Emily context from session 1"));
            assert_eq!(display_line.as_deref(), Some("cargo check"));
            assert!(run_id.is_some());
        }
        other => panic!("expected local-agent payload, got {other:?}"),
    }
}

fn seeded_group_orchestrator() -> GroupOrchestratorSnapshot {
    GroupOrchestratorSnapshot {
        group_id: 1,
        group_path: "/workspace/demo".to_string(),
        terminals: vec![GroupTerminalState {
            session_id: 1,
            title: "Agent".to_string(),
            role: SessionRole::Agent,
            status: SessionStatus::Idle,
            cwd: "/workspace/demo".to_string(),
            is_selected: true,
            is_focused: true,
            is_runtime_ready: true,
            latest_round: TerminalRound {
                start_row: 1,
                end_row: 2,
                lines: vec!["git status".to_string()],
            },
        }],
    }
}

fn unique_locator(name: &str) -> DatabaseLocator {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let storage_path =
        std::env::temp_dir().join(format!("gestalt-{name}-{nonce}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&storage_path);
    DatabaseLocator {
        storage_path,
        namespace: "gestalt_test".to_string(),
        database: "default".to_string(),
    }
}

fn run_git(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("git command should run");
    if !output.status.success() {
        panic!(
            "git command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn write_file(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("file should be written");
}
