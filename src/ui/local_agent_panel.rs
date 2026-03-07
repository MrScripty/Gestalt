use crate::orchestration_log::{
    CommandKind, CommandPayload, OrchestrationLogStore, ReceiptStatus, RecentActivityRecord,
};
use crate::orchestrator::{self, GroupOrchestratorSnapshot, SessionWriteResult};
use crate::state::{AppState, GroupId, SessionStatus};
use crate::terminal::TerminalManager;
use dioxus::prelude::*;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[component]
pub(crate) fn LocalAgentPanel(
    app_state: Signal<AppState>,
    terminal_manager: Signal<Arc<TerminalManager>>,
    group_id: GroupId,
    group_orchestrator: GroupOrchestratorSnapshot,
    local_agent_command: Signal<String>,
    local_agent_feedback: Signal<String>,
) -> Element {
    let terminal_manager_for_agent = terminal_manager.read().clone();
    let terminal_manager_for_send = terminal_manager_for_agent.clone();
    let terminal_manager_for_interrupt = terminal_manager_for_agent.clone();
    let tracked_count = group_orchestrator.terminals.len();
    let group_path = group_orchestrator.group_path.clone();
    let local_agent_command_value = local_agent_command.read().clone();
    let local_agent_feedback_value = local_agent_feedback.read().clone();
    let recent_activity = use_signal(Vec::<RecentActivityRecord>::new);
    let activity_feedback = use_signal(String::new);
    let activity_loading = use_signal(|| true);
    let mut activity_loaded_key = use_signal(String::new);
    let mut local_agent_command = local_agent_command;
    let mut local_agent_feedback = local_agent_feedback;

    if activity_loaded_key.read().as_str() != group_path.as_str() {
        activity_loaded_key.set(group_path.clone());
        refresh_recent_activity(
            recent_activity,
            activity_loading,
            activity_feedback,
            group_path.clone(),
        );
    }

    let recent_activity_value = recent_activity.read().clone();
    let activity_feedback_value = activity_feedback.read().clone();
    let activity_loading_value = *activity_loading.read();
    let group_path_for_send = group_path.clone();
    let group_path_for_interrupt = group_path.clone();

    rsx! {
        article { class: "orchestrator-card",
            div { class: "orchestrator-head",
                h3 { "Local Agent" }
                p { "Group path: {group_orchestrator.group_path}" }
                p { "Group id: {group_orchestrator.group_id} | terminals: {tracked_count}" }
            }

            div { class: "orchestrator-controls",
                textarea {
                    class: "orchestrator-input",
                    rows: "3",
                    placeholder: "Broadcast command to every terminal in this group",
                    value: "{local_agent_command_value}",
                    oninput: move |event| local_agent_command.set(event.value()),
                }

                div { class: "orchestrator-actions",
                    button {
                        class: "orchestrator-btn send",
                        onclick: move |_| {
                            let command = local_agent_command.read().trim().to_string();
                            if command.is_empty() {
                                local_agent_feedback.set("Enter a command to send.".to_string());
                                return;
                            }

                            let state_snapshot = app_state.read().clone();
                            let results = orchestrator::send_local_agent_command_to_group(
                                &state_snapshot,
                                &terminal_manager_for_send,
                                group_id,
                                &command,
                            );

                            let mut state = app_state.write();
                            apply_orchestrator_results(&mut state, &results);
                            drop(state);

                            let ok_count = results.iter().filter(|result| result.error.is_none()).count();
                            let fail_count = results.len().saturating_sub(ok_count);
                            if ok_count > 0 {
                                local_agent_command.set(String::new());
                            }
                            local_agent_feedback.set(format!(
                                "Broadcast complete: {ok_count} success, {fail_count} failed."
                            ));
                            refresh_recent_activity(
                                recent_activity,
                                activity_loading,
                                activity_feedback,
                                group_path_for_send.clone(),
                            );
                        },
                        "Send To Group"
                    }

                    button {
                        class: "orchestrator-btn interrupt",
                        onclick: move |_| {
                            let state_snapshot = app_state.read().clone();
                            let results = orchestrator::interrupt_local_agent_group(
                                &state_snapshot,
                                &terminal_manager_for_interrupt,
                                group_id,
                            );

                            let mut state = app_state.write();
                            apply_orchestrator_results(&mut state, &results);
                            drop(state);

                            let ok_count = results.iter().filter(|result| result.error.is_none()).count();
                            let fail_count = results.len().saturating_sub(ok_count);
                            local_agent_feedback.set(format!(
                                "Interrupt complete: {ok_count} success, {fail_count} failed."
                            ));
                            refresh_recent_activity(
                                recent_activity,
                                activity_loading,
                                activity_feedback,
                                group_path_for_interrupt.clone(),
                            );
                        },
                        "Interrupt Group"
                    }
                }

                if !local_agent_feedback_value.is_empty() {
                    p { class: "orchestrator-feedback", "{local_agent_feedback_value}" }
                }
            }

            div { class: "orchestrator-history",
                div { class: "orchestrator-head",
                    h3 { "Recent Activity" }
                    p { "SQLite-backed orchestration history for this group path." }
                }
                if activity_loading_value {
                    p { class: "meta", "Loading recent orchestration activity..." }
                } else if !activity_feedback_value.is_empty() {
                    p { class: "orchestrator-feedback", "{activity_feedback_value}" }
                } else if recent_activity_value.is_empty() {
                    p { class: "meta", "No recorded activity for this group yet." }
                } else {
                    div { class: "orchestrator-list",
                        for item in recent_activity_value {
                            div {
                                class: "orchestrator-item terminal-inactive",
                                key: "orchestrator-activity-{item.command.command_id}",
                                div { class: "orchestrator-item-head",
                                    span { class: "name", "{activity_title(&item)}" }
                                    span { class: "badge state", "{activity_status(&item)}" }
                                }
                                p { class: "meta", "{activity_detail(&item)}" }
                                p { class: "preview", "{activity_timestamp(&item)}" }
                            }
                        }
                    }
                }
            }

            div { class: "orchestrator-list",
                for terminal in group_orchestrator.terminals {
                    {
                        let activity_class = if terminal.is_focused {
                            "terminal-focused"
                        } else if terminal.is_selected {
                            "terminal-selected"
                        } else {
                            "terminal-inactive"
                        };
                        let runtime_state = if terminal.is_runtime_ready {
                            "online"
                        } else {
                            "pending"
                        };
                        let round_range = format!(
                            "rows {}-{}",
                            terminal.latest_round.start_row,
                            terminal.latest_round.end_row,
                        );
                        let preview = summarize_round_preview(&terminal.latest_round.text());

                        rsx! {
                            div {
                                class: "orchestrator-item {activity_class}",
                                key: "orchestrator-{terminal.session_id}",
                                div { class: "orchestrator-item-head",
                                    span { class: "name", "{terminal.title}" }
                                    span { class: "badge role", "{terminal.role.badge()}" }
                                    span { class: "badge state", "{terminal.status.label()}" }
                                }
                                p { class: "meta", "cwd: {terminal.cwd}" }
                                p { class: "meta", "runtime: {runtime_state} | {round_range}" }
                                p { class: "preview", "{preview}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn apply_orchestrator_results(app_state: &mut AppState, results: &[SessionWriteResult]) {
    for result in results {
        if result.error.is_some() {
            app_state.set_session_status(result.session_id, SessionStatus::Error);
        }
    }
}

fn summarize_round_preview(text: &str) -> String {
    let normalized = text
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .take(2)
        .collect::<Vec<_>>()
        .join(" | ");

    if normalized.is_empty() {
        return "(no output yet)".to_string();
    }

    let mut preview = String::new();
    let mut chars = normalized.chars();
    for _ in 0..180 {
        let Some(ch) = chars.next() else {
            return normalized;
        };
        preview.push(ch);
    }

    if chars.next().is_some() {
        preview.push_str("...");
    }

    preview
}

fn refresh_recent_activity(
    mut recent_activity: Signal<Vec<RecentActivityRecord>>,
    mut activity_loading: Signal<bool>,
    mut activity_feedback: Signal<String>,
    group_path: String,
) {
    activity_loading.set(true);
    activity_feedback.set(String::new());
    spawn(async move {
        let load_result = tokio::task::spawn_blocking(move || {
            OrchestrationLogStore::default().load_recent_activity_for_group_path(&group_path, 6)
        })
        .await;

        match load_result {
            Ok(Ok(records)) => {
                recent_activity.set(records);
                activity_feedback.set(String::new());
            }
            Ok(Err(error)) => {
                recent_activity.set(Vec::new());
                activity_feedback.set(error.to_string());
            }
            Err(error) => {
                recent_activity.set(Vec::new());
                activity_feedback.set(format!("Failed loading recent activity: {error}"));
            }
        }

        activity_loading.set(false);
    });
}

fn activity_title(item: &RecentActivityRecord) -> String {
    match &item.command.payload {
        CommandPayload::LocalAgentSendLine { line, .. } => format!("Local Agent: {line}"),
        CommandPayload::LocalAgentInterrupt { .. } => "Local Agent: interrupt".to_string(),
        CommandPayload::BroadcastSendLine { line, .. } => format!("Broadcast: {line}"),
        CommandPayload::BroadcastInterrupt { .. } => "Broadcast: interrupt".to_string(),
        CommandPayload::GitStageFiles { paths, .. } => format!("Git: stage {}", paths.join(", ")),
        CommandPayload::GitUnstageFiles { paths, .. } => {
            format!("Git: unstage {}", paths.join(", "))
        }
        CommandPayload::GitCreateCommit { title, .. } => format!("Git: commit {title}"),
        CommandPayload::GitUpdateCommitMessage { target_sha, .. } => {
            format!("Git: rewrite {target_sha}")
        }
        CommandPayload::GitCreateTag { tag_name, .. } => format!("Git: tag {tag_name}"),
        CommandPayload::GitCheckoutTarget {
            target_kind,
            target,
            ..
        } => format!("Git: checkout {target_kind} {target}"),
        CommandPayload::GitCreateWorktree {
            new_path, target, ..
        } => format!("Git: worktree {new_path} <- {target}"),
    }
}

fn activity_status(item: &RecentActivityRecord) -> &'static str {
    match item.receipt.as_ref().map(|receipt| receipt.status) {
        Some(ReceiptStatus::Succeeded) => "SUCCEEDED",
        Some(ReceiptStatus::PartiallySucceeded) => "PARTIAL",
        Some(ReceiptStatus::Failed) => "FAILED",
        None => "PENDING",
    }
}

fn activity_detail(item: &RecentActivityRecord) -> String {
    match &item.command.kind {
        CommandKind::LocalAgentSendLine | CommandKind::LocalAgentInterrupt => {
            format!("Timeline {}", item.command.timeline_id)
        }
        _ => format!("Kind {:?}", item.command.kind),
    }
}

fn activity_timestamp(item: &RecentActivityRecord) -> String {
    let requested = format_unix_ms(item.command.requested_at_unix_ms);
    match item.receipt.as_ref() {
        Some(receipt) => format!(
            "Requested {requested} | completed {}",
            format_unix_ms(receipt.completed_at_unix_ms)
        ),
        None => format!("Requested {requested}"),
    }
}

fn format_unix_ms(unix_ms: i64) -> String {
    if unix_ms <= 0 {
        return "unknown".to_string();
    }

    let seconds = unix_ms / 1_000;
    let millis = unix_ms.rem_euclid(1_000);
    let now_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
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

    format!("{}.{:03}s", seconds, millis)
}
