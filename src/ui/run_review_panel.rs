use crate::git::{FileChange, RepoContext, RepoSnapshot};
use crate::run_checkpoints::{RunReview, RunReviewFile};
use dioxus::prelude::*;

#[component]
pub(crate) fn RunReviewPanel(
    active_group_path: String,
    repo_context: Signal<Option<RepoContext>>,
    git_refresh_nonce: Signal<u64>,
) -> Element {
    let repo_context_value = repo_context.read().clone();
    let refresh_nonce = *git_refresh_nonce.read();
    let mut review = use_signal(|| None::<RunReview>);
    let review_value = review.read().clone();
    let mut review_feedback = use_signal(String::new);
    let review_feedback_value = review_feedback.read().clone();
    let review_loading = use_signal(|| true);
    let review_loading_value = *review_loading.read();
    let mut review_loaded_key = use_signal(String::new);
    let refresh_key = format!(
        "{}::{refresh_nonce}::{}",
        active_group_path,
        repo_context_revision_key(repo_context_value.as_ref())
    );

    if review_loaded_key.read().as_str() != refresh_key.as_str() {
        review_loaded_key.set(refresh_key);
        refresh_run_review(
            review,
            review_loading,
            review_feedback,
            active_group_path.clone(),
        );
    }

    rsx! {
        article { class: "run-review-card",
            div { class: "run-review-head",
                h3 { "Run Review" }
                p { "Latest Local Agent checkpoint for {active_group_path}" }
            }

            if review_loading_value {
                p { class: "meta", "Loading run review..." }
            } else if !review_feedback_value.is_empty() {
                p { class: "run-review-feedback", "{review_feedback_value}" }
            } else if let Some(review) = review_value {
                {
                    let changed_files = review.changed_file_count();
                    let new_files = review.new_file_count();
                    let started_at = format_unix_ms(review.checkpoint.started_at_unix_ms);
                    let baseline_head = review
                        .checkpoint
                        .head_sha
                        .clone()
                        .unwrap_or_else(|| "(none)".to_string());
                    let current_head = review
                        .current_head_sha
                        .clone()
                        .unwrap_or_else(|| "(none)".to_string());
                    let baseline_branch = review
                        .checkpoint
                        .branch_name
                        .clone()
                        .unwrap_or_else(|| "(detached)".to_string());
                    let current_branch = review
                        .current_branch_name
                        .clone()
                        .unwrap_or_else(|| "(detached)".to_string());

                    rsx! {
                        div { class: "run-review-summary",
                            span { class: "run-review-badge", "{changed_files} file(s)" }
                            if new_files > 0 {
                                span { class: "run-review-badge accent", "{new_files} new" }
                            }
                            if review.head_changed_since_start {
                                span { class: "run-review-badge accent", "HEAD moved" }
                            }
                            if review.branch_changed_since_start {
                                span { class: "run-review-badge accent", "Branch changed" }
                            }
                        }

                        p { class: "run-review-meta", "Started {started_at} | command: {review.checkpoint.command_line}" }
                        p { class: "run-review-meta", "Baseline branch {baseline_branch} | current branch {current_branch}" }
                        p { class: "run-review-meta", "Baseline HEAD {baseline_head} | current HEAD {current_head}" }

                        if review.files.is_empty() {
                            p { class: "run-review-empty",
                                if review.head_changed_since_start || review.branch_changed_since_start {
                                    "No dirty working-tree changes remain, but repo history moved since this run started."
                                } else {
                                    "No repo changes detected since this run started."
                                }
                            }
                        } else {
                            div { class: "run-review-list",
                                for file in review.files.clone() {
                                    {
                                        let detail = review_file_detail(&file);
                                        rsx! {
                                            div { class: "run-review-item",
                                                div { class: "run-review-item-head",
                                                    span { class: "run-review-code", "{file.code}" }
                                                    span { class: "run-review-path", "{file.path}" }
                                                }
                                                p { class: "run-review-meta", "{detail}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                p { class: "run-review-empty", "No run checkpoint recorded for this group yet." }
            }
        }
    }
}

fn refresh_run_review(
    mut review: Signal<Option<RunReview>>,
    mut review_loading: Signal<bool>,
    mut review_feedback: Signal<String>,
    group_path: String,
) {
    review_loading.set(true);
    review_feedback.set(String::new());
    spawn(async move {
        let load_result = tokio::task::spawn_blocking(move || {
            crate::run_checkpoints::load_latest_run_review_for_group_path(&group_path)
        })
        .await;

        match load_result {
            Ok(Ok(review_result)) => {
                review.set(review_result);
                review_feedback.set(String::new());
            }
            Ok(Err(error)) => {
                review.set(None);
                review_feedback.set(error.to_string());
            }
            Err(error) => {
                review.set(None);
                review_feedback.set(format!("Failed loading run review: {error}"));
            }
        }

        review_loading.set(false);
    });
}

fn repo_context_revision_key(context: Option<&RepoContext>) -> String {
    match context {
        None => "none".to_string(),
        Some(RepoContext::NotRepo { inspected_path }) => format!("not-repo:{inspected_path}"),
        Some(RepoContext::Available(snapshot)) => snapshot_revision_key(snapshot),
    }
}

fn snapshot_revision_key(snapshot: &RepoSnapshot) -> String {
    let changes = snapshot
        .changes
        .iter()
        .map(file_change_key)
        .collect::<Vec<_>>()
        .join("|");
    format!(
        "{}::{}::{}::{}",
        snapshot.root,
        snapshot.head.clone().unwrap_or_default(),
        snapshot.current_branch.clone().unwrap_or_default(),
        changes
    )
}

fn file_change_key(change: &FileChange) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        change.path,
        change.code,
        u8::from(change.is_staged),
        u8::from(change.is_unstaged),
        u8::from(change.is_untracked),
    )
}

fn review_file_detail(file: &RunReviewFile) -> String {
    let mut parts = Vec::new();
    if file.is_new_since_start {
        parts.push("new since run start");
    }
    if file.status_changed_since_start && !file.is_new_since_start {
        parts.push("status changed");
    }
    if file.worktree_changed_since_start {
        parts.push("worktree changed");
    }
    if file.index_changed_since_start {
        parts.push("index changed");
    }
    if file.is_staged {
        parts.push("staged");
    }
    if file.is_unstaged {
        parts.push("unstaged");
    }
    if file.is_untracked {
        parts.push("untracked");
    }

    if parts.is_empty() {
        return "changed since run start".to_string();
    }

    parts.join(" | ")
}

fn format_unix_ms(unix_ms: i64) -> String {
    if unix_ms <= 0 {
        return "unknown".to_string();
    }

    let seconds = unix_ms / 1_000;
    let now_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let delta = now_seconds.saturating_sub(seconds);

    if delta < 60 {
        return format!("{delta}s ago");
    }
    if delta < 3_600 {
        return format!("{}m ago", delta / 60);
    }
    if delta < 86_400 {
        return format!("{}h ago", delta / 3_600);
    }

    format!("{}d ago", delta / 86_400)
}

#[cfg(test)]
mod tests {
    use super::{repo_context_revision_key, review_file_detail};
    use crate::git::{RepoContext, RepoSnapshot};
    use crate::run_checkpoints::RunReviewFile;

    #[test]
    fn repo_context_revision_key_changes_with_changed_files() {
        let base = RepoContext::Available(RepoSnapshot {
            root: "/tmp/repo".to_string(),
            head: Some("abc".to_string()),
            current_branch: Some("main".to_string()),
            has_upstream: false,
            branches: Vec::new(),
            commits: Vec::new(),
            changes: Vec::new(),
            unstaged_count: 0,
            tags: Vec::new(),
        });
        let changed = RepoContext::Available(RepoSnapshot {
            root: "/tmp/repo".to_string(),
            head: Some("abc".to_string()),
            current_branch: Some("main".to_string()),
            has_upstream: false,
            branches: Vec::new(),
            commits: Vec::new(),
            changes: vec![crate::git::FileChange {
                path: "notes.txt".to_string(),
                code: "??".to_string(),
                is_staged: false,
                is_unstaged: false,
                is_untracked: true,
            }],
            unstaged_count: 1,
            tags: Vec::new(),
        });

        assert_ne!(
            repo_context_revision_key(Some(&base)),
            repo_context_revision_key(Some(&changed))
        );
    }

    #[test]
    fn review_file_detail_lists_change_reasons() {
        let detail = review_file_detail(&RunReviewFile {
            path: "notes.txt".to_string(),
            code: "??".to_string(),
            is_staged: false,
            is_unstaged: false,
            is_untracked: true,
            is_new_since_start: true,
            status_changed_since_start: true,
            worktree_changed_since_start: true,
            index_changed_since_start: false,
        });

        assert!(detail.contains("new since run start"));
        assert!(detail.contains("worktree changed"));
        assert!(detail.contains("untracked"));
    }
}
