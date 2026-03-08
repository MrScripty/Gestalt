use crate::state::GroupId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunCheckpointFile {
    pub path: String,
    pub code: String,
    pub is_staged: bool,
    pub is_unstaged: bool,
    pub is_untracked: bool,
    pub worktree_blob_sha: Option<String>,
    pub index_blob_sha: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRunCheckpointRecord {
    pub run_id: String,
    pub group_id: GroupId,
    pub group_path: String,
    pub command_line: String,
    pub repo_root: String,
    pub started_at_unix_ms: i64,
    pub head_sha: Option<String>,
    pub branch_name: Option<String>,
    pub baseline_files: Vec<RunCheckpointFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCheckpointRecord {
    pub run_id: String,
    pub group_id: GroupId,
    pub group_path: String,
    pub command_line: String,
    pub repo_root: String,
    pub started_at_unix_ms: i64,
    pub head_sha: Option<String>,
    pub branch_name: Option<String>,
    pub baseline_files: Vec<RunCheckpointFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReviewFile {
    pub path: String,
    pub code: String,
    pub is_staged: bool,
    pub is_unstaged: bool,
    pub is_untracked: bool,
    pub is_new_since_start: bool,
    pub status_changed_since_start: bool,
    pub worktree_changed_since_start: bool,
    pub index_changed_since_start: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReview {
    pub checkpoint: RunCheckpointRecord,
    pub current_head_sha: Option<String>,
    pub current_branch_name: Option<String>,
    pub head_changed_since_start: bool,
    pub branch_changed_since_start: bool,
    pub files: Vec<RunReviewFile>,
}

impl RunReview {
    pub fn changed_file_count(&self) -> usize {
        self.files.len()
    }

    pub fn new_file_count(&self) -> usize {
        self.files
            .iter()
            .filter(|file| file.is_new_since_start)
            .count()
    }
}
