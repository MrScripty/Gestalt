use gestalt::git::RepoContext;
use gestalt::orchestrator;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn load_repo_context_switches_between_repo_and_non_repo_paths() {
    let repo = TestRepo::new("git-panel-context");
    let non_repo_path = temp_dir("git-panel-nonrepo");

    let repo_context = orchestrator::git::load_repo_context(&repo.path.to_string_lossy())
        .expect("repo context should load");
    assert!(matches!(repo_context, RepoContext::Available(_)));

    let non_repo_context = orchestrator::git::load_repo_context(&non_repo_path.to_string_lossy())
        .expect("non-repo context should return graceful state");
    match non_repo_context {
        RepoContext::NotRepo { inspected_path } => {
            assert_eq!(inspected_path, non_repo_path.to_string_lossy());
        }
        RepoContext::Available(_) => panic!("expected non-repo context"),
    }

    let _ = std::fs::remove_dir_all(non_repo_path);
}

struct TestRepo {
    path: PathBuf,
}

impl TestRepo {
    fn new(prefix: &str) -> Self {
        let path = temp_dir(prefix);
        run_git(&path, &["init"]);
        run_git(&path, &["config", "user.email", "gestalt-test@example.com"]);
        run_git(&path, &["config", "user.name", "Gestalt Test"]);
        std::fs::write(path.join("README.md"), "test\n").expect("write should succeed");
        run_git(&path, &["add", "README.md"]);
        run_git(&path, &["commit", "-m", "chore: init"]);
        Self { path }
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn temp_dir(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let path = std::env::temp_dir().join(format!("{prefix}-{nonce}"));
    std::fs::create_dir_all(&path).expect("temp dir should be created");
    path
}

fn run_git(cwd: &PathBuf, args: &[&str]) {
    let output = std::process::Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("git command should run");
    if !output.status.success() {
        panic!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
