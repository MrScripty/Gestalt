mod autosave;
mod command_palette;
mod commands_panel;
mod file_browser_panel;
mod git_helpers;
mod git_panel;
mod git_refresh;
mod insert_command_mode;
mod local_agent_panel;
mod sidebar_panel_host;
mod tab_rail;
mod terminal_input;
mod terminal_view;
mod workspace;

use crate::emily_bridge::EmilyBridge;
use crate::git::RepoContext;
use crate::persistence;
use crate::state::{SessionId, SessionStatus};
use crate::terminal::{PersistedTerminalState, TerminalManager, TerminalMemorySink};
use crate::ui::autosave::{AutosaveRequest, AutosaveSignature, AutosaveWorker};
use crate::ui::git_refresh::use_git_refresh_coordinator;
use crate::ui::insert_command_mode::InsertModeState;
use crate::ui::sidebar_panel_host::SidebarPanelKind;
use crate::ui::tab_rail::TabRail;
use crate::ui::terminal_input::measure_terminal_viewport;
use crate::ui::workspace::WorkspaceMain;
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

const STYLE: &str = concat!(
    include_str!("style/base.css"),
    include_str!("style/workspace.css"),
    include_str!("style/git_panel.css"),
    include_str!("style/commands_panel.css"),
    include_str!("style/file_browser_panel.css")
);
const TERMINAL_REFRESH_POLL_MS: u64 = 33;
const TERMINAL_RESIZE_POLL_MS: u64 = 180;
const TERMINAL_STARTUP_SYNC_POLL_MS: u64 = 250;
const AUTOSAVE_POLL_MS: u64 = 1_200;
const AUTOSAVE_PERSISTED_HISTORY_LINES: usize = 4_000;
const AUTOSAVE_QUEUE_CAPACITY: usize = 1;
const RAIL_WIDTH_DEFAULT_PX: i32 = 330;
const RAIL_WIDTH_MIN_PX: i32 = 240;
const RAIL_WIDTH_MAX_PX: i32 = 620;
const RAIL_SPLIT_STEP_PX: i32 = 16;
const SHELL_SPLITTER_SIZE_PX: i32 = 8;
const RUNNER_WIDTH_DEFAULT_PX: i32 = 340;
const SPLIT_RATIO_DEFAULT: f64 = 0.5;

