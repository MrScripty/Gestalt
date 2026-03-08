use crate::emily_bridge::EmilyBridge;
use crate::git::RepoContext;
use crate::orchestrator::GroupOrchestratorSnapshot;
use crate::state::{AppState, AuxiliaryPanelHost, GroupId};
use crate::terminal::TerminalManager;
use crate::ui::UiState;
use crate::ui::auxiliary_panel_host::DockedAuxiliaryPanelHost;
use dioxus::prelude::*;
use std::sync::Arc;

#[component]
pub(crate) fn SidebarPanelHost(
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
    rsx! {
        DockedAuxiliaryPanelHost {
            host: AuxiliaryPanelHost::SidePanel,
            app_state: app_state,
            ui_state: ui_state,
            terminal_manager: terminal_manager,
            emily_bridge: emily_bridge,
            group_id: group_id,
            group_orchestrator: group_orchestrator,
            active_group_path: active_group_path,
            repo_context: repo_context,
            repo_loading: repo_loading,
            git_refresh_nonce: git_refresh_nonce,
        }
    }
}
