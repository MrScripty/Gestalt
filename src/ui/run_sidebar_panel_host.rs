use crate::orchestrator::GroupOrchestratorSnapshot;
use crate::state::{AppState, GroupId};
use crate::terminal::TerminalManager;
use crate::ui::local_agent_panel::LocalAgentPanel;
use crate::ui::notes_panel::NotesPanel;
use dioxus::prelude::*;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunSidebarPanelKind {
    LocalAgent,
    Notes,
}

#[component]
pub(crate) fn RunSidebarPanelHost(
    app_state: Signal<AppState>,
    terminal_manager: Signal<Arc<TerminalManager>>,
    group_id: GroupId,
    group_orchestrator: Option<GroupOrchestratorSnapshot>,
    local_agent_command: Signal<String>,
    local_agent_feedback: Signal<String>,
    run_sidebar_panel: Signal<RunSidebarPanelKind>,
) -> Element {
    let active_panel = *run_sidebar_panel.read();

    rsx! {
        div { class: "run-sidebar-panel-host",
            div { class: "run-sidebar-switcher", role: "tablist", aria_label: "Run sidebar panels",
                {
                    let class = if active_panel == RunSidebarPanelKind::LocalAgent {
                        "run-sidebar-tab active"
                    } else {
                        "run-sidebar-tab"
                    };
                    rsx! {
                        button {
                            class: "{class}",
                            r#type: "button",
                            role: "tab",
                            aria_selected: active_panel == RunSidebarPanelKind::LocalAgent,
                            onclick: move |_| run_sidebar_panel.set(RunSidebarPanelKind::LocalAgent),
                            "Local Agent"
                        }
                    }
                }
                {
                    let class = if active_panel == RunSidebarPanelKind::Notes {
                        "run-sidebar-tab active"
                    } else {
                        "run-sidebar-tab"
                    };
                    rsx! {
                        button {
                            class: "{class}",
                            r#type: "button",
                            role: "tab",
                            aria_selected: active_panel == RunSidebarPanelKind::Notes,
                            onclick: move |_| run_sidebar_panel.set(RunSidebarPanelKind::Notes),
                            "Notes"
                        }
                    }
                }
            }
            div { class: "run-sidebar-body",
                if active_panel == RunSidebarPanelKind::LocalAgent {
                    if let Some(group_orchestrator) = group_orchestrator {
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
                } else {
                    NotesPanel {
                        app_state: app_state,
                    }
                }
            }
        }
    }
}
