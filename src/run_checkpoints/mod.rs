mod error;
mod model;
mod store;

pub use error::RunCheckpointError;
pub use model::{
    NewRunCheckpointRecord, RunCheckpointFile, RunCheckpointRecord, RunReview, RunReviewFile,
};
pub use store::RunCheckpointStore;

use crate::git::{self, RepoContext};
use crate::state::GroupId;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub fn capture_run_checkpoint(
    group_id: GroupId,
    group_path: &str,
    command_line: &str,
) -> Result<Option<RunCheckpointRecord>, RunCheckpointError> {
    let trimmed_command = command_line.trim();
    if trimmed_command.is_empty() {
        return Err(RunCheckpointError::InvalidData(
            "command line is required".to_string(),
        ));
    }

    let context = git::load_repo_context(group_path, 1)?;
    let snapshot = match context {
        RepoContext::Available(snapshot) => snapshot,
        RepoContext::NotRepo { .. } => return Ok(None),
    };

    let mut baseline_files = Vec::with_capacity(snapshot.changes.len());
    for change in &snapshot.changes {
        let file_state = git::load_repo_file_state(&snapshot.root, &change.path)?;
        baseline_files.push(RunCheckpointFile {
            path: change.path.clone(),
            code: change.code.clone(),
            is_staged: change.is_staged,
            is_unstaged: change.is_unstaged,
            is_untracked: change.is_untracked,
            worktree_blob_sha: file_state.worktree_blob_sha,
            index_blob_sha: file_state.index_blob_sha,
        });
    }

    RunCheckpointStore::default()
        .record_checkpoint(NewRunCheckpointRecord {
            run_id: Uuid::new_v4().to_string(),
            group_id,
            group_path: group_path.to_string(),
            command_line: trimmed_command.to_string(),
            repo_root: snapshot.root,
            started_at_unix_ms: current_unix_ms(),
            head_sha: snapshot.head,
            branch_name: snapshot.current_branch,
            baseline_files,
        })
        .map(Some)
}

pub fn load_latest_run_checkpoint_for_group_path(
    group_path: &str,
) -> Result<Option<RunCheckpointRecord>, RunCheckpointError> {
    RunCheckpointStore::default().load_latest_for_group_path(group_path)
}

pub fn load_latest_run_review_for_group_path(
    group_path: &str,
) -> Result<Option<RunReview>, RunCheckpointError> {
    let Some(checkpoint) = load_latest_run_checkpoint_for_group_path(group_path)? else {
        return Ok(None);
    };
    let context = git::load_repo_context(group_path, 1)?;
    let snapshot = match context {
        RepoContext::Available(snapshot) => snapshot,
        RepoContext::NotRepo { inspected_path } => {
            return Err(RunCheckpointError::InvalidData(format!(
                "latest checkpoint points to a repo, but '{inspected_path}' is no longer a repository"
            )));
        }
    };
    let repo_root = snapshot.root.clone();
    let head_sha = snapshot.head;
    let branch_name = snapshot.current_branch;
    let changes = snapshot.changes;
    build_run_review(checkpoint, &repo_root, head_sha, branch_name, changes).map(Some)
}

fn build_run_review(
    checkpoint: RunCheckpointRecord,
    current_repo_root: &str,
    current_head_sha: Option<String>,
    current_branch_name: Option<String>,
    current_changes: Vec<crate::git::FileChange>,
) -> Result<RunReview, RunCheckpointError> {
    if checkpoint.repo_root != current_repo_root {
        return Err(RunCheckpointError::InvalidData(format!(
            "checkpoint repo root '{}' does not match current repo root '{}'",
            checkpoint.repo_root, current_repo_root
        )));
    }

    let baseline_by_path = checkpoint
        .baseline_files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect::<HashMap<_, _>>();
    let mut files = Vec::new();

    for change in current_changes {
        let current_file_state = git::load_repo_file_state(current_repo_root, &change.path)?;
        let baseline = baseline_by_path.get(change.path.as_str()).copied();
        let is_new_since_start = baseline.is_none();
        let status_changed_since_start = baseline.map_or(true, |baseline| {
            baseline.code != change.code
                || baseline.is_staged != change.is_staged
                || baseline.is_unstaged != change.is_unstaged
                || baseline.is_untracked != change.is_untracked
        });
        let worktree_changed_since_start = baseline.map_or(true, |baseline| {
            baseline.worktree_blob_sha != current_file_state.worktree_blob_sha
        });
        let index_changed_since_start = baseline.map_or(true, |baseline| {
            baseline.index_blob_sha != current_file_state.index_blob_sha
        });

        if !(is_new_since_start
            || status_changed_since_start
            || worktree_changed_since_start
            || index_changed_since_start)
        {
            continue;
        }

        files.push(RunReviewFile {
            path: change.path,
            code: change.code,
            is_staged: change.is_staged,
            is_unstaged: change.is_unstaged,
            is_untracked: change.is_untracked,
            is_new_since_start,
            status_changed_since_start,
            worktree_changed_since_start,
            index_changed_since_start,
        });
    }

    Ok(RunReview {
        head_changed_since_start: checkpoint.head_sha != current_head_sha,
        branch_changed_since_start: checkpoint.branch_name != current_branch_name,
        checkpoint,
        current_head_sha,
        current_branch_name,
        files,
    })
}

fn current_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
