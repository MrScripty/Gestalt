use gestalt::orchestration_log::{
    CommandKind, EventPayload, OrchestrationLogStore, ReceiptPayload, ReceiptStatus, TimelineEntry,
};
use gestalt::orchestrator;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn unique_db_path(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!("gestalt-{name}-{nonce}.sqlite3"))
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
fn stage_files_records_git_timeline() {
    let _guard = env_lock().lock().expect("env lock");
    let path = unique_db_path("orchestrator-git-log");
    unsafe {
        std::env::set_var("GESTALT_ORCHESTRATION_DB_PATH", &path);
    }

    let repo = TestRepo::new("orchestrator-git-log");
    write_file(&repo.path().join("notes.txt"), "hello\n");
    let repo_path = repo.path().to_string_lossy().to_string();

    let results = orchestrator::git::stage_files(&repo_path, &["notes.txt".to_string()]);
    assert_eq!(results.len(), 1);
    assert!(results[0].error.is_none());

    let store = OrchestrationLogStore::default();
    let recent = store
        .load_recent_commands(1)
        .expect("recent command should load");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].kind, CommandKind::GitStageFiles);
    let timeline = store
        .load_timeline(&recent[0].command_id)
        .expect("timeline should load");
    assert_eq!(timeline.len(), 3);
    assert!(matches!(timeline[0], TimelineEntry::Command(_)));
    assert!(matches!(
        timeline[1],
        TimelineEntry::Event(ref event)
            if matches!(event.payload, EventPayload::GitPathSucceeded { .. })
    ));
    assert!(matches!(timeline[2], TimelineEntry::Receipt(_)));

    unsafe {
        std::env::remove_var("GESTALT_ORCHESTRATION_DB_PATH");
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn stage_files_records_partial_receipt_when_paths_fail() {
    let _guard = env_lock().lock().expect("env lock");
    let path = unique_db_path("orchestrator-git-log-partial");
    unsafe {
        std::env::set_var("GESTALT_ORCHESTRATION_DB_PATH", &path);
    }

    let repo = TestRepo::new("orchestrator-git-log-partial");
    write_file(&repo.path().join("notes.txt"), "hello\n");
    let repo_path = repo.path().to_string_lossy().to_string();

    let results = orchestrator::git::stage_files(
        &repo_path,
        &["notes.txt".to_string(), "missing.txt".to_string()],
    );
    assert_eq!(results.len(), 2);
    assert!(results[0].error.is_none());
    assert!(results[1].error.is_some());

    let store = OrchestrationLogStore::default();
    let recent = store
        .load_recent_commands(1)
        .expect("recent command should load");
    let timeline = store
        .load_timeline(&recent[0].command_id)
        .expect("timeline should load");
    assert_eq!(timeline.len(), 4);
    assert!(matches!(timeline[0], TimelineEntry::Command(_)));
    assert!(matches!(
        timeline[1],
        TimelineEntry::Event(ref event)
            if matches!(event.payload, EventPayload::GitPathSucceeded { ref path } if path == "notes.txt")
    ));
    assert!(matches!(
        timeline[2],
        TimelineEntry::Event(ref event)
            if matches!(event.payload, EventPayload::GitPathFailed { ref path, .. } if path == "missing.txt")
    ));
    match timeline.last().expect("receipt should exist") {
        TimelineEntry::Receipt(receipt) => {
            assert_eq!(receipt.status, ReceiptStatus::PartiallySucceeded);
            assert!(matches!(
                receipt.payload,
                ReceiptPayload::Git {
                    ok_count: 1,
                    fail_count: 1,
                    ref summary,
                } if summary == "staged files"
            ));
        }
        other => panic!("expected receipt entry, got {other:?}"),
    }

    unsafe {
        std::env::remove_var("GESTALT_ORCHESTRATION_DB_PATH");
    }
    let _ = std::fs::remove_file(path);
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
