use crate::git::{self, CheckoutTarget, CommitDraft, GitError, RepoContext, RepoPathMarks};
use crate::orchestrator::events::{
    GitCommandExecuted, GitCommandKind, OrchestratorEvent, event_bus,
};

#[derive(Debug, Clone)]
pub struct FileOpResult {
    pub path: String,
    pub error: Option<GitError>,
}

pub fn load_repo_context(group_path: &str) -> Result<RepoContext, GitError> {
    git::load_repo_context(group_path, git::DEFAULT_COMMIT_LIMIT)
}

pub fn load_repo_path_marks(group_path: &str) -> Result<RepoPathMarks, GitError> {
    git::load_repo_path_marks(group_path)
}

pub fn stage_files(group_path: &str, paths: &[String]) -> Vec<FileOpResult> {
    let results = paths
        .iter()
        .map(|path| FileOpResult {
            path: path.clone(),
            error: git::stage_file(group_path, path).err(),
        })
        .collect::<Vec<_>>();
    emit_git_command_event(group_path, GitCommandKind::StageFiles, all_ok(&results));
    results
}

pub fn unstage_files(group_path: &str, paths: &[String]) -> Vec<FileOpResult> {
    let results = paths
        .iter()
        .map(|path| FileOpResult {
            path: path.clone(),
            error: git::unstage_file(group_path, path).err(),
        })
        .collect::<Vec<_>>();
    emit_git_command_event(group_path, GitCommandKind::UnstageFiles, all_ok(&results));
    results
}

pub fn create_commit(group_path: &str, draft: CommitDraft) -> Result<String, GitError> {
    let result = git::create_commit(group_path, &draft);
    emit_git_command_event(group_path, GitCommandKind::CreateCommit, result.is_ok());
    result
}

pub fn create_tag(group_path: &str, name: &str, message: &str, sha: &str) -> Result<(), GitError> {
    let result = git::create_tag(group_path, name, message, sha);
    emit_git_command_event(group_path, GitCommandKind::CreateTag, result.is_ok());
    result
}

pub fn checkout_target(group_path: &str, target: CheckoutTarget) -> Result<(), GitError> {
    let result = git::checkout_target(group_path, &target);
    emit_git_command_event(group_path, GitCommandKind::CheckoutTarget, result.is_ok());
    result
}

pub fn create_worktree(group_path: &str, new_path: &str, target: &str) -> Result<(), GitError> {
    let result = git::create_worktree(group_path, new_path, target);
    emit_git_command_event(group_path, GitCommandKind::CreateWorktree, result.is_ok());
    result
}

fn all_ok(results: &[FileOpResult]) -> bool {
    results.iter().all(|result| result.error.is_none())
}

fn emit_git_command_event(group_path: &str, command: GitCommandKind, success: bool) {
    event_bus().publish(OrchestratorEvent::GitCommandExecuted(GitCommandExecuted {
        group_path: group_path.to_string(),
        command,
        success,
    }));
}
