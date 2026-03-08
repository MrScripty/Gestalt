mod command;
mod error;
mod model;
mod parse;

pub use error::GitError;
pub(crate) use model::RepoFileState;
pub use model::{
    BranchInfo, CheckoutTarget, CommitDetails, CommitDraft, CommitInfo, FileChange, RepoContext,
    RepoPathMarks, RepoSnapshot, TagInfo,
};

use command::{run_git, run_git_with_env};
use parse::{
    parse_branches, parse_graph_commits, parse_status_porcelain, parse_status_with_ignored,
    parse_tags,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_COMMIT_LIMIT: usize = 120;
const COMMIT_LOG_FORMAT: &str = "%x00%H%x1f%h%x1f%an%x1f%ad%x1f%s%x1f%D%x1f%P";
const TAG_FORMAT: &str = "%(refname:short)\t%(objectname)\t%(*objectname)";
const FIELD_DELIMITER: char = '\u{1f}';

pub fn load_repo_context(group_path: &str, commit_limit: usize) -> Result<RepoContext, GitError> {
    let root_output = run_git(group_path, &["rev-parse", "--show-toplevel"]);
    let repo_root = match root_output {
        Ok(output) => output.stdout.trim().to_string(),
        Err(GitError::NotRepo { .. }) => {
            return Ok(RepoContext::NotRepo {
                inspected_path: group_path.to_string(),
            });
        }
        Err(error) => return Err(error),
    };

    let current_branch = run_git(&repo_root, &["branch", "--show-current"])
        .map(|output| {
            let value = output.stdout.trim().to_string();
            if value.is_empty() { None } else { Some(value) }
        })
        .ok()
        .flatten();

    let head = run_git(&repo_root, &["rev-parse", "HEAD"])
        .map(|output| output.stdout.trim().to_string())
        .ok()
        .filter(|value| !value.is_empty());

    let branch_output = run_git(&repo_root, &["branch", "--all", "--no-color"])?;
    let branches = parse_branches(&branch_output.stdout);

    let commit_count = commit_limit.max(1).to_string();
    let commit_format_arg = format!("--pretty=format:{COMMIT_LOG_FORMAT}");
    let commit_output = run_git(
        &repo_root,
        &[
            "log",
            "--graph",
            "--decorate=short",
            "--date=iso-strict",
            commit_format_arg.as_str(),
            "-n",
            commit_count.as_str(),
        ],
    )?;
    let mut commits = parse_graph_commits(&commit_output.stdout)?;
    let has_upstream = run_git(
        &repo_root,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )
    .is_ok();
    if has_upstream {
        let unpushed = load_unpushed_commit_map(&repo_root, commit_limit)?;
        for commit in &mut commits {
            commit.is_unpushed = unpushed.contains(&commit.sha);
        }
    }

    let change_output = run_git(
        &repo_root,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
        ],
    )?;
    let changes = parse_status_porcelain(&change_output.stdout);
    let unstaged_count = changes
        .iter()
        .filter(|change| change.is_unstaged || change.is_untracked)
        .count();

    let tag_format_arg = format!("--format={TAG_FORMAT}");
    let tag_output = run_git(
        &repo_root,
        &[
            "for-each-ref",
            "refs/tags",
            "--sort=-creatordate",
            tag_format_arg.as_str(),
        ],
    )?;
    let tags = parse_tags(&tag_output.stdout);

    Ok(RepoContext::Available(RepoSnapshot {
        root: repo_root,
        head,
        current_branch,
        has_upstream,
        branches,
        commits,
        changes,
        unstaged_count,
        tags,
    }))
}

pub fn stage_file(group_path: &str, path: &str) -> Result<(), GitError> {
    let path = validate_non_empty(path, "File path")?;
    run_git(group_path, &["add", "--", &path])?;
    Ok(())
}

pub fn unstage_file(group_path: &str, path: &str) -> Result<(), GitError> {
    let path = validate_non_empty(path, "File path")?;
    run_git(group_path, &["restore", "--staged", "--", &path])?;
    Ok(())
}

