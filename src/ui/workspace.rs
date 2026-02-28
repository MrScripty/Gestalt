use crate::orchestrator::{self, GroupOrchestratorSnapshot, SessionWriteResult};
use crate::state::{AppState, SessionId, SessionStatus};
use crate::terminal::{TerminalManager, TerminalSnapshot};
use crate::ui::terminal_view::{pending_terminal_snapshot, terminal_shell};
use dioxus::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
struct TerminalPaneData {
    terminal: TerminalSnapshot,
    cwd: String,
    is_runtime_ready: bool,
}

#[component]
pub(crate) fn WorkspaceMain(
    app_state: Signal<AppState>,
    terminal_manager: Signal<Arc<TerminalManager>>,
    focused_terminal: Signal<Option<SessionId>>,
    round_anchor: Signal<Option<(SessionId, u16)>>,
    local_agent_command: Signal<String>,
    local_agent_feedback: Signal<String>,
    persistence_feedback: Signal<String>,
    refresh_tick: Signal<u64>,
) -> Element {
    let _ = *refresh_tick.read();
    let snapshot = app_state.read().clone();
    let busy_count = snapshot.session_count_by_status(SessionStatus::Busy);
    let error_count = snapshot.session_count_by_status(SessionStatus::Error);
    let idle_count = snapshot.session_count_by_status(SessionStatus::Idle);
    let focused_terminal_id = *focused_terminal.read();

    let active_group_id = snapshot.active_group_id();
    let (active_agents, active_runner) = active_group_id
        .map(|group_id| snapshot.workspace_sessions_for_group(group_id))
        .unwrap_or_default();
    let active_group_sessions = active_group_id
        .map(|group_id| snapshot.sessions_in_group(group_id))
        .unwrap_or_default();
    let active_path = active_group_id
        .and_then(|group_id| snapshot.group_path(group_id))
        .unwrap_or(".")
        .to_string();

    let terminal_snapshot_by_id: HashMap<SessionId, TerminalPaneData> = {
        let mut panes = HashMap::new();
        let runtime = terminal_manager.read().clone();
        for session in &active_group_sessions {
            let runtime_snapshot = runtime.snapshot(session.id);
            let is_runtime_ready = runtime_snapshot.is_some();
            let terminal = runtime_snapshot.unwrap_or_else(pending_terminal_snapshot);
            let cwd = runtime.session_cwd(session.id).unwrap_or_else(|| {
                snapshot
                    .group_path(session.group_id)
                    .unwrap_or(".")
                    .to_string()
            });
            panes.insert(
                session.id,
                TerminalPaneData {
                    terminal,
                    cwd,
                    is_runtime_ready,
                },
            );
        }
        panes
    };

    let orchestrator_runtime_by_id = terminal_snapshot_by_id
        .iter()
        .map(|(session_id, pane)| {
            (
                *session_id,
                orchestrator::SessionRuntimeView {
                    lines: pane.terminal.lines.clone(),
                    cwd: pane.cwd.clone(),
                    is_runtime_ready: pane.is_runtime_ready,
                },
            )
        })
        .collect::<HashMap<SessionId, orchestrator::SessionRuntimeView>>();
    let orchestrator_snapshot: Option<GroupOrchestratorSnapshot> =
        active_group_id.map(|group_id| {
            orchestrator::snapshot_group_from_runtime(
                &snapshot,
                group_id,
                focused_terminal_id,
                &orchestrator_runtime_by_id,
            )
        });

    let mut local_agent_command = local_agent_command;
    let mut local_agent_feedback = local_agent_feedback;
    let local_agent_command_value = local_agent_command.read().clone();
    let local_agent_feedback_value = local_agent_feedback.read().clone();
    let persistence_feedback_value = persistence_feedback.read().clone();

    rsx! {
        main { class: "workspace",
            header { class: "workspace-head",
                div {
                    h2 { "Workspace" }
                    if active_group_id.is_some() {
                        p {
                            "Active path: "
                            b { "{active_path}" }
                        }
                    }
                    p { class: "meta-tip", "Each group defaults to Agent A + Agent B + blue Run/Compile pane." }
                    if !persistence_feedback_value.is_empty() {
                        p { class: "meta-tip", "{persistence_feedback_value}" }
                    }
                }

                div { class: "status-summary",
                    span { class: "badge idle", "Idle {idle_count}" }
                    span { class: "badge busy", "Busy {busy_count}" }
                    span { class: "badge error", "Error {error_count}" }
                }
            }

            if let Some(group_id) = active_group_id {
                {
                    let group_name = snapshot.group_label(group_id);

                    rsx! {
                        div { class: "workspace-layout",
                            div { class: "agent-stack",
                                for session in active_agents {
                                    {
                                        let session_id = session.id;
                                        let selected = snapshot.selected_session == Some(session_id);
                                        let terminal_is_focused = focused_terminal_id == Some(session_id);
                                        let pane_class = if selected {
                                            "terminal-card agent selected"
                                        } else {
                                            "terminal-card agent"
                                        };
                                        let card_style = format!("border-top-color: var({});", session.status.css_var());
                                        let badge_style = format!("background: var({});", session.status.css_var());
                                        let pane = terminal_snapshot_by_id
                                            .get(&session_id)
                                            .cloned()
                                            .unwrap_or_else(|| TerminalPaneData {
                                                terminal: pending_terminal_snapshot(),
                                                cwd: snapshot
                                                    .group_path(session.group_id)
                                                    .unwrap_or(".")
                                                    .to_string(),
                                                is_runtime_ready: false,
                                            });
                                        let terminal = pane.terminal;
                                        let cwd = pane.cwd;
                                        let terminal_manager_for_input = terminal_manager.read().clone();

                                        rsx! {
                                            article {
                                                class: "{pane_class}",
                                                key: "agent-card-{session_id}",
                                                style: "{card_style}",
                                                onclick: move |_| app_state.write().select_session(session_id),

                                                div { class: "terminal-head",
                                                    div {
                                                        h4 { "{session.title}" }
                                                        p { class: "sub", "{group_name}" }
                                                        p { class: "terminal-meta", "cwd: {cwd}" }
                                                    }

                                                    button {
                                                        class: "status-cycle",
                                                        style: "{badge_style}",
                                                        onclick: move |event| {
                                                            event.stop_propagation();
                                                            app_state.write().cycle_session_status(session_id);
                                                        },
                                                        "{session.status.label()}"
                                                    }
                                                }

                                                {terminal_shell(
                                                    session_id,
                                                    terminal_is_focused,
                                                    terminal,
                                                    terminal_manager_for_input,
                                                    app_state,
                                                    focused_terminal,
                                                    round_anchor,
                                                )}
                                            }
                                        }
                                    }
                                }
                            }

                            aside { class: "run-sidebar",
                                if let Some(session) = active_runner {
                                    {
                                        let session_id = session.id;
                                        let selected = snapshot.selected_session == Some(session_id);
                                        let terminal_is_focused = focused_terminal_id == Some(session_id);
                                        let pane_class = if selected {
                                            "terminal-card runner selected"
                                        } else {
                                            "terminal-card runner"
                                        };
                                        let card_style = format!("border-top-color: var({});", session.status.css_var());
                                        let badge_style = format!("background: var({});", session.status.css_var());
                                        let pane = terminal_snapshot_by_id
                                            .get(&session_id)
                                            .cloned()
                                            .unwrap_or_else(|| TerminalPaneData {
                                                terminal: pending_terminal_snapshot(),
                                                cwd: snapshot
                                                    .group_path(session.group_id)
                                                    .unwrap_or(".")
                                                    .to_string(),
                                                is_runtime_ready: false,
                                            });
                                        let terminal = pane.terminal;
                                        let cwd = pane.cwd;
                                        let terminal_manager_for_input = terminal_manager.read().clone();

                                        rsx! {
                                            article {
                                                class: "{pane_class}",
                                                key: "runner-card-{session_id}",
                                                style: "{card_style}",
                                                onclick: move |_| app_state.write().select_session(session_id),

                                                div { class: "terminal-head",
                                                    div {
                                                        h4 { "{session.title}" }
                                                        p { class: "sub", "{group_name}" }
                                                        p { class: "terminal-meta", "cwd: {cwd}" }
                                                    }

                                                    button {
                                                        class: "status-cycle",
                                                        style: "{badge_style}",
                                                        onclick: move |event| {
                                                            event.stop_propagation();
                                                            app_state.write().cycle_session_status(session_id);
                                                        },
                                                        "{session.status.label()}"
                                                    }
                                                }

                                                {terminal_shell(
                                                    session_id,
                                                    terminal_is_focused,
                                                    terminal,
                                                    terminal_manager_for_input,
                                                    app_state,
                                                    focused_terminal,
                                                    round_anchor,
                                                )}
                                            }
                                        }
                                    }
                                } else {
                                    div { class: "runner-empty",
                                        h3 { "No Run Pane" }
                                        p { "Create or move a RUN tab into this group." }
                                    }
                                }

                                if let Some(group_orchestrator) = orchestrator_snapshot.clone() {
                                    {
                                        let terminal_manager_for_agent = terminal_manager.read().clone();
                                        let group_session_ids = orchestrator::group_session_ids(&snapshot, group_id);
                                        let group_session_ids_for_send = group_session_ids.clone();
                                        let group_session_ids_for_interrupt = group_session_ids.clone();
                                        let terminal_manager_for_send = terminal_manager_for_agent.clone();
                                        let terminal_manager_for_interrupt = terminal_manager_for_agent.clone();
                                        let tracked_count = group_orchestrator.terminals.len();

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

                                                                let results = orchestrator::send_line_to_sessions(
                                                                    &terminal_manager_for_send,
                                                                    &group_session_ids_for_send,
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
                                                                let results = orchestrator::interrupt_sessions(
                                                                    &terminal_manager_for_interrupt,
                                                                    &group_session_ids_for_interrupt,
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
                                }
                            }
                        }
                    }
                }
            } else {
                div { class: "workspace-empty",
                    h3 { "No groups yet" }
                    p { "Create a path group to start your 3-terminal workspace." }
                }
            }
        }
    }
}

fn apply_orchestrator_results(app_state: &mut AppState, results: &[SessionWriteResult]) {
    for result in results {
        if result.error.is_none() {
            app_state.set_session_status(result.session_id, SessionStatus::Busy);
        } else {
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
