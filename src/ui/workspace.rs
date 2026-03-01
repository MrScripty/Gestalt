use crate::orchestrator::{self, GroupOrchestratorSnapshot};
use crate::state::{AppState, SessionId, SessionStatus};
use crate::terminal::{TerminalManager, TerminalSnapshot};
use crate::ui::insert_command_mode::InsertModeState;
use crate::ui::local_agent_panel::LocalAgentPanel;
use crate::ui::sidebar_panel_host::{SidebarPanelHost, SidebarPanelKind};
use crate::ui::terminal_view::{
    TerminalInteractionSignals, pending_terminal_snapshot, terminal_shell,
};
use dioxus::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
struct TerminalPaneData {
    terminal: TerminalSnapshot,
    cwd: String,
    is_runtime_ready: bool,
}

const RUNNER_WIDTH_MIN_PX: i32 = 260;
const RUNNER_WIDTH_MAX_PX: i32 = 760;
const RUNNER_WIDTH_STEP_PX: i32 = 16;
const SIDE_PANEL_WIDTH_PX: i32 = 380;
const STACK_SPLIT_STEP_RATIO: f64 = 0.03;
const STACK_SPLIT_MIN_RATIO: f64 = 0.28;
const STACK_SPLIT_MAX_RATIO: f64 = 0.72;
const STACK_SPLIT_DRAG_SENSITIVITY_PX: f64 = 520.0;