pub fn create_commit(group_path: &str, draft: &CommitDraft) -> Result<String, GitError> {
    draft.validate()?;

    let title = draft.title.trim();
    let message = draft.message.trim();
    let mut args = vec!["commit", "-m", title];
    if !message.is_empty() {
        args.push("-m");
        args.push(message);
    }

    let output = run_git(group_path, &args)?;
    Ok(output.stdout.trim().to_string())
}

pub fn load_commit_details(group_path: &str, sha: &str) -> Result<CommitDetails, GitError> {
    let sha = validate_non_empty(sha, "Commit SHA")?;
    let details_output = run_git(
        group_path,
        &["show", "--quiet", "--format=%H%x1f%h%x1f%s%x1f%B", &sha],
    )?;
    let parsed = parse_commit_details(&details_output.stdout, &sha)?;

    let is_unpushed = run_git(
        group_path,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )
    .ok()
    .and_then(|_| load_unpushed_commit_map(group_path, DEFAULT_COMMIT_LIMIT).ok())
    .is_some_and(|map| map.contains(&sha));

    Ok(CommitDetails {
        sha: parsed.sha,
        short_sha: parsed.short_sha,
        title: parsed.title,
        message: parsed.message,
        is_unpushed,
    })
}

pub fn update_commit_message(
    group_path: &str,
    sha: &str,
    draft: &CommitDraft,
) -> Result<(), GitError> {
    draft.validate()?;
    let target_sha = validate_non_empty(sha, "Commit SHA")?;
    let head_sha = run_git(group_path, &["rev-parse", "HEAD"])?
        .stdout
        .trim()
        .to_string();
    if head_sha == target_sha {
        amend_head_commit_message(group_path, draft)?;
        return Ok(());
    }

    if !is_unpushed_commit(group_path, &target_sha)? {
        return Err(GitError::InvalidInput(
            "Only unpushed commits can be edited.".to_string(),
        ));
    }

    rewrite_unpushed_linear_history(group_path, &target_sha, draft)
}

pub fn create_tag(group_path: &str, name: &str, message: &str, sha: &str) -> Result<(), GitError> {
    let name = validate_non_empty(name, "Tag name")?;
    let message = validate_non_empty(message, "Tag message")?;
    let sha = validate_non_empty(sha, "Commit SHA")?;
    run_git(group_path, &["tag", "-a", &name, "-m", &message, &sha])?;
    Ok(())
}

pub fn checkout_target(group_path: &str, target: &CheckoutTarget) -> Result<(), GitError> {
    match target {
        CheckoutTarget::Branch(branch) => {
            let branch = validate_non_empty(branch, "Branch name")?;
            run_git(group_path, &["switch", &branch])?;
        }
        CheckoutTarget::Commit(sha) => {
            let sha = validate_non_empty(sha, "Commit SHA")?;
            run_git(group_path, &["switch", "--detach", &sha])?;
        }
    }

    Ok(())
}

pub fn create_worktree(group_path: &str, new_path: &str, target: &str) -> Result<(), GitError> {
    let new_path = validate_non_empty(new_path, "Worktree path")?;
    let target = validate_non_empty(target, "Worktree target")?;

    run_git(group_path, &["worktree", "add", &new_path, &target])?;
    Ok(())
}

pub fn repo_root(group_path: &str) -> Result<String, GitError> {
    Ok(run_git(group_path, &["rev-parse", "--show-toplevel"])?
        .stdout
        .trim()
        .to_string())
}

pub fn git_dir(group_path: &str) -> Result<String, GitError> {
    Ok(run_git(group_path, &["rev-parse", "--absolute-git-dir"])?
        .stdout
        .trim()
        .to_string())
}

pub fn repo_change_fingerprint_from_root(repo_root: &str) -> Result<String, GitError> {
    let status = run_git(
        repo_root,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
        ],
    )?
    .stdout;

    Ok(format!("{repo_root}\n{status}"))
}

pub fn repo_change_fingerprint(group_path: &str) -> Result<String, GitError> {
    let root = repo_root(group_path)?;
    repo_change_fingerprint_from_root(&root)
}

