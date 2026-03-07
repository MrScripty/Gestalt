use crate::orchestrator::{self, GroupOrchestratorSnapshot, SessionWriteResult};
use crate::state::{AppState, GroupId, SessionStatus};
use crate::terminal::TerminalManager;
use dioxus::prelude::*;
use std::sync::Arc;

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
    let local_agent_command_value = local_agent_command.read().clone();
    let local_agent_feedback_value = local_agent_feedback.read().clone();
    let mut local_agent_command = local_agent_command;
    let mut local_agent_feedback = local_agent_feedback;

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
                            let results = orchestrator::broadcast_line_to_group(
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
                        },
                        "Send To Group"
                    }

                    button {
                        class: "orchestrator-btn interrupt",
                        onclick: move |_| {
                            let state_snapshot = app_state.read().clone();
                            let results = orchestrator::interrupt_group(
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
                        },
                        "Interrupt Group"
                    }
                }

                if !local_agent_feedback_value.is_empty() {
                    p { class: "orchestrator-feedback", "{local_agent_feedback_value}" }
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
