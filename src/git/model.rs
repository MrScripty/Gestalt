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
    pub branches: Vec<BranchInfo>,
    pub commits: Vec<CommitInfo>,
    pub changes: Vec<FileChange>,
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
    pub decorations: Vec<String>,
    pub graph_prefix: String,
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
pub enum CheckoutTarget {
    Branch(String),
    Commit(String),
}