pub(crate) fn load_repo_file_state(repo_root: &str, path: &str) -> Result<RepoFileState, GitError> {
    let path = validate_non_empty(path, "File path")?;
    let worktree_blob_sha = load_worktree_blob_sha(repo_root, &path)?;
    let index_blob_sha = load_index_blob_sha(repo_root, &path)?;
    Ok(RepoFileState {
        worktree_blob_sha,
        index_blob_sha,
    })
}

pub fn load_repo_path_marks(group_path: &str) -> Result<RepoPathMarks, GitError> {
    let root_output = run_git(group_path, &["rev-parse", "--show-toplevel"]);
    let repo_root = match root_output {
        Ok(output) => output.stdout.trim().to_string(),
        Err(GitError::NotRepo { .. }) => return Ok(RepoPathMarks::default()),
        Err(error) => return Err(error),
    };

    let status_output = run_git(
        &repo_root,
        &[
            "-c",
            "core.quotepath=false",
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignored=matching",
        ],
    )?
    .stdout;

    let (modified_paths, ignored_paths) = parse_status_with_ignored(&status_output);

    Ok(RepoPathMarks {
        repo_root: Some(repo_root),
        modified_paths,
        ignored_paths,
    })
}

fn load_unpushed_commit_map(
    group_path: &str,
    commit_limit: usize,
) -> Result<HashSet<String>, GitError> {
    let max_count = format!("--max-count={}", commit_limit.max(1));
    let output = run_git(
        group_path,
        &["rev-list", max_count.as_str(), "@{upstream}..HEAD"],
    )?;
    Ok(output
        .stdout
        .lines()
        .map(str::trim)
        .filter(|sha| !sha.is_empty())
        .map(ToString::to_string)
        .collect())
}

fn load_worktree_blob_sha(repo_root: &str, path: &str) -> Result<Option<String>, GitError> {
    let absolute = Path::new(repo_root).join(path);
    if !absolute.exists() || absolute.is_dir() {
        return Ok(None);
    }

    let output = run_git(repo_root, &["hash-object", "--no-filters", "--", path])?;
    let sha = output.stdout.trim().to_string();
    if sha.is_empty() {
        return Ok(None);
    }
    Ok(Some(sha))
}

fn load_index_blob_sha(repo_root: &str, path: &str) -> Result<Option<String>, GitError> {
    let output = run_git(repo_root, &["ls-files", "-s", "--", path])?;
    let sha = output.stdout.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let _mode = fields.next()?;
        let sha = fields.next()?;
        Some(sha.to_string())
    });
    Ok(sha)
}

fn is_unpushed_commit(group_path: &str, sha: &str) -> Result<bool, GitError> {
    let unpushed = load_unpushed_commit_map(group_path, DEFAULT_COMMIT_LIMIT)?;
    Ok(unpushed.contains(sha))
}

fn amend_head_commit_message(group_path: &str, draft: &CommitDraft) -> Result<(), GitError> {
    let title = draft.title.trim();
    let message = draft.message.trim();
    let mut args = vec![
        "commit".to_string(),
        "--amend".to_string(),
        "-m".to_string(),
        title.to_string(),
    ];
    if !message.is_empty() {
        args.push("-m".to_string());
        args.push(message.to_string());
    }
    run_git(group_path, &args)?;
    Ok(())
}

