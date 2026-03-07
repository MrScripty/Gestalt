mod autosave;
mod command_palette;
mod commands_panel;
mod file_browser_panel;
mod file_browser_scan;
mod git_helpers;
mod git_panel;
mod git_refresh;
mod insert_command_mode;
mod local_agent_panel;
mod notes_panel;
mod run_sidebar_panel_host;
mod sidebar_panel_host;
mod tab_rail;
mod terminal_input;
mod terminal_view;
mod workspace;

use crate::emily_bridge::EmilyBridge;
use crate::git::RepoContext;
use crate::local_restore;
use crate::orchestrator::{self, STARTUP_BACKGROUND_TICK_MS, StartupCoordinator};
use crate::pantograph_host::build_deferred_embedding_provider_from_env;
use crate::persistence;
use crate::resource_monitor::{RESOURCE_POLL_MS, ResourceSnapshot, sample_resource_snapshot};
use crate::state::{SessionId, SessionStatus, clamp_ui_scale};
use crate::terminal::{PersistedTerminalState, TerminalManager, TerminalMemorySink};
use crate::ui::autosave::{AutosaveRequest, AutosaveSignature, AutosaveWorker};
use crate::ui::git_refresh::use_git_refresh_coordinator;
use crate::ui::insert_command_mode::InsertModeState;
use crate::ui::sidebar_panel_host::SidebarPanelKind;
use crate::ui::tab_rail::TabRail;
use crate::ui::terminal_input::measure_terminal_viewport;
use crate::ui::workspace::WorkspaceMain;
use dioxus::prelude::*;
use emily::model::{VectorizationConfigPatch, VectorizationRunRequest};
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
const AUTOSAVE_DEBOUNCE_MS: u64 = 1_200;
pub(crate) const EMILY_HISTORY_BACKFILL_PAGE_LINES: usize = 1_200;
const TERMINAL_MIN_RESIZE_COLS: u16 = 80;
const AUTOSAVE_QUEUE_CAPACITY: usize = 1;
const RAIL_WIDTH_DEFAULT_PX: i32 = 330;
const RAIL_WIDTH_MIN_PX: i32 = 240;
const RAIL_WIDTH_MAX_PX: i32 = 620;
const RAIL_SPLIT_STEP_PX: i32 = 16;
const SHELL_SPLITTER_SIZE_PX: i32 = 8;
const GUI_SCALE_STEP: f64 = 0.1;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TerminalHistoryState {
    pub before_sequence: Option<u64>,
    pub is_loading: bool,
    pub exhausted: bool,
}

