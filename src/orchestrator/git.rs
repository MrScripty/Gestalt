use crate::git::{self, CheckoutTarget, CommitDraft, GitError, RepoContext};

#[derive(Debug, Clone)]
pub struct FileOpResult {
    pub path: String,
    pub error: Option<GitError>,
}

pub fn load_repo_context(group_path: &str) -> Result<RepoContext, GitError> {
    git::load_repo_context(group_path, git::DEFAULT_COMMIT_LIMIT)
}

pub fn stage_files(group_path: &str, paths: &[String]) -> Vec<FileOpResult> {
    paths
        .iter()
        .map(|path| FileOpResult {
            path: path.clone(),
            error: git::stage_file(group_path, path).err(),
        })
        .collect()
}

pub fn unstage_files(group_path: &str, paths: &[String]) -> Vec<FileOpResult> {
    paths
        .iter()
        .map(|path| FileOpResult {
            path: path.clone(),
            error: git::unstage_file(group_path, path).err(),
        })
        .collect()
}

pub fn create_commit(group_path: &str, draft: CommitDraft) -> Result<String, GitError> {
    git::create_commit(group_path, &draft)
}

pub fn create_tag(group_path: &str, name: &str, message: &str, sha: &str) -> Result<(), GitError> {
    git::create_tag(group_path, name, message, sha)
}

pub fn checkout_target(group_path: &str, target: CheckoutTarget) -> Result<(), GitError> {
    git::checkout_target(group_path, &target)
}

pub fn create_worktree(group_path: &str, new_path: &str, target: &str) -> Result<(), GitError> {
    git::create_worktree(group_path, new_path, target)
}
