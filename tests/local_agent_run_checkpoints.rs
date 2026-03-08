use gestalt::orchestration_log::{CommandPayload, OrchestrationLogStore};
use gestalt::orchestrator;
use gestalt::run_checkpoints;
use gestalt::state::WorkspaceState;
use gestalt::terminal::TerminalManager;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

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
fn start_local_agent_run_records_checkpoint_and_run_id() {
    let _guards = DbGuards::new("local-agent-run");
    let repo = TestRepo::new("local-agent-run");
    let repo_path = repo.path().to_string_lossy().to_string();
    let mut workspace = WorkspaceState::default();
    let (group_id, _) = workspace.create_group_with_defaults(repo_path.clone());
    let terminal_manager = TerminalManager::new();

    let dispatch =
        orchestrator::start_local_agent_run(&workspace, &terminal_manager, group_id, "cargo check")
            .expect("run start should succeed");
    assert_eq!(dispatch.results.len(), 3);
    let run_id = dispatch
        .run_id
        .expect("repo run should capture a checkpoint");

    let checkpoint = run_checkpoints::load_latest_run_checkpoint_for_group_path(&repo_path)
        .expect("checkpoint should load")
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.run_id, run_id);

    let store = OrchestrationLogStore::default();
    let recent = store
        .load_recent_commands(1)
        .expect("recent command should load");
    assert_eq!(recent.len(), 1);
    match &recent[0].payload {
        CommandPayload::LocalAgentSendLine {
            line,
            run_id: payload_run_id,
            ..
        } => {
            assert_eq!(line, "cargo check");
            assert_eq!(payload_run_id.as_deref(), Some(run_id.as_str()));
        }
        other => panic!("expected local-agent payload, got {other:?}"),
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