/// Root desktop UI component.
#[component]
pub fn App() -> Element {
    let initial_workspace = use_signal(|| persistence::load_workspace().ok().flatten());
    let mut app_state = {
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
            let projection_map = local_restore::load_projection_map().unwrap_or_default();
            loaded
                .as_ref()
                .map(|workspace| {
                    let mut map = workspace
                        .terminals
                        .iter()
                        .cloned()
                        .map(|mut terminal| {
                            terminal.lines.clear();
                            (terminal.session_id, terminal)
                        })
                        .collect::<HashMap<SessionId, PersistedTerminalState>>();
                    for (session_id, terminal) in &mut map {
                        if let Some(projection) = projection_map.get(session_id) {
                            terminal.cwd = projection.cwd.clone();
                            terminal.rows = projection.rows;
                            terminal.cols = projection.cols;
                            terminal.cursor_row = projection.cursor_row;
                            terminal.cursor_col = projection.cursor_col;
                            terminal.hide_cursor = projection.hide_cursor;
                            terminal.bracketed_paste = projection.bracketed_paste;
                        }
                    }
                    map
                })
                .unwrap_or_default()
        })
    };
    let dragging_tab = use_signal(|| None::<SessionId>);
    let startup_notify = use_signal(|| Arc::new(tokio::sync::Notify::new()));
    let emily_bridge = use_signal(|| Arc::new(initialize_emily_bridge()));
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
    let autosave_dirty_notify = use_signal(|| Arc::new(tokio::sync::Notify::new()));
    let git_refresh_notify = use_signal(|| Arc::new(tokio::sync::Notify::new()));
    let refresh_tick = use_signal(|| 0_u64);
    let focused_terminal = use_signal(|| None::<SessionId>);
    let round_anchor = use_signal(|| None::<(SessionId, u16)>);
    let local_agent_command = use_signal(String::new);
    let local_agent_feedback = use_signal(String::new);
    let renaming_tab = use_signal(|| None::<SessionId>);
    let rename_draft = use_signal(String::new);
    let mut rail_width_px = use_signal(|| RAIL_WIDTH_DEFAULT_PX);
    let mut rail_drag_start = use_signal(|| None::<(f64, i32)>);
    let git_context = use_signal(|| None::<RepoContext>);
    let git_context_loading = use_signal(|| false);
    let git_refresh_nonce = use_signal(|| 0_u64);
    let sidebar_panel = use_signal(|| SidebarPanelKind::Commands);
    let sidebar_open = use_signal(|| true);
    let insert_mode_state = use_signal(|| None::<InsertModeState>);
    let terminal_history_state =
        use_signal(std::collections::HashMap::<SessionId, TerminalHistoryState>::new);
    let mut embedding_settings_open = use_signal(|| false);
    let mut embedding_profile_draft = use_signal(String::new);
    let mut embedding_feedback = use_signal(String::new);
    let vectorization_status = {
        let emily_bridge = emily_bridge.read().clone();
        use_signal(move || emily_bridge.vectorization_status())
    };

    {
        let autosave_dirty_notify = autosave_dirty_notify.read().clone();
        use_effect(move || {
            let _ = app_state.read().revision();
            autosave_dirty_notify.notify_one();
        });
    }

    {
        let git_refresh_notify = git_refresh_notify.read().clone();
        use_effect(move || {
            let state = app_state.read();
            let _ = state
                .groups
                .iter()
                .map(|group| group.path.clone())
                .collect::<Vec<_>>();
            let _ = state
                .active_group_id()
                .and_then(|group_id| state.group_path(group_id))
                .map(ToString::to_string);
            git_refresh_notify.notify_one();
        });
    }

    {
        let git_refresh_notify = git_refresh_notify.read().clone();
        use_effect(move || {
            let _ = *git_refresh_nonce.read();
            git_refresh_notify.notify_one();
        });
    }

    {
        let emily_bridge = emily_bridge.read().clone();
        let mut vectorization_status = vectorization_status;
        use_future(move || {
            let emily_bridge = emily_bridge.clone();
            async move {
                let mut receiver = emily_bridge.subscribe_vectorization_status();
                loop {
                    if receiver.changed().await.is_err() {
                        break;
                    }
                    vectorization_status.set(receiver.borrow().clone());
                }
            }
        });
    }

    let mut resource_snapshot = use_signal(ResourceSnapshot::default);
    {
        let terminal_manager = terminal_manager.read().clone();
        use_future(move || {
            let terminal_manager = terminal_manager.clone();
            async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(RESOURCE_POLL_MS)).await;
                    let session_roots = {
                        let state = app_state.read();
                        state
                            .sessions
                            .iter()
                            .filter_map(|session| {
                                terminal_manager
                                    .session_process_id(session.id)
                                    .map(|pid| (session.id, pid))
                            })
                            .collect::<Vec<_>>()
                    };
                    resource_snapshot.set(sample_resource_snapshot(&session_roots));
                }
            }
        });
    }

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
                        let cols = cols.max(TERMINAL_MIN_RESIZE_COLS);

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
        git_refresh_notify.read().clone(),
    );

    {
        let mut app_state_signal = app_state;
        let terminal_manager = terminal_manager.read().clone();
        let emily_bridge = emily_bridge.read().clone();
        let mut restored_terminals = restored_terminals;
        let mut terminal_history_state = terminal_history_state;
        let startup_notify = startup_notify.read().clone();
        use_future(move || {
            let terminal_manager = terminal_manager.clone();
            let emily_bridge = emily_bridge.clone();
            let startup_notify = startup_notify.clone();
            async move {
                let mut startup_coordinator = StartupCoordinator::new();
                let mut first_pass = true;

                loop {
                    if first_pass {
                        first_pass = false;
                    } else {
                        let snapshot = app_state_signal.read().clone();
                        if startup_coordinator.has_deferred_targets(&snapshot) {
                            tokio::select! {
                                _ = startup_notify.notified() => {}
                                _ = tokio::time::sleep(Duration::from_millis(STARTUP_BACKGROUND_TICK_MS)) => {}
                            }
                        } else {
                            startup_notify.notified().await;
                        }
                    }

                    let snapshot = app_state_signal.read().clone();
                    let active_session_ids = snapshot
                        .sessions
                        .iter()
                        .map(|session| session.id)
                        .collect::<HashSet<_>>();
                    terminal_history_state
                        .write()
                        .retain(|session_id, _| active_session_ids.contains(session_id));

                    apply_history_load_updates(
                        terminal_history_state,
                        startup_coordinator
                            .load_pending_history(&snapshot, &emily_bridge, &terminal_manager)
                            .await,
                    );

                    let result = {
                        let mut restored = restored_terminals.write();
                        startup_coordinator.start_due_sessions(
                            &snapshot,
                            &terminal_manager,
                            &mut restored,
                        )
                    };
                    apply_history_load_updates(terminal_history_state, result.history_updates);

                    if !result.failed_session_ids.is_empty() {
                        let mut state = app_state_signal.write();
                        for session_id in result.failed_session_ids {
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
        let autosave_dirty_notify = autosave_dirty_notify.read().clone();
        let mut persistence_feedback = persistence_feedback;
        use_future(move || {
            let terminal_manager = terminal_manager.clone();
            let autosave_worker = autosave_worker.clone();
            let autosave_dirty_notify = autosave_dirty_notify.clone();
            async move {
                let mut terminal_events = terminal_manager.subscribe_events();
                let mut last_saved_signature = None::<AutosaveSignature>;
                let mut inflight_signature = None::<AutosaveSignature>;
                let mut deferred_request = None::<AutosaveRequest>;
                let mut save_deadline = None::<tokio::time::Instant>;

                loop {
                    if let Some(deadline) = save_deadline {
                        tokio::select! {
                            result = autosave_worker.recv_result() => {
                                if let Some(result) = result {
                                    if result.error.is_none() {
                                        last_saved_signature = Some(result.signature.clone());
                                        persistence_feedback.set(String::new());
                                    } else if let Some(error) = result.error {
                                        persistence_feedback.set(error);
                                    }

                                    if inflight_signature.as_ref() == Some(&result.signature) {
                                        inflight_signature = None;
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
                                }
                            }
                            terminal_event = terminal_events.recv() => {
                                match terminal_event {
                                    Ok(event)
                                        if event.kind == crate::terminal::TerminalEventKind::SnapshotChanged =>
                                    {
                                        save_deadline = Some(
                                            tokio::time::Instant::now()
                                                + Duration::from_millis(AUTOSAVE_DEBOUNCE_MS),
                                        );
                                    }
                                    Ok(_) => {}
                                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                        save_deadline = Some(
                                            tokio::time::Instant::now()
                                                + Duration::from_millis(AUTOSAVE_DEBOUNCE_MS),
                                        );
                                    }
                                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                                }
                            }
                            _ = autosave_dirty_notify.notified() => {
                                save_deadline = Some(
                                    tokio::time::Instant::now()
                                        + Duration::from_millis(AUTOSAVE_DEBOUNCE_MS),
                                );
                            }
                            _ = tokio::time::sleep_until(deadline) => {
                                save_deadline = None;
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

                                let terminal_manager_for_snapshot = terminal_manager.clone();
                                let workspace = match tokio::task::spawn_blocking(move || {
                                    persistence::build_workspace_snapshot(
                                        &state,
                                        &terminal_manager_for_snapshot,
                                    )
                                })
                                .await
                                {
                                    Ok(workspace) => workspace,
                                    Err(error) => {
                                        persistence_feedback
                                            .set(format!("Autosave snapshot build failed: {error}"));
                                        continue;
                                    }
                                };

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
                    } else {
                        tokio::select! {
                            result = autosave_worker.recv_result() => {
                                if let Some(result) = result {
                                    if result.error.is_none() {
                                        last_saved_signature = Some(result.signature.clone());
                                        persistence_feedback.set(String::new());
                                    } else if let Some(error) = result.error {
                                        persistence_feedback.set(error);
                                    }

                                    if inflight_signature.as_ref() == Some(&result.signature) {
                                        inflight_signature = None;
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
                                }
                            }
                            terminal_event = terminal_events.recv() => {
                                match terminal_event {
                                    Ok(event)
                                        if event.kind == crate::terminal::TerminalEventKind::SnapshotChanged =>
                                    {
                                        save_deadline = Some(
                                            tokio::time::Instant::now()
                                                + Duration::from_millis(AUTOSAVE_DEBOUNCE_MS),
                                        );
                                    }
                                    Ok(_) => {}
                                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                        save_deadline = Some(
                                            tokio::time::Instant::now()
                                                + Duration::from_millis(AUTOSAVE_DEBOUNCE_MS),
                                        );
                                    }
                                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                                }
                            }
                            _ = autosave_dirty_notify.notified() => {
                                save_deadline = Some(
                                    tokio::time::Instant::now()
                                        + Duration::from_millis(AUTOSAVE_DEBOUNCE_MS),
                                );
                            }
                        }
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
            let _ = local_restore::save_projection(&workspace.terminals);
            let _ = persistence::save_workspace(&workspace);
        }
    });

    let shell_style = format!(
        "--rail-width: {}px; --splitter-size: {}px; --font-scale: {:.2};",
        *rail_width_px.read(),
        SHELL_SPLITTER_SIZE_PX,
        app_state.read().ui_scale(),
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
            onkeydown: move |event| {
                let data = event.data();
                let key = data.key();
                let modifiers = data.modifiers();
                if let Some(direction) =
                    gui_scale_direction(&key, modifiers.ctrl(), modifiers.meta(), modifiers.alt())
                {
                    event.prevent_default();
                    event.stop_propagation();
                    let current_scale = app_state.read().ui_scale();
                    let next_scale = next_gui_scale(current_scale, direction);
                    app_state.write().set_ui_scale(next_scale);
                }
            },
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
                resource_snapshot: resource_snapshot,
                on_startup_nudge: move |_| startup_notify.read().notify_one(),
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
                emily_bridge: emily_bridge,
                vectorization_status: vectorization_status,
                terminal_manager: terminal_manager,
                resource_snapshot: resource_snapshot,
                focused_terminal: focused_terminal,
                round_anchor: round_anchor,
                terminal_history_state: terminal_history_state,
                local_agent_command: local_agent_command,
                local_agent_feedback: local_agent_feedback,
                persistence_feedback: persistence_feedback,
                refresh_tick: refresh_tick,
                git_context: git_context,
                git_context_loading: git_context_loading,
                git_refresh_nonce: git_refresh_nonce,
                sidebar_panel: sidebar_panel,
                sidebar_open: sidebar_open,
                insert_mode_state: insert_mode_state,
                on_open_embedding_settings: move |_| {
                    let status = vectorization_status.read().clone();
                    embedding_profile_draft.set(status.config.profile_id.clone());
                    embedding_feedback.set(String::new());
                    embedding_settings_open.set(true);
                },
            }
            if *embedding_settings_open.read() {
                div {
                    class: "dialog-overlay",
                    onclick: move |_| {
                        embedding_settings_open.set(false);
                    },
                    div {
                        class: "dialog-card",
                        onclick: move |event| {
                            event.stop_propagation();
                        },
                        h3 { "Embedding Settings" }
                        {
                            let status = vectorization_status.read().clone();
                            let enabled = status.config.enabled;
                            let provider_label = if status.provider_available {
                                "available"
                            } else {
                                "not available"
                            };
                            let provider_status = status.provider_status.clone();
                            let active_job = status.active_job.clone();
                            let last_job = status.last_job.clone();
                            let feedback = embedding_feedback.read().clone();
                            rsx! {
                                p {
                                    class: "meta-tip",
                                    "Provider: {provider_label}"
                                }
                                if let Some(provider_status) = provider_status {
                                    p { class: "meta-tip", "Provider state: {provider_status.state}" }
                                    if let Some(session_id) = provider_status.session_id {
                                        p { class: "meta-tip", "Session: {session_id}" }
                                    }
                                    if let Some(queue_items) = provider_status.queue_items {
                                        p { class: "meta-tip", "Queued items: {queue_items}" }
                                    }
                                    if let Some(queued_runs) = provider_status.queued_runs {
                                        p { class: "meta-tip", "Queued runs: {queued_runs}" }
                                    }
                                    if let Some(last_error) = provider_status.last_error {
                                        p { class: "meta-tip", "Provider error: {last_error}" }
                                    }
                                }
                                p {
                                    class: "meta-tip",
                                    "Current profile: {status.config.profile_id}"
                                }
                                p {
                                    class: "meta-tip",
                                    "Dimensions: {status.config.expected_dimensions}"
                                }
                                if let Some(job) = active_job {
                                    {
                                        let job_summary = format!(
                                            "Active job: {:?} {}/{}, failed {}",
                                            job.kind, job.vectorized, job.processed, job.failed
                                        );
                                        rsx! {
                                            p { class: "meta-tip", "{job_summary}" }
                                        }
                                    }
                                } else if let Some(job) = last_job {
                                    {
                                        let job_summary = format!(
                                            "Last job: {:?} ({:?}) {}/{}, failed {}",
                                            job.kind, job.state, job.vectorized, job.processed, job.failed
                                        );
                                        rsx! {
                                            p { class: "meta-tip", "{job_summary}" }
                                        }
                                    }
                                }
                                if !feedback.is_empty() {
                                    p {
                                        class: "meta-tip",
                                        "{feedback}"
                                    }
                                }
                                div { class: "dialog-field",
                                    label { r#for: "embedding-profile", "Embedding Profile" }
                                    input {
                                        id: "embedding-profile",
                                        value: "{embedding_profile_draft.read()}",
                                        oninput: move |event| {
                                            embedding_profile_draft.set(event.value());
                                        }
                                    }
                                }
                                div { class: "dialog-actions",
                                    button {
                                        r#type: "button",
                                        onclick: move |_| {
                                            let current = vectorization_status.read().config.enabled;
                                            let emily_bridge = emily_bridge.read().clone();
                                            embedding_feedback.set("Updating embedding state...".to_string());
                                            spawn(async move {
                                                let result = emily_bridge
                                                    .update_vectorization_config_async(VectorizationConfigPatch {
                                                        enabled: Some(!current),
                                                        ..VectorizationConfigPatch::default()
                                                    })
                                                    .await;
                                                match result {
                                                    Ok(config) => {
                                                        embedding_feedback.set(format!(
                                                            "Embedding {}",
                                                            if config.enabled { "enabled" } else { "disabled" }
                                                        ));
                                                    }
                                                    Err(error) => embedding_feedback.set(error),
                                                }
                                            });
                                        },
                                        if enabled { "Disable Embedding" } else { "Enable Embedding" }
                                    }
                                    button {
                                        r#type: "button",
                                        onclick: move |_| {
                                            let profile_id = embedding_profile_draft.read().trim().to_string();
                                            let emily_bridge = emily_bridge.read().clone();
                                            embedding_feedback.set("Saving embedding profile...".to_string());
                                            spawn(async move {
                                                let result = emily_bridge
                                                    .update_vectorization_config_async(VectorizationConfigPatch {
                                                        profile_id: Some(profile_id),
                                                        ..VectorizationConfigPatch::default()
                                                    })
                                                    .await;
                                                match result {
                                                    Ok(config) => {
                                                        embedding_feedback.set(format!(
                                                            "Profile saved: {}",
                                                            config.profile_id
                                                        ));
                                                    }
                                                    Err(error) => embedding_feedback.set(error),
                                                }
                                            });
                                        },
                                        "Save Profile"
                                    }
                                    button {
                                        r#type: "button",
                                        onclick: move |_| {
                                            let emily_bridge = emily_bridge.read().clone();
                                            embedding_feedback.set("Starting backfill...".to_string());
                                            spawn(async move {
                                                let result = emily_bridge
                                                    .start_backfill_async(VectorizationRunRequest { stream_id: None })
                                                    .await;
                                                match result {
                                                    Ok(job) => embedding_feedback.set(format!(
                                                        "Backfill started: {}",
                                                        job.job_id
                                                    )),
                                                    Err(error) => embedding_feedback.set(error),
                                                }
                                            });
                                        },
                                        "Backfill Missing"
                                    }
                                    button {
                                        r#type: "button",
                                        onclick: move |_| {
                                            let emily_bridge = emily_bridge.read().clone();
                                            embedding_feedback.set("Starting revectorize...".to_string());
                                            spawn(async move {
                                                let result = emily_bridge
                                                    .start_revectorize_async(VectorizationRunRequest { stream_id: None })
                                                    .await;
                                                match result {
                                                    Ok(job) => embedding_feedback.set(format!(
                                                        "Revectorize started: {}",
                                                        job.job_id
                                                    )),
                                                    Err(error) => embedding_feedback.set(error),
                                                }
                                            });
                                        },
                                        "Revectorize All"
                                    }
                                    if let Some(job) = vectorization_status.read().active_job.clone() {
                                        button {
                                            r#type: "button",
                                            onclick: move |_| {
                                                let emily_bridge = emily_bridge.read().clone();
                                                let job_id = job.job_id.clone();
                                                embedding_feedback.set("Cancelling job...".to_string());
                                                spawn(async move {
                                                    let result = emily_bridge
                                                        .cancel_vectorization_job_async(job_id)
                                                        .await;
                                                    match result {
                                                        Ok(()) => embedding_feedback.set("Cancellation requested".to_string()),
                                                        Err(error) => embedding_feedback.set(error),
                                                    }
                                                });
                                            },
                                            "Cancel Job"
                                        }
                                    }
                                    button {
                                        r#type: "button",
                                        onclick: move |_| {
                                            embedding_settings_open.set(false);
                                        },
                                        "Close"
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

fn apply_history_load_updates(
    mut terminal_history_state: Signal<HashMap<SessionId, TerminalHistoryState>>,
    updates: Vec<orchestrator::HistoryLoadUpdate>,
) {
    if updates.is_empty() {
        return;
    }

    let mut history_state = terminal_history_state.write();
    for update in updates {
        history_state.insert(
            update.session_id,
            TerminalHistoryState {
                before_sequence: update.state.before_sequence,
                is_loading: update.state.is_loading,
                exhausted: update.state.exhausted,
            },
        );
    }
}

fn gui_scale_direction(key: &Key, ctrl: bool, meta: bool, alt: bool) -> Option<f64> {
    if (!ctrl && !meta) || alt {
        return None;
    }

    match key {
        Key::Character(text) if text == "+" || text == "=" => Some(GUI_SCALE_STEP),
        Key::Character(text) if text == "-" || text == "_" => Some(-GUI_SCALE_STEP),
        _ => None,
    }
}

fn initialize_emily_bridge() -> EmilyBridge {
    match build_deferred_embedding_provider_from_env() {
        Ok(provider) => EmilyBridge::new_default_with_embedding_provider(provider),
        Err(error) => {
            eprintln!("Pantograph embedding provider unavailable: {error}");
            EmilyBridge::new_default_with_provider_error(error)
        }
    }
}

fn next_gui_scale(current: f64, step: f64) -> f64 {
    let next = clamp_ui_scale(current + step);
    (next * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::{gui_scale_direction, next_gui_scale};
    use dioxus::prelude::Key;

    #[test]
    fn recognizes_ctrl_plus_for_zoom_in() {
        assert_eq!(
            gui_scale_direction(&Key::Character("+".to_string()), true, false, false),
            Some(0.1)
        );
    }

    #[test]
    fn recognizes_ctrl_minus_for_zoom_out() {
        assert_eq!(
            gui_scale_direction(&Key::Character("-".to_string()), true, false, false),
            Some(-0.1)
        );
    }

    #[test]
    fn clamps_scale_to_bounds() {
        assert_eq!(next_gui_scale(1.8, 0.1), 1.8);
        assert_eq!(next_gui_scale(0.7, -0.1), 0.7);
    }
}
