use crate::git::{
    self, CheckoutTarget, CommitDetails, CommitDraft, GitError, RepoContext, RepoPathMarks,
};
use crate::orchestration_log::{
    CommandPayload, EventPayload, NewCommandRecord, NewEventRecord, NewReceiptRecord,
    OrchestrationLogStore, ReceiptPayload, ReceiptStatus,
};
use crate::orchestrator::events::{
    GitCommandExecuted, GitCommandKind, OrchestratorEvent, event_bus,
};
use crate::path_validation;
use uuid::Uuid;

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
    let command_id = Uuid::new_v4().to_string();
    let now_ms = current_unix_ms();
    let store = OrchestrationLogStore::default();
    if let Err(error) = store.record_command(NewCommandRecord {
        command_id: command_id.clone(),
        timeline_id: command_id.clone(),
        requested_at_unix_ms: now_ms,
        recorded_at_unix_ms: now_ms,
        payload: CommandPayload::GitStageFiles {
            group_path: group_path.to_string(),
            paths: paths.to_vec(),
        },
    }) {
        return blocked_file_results(
            paths,
            format!("failed recording orchestration command: {error}"),
        );
    }

    let results = paths
        .iter()
        .map(|path| FileOpResult {
            path: path.clone(),
            error: git::stage_file(group_path, path).err(),
        })
        .collect::<Vec<_>>();
    record_file_results(&store, &command_id, &results, "staged files");
    emit_git_command_event(group_path, GitCommandKind::StageFiles, all_ok(&results));
    results
}

pub fn unstage_files(group_path: &str, paths: &[String]) -> Vec<FileOpResult> {
    let command_id = Uuid::new_v4().to_string();
    let now_ms = current_unix_ms();
    let store = OrchestrationLogStore::default();
    if let Err(error) = store.record_command(NewCommandRecord {
        command_id: command_id.clone(),
        timeline_id: command_id.clone(),
        requested_at_unix_ms: now_ms,
        recorded_at_unix_ms: now_ms,
        payload: CommandPayload::GitUnstageFiles {
            group_path: group_path.to_string(),
            paths: paths.to_vec(),
        },
    }) {
        return blocked_file_results(
            paths,
            format!("failed recording orchestration command: {error}"),
        );
    }

    let results = paths
        .iter()
        .map(|path| FileOpResult {
            path: path.clone(),
            error: git::unstage_file(group_path, path).err(),
        })
        .collect::<Vec<_>>();
    record_file_results(&store, &command_id, &results, "unstaged files");
    emit_git_command_event(group_path, GitCommandKind::UnstageFiles, all_ok(&results));
    results
}

pub fn create_commit(group_path: &str, draft: CommitDraft) -> Result<String, GitError> {
    let command_id = Uuid::new_v4().to_string();
    let now_ms = current_unix_ms();
    let store = OrchestrationLogStore::default();
    store
        .record_command(NewCommandRecord {
            command_id: command_id.clone(),
            timeline_id: command_id.clone(),
            requested_at_unix_ms: now_ms,
            recorded_at_unix_ms: now_ms,
            payload: CommandPayload::GitCreateCommit {
                group_path: group_path.to_string(),
                title: draft.title.clone(),
                has_message_body: !draft.message.trim().is_empty(),
            },
        })
        .map_err(log_error_to_git_error)?;
    let result = git::create_commit(group_path, &draft);
    record_single_git_result(
        &store,
        &command_id,
        result
            .as_ref()
            .map(|output| format!("created commit {output}"))
            .map_err(|error| error.to_string()),
    );
    emit_git_command_event(group_path, GitCommandKind::CreateCommit, result.is_ok());
    result
}

pub fn load_commit_details(group_path: &str, sha: &str) -> Result<CommitDetails, GitError> {
    git::load_commit_details(group_path, sha)
}

pub fn update_commit_message(
    group_path: &str,
    sha: &str,
    draft: CommitDraft,
) -> Result<(), GitError> {
    let command_id = Uuid::new_v4().to_string();
    let now_ms = current_unix_ms();
    let store = OrchestrationLogStore::default();
    store
        .record_command(NewCommandRecord {
            command_id: command_id.clone(),
            timeline_id: command_id.clone(),
            requested_at_unix_ms: now_ms,
            recorded_at_unix_ms: now_ms,
            payload: CommandPayload::GitUpdateCommitMessage {
                group_path: group_path.to_string(),
                target_sha: sha.to_string(),
                title: draft.title.clone(),
                has_message_body: !draft.message.trim().is_empty(),
            },
        })
        .map_err(log_error_to_git_error)?;
    let result = git::update_commit_message(group_path, sha, &draft);
    record_single_git_result(
        &store,
        &command_id,
        result
            .as_ref()
            .map(|_| format!("updated commit message for {sha}"))
            .map_err(|error| error.to_string()),
    );
    emit_git_command_event(
        group_path,
        GitCommandKind::UpdateCommitMessage,
        result.is_ok(),
    );
    result
}

