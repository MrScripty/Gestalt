use gestalt::git::{CheckoutTarget, CommitDraft, RepoContext};
use gestalt::orchestrator;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
fn stage_unstage_and_commit_flow() {
    let repo = TestRepo::new("git-panel-stage");
    let repo_path = repo.path().to_string_lossy().to_string();

    write_file(&repo.path().join("notes.txt"), "hello\n");

    let stage_results = orchestrator::git::stage_files(&repo_path, &["notes.txt".to_string()]);
    assert!(stage_results.iter().all(|result| result.error.is_none()));

    let context =
        orchestrator::git::load_repo_context(&repo_path).expect("repo context should load");
    let snapshot = match context {
        RepoContext::Available(snapshot) => snapshot,
        RepoContext::NotRepo { .. } => panic!("expected repo context"),
    };
    let staged = snapshot
        .changes
        .iter()
        .find(|change| change.path == "notes.txt")
        .expect("staged file should be present");
    assert!(staged.is_staged);

    let unstage_results = orchestrator::git::unstage_files(&repo_path, &["notes.txt".to_string()]);
    assert!(unstage_results.iter().all(|result| result.error.is_none()));

    let restaged = orchestrator::git::stage_files(&repo_path, &["notes.txt".to_string()]);
    assert!(restaged.iter().all(|result| result.error.is_none()));

    let commit_result = orchestrator::git::create_commit(
        &repo_path,
        CommitDraft {
            title: "feat: add notes".to_string(),
            message: "details".to_string(),
        },
    );
    assert!(commit_result.is_ok());

    let context =
        orchestrator::git::load_repo_context(&repo_path).expect("repo context should load");
    let snapshot = match context {
        RepoContext::Available(snapshot) => snapshot,
        RepoContext::NotRepo { .. } => panic!("expected repo context"),
    };
    assert_eq!(snapshot.commits[0].subject, "feat: add notes");
}

#[test]
fn tag_checkout_and_worktree_flow() {
    let repo = TestRepo::new("git-panel-tag");
    let repo_path = repo.path().to_string_lossy().to_string();

    run_git(repo.path(), &["switch", "-c", "feature/demo"]);
    write_file(&repo.path().join("feature.txt"), "feature\n");
    run_git(repo.path(), &["add", "feature.txt"]);
    run_git(repo.path(), &["commit", "-m", "feat: branch commit"]);

    orchestrator::git::checkout_target(&repo_path, CheckoutTarget::Branch("master".to_string()))
        .or_else(|_| {
            orchestrator::git::checkout_target(
                &repo_path,
                CheckoutTarget::Branch("main".to_string()),
            )
        })
        .expect("should checkout primary branch");

    let context =
        orchestrator::git::load_repo_context(&repo_path).expect("repo context should load");
    let snapshot = match context {
        RepoContext::Available(snapshot) => snapshot,
        RepoContext::NotRepo { .. } => panic!("expected repo context"),
    };

    let head_sha = snapshot.head.clone().expect("head sha should exist");
    orchestrator::git::create_tag(&repo_path, "v0.1.0", "release", &head_sha)
        .expect("tag creation should succeed");

    let after_tag =
        orchestrator::git::load_repo_context(&repo_path).expect("repo context should load");
    let tagged_snapshot = match after_tag {
        RepoContext::Available(snapshot) => snapshot,
        RepoContext::NotRepo { .. } => panic!("expected repo context"),
    };
    assert!(tagged_snapshot.tags.iter().any(|tag| tag.name == "v0.1.0"));

    orchestrator::git::checkout_target(&repo_path, CheckoutTarget::Commit(head_sha.clone()))
        .expect("checkout commit should succeed");

    let detached =
        orchestrator::git::load_repo_context(&repo_path).expect("repo context should load");
    let detached_snapshot = match detached {
        RepoContext::Available(snapshot) => snapshot,
        RepoContext::NotRepo { .. } => panic!("expected repo context"),
    };
    assert!(detached_snapshot.current_branch.is_none());

    let worktree_nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let worktree_path =
        std::env::temp_dir().join(format!("git-panel-worktree-target-{worktree_nonce}"));
    let worktree_string = worktree_path.to_string_lossy().to_string();
    if worktree_path.exists() {
        let _ = std::fs::remove_dir_all(&worktree_path);
    }

    orchestrator::git::create_worktree(&repo_path, &worktree_string, "master")
        .or_else(|_| orchestrator::git::create_worktree(&repo_path, &worktree_string, "main"))
        .expect("worktree creation should succeed");

    let worktree_top = run_git(&worktree_path, &["rev-parse", "--show-toplevel"]);
    assert!(worktree_top.contains("git-panel-worktree-target-"));

    let _ = std::fs::remove_dir_all(&worktree_path);
}

