use std::collections::HashSet;

#[derive(Debug, Clone)]
pub enum RepoContext {
    Available(RepoSnapshot),
    NotRepo { inspected_path: String },
}

#[derive(Debug, Clone)]
pub struct RepoSnapshot {
    pub root: String,
    pub head: Option<String>,
    pub current_branch: Option<String>,
    pub has_upstream: bool,
    pub branches: Vec<BranchInfo>,
    pub commits: Vec<CommitInfo>,
    pub changes: Vec<FileChange>,
    pub unstaged_count: usize,
    pub tags: Vec<TagInfo>,
}

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub sha: String,
    pub short_sha: String,
    pub author: String,
    pub authored_at: String,
    pub subject: String,
    pub body_preview: String,
    pub decorations: Vec<String>,
    pub graph_prefix: String,
    pub parents: Vec<String>,
    pub is_unpushed: bool,
}

#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: String,
    pub code: String,
    pub is_staged: bool,
    pub is_unstaged: bool,
    pub is_untracked: bool,
}

#[derive(Debug, Clone)]
pub struct TagInfo {
    pub name: String,
    pub target_sha: String,
    pub annotated: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RepoPathMarks {
    pub repo_root: Option<String>,
    pub modified_paths: HashSet<String>,
    pub ignored_paths: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct CommitDraft {
    pub title: String,
    pub message: String,
}

impl CommitDraft {
    pub fn validate(&self) -> Result<(), crate::git::GitError> {
        if self.title.trim().is_empty() {
            return Err(crate::git::GitError::InvalidInput(
                "Commit title is required.".to_string(),
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct CommitDetails {
    pub sha: String,
    pub short_sha: String,
    pub title: String,
    pub message: String,
    pub is_unpushed: bool,
}

#[derive(Debug, Clone)]
pub enum CheckoutTarget {
    Branch(String),
    Commit(String),
}