/// Root desktop UI component.
#[component]
pub fn App() -> Element {
    let initial_workspace = use_signal(|| persistence::load_workspace().ok().flatten());
    let app_state = {
        let loaded = initial_workspace.read().clone();
        use_signal(move || {
            loaded
                .as_ref()
                .map(|workspace| workspace.app_state.clone().into_restored())
                .unwrap_or_default()
        })
    };
    let restored_terminals = {
        let loaded = initial_workspace.read().clone();
        use_signal(move || {
            loaded
                .as_ref()
                .map(|workspace| {
                    workspace
                        .terminals
                        .iter()
                        .cloned()
                        .map(|terminal| (terminal.session_id, terminal))
                        .collect::<HashMap<SessionId, PersistedTerminalState>>()
                })
                .unwrap_or_default()
        })
    };
    let dragging_tab = use_signal(|| None::<SessionId>);
    let emily_bridge = use_signal(|| Arc::new(EmilyBridge::new_default()));
    let terminal_manager = {
        let emily_bridge = emily_bridge.read().clone();
        use_signal(move || {
            let sink: Arc<dyn TerminalMemorySink> = emily_bridge.clone();
            Arc::new(TerminalManager::new_with_memory_sink(Some(sink)))
        })
    };
    let autosave_worker = {
        let loaded = initial_workspace.read().clone();
        use_signal(move || {
            let initial_fingerprint = loaded
                .as_ref()
                .and_then(|workspace| workspace.stable_fingerprint().ok());
            Arc::new(AutosaveWorker::spawn(
                AUTOSAVE_QUEUE_CAPACITY,
                initial_fingerprint,
            ))
        })
    };
    let new_group_path = use_signal(String::new);
    let persistence_feedback = use_signal(String::new);
    let refresh_tick = use_signal(|| 0_u64);
    let focused_terminal = use_signal(|| None::<SessionId>);
    let round_anchor = use_signal(|| None::<(SessionId, u16)>);
    let local_agent_command = use_signal(String::new);
    let local_agent_feedback = use_signal(String::new);
    let renaming_tab = use_signal(|| None::<SessionId>);
    let rename_draft = use_signal(String::new);
    let mut rail_width_px = use_signal(|| RAIL_WIDTH_DEFAULT_PX);
    let mut rail_drag_start = use_signal(|| None::<(f64, i32)>);
    let runner_width_px = use_signal(|| RUNNER_WIDTH_DEFAULT_PX);
    let agent_top_ratio = use_signal(|| SPLIT_RATIO_DEFAULT);
    let runner_top_ratio = use_signal(|| SPLIT_RATIO_DEFAULT);
    let git_context = use_signal(|| None::<RepoContext>);
    let git_context_loading = use_signal(|| false);
    let git_refresh_nonce = use_signal(|| 0_u64);
    let sidebar_panel = use_signal(|| SidebarPanelKind::Commands);
    let sidebar_open = use_signal(|| true);
    let insert_mode_state = use_signal(|| None::<InsertModeState>);

    {
        let mut refresh_tick = refresh_tick;
        let terminal_manager = terminal_manager.read().clone();
        use_future(move || {
            let terminal_manager = terminal_manager.clone();
            async move {
                let mut last_revisions = Vec::<(SessionId, u64)>::new();
                loop {
                    tokio::time::sleep(Duration::from_millis(TERMINAL_REFRESH_POLL_MS)).await;

                    let active_session_ids = {
                        let state = app_state.read();
                        state
                            .active_group_id()
                            .map(|group_id| state.session_ids_in_group(group_id))
                    };
                    let Some(active_session_ids) = active_session_ids else {
                        if !last_revisions.is_empty() {
                            last_revisions.clear();
                            let next = *refresh_tick.read() + 1;
                            refresh_tick.set(next);
                        }
                        continue;
                    };

                    let mut revisions = active_session_ids
                        .into_iter()
                        .map(|session_id| {
                            (
                                session_id,
                                terminal_manager
                                    .session_snapshot_revision(session_id)
                                    .unwrap_or(0),
                            )
                        })
                        .collect::<Vec<_>>();
                    revisions.sort_unstable_by_key(|(session_id, _)| *session_id);

                    if revisions != last_revisions {
                        last_revisions = revisions;
                        let next = *refresh_tick.read() + 1;
                        refresh_tick.set(next);
                    }
                }
            }
        });
    }

    {
        let terminal_manager = terminal_manager.read().clone();
        use_future(move || {
            let terminal_manager = terminal_manager.clone();
            async move {
                let mut last_sizes: HashMap<SessionId, (u16, u16)> = HashMap::new();

                loop {
                    tokio::time::sleep(Duration::from_millis(TERMINAL_RESIZE_POLL_MS)).await;

                    let active_session_ids = {
                        let state = app_state.read();
                        state
                            .active_group_id()
                            .map(|group_id| state.workspace_session_ids_for_group(group_id))
                    };
                    let Some(active_session_ids) = active_session_ids else {
                        last_sizes.clear();
                        continue;
                    };

                    let active_session_set: HashSet<SessionId> =
                        active_session_ids.iter().copied().collect();
                    last_sizes.retain(|session_id, _| active_session_set.contains(session_id));

                    for session_id in active_session_ids {
                        let body_id = format!("terminal-body-{session_id}");
                        let Some((rows, cols)) = measure_terminal_viewport(body_id).await else {
                            continue;
                        };

                        if last_sizes.get(&session_id).copied() == Some((rows, cols)) {
                            continue;
                        }

                        if terminal_manager
                            .resize_session(session_id, rows, cols)
                            .is_ok()
                        {
                            last_sizes.insert(session_id, (rows, cols));
                        }
                    }
                }
            }
        });
    }

    use_git_refresh_coordinator(
        app_state,
        git_context,
        git_context_loading,
        git_refresh_nonce,
    );

    {
        let mut app_state_signal = app_state;
        let terminal_manager = terminal_manager.read().clone();
        let emily_bridge = emily_bridge.read().clone();
        let mut restored_terminals = restored_terminals;
        use_future(move || {
            let terminal_manager = terminal_manager.clone();
            let emily_bridge = emily_bridge.clone();
            async move {
                let mut started_session_ids = HashSet::<SessionId>::new();

                loop {
                    tokio::time::sleep(Duration::from_millis(TERMINAL_STARTUP_SYNC_POLL_MS)).await;

                    let snapshot = app_state_signal.read().clone();
                    let active_session_ids = snapshot
                        .sessions
                        .iter()
                        .map(|session| session.id)
                        .collect::<HashSet<_>>();
                    started_session_ids
                        .retain(|session_id| active_session_ids.contains(session_id));

                    let mut failed_starts = Vec::new();
                    {
                        let mut restored = restored_terminals.write();
                        for session in &snapshot.sessions {
                            if started_session_ids.contains(&session.id) {
                                continue;
                            }

                            if let Some(mut restored_terminal) = restored.remove(&session.id) {
                                let emily_lines = emily_bridge
                                    .recent_lines(session.id, AUTOSAVE_PERSISTED_HISTORY_LINES);
                                if !emily_lines.is_empty() {
                                    restored_terminal.lines = emily_lines;
                                }
                                terminal_manager.seed_restored_terminal(restored_terminal);
                            }

                            if let Some(path) = snapshot.group_path(session.group_id) {
                                match terminal_manager.ensure_session(session.id, path) {
                                    Ok(()) => {
                                        started_session_ids.insert(session.id);
                                    }
                                    Err(_) => failed_starts.push(session.id),
                                }
                            }
                        }
                    }

                    if !failed_starts.is_empty() {
                        let mut state = app_state_signal.write();
                        for session_id in failed_starts {
                            state.set_session_status(session_id, SessionStatus::Error);
                        }
                    }
                }
            }
        });
    }

    {
        let terminal_manager = terminal_manager.read().clone();
        let autosave_worker = autosave_worker.read().clone();
        let mut persistence_feedback = persistence_feedback;
        use_future(move || {
            let terminal_manager = terminal_manager.clone();
            let autosave_worker = autosave_worker.clone();
            async move {
                let mut last_saved_signature = None::<AutosaveSignature>;
                let mut inflight_signature = None::<AutosaveSignature>;
                let mut deferred_request = None::<AutosaveRequest>;

                loop {
                    tokio::time::sleep(Duration::from_millis(AUTOSAVE_POLL_MS)).await;

                    for result in autosave_worker.drain_results() {
                        if result.error.is_none() {
                            last_saved_signature = Some(result.signature.clone());
                            persistence_feedback.set(String::new());
                        } else if let Some(error) = result.error {
                            persistence_feedback.set(error);
                        }

                        if inflight_signature.as_ref() == Some(&result.signature) {
                            inflight_signature = None;
                        }
                    }

                    if inflight_signature.is_none()
                        && let Some(request) = deferred_request.take()
                    {
                        match autosave_worker.try_enqueue(request.clone()) {
                            Ok(()) => {
                                inflight_signature = Some(request.signature);
                            }
                            Err(error) => {
                                deferred_request = Some(request);
                                persistence_feedback.set(error);
                            }
                        }
                    }

                    let state = app_state.read().clone();
                    let mut terminal_revisions = state
                        .sessions
                        .iter()
                        .map(|session| {
                            (
                                session.id,
                                terminal_manager
                                    .session_snapshot_revision(session.id)
                                    .unwrap_or(0),
                            )
                        })
                        .collect::<Vec<_>>();
                    terminal_revisions.sort_unstable_by_key(|(session_id, _)| *session_id);
                    let save_signature = (state.revision(), terminal_revisions);

                    if last_saved_signature.as_ref() == Some(&save_signature) {
                        continue;
                    }
                    if inflight_signature.as_ref() == Some(&save_signature) {
                        continue;
                    }
                    if deferred_request.as_ref().map(|request| &request.signature)
                        == Some(&save_signature)
                    {
                        continue;
                    }

                    let workspace = persistence::build_workspace_snapshot_with_history_limit(
                        &state,
                        &terminal_manager,
                        AUTOSAVE_PERSISTED_HISTORY_LINES,
                    );

                    let request = AutosaveRequest {
                        workspace,
                        signature: save_signature.clone(),
                    };

                    if inflight_signature.is_none() {
                        match autosave_worker.try_enqueue(request.clone()) {
                            Ok(()) => {
                                inflight_signature = Some(save_signature);
                            }
                            Err(error) => {
                                deferred_request = Some(request);
                                persistence_feedback.set(error);
                            }
                        }
                    } else {
                        deferred_request = Some(request);
                    }
                }
            }
        });
    }

    use_drop({
        let terminal_manager = terminal_manager.read().clone();
        let autosave_worker = autosave_worker.read().clone();
        move || {
            autosave_worker.shutdown();
            let state = app_state.read().clone();
            let workspace = persistence::build_workspace_snapshot(&state, &terminal_manager);
            let _ = persistence::save_workspace(&workspace);
        }
    });

    let shell_style = format!(
        "--rail-width: {}px; --splitter-size: {}px;",
        *rail_width_px.read(),
        SHELL_SPLITTER_SIZE_PX
    );
    let shell_class = if rail_drag_start.read().is_some() {
        "shell resizing"
    } else {
        "shell"
    };

    rsx! {
        style { "{STYLE}" }

        div {
            class: "{shell_class}",
            style: "{shell_style}",
            onmousemove: move |event| {
                let Some((start_x, start_width)) = *rail_drag_start.read() else {
                    return;
                };

                let pointer_x = event.data().client_coordinates().x;
                let next_width = (f64::from(start_width) + (pointer_x - start_x)).round() as i32;
                rail_width_px.set(next_width.clamp(RAIL_WIDTH_MIN_PX, RAIL_WIDTH_MAX_PX));
            },
            onmouseup: move |_| {
                rail_drag_start.set(None);
            },
            onmouseleave: move |_| {
                rail_drag_start.set(None);
            },
            TabRail {
                app_state: app_state,
                terminal_manager: terminal_manager,
                dragging_tab: dragging_tab,
                new_group_path: new_group_path,
                renaming_tab: renaming_tab,
                rename_draft: rename_draft,
            }
            button {
                class: "panel-splitter panel-splitter-vertical shell-splitter",
                r#type: "button",
                aria_label: "Resize tab rail",
                onmousedown: move |event| {
                    event.prevent_default();
                    let start_x = event.data().client_coordinates().x;
                    rail_drag_start.set(Some((start_x, *rail_width_px.read())));
                },
                onkeydown: move |event| {
                    match event.key() {
                        Key::ArrowLeft => {
                            event.prevent_default();
                            let next = *rail_width_px.read() - RAIL_SPLIT_STEP_PX;
                            rail_width_px.set(next.clamp(RAIL_WIDTH_MIN_PX, RAIL_WIDTH_MAX_PX));
                        }
                        Key::ArrowRight => {
                            event.prevent_default();
                            let next = *rail_width_px.read() + RAIL_SPLIT_STEP_PX;
                            rail_width_px.set(next.clamp(RAIL_WIDTH_MIN_PX, RAIL_WIDTH_MAX_PX));
                        }
                        _ => {}
                    }
                },
            }

            WorkspaceMain {
                app_state: app_state,
                terminal_manager: terminal_manager,
                focused_terminal: focused_terminal,
                round_anchor: round_anchor,
                local_agent_command: local_agent_command,
                local_agent_feedback: local_agent_feedback,
                persistence_feedback: persistence_feedback,
                refresh_tick: refresh_tick,
                runner_width_px: runner_width_px,
                agent_top_ratio: agent_top_ratio,
                runner_top_ratio: runner_top_ratio,
                git_context: git_context,
                git_context_loading: git_context_loading,
                git_refresh_nonce: git_refresh_nonce,
                sidebar_panel: sidebar_panel,
                sidebar_open: sidebar_open,
                insert_mode_state: insert_mode_state,
            }
        }
    }
}