#[test]
fn commit_details_and_message_edit_flow() {
    let repo = TestRepo::new("git-panel-edit");
    let repo_path = repo.path().to_string_lossy().to_string();

    let remote_nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let remote_path = std::env::temp_dir().join(format!("git-panel-remote-{remote_nonce}.git"));
    if remote_path.exists() {
        let _ = std::fs::remove_dir_all(&remote_path);
    }
    run_git(
        Path::new("/"),
        &["init", "--bare", &remote_path.to_string_lossy()],
    );

    run_git(
        repo.path(),
        &["remote", "add", "origin", &remote_path.to_string_lossy()],
    );
    let branch = run_git(repo.path(), &["branch", "--show-current"]);
    run_git(repo.path(), &["push", "-u", "origin", &branch]);

    write_file(&repo.path().join("edit.txt"), "first\n");
    run_git(repo.path(), &["add", "edit.txt"]);
    run_git(repo.path(), &["commit", "-m", "feat: local edit"]);

    let context =
        orchestrator::git::load_repo_context(&repo_path).expect("repo context should load");
    let snapshot = match context {
        RepoContext::Available(snapshot) => snapshot,
        RepoContext::NotRepo { .. } => panic!("expected repo context"),
    };
    let head_sha = snapshot.commits[0].sha.clone();
    assert!(snapshot.commits[0].is_unpushed);
    assert!(snapshot.has_upstream);

    let details = orchestrator::git::load_commit_details(&repo_path, &head_sha)
        .expect("commit details should load");
    assert!(details.is_unpushed);
    assert_eq!(details.title, "feat: local edit");

    orchestrator::git::update_commit_message(
        &repo_path,
        &head_sha,
        CommitDraft {
            title: "feat: edited title".to_string(),
            message: "edited body".to_string(),
        },
    )
    .expect("head commit message update should succeed");

    let after_context =
        orchestrator::git::load_repo_context(&repo_path).expect("repo context should load");
    let after_snapshot = match after_context {
        RepoContext::Available(snapshot) => snapshot,
        RepoContext::NotRepo { .. } => panic!("expected repo context"),
    };
    assert_eq!(after_snapshot.commits[0].subject, "feat: edited title");
    let updated_head_sha = after_snapshot.commits[0].sha.clone();
    let updated_details = orchestrator::git::load_commit_details(&repo_path, &updated_head_sha)
        .expect("updated commit details should load");
    assert_eq!(updated_details.title, "feat: edited title");
    assert_eq!(updated_details.message, "feat: edited title\n\nedited body");

    let _ = std::fs::remove_dir_all(remote_path);
}

#[test]
fn non_head_unpushed_commit_message_edit_rewrites_linear_history() {
    let repo = TestRepo::new("git-panel-rewrite");
    let repo_path = repo.path().to_string_lossy().to_string();

    let remote_nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let remote_path =
        std::env::temp_dir().join(format!("git-panel-rewrite-remote-{remote_nonce}.git"));
    if remote_path.exists() {
        let _ = std::fs::remove_dir_all(&remote_path);
    }
    run_git(
        Path::new("/"),
        &["init", "--bare", &remote_path.to_string_lossy()],
    );

    run_git(
        repo.path(),
        &["remote", "add", "origin", &remote_path.to_string_lossy()],
    );
    let branch = run_git(repo.path(), &["branch", "--show-current"]);
    run_git(repo.path(), &["push", "-u", "origin", &branch]);

    write_file(&repo.path().join("a.txt"), "a\n");
    run_git(repo.path(), &["add", "a.txt"]);
    run_git(repo.path(), &["commit", "-m", "feat: commit a"]);

    write_file(&repo.path().join("b.txt"), "b\n");
    run_git(repo.path(), &["add", "b.txt"]);
    run_git(repo.path(), &["commit", "-m", "feat: commit b"]);

    let context =
        orchestrator::git::load_repo_context(&repo_path).expect("repo context should load");
    let snapshot = match context {
        RepoContext::Available(snapshot) => snapshot,
        RepoContext::NotRepo { .. } => panic!("expected repo context"),
    };
    let commit_a_sha = snapshot
        .commits
        .iter()
        .find(|commit| commit.subject == "feat: commit a")
        .map(|commit| commit.sha.clone())
        .expect("commit a should exist");

    orchestrator::git::update_commit_message(
        &repo_path,
        &commit_a_sha,
        CommitDraft {
            title: "feat: rewritten a".to_string(),
            message: String::new(),
        },
    )
    .expect("non-head unpushed rewrite should succeed");

    let after_context =
        orchestrator::git::load_repo_context(&repo_path).expect("repo context should load");
    let after_snapshot = match after_context {
        RepoContext::Available(snapshot) => snapshot,
        RepoContext::NotRepo { .. } => panic!("expected repo context"),
    };
    assert_eq!(after_snapshot.commits[0].subject, "feat: commit b");
    assert!(
        after_snapshot
            .commits
            .iter()
            .any(|commit| commit.subject == "feat: rewritten a")
    );

    let _ = std::fs::remove_dir_all(remote_path);
}

fn run_git(cwd: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("git command should run");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("git {:?} failed: {}", args, stderr);
    }

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn write_file(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("file write should succeed");
}
