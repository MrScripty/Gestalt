use crate::git::RepoContext;
use crate::orchestrator::GroupOrchestratorSnapshot;
use crate::state::{AppState, GroupId};
use crate::terminal::TerminalManager;
use crate::ui::UiState;
use crate::ui::commands_panel::CommandsPanel;
use crate::ui::file_browser_panel::FileBrowserPanel;
use crate::ui::git_panel::GitPanel;
use crate::ui::local_agent_panel::LocalAgentPanel;
use dioxus::prelude::*;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SidebarPanelKind {
    #[allow(dead_code)]
    LocalAgent,
    Commands,
    Git,
    FileBrowser,
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
    ui_state: Signal<UiState>,
    terminal_manager: Signal<Arc<TerminalManager>>,
    group_id: GroupId,
    group_orchestrator: Option<GroupOrchestratorSnapshot>,
    active_group_path: String,
    repo_context: Signal<Option<RepoContext>>,
    repo_loading: Signal<bool>,
    git_refresh_nonce: Signal<u64>,
) -> Element {
    let active_panel = ui_state.read().sidebar_panel;

    rsx! {
        div { class: "side-panel-host",
            div { class: "side-panel-switcher", role: "tablist", aria_label: "Sidebar panels",
                {panel_button("Commands", "Insert command library", SidebarPanelKind::Commands, active_panel, ui_state)}
                {panel_button("Git", "Git repository context", SidebarPanelKind::Git, active_panel, ui_state)}
                {panel_button("Files", "Browse files in this path group", SidebarPanelKind::FileBrowser, active_panel, ui_state)}
            }

            div { class: "side-panel-body",
                {
                    match active_panel {
                        SidebarPanelKind::LocalAgent => {
                            if let Some(group_orchestrator) = group_orchestrator {
                                rsx! {
                                    LocalAgentPanel {
                                        app_state: app_state,
                                        ui_state: ui_state,
                                        terminal_manager: terminal_manager,
                                        group_id: group_id,
                                        git_refresh_nonce: git_refresh_nonce,
                                        group_orchestrator: group_orchestrator,
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
                            CommandsPanel {
                                app_state: app_state,
                                terminal_manager: terminal_manager,
                            }
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
                        SidebarPanelKind::FileBrowser => rsx! {
                            FileBrowserPanel {
                                app_state: app_state,
                                terminal_manager: terminal_manager,
                                group_id: group_id,
                                active_group_path: active_group_path,
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
    mut ui_state: Signal<UiState>,
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
            onclick: move |_| {
                ui_state.write().sidebar_panel = select_sidebar_panel(active_panel, kind)
            },
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
            SidebarPanelKind::FileBrowser,
        ];

        for current in panels {
            for requested in panels {
                assert_eq!(select_sidebar_panel(current, requested), requested);
            }
        }
    }
}
