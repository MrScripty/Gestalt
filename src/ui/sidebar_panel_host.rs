use crate::git::RepoContext;
use crate::orchestrator::GroupOrchestratorSnapshot;
use crate::state::{AppState, GroupId};
use crate::terminal::TerminalManager;
use crate::ui::commands_panel::CommandsPanel;
use crate::ui::git_panel::GitPanel;
use crate::ui::local_agent_panel::LocalAgentPanel;
use dioxus::prelude::*;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SidebarPanelKind {
    LocalAgent,
    Commands,
    Git,
}

pub(crate) fn select_sidebar_panel(
    _current: SidebarPanelKind,
    requested: SidebarPanelKind,
) -> SidebarPanelKind {
    requested
}

#[component]
pub(crate) fn SidebarPanelHost(
    app_state: Signal<AppState>,
    terminal_manager: Signal<Arc<TerminalManager>>,
    group_id: GroupId,
    group_orchestrator: Option<GroupOrchestratorSnapshot>,
    local_agent_command: Signal<String>,
    local_agent_feedback: Signal<String>,
    active_group_path: String,
    repo_context: Signal<Option<RepoContext>>,
    repo_loading: Signal<bool>,
    git_refresh_nonce: Signal<u64>,
    sidebar_panel: Signal<SidebarPanelKind>,
) -> Element {
    let active_panel = *sidebar_panel.read();

    rsx! {
        div { class: "side-panel-host",
            div { class: "side-panel-switcher", role: "tablist", aria_label: "Sidebar panels",
                {panel_button("Agent", "Local agent orchestrator", SidebarPanelKind::LocalAgent, active_panel, sidebar_panel)}
                {panel_button("Commands", "Insert command library", SidebarPanelKind::Commands, active_panel, sidebar_panel)}
                {panel_button("Git", "Git repository context", SidebarPanelKind::Git, active_panel, sidebar_panel)}
            }

            div { class: "side-panel-body",
                {
                    match active_panel {
                        SidebarPanelKind::LocalAgent => {
                            if let Some(group_orchestrator) = group_orchestrator {
                                rsx! {
                                    LocalAgentPanel {
                                        app_state: app_state,
                                        terminal_manager: terminal_manager,
                                        group_id: group_id,
                                        group_orchestrator: group_orchestrator,
                                        local_agent_command: local_agent_command,
                                        local_agent_feedback: local_agent_feedback,
                                    }
                                }
                            } else {
                                rsx! {
                                    div { class: "sidebar-panel-empty",
                                        p { "Local agent context is not available." }
                                    }
                                }
                            }
                        }
                        SidebarPanelKind::Commands => rsx! {
                            CommandsPanel { app_state: app_state }
                        },
                        SidebarPanelKind::Git => rsx! {
                            GitPanel {
                                app_state: app_state,
                                terminal_manager: terminal_manager,
                                active_group_path: active_group_path,
                                repo_context: repo_context,
                                repo_loading: repo_loading,
                                git_refresh_nonce: git_refresh_nonce,
                            }
                        },
                    }
                }
            }
        }
    }
}

fn panel_button(
    label: &'static str,
    title: &'static str,
    kind: SidebarPanelKind,
    active_panel: SidebarPanelKind,
    mut sidebar_panel: Signal<SidebarPanelKind>,
) -> Element {
    let class = if kind == active_panel {
        "side-panel-tab active"
    } else {
        "side-panel-tab"
    };

    rsx! {
        button {
            class: "{class}",
            r#type: "button",
            role: "tab",
            title: "{title}",
            aria_selected: kind == active_panel,
            onclick: move |_| sidebar_panel.set(select_sidebar_panel(active_panel, kind)),
            "{label}"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SidebarPanelKind, select_sidebar_panel};

    #[test]
    fn switcher_supports_toggling_across_all_panels() {
        let panels = [
            SidebarPanelKind::LocalAgent,
            SidebarPanelKind::Commands,
            SidebarPanelKind::Git,
        ];

        for current in panels {
            for requested in panels {
                assert_eq!(select_sidebar_panel(current, requested), requested);
            }
        }
    }
}
