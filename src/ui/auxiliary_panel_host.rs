use crate::emily_bridge::EmilyBridge;
use crate::git::RepoContext;
use crate::orchestrator::GroupOrchestratorSnapshot;
use crate::state::{AppState, AuxiliaryPanelHost, AuxiliaryPanelKind, GroupId};
use crate::terminal::TerminalManager;
use crate::ui::UiState;
use crate::ui::commands_panel::CommandsPanel;
use crate::ui::file_browser_panel::FileBrowserPanel;
use crate::ui::git_panel::GitPanel;
use crate::ui::local_agent_panel::LocalAgentPanel;
use crate::ui::notes_panel::NotesPanel;
use crate::ui::run_review_panel::RunReviewPanel;
use dioxus::prelude::*;
use std::sync::Arc;

#[component]
pub(crate) fn DockedAuxiliaryPanelHost(
    host: AuxiliaryPanelHost,
    app_state: Signal<AppState>,
    ui_state: Signal<UiState>,
    terminal_manager: Signal<Arc<TerminalManager>>,
    emily_bridge: Signal<Arc<EmilyBridge>>,
    group_id: GroupId,
    group_orchestrator: Option<GroupOrchestratorSnapshot>,
    active_group_path: String,
    repo_context: Signal<Option<RepoContext>>,
    repo_loading: Signal<bool>,
    git_refresh_nonce: Signal<u64>,
    mut dragging_panel: Signal<Option<AuxiliaryPanelKind>>,
) -> Element {
    let snapshot = app_state.read().clone();
    let tabs = snapshot.auxiliary_panel_tabs(host);
    let has_tabs = !tabs.is_empty();
    let active_panel = snapshot.active_auxiliary_panel(host);
    let is_drop_target_active = dragging_panel.read().is_some();
    let tablist_aria_label = match host {
        AuxiliaryPanelHost::RunSidebar => "Run sidebar tabs",
        AuxiliaryPanelHost::SidePanel => "Right sidebar tabs",
    };
    let tablist_class = if is_drop_target_active {
        "aux-panel-tabs drag-target"
    } else {
        "aux-panel-tabs"
    };

    rsx! {
        div { class: "aux-panel-host",
            div {
                class: "{tablist_class}",
                role: "tablist",
                aria_label: "{tablist_aria_label}",
                ondragover: move |event| {
                    event.prevent_default();
                },
                ondrop: move |event| {
                    event.prevent_default();
                    if let Some(source_panel) = *dragging_panel.read() {
                        app_state
                            .write()
                            .move_auxiliary_panel_to_host_end(source_panel, host);
                    }
                    dragging_panel.set(None);
                },
                ondragleave: move |_| {},
                for panel in tabs {
                    {
                        let panel_id = panel;
                        let is_active = active_panel == Some(panel_id);
                        let is_dragging = *dragging_panel.read() == Some(panel_id);
                        let class = if is_dragging {
                            "aux-panel-tab dragging"
                        } else if is_active {
                            "aux-panel-tab active"
                        } else {
                            "aux-panel-tab"
                        };

                        rsx! {
                            button {
                                key: "aux-panel-tab-{panel_id.label()}",
                                class: "{class}",
                                r#type: "button",
                                role: "tab",
                                aria_selected: is_active,
                                draggable: "true",
                                onclick: move |_| {
                                    app_state.write().set_active_auxiliary_panel(host, panel_id);
                                },
                                ondragstart: move |_| {
                                    dragging_panel.set(Some(panel_id));
                                },
                                ondragend: move |_| {
                                    dragging_panel.set(None);
                                },
                                ondragover: move |event| {
                                    event.stop_propagation();
                                    event.prevent_default();
                                },
                                ondrop: move |event| {
                                    event.stop_propagation();
                                    event.prevent_default();
                                    if let Some(source_panel) = *dragging_panel.read() {
                                        app_state
                                            .write()
                                            .move_auxiliary_panel_before(source_panel, panel_id);
                                    }
                                    dragging_panel.set(None);
                                },
                                "{panel_id.label()}"
                            }
                        }
                    }
                }
                if !has_tabs {
                    span { class: "aux-panel-drop-hint", "Drop a tab here" }
                }
            }

            div { class: "aux-panel-body",
                if let Some(active_panel) = active_panel {
                    {render_auxiliary_panel_body(
                        active_panel,
                        app_state,
                        ui_state,
                        terminal_manager,
                        emily_bridge,
                        group_id,
                        group_orchestrator.clone(),
                        active_group_path.clone(),
                        repo_context,
                        repo_loading,
                        git_refresh_nonce,
                    )}
                } else {
                    div { class: "sidebar-panel-empty",
                        p { "No panel tabs are docked here." }
                    }
                }
            }
        }
    }
}

fn render_auxiliary_panel_body(
    panel: AuxiliaryPanelKind,
    app_state: Signal<AppState>,
    ui_state: Signal<UiState>,
    terminal_manager: Signal<Arc<TerminalManager>>,
    emily_bridge: Signal<Arc<EmilyBridge>>,
    group_id: GroupId,
    group_orchestrator: Option<GroupOrchestratorSnapshot>,
    active_group_path: String,
    repo_context: Signal<Option<RepoContext>>,
    repo_loading: Signal<bool>,
    git_refresh_nonce: Signal<u64>,
) -> Element {
    match panel {
        AuxiliaryPanelKind::LocalAgent => {
            if let Some(group_orchestrator) = group_orchestrator {
                rsx! {
                    LocalAgentPanel {
                        app_state: app_state,
                        ui_state: ui_state,
                        terminal_manager: terminal_manager,
                        emily_bridge: emily_bridge,
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
        AuxiliaryPanelKind::RunReview => rsx! {
            RunReviewPanel {
                active_group_path: active_group_path,
                repo_context: repo_context,
                git_refresh_nonce: git_refresh_nonce,
            }
        },
        AuxiliaryPanelKind::Notes => rsx! {
            NotesPanel {
                app_state: app_state,
            }
        },
        AuxiliaryPanelKind::Commands => rsx! {
            CommandsPanel {
                app_state: app_state,
                terminal_manager: terminal_manager,
            }
        },
        AuxiliaryPanelKind::Git => rsx! {
            GitPanel {
                app_state: app_state,
                terminal_manager: terminal_manager,
                active_group_path: active_group_path,
                repo_context: repo_context,
                repo_loading: repo_loading,
                git_refresh_nonce: git_refresh_nonce,
            }
        },
        AuxiliaryPanelKind::Files => rsx! {
            FileBrowserPanel {
                app_state: app_state,
                terminal_manager: terminal_manager,
                group_id: group_id,
                active_group_path: active_group_path,
            }
        },
    }
}