pub fn create_tag(group_path: &str, name: &str, message: &str, sha: &str) -> Result<(), GitError> {
    let command_id = Uuid::new_v4().to_string();
    let now_ms = current_unix_ms();
    let store = OrchestrationLogStore::default();
    store
        .record_command(NewCommandRecord {
            command_id: command_id.clone(),
            timeline_id: command_id.clone(),
            requested_at_unix_ms: now_ms,
            recorded_at_unix_ms: now_ms,
            payload: CommandPayload::GitCreateTag {
                group_path: group_path.to_string(),
                tag_name: name.to_string(),
                target_sha: sha.to_string(),
            },
        })
        .map_err(log_error_to_git_error)?;
    let result = git::create_tag(group_path, name, message, sha);
    record_single_git_result(
        &store,
        &command_id,
        result
            .as_ref()
            .map(|_| format!("created tag {name} for {sha}"))
            .map_err(|error| error.to_string()),
    );
    emit_git_command_event(group_path, GitCommandKind::CreateTag, result.is_ok());
    result
}

pub fn delete_tag(group_path: &str, name: &str) -> Result<(), GitError> {
    let command_id = Uuid::new_v4().to_string();
    let now_ms = current_unix_ms();
    let store = OrchestrationLogStore::default();
    store
        .record_command(NewCommandRecord {
            command_id: command_id.clone(),
            timeline_id: command_id.clone(),
            requested_at_unix_ms: now_ms,
            recorded_at_unix_ms: now_ms,
            payload: CommandPayload::GitDeleteTag {
                group_path: group_path.to_string(),
                tag_name: name.to_string(),
            },
        })
        .map_err(log_error_to_git_error)?;
    let result = git::delete_tag(group_path, name);
    record_single_git_result(
        &store,
        &command_id,
        result
            .as_ref()
            .map(|_| format!("deleted tag {name}"))
            .map_err(|error| error.to_string()),
    );
    emit_git_command_event(group_path, GitCommandKind::DeleteTag, result.is_ok());
    result
}

pub fn update_tag(
    group_path: &str,
    old_name: &str,
    new_name: &str,
    message: &str,
    sha: &str,
) -> Result<(), GitError> {
    let command_id = Uuid::new_v4().to_string();
    let now_ms = current_unix_ms();
    let store = OrchestrationLogStore::default();
    store
        .record_command(NewCommandRecord {
            command_id: command_id.clone(),
            timeline_id: command_id.clone(),
            requested_at_unix_ms: now_ms,
            recorded_at_unix_ms: now_ms,
            payload: CommandPayload::GitUpdateTag {
                group_path: group_path.to_string(),
                old_tag_name: old_name.to_string(),
                new_tag_name: new_name.to_string(),
                target_sha: sha.to_string(),
            },
        })
        .map_err(log_error_to_git_error)?;
    let result = git::update_tag(group_path, old_name, new_name, message, sha);
    record_single_git_result(
        &store,
        &command_id,
        result
            .as_ref()
            .map(|_| format!("updated tag {old_name} -> {new_name} for {sha}"))
            .map_err(|error| error.to_string()),
    );
    emit_git_command_event(group_path, GitCommandKind::UpdateTag, result.is_ok());
    result
}

pub fn checkout_target(group_path: &str, target: CheckoutTarget) -> Result<(), GitError> {
    let target_description = match &target {
        CheckoutTarget::Branch(branch) => ("branch".to_string(), branch.clone()),
        CheckoutTarget::Commit(sha) => ("commit".to_string(), sha.clone()),
    };
    let command_id = Uuid::new_v4().to_string();
    let now_ms = current_unix_ms();
    let store = OrchestrationLogStore::default();
    store
        .record_command(NewCommandRecord {
            command_id: command_id.clone(),
            timeline_id: command_id.clone(),
            requested_at_unix_ms: now_ms,
            recorded_at_unix_ms: now_ms,
            payload: CommandPayload::GitCheckoutTarget {
                group_path: group_path.to_string(),
                target: target_description.1.clone(),
                target_kind: target_description.0.clone(),
            },
        })
        .map_err(log_error_to_git_error)?;
    let result = git::checkout_target(group_path, &target);
    record_single_git_result(
        &store,
        &command_id,
        result
            .as_ref()
            .map(|_| {
                format!(
                    "checked out {} {}",
                    target_description.0, target_description.1
                )
            })
            .map_err(|error| error.to_string()),
    );
    emit_git_command_event(group_path, GitCommandKind::CheckoutTarget, result.is_ok());
    result
}