fn run_sidebar_style_for_panel(_panel: SidebarPanelKind, ratio: f64) -> String {
    format!("--runner-top-ratio: {:.2}%;", ratio * 100.0)
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
    runner_width_px: Signal<i32>,
    agent_top_ratio: Signal<f64>,
    runner_top_ratio: Signal<f64>,
    git_context: Signal<Option<crate::git::RepoContext>>,
    git_context_loading: Signal<bool>,
    git_refresh_nonce: Signal<u64>,
    sidebar_panel: Signal<SidebarPanelKind>,
    sidebar_open: Signal<bool>,
    insert_mode_state: Signal<Option<InsertModeState>>,
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

    let mut runner_width_px = runner_width_px;
    let mut agent_top_ratio = agent_top_ratio;
    let mut runner_top_ratio = runner_top_ratio;
    let mut sidebar_open = sidebar_open;
    let persistence_feedback_value = persistence_feedback.read().clone();
    let mut runner_drag_start = use_signal(|| None::<(f64, i32)>);
    let mut agent_drag_start = use_signal(|| None::<(f64, f64)>);
    let mut sidebar_drag_start = use_signal(|| None::<(f64, f64)>);

    let runner_width = (*runner_width_px.read()).clamp(RUNNER_WIDTH_MIN_PX, RUNNER_WIDTH_MAX_PX);
    let agent_ratio = (*agent_top_ratio.read()).clamp(STACK_SPLIT_MIN_RATIO, STACK_SPLIT_MAX_RATIO);
    let sidebar_ratio =
        (*runner_top_ratio.read()).clamp(STACK_SPLIT_MIN_RATIO, STACK_SPLIT_MAX_RATIO);
    let workspace_layout_style =
        format!("--runner-width: {runner_width}px; --side-panel-width: {SIDE_PANEL_WIDTH_PX}px;");
    let sidebar_open_value = *sidebar_open.read();
    let workspace_layout_class = if sidebar_open_value {
        "workspace-layout with-side-panel"
    } else {
        "workspace-layout"
    };
    let agent_stack_style = format!("--agent-top-ratio: {:.2}%;", agent_ratio * 100.0);
    let run_sidebar_style = run_sidebar_style_for_panel(*sidebar_panel.read(), sidebar_ratio);
    let interaction = TerminalInteractionSignals {
        app_state,
        focused_terminal,
        round_anchor,
        insert_mode_state,
    };
    let workspace_class = if runner_drag_start.read().is_some()
        || agent_drag_start.read().is_some()
        || sidebar_drag_start.read().is_some()
    {
        "workspace resizing"
    } else {
        "workspace"
    };

    rsx! {
        main {
            class: "{workspace_class}",
            onmousemove: move |event| {
                let pointer = event.data().client_coordinates();

                if let Some((start_x, start_width)) = *runner_drag_start.read() {
                    let delta_x = pointer.x - start_x;
                    let next_width = (f64::from(start_width) - delta_x).round() as i32;
                    runner_width_px.set(next_width.clamp(RUNNER_WIDTH_MIN_PX, RUNNER_WIDTH_MAX_PX));
                }

                if let Some((start_y, start_ratio)) = *agent_drag_start.read() {
                    let delta_y = pointer.y - start_y;
                    let next_ratio = (start_ratio + (delta_y / STACK_SPLIT_DRAG_SENSITIVITY_PX))
                        .clamp(STACK_SPLIT_MIN_RATIO, STACK_SPLIT_MAX_RATIO);
                    agent_top_ratio.set(next_ratio);
                }

                if let Some((start_y, start_ratio)) = *sidebar_drag_start.read() {
                    let delta_y = pointer.y - start_y;
                    let next_ratio = (start_ratio + (delta_y / STACK_SPLIT_DRAG_SENSITIVITY_PX))
                        .clamp(STACK_SPLIT_MIN_RATIO, STACK_SPLIT_MAX_RATIO);
                    runner_top_ratio.set(next_ratio);
                }
            },
            onmouseup: move |_| {
                runner_drag_start.set(None);
                agent_drag_start.set(None);
                sidebar_drag_start.set(None);
            },
            onmouseleave: move |_| {
                runner_drag_start.set(None);
                agent_drag_start.set(None);
                sidebar_drag_start.set(None);
            },
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

                div { class: "workspace-head-controls",
                    div { class: "status-summary",
                        span { class: "badge idle", "Idle {idle_count}" }
                        span { class: "badge busy", "Busy {busy_count}" }
                        span { class: "badge error", "Error {error_count}" }
                    }
                    button {
                        class: "side-panel-toggle",
                        r#type: "button",
                        aria_controls: "workspace-right-panel",
                        aria_expanded: sidebar_open_value,
                        onclick: move |_| {
                            let next = !*sidebar_open.read();
                            sidebar_open.set(next);
                        },
                        if sidebar_open_value { "Hide Panels" } else { "Show Panels" }
                    }
                }
            }

            if let Some(group_id) = active_group_id {
                {
                    let group_name = snapshot.group_label(group_id);
                    let has_agent_split = active_agents.len() > 1;
                    let agent_stack_class = if has_agent_split {
                        "agent-stack split-enabled"
                    } else {
                        "agent-stack"
                    };

                    rsx! {
                        div { class: "{workspace_layout_class}", style: "{workspace_layout_style}",
                            div { class: "{agent_stack_class}", style: "{agent_stack_style}",
                                for (index, session) in active_agents.into_iter().enumerate() {
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
                                                    interaction,
                                                )}
                                            }

                                            if has_agent_split && index == 0 {
                                                button {
                                                    class: "panel-splitter panel-splitter-horizontal terminal-row-splitter",
                                                    r#type: "button",
                                                    aria_label: "Resize agent terminals",
                                                    onmousedown: move |event| {
                                                        event.prevent_default();
                                                        let start_y = event.data().client_coordinates().y;
                                                        agent_drag_start.set(Some((start_y, *agent_top_ratio.read())));
                                                    },
                                                    onkeydown: move |event| {
                                                        match event.key() {
                                                            Key::ArrowUp => {
                                                                event.prevent_default();
                                                                let next = *agent_top_ratio.read() - STACK_SPLIT_STEP_RATIO;
                                                                agent_top_ratio.set(
                                                                    next.clamp(STACK_SPLIT_MIN_RATIO, STACK_SPLIT_MAX_RATIO),
                                                                );
                                                            }
                                                            Key::ArrowDown => {
                                                                event.prevent_default();
                                                                let next = *agent_top_ratio.read() + STACK_SPLIT_STEP_RATIO;
                                                                agent_top_ratio.set(
                                                                    next.clamp(STACK_SPLIT_MIN_RATIO, STACK_SPLIT_MAX_RATIO),
                                                                );
                                                            }
                                                            _ => {}
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            button {
                                class: "panel-splitter panel-splitter-vertical workspace-divider",
                                r#type: "button",
                                aria_label: "Resize run sidebar",
                                onmousedown: move |event| {
                                    event.prevent_default();
                                    let start_x = event.data().client_coordinates().x;
                                    runner_drag_start.set(Some((start_x, *runner_width_px.read())));
                                },
                                onkeydown: move |event| {
                                    match event.key() {
                                        Key::ArrowLeft => {
                                            event.prevent_default();
                                            let next = *runner_width_px.read() + RUNNER_WIDTH_STEP_PX;
                                            runner_width_px
                                                .set(next.clamp(RUNNER_WIDTH_MIN_PX, RUNNER_WIDTH_MAX_PX));
                                        }
                                        Key::ArrowRight => {
                                            event.prevent_default();
                                            let next = *runner_width_px.read() - RUNNER_WIDTH_STEP_PX;
                                            runner_width_px
                                                .set(next.clamp(RUNNER_WIDTH_MIN_PX, RUNNER_WIDTH_MAX_PX));
                                        }
                                        _ => {}
                                    }
                                },
                            }

                            aside { class: "run-sidebar split-enabled", style: "{run_sidebar_style}",
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
                                                    interaction,
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

                                button {
                                    class: "panel-splitter panel-splitter-horizontal terminal-row-splitter",
                                    r#type: "button",
                                    aria_label: "Resize run and local agent panes",
                                    onmousedown: move |event| {
                                        event.prevent_default();
                                        let start_y = event.data().client_coordinates().y;
                                        sidebar_drag_start.set(Some((start_y, *runner_top_ratio.read())));
                                    },
                                    onkeydown: move |event| {
                                        match event.key() {
                                            Key::ArrowUp => {
                                                event.prevent_default();
                                                let next = *runner_top_ratio.read() - STACK_SPLIT_STEP_RATIO;
                                                runner_top_ratio.set(
                                                    next.clamp(STACK_SPLIT_MIN_RATIO, STACK_SPLIT_MAX_RATIO),
                                                );
                                            }
                                            Key::ArrowDown => {
                                                event.prevent_default();
                                                let next = *runner_top_ratio.read() + STACK_SPLIT_STEP_RATIO;
                                                runner_top_ratio.set(
                                                    next.clamp(STACK_SPLIT_MIN_RATIO, STACK_SPLIT_MAX_RATIO),
                                                );
                                            }
                                            _ => {}
                                        }
                                    },
                                }

                                if let Some(group_orchestrator) = orchestrator_snapshot.clone() {
                                    LocalAgentPanel {
                                        app_state: app_state,
                                        terminal_manager: terminal_manager,
                                        group_id: group_id,
                                        group_orchestrator: group_orchestrator,
                                        local_agent_command: local_agent_command,
                                        local_agent_feedback: local_agent_feedback,
                                    }
                                } else {
                                    div { class: "sidebar-panel-empty",
                                        p { "Local agent context is not available." }
                                    }
                                }
                            }

                            if sidebar_open_value {
                                div { class: "workspace-divider workspace-divider-static", aria_hidden: "true" }

                                aside { id: "workspace-right-panel", class: "workspace-side-panel",
                                    SidebarPanelHost {
                                        app_state: app_state,
                                        terminal_manager: terminal_manager,
                                        group_id: group_id,
                                        group_orchestrator: orchestrator_snapshot.clone(),
                                        local_agent_command: local_agent_command,
                                        local_agent_feedback: local_agent_feedback,
                                        active_group_path: active_path.clone(),
                                        repo_context: git_context,
                                        repo_loading: git_context_loading,
                                        git_refresh_nonce: git_refresh_nonce,
                                        sidebar_panel: sidebar_panel,
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

#[cfg(test)]
mod tests {
    use super::run_sidebar_style_for_panel;
    use crate::ui::sidebar_panel_host::SidebarPanelKind;

    #[test]
    fn sidebar_layout_style_is_panel_agnostic() {
        let ratio = 0.57;
        let local = run_sidebar_style_for_panel(SidebarPanelKind::LocalAgent, ratio);
        let commands = run_sidebar_style_for_panel(SidebarPanelKind::Commands, ratio);
        let git = run_sidebar_style_for_panel(SidebarPanelKind::Git, ratio);

        assert_eq!(local, commands);
        assert_eq!(local, git);
    }
}