fn rewrite_unpushed_linear_history(
    group_path: &str,
    target_sha: &str,
    draft: &CommitDraft,
) -> Result<(), GitError> {
    if run_git(
        group_path,
        &["merge-base", "--is-ancestor", target_sha, "HEAD"],
    )
    .is_err()
    {
        return Err(GitError::InvalidInput(
            "Selected commit is not an ancestor of HEAD.".to_string(),
        ));
    }

    if !run_git(group_path, &["status", "--porcelain"])?
        .stdout
        .trim()
        .is_empty()
    {
        return Err(GitError::InvalidInput(
            "Working tree must be clean to edit a non-HEAD commit.".to_string(),
        ));
    }

    let head_sha = run_git(group_path, &["rev-parse", "HEAD"])?
        .stdout
        .trim()
        .to_string();
    let descendants = run_git(
        group_path,
        &["rev-list", "--reverse", &format!("{target_sha}..HEAD")],
    )?;
    let mut rewrite_order = vec![target_sha.to_string()];
    rewrite_order.extend(
        descendants
            .stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToString::to_string),
    );

    let mut rewritten: HashMap<String, String> = HashMap::new();
    for (index, old_sha) in rewrite_order.iter().enumerate() {
        let mut metadata = load_rewrite_metadata(group_path, old_sha)?;
        if metadata.parents.len() > 1 {
            return Err(GitError::InvalidInput(
                "Editing merge commits is not supported yet.".to_string(),
            ));
        }

        if index == 0 {
            metadata.message = build_commit_message(draft);
        }

        let remapped_parents = metadata
            .parents
            .iter()
            .map(|parent| {
                rewritten
                    .get(parent)
                    .cloned()
                    .unwrap_or_else(|| parent.clone())
            })
            .collect::<Vec<_>>();
        let new_sha = create_commit_object(group_path, &metadata, &remapped_parents)?;
        rewritten.insert(old_sha.clone(), new_sha);
    }

    let Some(new_head) = rewritten.get(&head_sha).cloned() else {
        return Err(GitError::ParseError {
            command: "git commit-tree".to_string(),
            details: "Failed to compute rewritten HEAD.".to_string(),
        });
    };

    let current_branch = run_git(group_path, &["branch", "--show-current"])?
        .stdout
        .trim()
        .to_string();
    if current_branch.is_empty() {
        return Err(GitError::InvalidInput(
            "Detached HEAD is not supported for commit message rewrite.".to_string(),
        ));
    }
    let branch_ref = format!("refs/heads/{current_branch}");
    run_git(
        group_path,
        &["update-ref", &branch_ref, &new_head, &head_sha],
    )?;
    Ok(())
}

#[derive(Debug, Clone)]
struct RewriteMetadata {
    tree: String,
    parents: Vec<String>,
    message: String,
    author_name: String,
    author_email: String,
    author_date: String,
    committer_name: String,
    committer_email: String,
    committer_date: String,
}

fn load_rewrite_metadata(group_path: &str, sha: &str) -> Result<RewriteMetadata, GitError> {
    let output = run_git(
        group_path,
        &[
            "show",
            "--quiet",
            "--format=%H%x1f%T%x1f%P%x1f%an%x1f%ae%x1f%aI%x1f%cn%x1f%ce%x1f%cI%x1f%B",
            sha,
        ],
    )?;
    let fields = output
        .stdout
        .splitn(10, FIELD_DELIMITER)
        .collect::<Vec<_>>();
    if fields.len() < 10 {
        return Err(GitError::ParseError {
            command: "git show".to_string(),
            details: format!(
                "Expected 10 fields for commit metadata, found {}",
                fields.len()
            ),
        });
    }

    Ok(RewriteMetadata {
        tree: fields[1].trim().to_string(),
        parents: fields[2]
            .split_whitespace()
            .map(ToString::to_string)
            .collect(),
        author_name: fields[3].trim().to_string(),
        author_email: fields[4].trim().to_string(),
        author_date: fields[5].trim().to_string(),
        committer_name: fields[6].trim().to_string(),
        committer_email: fields[7].trim().to_string(),
        committer_date: fields[8].trim().to_string(),
        message: fields[9].to_string(),
    })
}

