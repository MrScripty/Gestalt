use gestalt::run_checkpoints;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct RunCheckpointDbGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    path: PathBuf,
}

impl RunCheckpointDbGuard {
    fn new(name: &str) -> Self {
        let lock = env_lock().lock().expect("env lock");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let path =
            std::env::temp_dir().join(format!("gestalt-{name}-run-checkpoints-{nonce}.sqlite3"));
        unsafe {
            std::env::set_var("GESTALT_RUN_CHECKPOINT_DB_PATH", &path);
        }
        Self { _lock: lock, path }
    }
}

impl Drop for RunCheckpointDbGuard {
    fn drop(&mut self) {
        unsafe {
            std::env::remove_var("GESTALT_RUN_CHECKPOINT_DB_PATH");
        }
        let _ = std::fs::remove_file(&self.path);
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
fn capture_checkpoint_returns_none_for_non_repo_path() {
    let _db_guard = RunCheckpointDbGuard::new("run-checkpoints-non-repo");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let path = std::env::temp_dir().join(format!("gestalt-run-checkpoints-non-repo-{nonce}"));
    std::fs::create_dir_all(&path).expect("temp dir should be created");

    let checkpoint = run_checkpoints::capture_run_checkpoint(
        7,
        path.to_str().expect("path should be utf-8"),
        "cargo check",
    )
    .expect("non-repo capture should not fail");
    assert!(checkpoint.is_none());

    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn latest_review_ignores_dirty_baseline_until_file_changes_again() {
    let _db_guard = RunCheckpointDbGuard::new("run-checkpoints-review");
    let repo = TestRepo::new("run-checkpoints-review");
    let repo_path = repo.path().to_string_lossy().to_string();

    write_file(&repo.path().join("README.md"), "# dirty before run\n");

    let checkpoint = run_checkpoints::capture_run_checkpoint(11, &repo_path, "cargo check")
        .expect("checkpoint capture should succeed")
        .expect("repo checkpoint should exist");
    let loaded = run_checkpoints::load_latest_run_checkpoint_for_group_path(&repo_path)
        .expect("latest checkpoint should load")
        .expect("checkpoint should persist");
    assert_eq!(loaded.run_id, checkpoint.run_id);

    let initial_review = run_checkpoints::load_latest_run_review_for_group_path(&repo_path)
        .expect("review should load")
        .expect("review should exist");
    assert_eq!(initial_review.changed_file_count(), 0);

    write_file(&repo.path().join("README.md"), "# dirty after run\n");
    write_file(&repo.path().join("notes.txt"), "new file\n");

    let review = run_checkpoints::load_latest_run_review_for_group_path(&repo_path)
        .expect("review should load")
        .expect("review should exist");
    assert_eq!(review.changed_file_count(), 2);
    assert_eq!(review.new_file_count(), 1);

    let readme = review
        .files
        .iter()
        .find(|file| file.path == "README.md")
        .expect("readme entry should exist");
    assert!(!readme.is_new_since_start);
    assert!(readme.worktree_changed_since_start);

    let notes = review
        .files
        .iter()
        .find(|file| file.path == "notes.txt")
        .expect("notes entry should exist");
    assert!(notes.is_new_since_start);
    assert!(notes.is_untracked);
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
