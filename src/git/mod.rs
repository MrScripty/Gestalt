mod command;
mod error;
mod model;
mod parse;

pub use error::GitError;
pub use model::{
    BranchInfo, CheckoutTarget, CommitDraft, CommitInfo, FileChange, RepoContext, RepoPathMarks,
    RepoSnapshot, TagInfo,
};

use command::run_git;
use parse::{
    parse_branches, parse_graph_commits, parse_status_porcelain, parse_status_with_ignored,
    parse_tags,
};

pub const DEFAULT_COMMIT_LIMIT: usize = 120;
const COMMIT_LOG_FORMAT: &str = "%x00%H%x1f%h%x1f%an%x1f%ad%x1f%s%x1f%D";
const TAG_FORMAT: &str = "%(refname:short)\t%(objectname)\t%(*objectname)";

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
    let commits = parse_graph_commits(&commit_output.stdout)?;

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
        branches,
        commits,
        changes,
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

fn validate_non_empty(value: &str, label: &str) -> Result<String, GitError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(GitError::InvalidInput(format!("{label} is required.")));
    }

    Ok(trimmed.to_string())
}