fn create_commit_object(
    group_path: &str,
    metadata: &RewriteMetadata,
    parents: &[String],
) -> Result<String, GitError> {
    let message_path = temp_path("git-edit-message", "txt");
    std::fs::write(&message_path, metadata.message.as_bytes()).map_err(|error| GitError::Io {
        details: error.to_string(),
    })?;

    let mut args = vec!["commit-tree".to_string(), metadata.tree.clone()];
    for parent in parents {
        args.push("-p".to_string());
        args.push(parent.clone());
    }
    args.push("-F".to_string());
    args.push(message_path.to_string_lossy().to_string());

    let envs = vec![
        ("GIT_AUTHOR_NAME".to_string(), metadata.author_name.clone()),
        (
            "GIT_AUTHOR_EMAIL".to_string(),
            metadata.author_email.clone(),
        ),
        ("GIT_AUTHOR_DATE".to_string(), metadata.author_date.clone()),
        (
            "GIT_COMMITTER_NAME".to_string(),
            metadata.committer_name.clone(),
        ),
        (
            "GIT_COMMITTER_EMAIL".to_string(),
            metadata.committer_email.clone(),
        ),
        (
            "GIT_COMMITTER_DATE".to_string(),
            metadata.committer_date.clone(),
        ),
    ];

    let output = run_git_with_env(group_path, &args, &envs);
    let _ = std::fs::remove_file(&message_path);
    output.map(|value| value.stdout.trim().to_string())
}

fn parse_commit_details(output: &str, command_sha: &str) -> Result<CommitDetails, GitError> {
    let fields = output.splitn(4, FIELD_DELIMITER).collect::<Vec<_>>();
    if fields.len() < 4 {
        return Err(GitError::ParseError {
            command: "git show".to_string(),
            details: format!(
                "Expected 4 fields for commit details of {command_sha}, found {}",
                fields.len()
            ),
        });
    }

    Ok(CommitDetails {
        sha: fields[0].trim().to_string(),
        short_sha: fields[1].trim().to_string(),
        title: fields[2].trim().to_string(),
        message: fields[3].trim().to_string(),
        is_unpushed: false,
    })
}

fn build_commit_message(draft: &CommitDraft) -> String {
    let title = draft.title.trim();
    let body = draft.message.trim();
    if body.is_empty() {
        title.to_string()
    } else {
        format!("{title}\n\n{body}")
    }
}

fn temp_path(prefix: &str, extension: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!("{prefix}-{nanos}.{extension}"))
}

fn validate_non_empty(value: &str, label: &str) -> Result<String, GitError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(GitError::InvalidInput(format!("{label} is required.")));
    }

    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::load_repo_path_marks;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn load_repo_path_marks_returns_default_for_non_repo() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let path = std::env::temp_dir().join(format!("gestalt-git-marks-nonrepo-{nonce}"));
        std::fs::create_dir_all(&path).expect("temp dir should be created");

        let marks = load_repo_path_marks(path.to_string_lossy().as_ref())
            .expect("non-repo path should not fail");
        assert!(marks.repo_root.is_none());
        assert!(marks.modified_paths.is_empty());
        assert!(marks.ignored_paths.is_empty());

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn load_repo_path_marks_collects_modified_and_ignored_paths() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let path = std::env::temp_dir().join(format!("gestalt-git-marks-repo-{nonce}"));
        std::fs::create_dir_all(&path).expect("temp dir should be created");

        run_git(path.as_path(), &["init"]);
        run_git(
            path.as_path(),
            &["config", "user.email", "marks-test@example.com"],
        );
        run_git(path.as_path(), &["config", "user.name", "Marks Test"]);
        std::fs::write(path.join(".gitignore"), "target/\n").expect("gitignore write should work");
        std::fs::write(path.join("README.md"), "baseline\n").expect("readme write should work");
        run_git(path.as_path(), &["add", ".gitignore", "README.md"]);
        run_git(path.as_path(), &["commit", "-m", "chore: init"]);

        std::fs::write(path.join("README.md"), "modified\n").expect("readme update should work");
        std::fs::create_dir_all(path.join("target")).expect("target dir should be created");
        std::fs::write(path.join("target/build.log"), "ignored\n")
            .expect("ignored file write should work");

        let marks =
            load_repo_path_marks(path.to_string_lossy().as_ref()).expect("repo marks should load");
        assert!(marks.repo_root.is_some());
        assert!(marks.modified_paths.contains("README.md"));
        assert!(marks.ignored_paths.contains("target"));

        let _ = std::fs::remove_dir_all(path);
    }

    fn run_git(cwd: &Path, args: &[&str]) {
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
}