pub fn create_worktree(group_path: &str, new_path: &str, target: &str) -> Result<(), GitError> {
    let validated_path =
        path_validation::validate_new_worktree_path(new_path).map_err(GitError::InvalidInput)?;
    let command_id = Uuid::new_v4().to_string();
    let now_ms = current_unix_ms();
    let store = OrchestrationLogStore::default();
    store
        .record_command(NewCommandRecord {
            command_id: command_id.clone(),
            timeline_id: command_id.clone(),
            requested_at_unix_ms: now_ms,
            recorded_at_unix_ms: now_ms,
            payload: CommandPayload::GitCreateWorktree {
                group_path: group_path.to_string(),
                new_path: validated_path.clone(),
                target: target.to_string(),
            },
        })
        .map_err(log_error_to_git_error)?;
    let result = git::create_worktree(group_path, &validated_path, target);
    record_single_git_result(
        &store,
        &command_id,
        result
            .as_ref()
            .map(|_| format!("created worktree {validated_path} from {target}"))
            .map_err(|error| error.to_string()),
    );
    emit_git_command_event(group_path, GitCommandKind::CreateWorktree, result.is_ok());
    result
}

fn all_ok(results: &[FileOpResult]) -> bool {
    results.iter().all(|result| result.error.is_none())
}

fn blocked_file_results(paths: &[String], message: String) -> Vec<FileOpResult> {
    paths
        .iter()
        .map(|path| FileOpResult {
            path: path.clone(),
            error: Some(GitError::Io {
                details: message.clone(),
            }),
        })
        .collect()
}

fn record_file_results(
    store: &OrchestrationLogStore,
    command_id: &str,
    results: &[FileOpResult],
    summary: &str,
) {
    let mut ok_count = 0usize;
    let mut fail_count = 0usize;

    for result in results {
        let payload = match result.error.as_ref() {
            Some(error) => {
                fail_count = fail_count.saturating_add(1);
                EventPayload::GitPathFailed {
                    path: result.path.clone(),
                    error: error.to_string(),
                }
            }
            None => {
                ok_count = ok_count.saturating_add(1);
                EventPayload::GitPathSucceeded {
                    path: result.path.clone(),
                }
            }
        };
        let event_time = current_unix_ms();
        let _ = store.append_event(
            command_id,
            NewEventRecord {
                occurred_at_unix_ms: event_time,
                recorded_at_unix_ms: event_time,
                payload,
            },
        );
    }

    finalize_git_receipt(store, command_id, ok_count, fail_count, summary.to_string());
}

fn record_single_git_result(
    store: &OrchestrationLogStore,
    command_id: &str,
    result: Result<String, String>,
) {
    let (ok_count, fail_count, payload) = match result {
        Ok(summary) => (
            1usize,
            0usize,
            EventPayload::GitOperationSucceeded {
                summary: summary.clone(),
            },
        ),
        Err(error) => (
            0usize,
            1usize,
            EventPayload::GitOperationFailed {
                error: error.clone(),
            },
        ),
    };

    let event_time = current_unix_ms();
    let summary = match &payload {
        EventPayload::GitOperationSucceeded { summary } => summary.clone(),
        EventPayload::GitOperationFailed { error } => error.clone(),
        _ => String::new(),
    };
    let _ = store.append_event(
        command_id,
        NewEventRecord {
            occurred_at_unix_ms: event_time,
            recorded_at_unix_ms: event_time,
            payload,
        },
    );
    finalize_git_receipt(store, command_id, ok_count, fail_count, summary);
}

fn finalize_git_receipt(
    store: &OrchestrationLogStore,
    command_id: &str,
    ok_count: usize,
    fail_count: usize,
    summary: String,
) {
    let status = if fail_count == 0 {
        ReceiptStatus::Succeeded
    } else if ok_count == 0 {
        ReceiptStatus::Failed
    } else {
        ReceiptStatus::PartiallySucceeded
    };
    let completed_at_unix_ms = current_unix_ms();
    let _ = store.finalize_receipt(
        command_id,
        NewReceiptRecord {
            completed_at_unix_ms,
            recorded_at_unix_ms: completed_at_unix_ms,
            status,
            payload: ReceiptPayload::Git {
                ok_count,
                fail_count,
                summary,
            },
        },
    );
}

fn log_error_to_git_error(error: crate::orchestration_log::OrchestrationLogError) -> GitError {
    GitError::Io {
        details: format!("failed recording orchestration command: {error}"),
    }
}

fn current_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn emit_git_command_event(group_path: &str, command: GitCommandKind, success: bool) {
    event_bus().publish(OrchestratorEvent::GitCommandExecuted(GitCommandExecuted {
        group_path: group_path.to_string(),
        command,
        success,
    }));
}
