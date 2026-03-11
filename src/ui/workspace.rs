use crate::emily_bridge::EmilyBridge;
use crate::orchestrator::{self, GroupOrchestratorSnapshot};
use crate::resource_monitor::ResourceSnapshot;
use crate::state::{AppState, AuxiliaryPanelKind, SessionId, WorkspaceState};
use crate::terminal::TerminalManager;
use crate::ui::UiState;
use crate::ui::run_sidebar_panel_host::RunSidebarPanelHost;
use crate::ui::sidebar_panel_host::SidebarPanelHost;
use crate::ui::terminal_view::{SnippetHotkeyState, TerminalInteractionSignals, terminal_shell};
use dioxus::prelude::*;
use emily::model::VectorizationStatus;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

const RUNNER_WIDTH_MIN_PX: i32 = 260;
const RUNNER_WIDTH_MAX_PX: i32 = 760;
const RUNNER_WIDTH_STEP_PX: i32 = 16;
const SIDE_PANEL_WIDTH_PX: i32 = 380;
const STACK_SPLIT_STEP_RATIO: f64 = 0.03;
const STACK_SPLIT_MIN_RATIO: f64 = 0.28;
const STACK_SPLIT_MAX_RATIO: f64 = 0.72;
const STACK_SPLIT_DRAG_SENSITIVITY_PX: f64 = 520.0;

fn run_sidebar_style(ratio: f64) -> String {
    format!("--runner-top-ratio: {:.2}%;", ratio * 100.0)
}

fn workspace_state_snapshot(app_state: Signal<AppState>) -> WorkspaceState {
    let state = app_state.read();
    state.workspace_state().clone()
}

#[component]
pub(crate) fn WorkspaceMain(
    app_state: Signal<AppState>,
    ui_state: Signal<UiState>,
    emily_bridge: Signal<Arc<EmilyBridge>>,
    vectorization_status: Signal<VectorizationStatus>,
    terminal_manager: Signal<Arc<TerminalManager>>,
    resource_snapshot: Signal<ResourceSnapshot>,
    refresh_tick: Signal<u64>,
    git_context: Signal<Option<crate::git::RepoContext>>,
    git_context_loading: Signal<bool>,
    git_refresh_nonce: Signal<u64>,
    on_open_embedding_settings: EventHandler<()>,
) -> Element {
    let _ = *refresh_tick.read();
    {
        let terminal_manager = terminal_manager.read().clone();
        use_future(move || {
            let terminal_manager = terminal_manager.clone();
            async move {
                let mut events = terminal_manager.subscribe_events();
                let mut idle_deadlines = HashMap::<SessionId, tokio::time::Instant>::new();
                let workspace_snapshot = workspace_state_snapshot(app_state);
                let updates = orchestrator::reconcile_session_statuses(
                    &workspace_snapshot,
                    &terminal_manager,
                    &mut idle_deadlines,
                );
                apply_status_updates(app_state, updates);

                loop {
                    if let Some(next_deadline) = idle_deadlines.values().min().copied() {
                        tokio::select! {
                            event = events.recv() => {
                                match event {
                                    Ok(event) => {
                                        if event.kind == crate::terminal::TerminalEventKind::Activity {
                                            let workspace_snapshot = workspace_state_snapshot(app_state);
                                            let updates = orchestrator::apply_session_activity(
                                                &workspace_snapshot,
                                                &terminal_manager,
                                                event.session_id,
                                                &mut idle_deadlines,
                                            )
                                            .into_iter()
                                            .collect();
                                            apply_status_updates(app_state, updates);
                                        }
                                    }
                                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                        let workspace_snapshot = workspace_state_snapshot(app_state);
                                        let updates = orchestrator::reconcile_session_statuses(
                                            &workspace_snapshot,
                                            &terminal_manager,
                                            &mut idle_deadlines,
                                        );
                                        apply_status_updates(app_state, updates);
                                    }
                                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                                }
                            }
                            _ = tokio::time::sleep_until(next_deadline) => {
                                let workspace_snapshot = workspace_state_snapshot(app_state);
                                let updates = orchestrator::reconcile_session_statuses(
                                    &workspace_snapshot,
                                    &terminal_manager,
                                    &mut idle_deadlines,
                                );
                                apply_status_updates(app_state, updates);
                            }
                        }
                    } else {
                        match events.recv().await {
                            Ok(event)
                                if event.kind == crate::terminal::TerminalEventKind::Activity =>
                            {
                                let workspace_snapshot = workspace_state_snapshot(app_state);
                                let updates = orchestrator::apply_session_activity(
                                    &workspace_snapshot,
                                    &terminal_manager,
                                    event.session_id,
                                    &mut idle_deadlines,
                                )
                                .into_iter()
                                .collect();
                                apply_status_updates(app_state, updates);
                            }
                            Ok(_) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                let workspace_snapshot = workspace_state_snapshot(app_state);
                                let updates = orchestrator::reconcile_session_statuses(
                                    &workspace_snapshot,
                                    &terminal_manager,
                                    &mut idle_deadlines,
                                );
                                apply_status_updates(app_state, updates);
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            }
        });
    }

    let resource_snapshot_value = resource_snapshot.read().clone();
    let vectorization_status_value = vectorization_status.read().clone();
    let snapshot = app_state.read().clone();
    let focused_terminal_id = ui_state.read().focused_terminal;
    let terminal_manager_for_projection = terminal_manager.read().clone();
    let workspace_projection = orchestrator::active_workspace_projection(
        snapshot.workspace_state(),
        &terminal_manager_for_projection,
        focused_terminal_id,
    );
    let busy_count = workspace_projection
        .as_ref()
        .map(|projection| projection.status_counts.busy)
        .unwrap_or(0);
    let error_count = workspace_projection
        .as_ref()
        .map(|projection| projection.status_counts.error)
        .unwrap_or(0);
    let idle_count = workspace_projection
        .as_ref()
        .map(|projection| projection.status_counts.idle)
        .unwrap_or(0);
    let active_group_id = workspace_projection
        .as_ref()
        .map(|projection| projection.group_id);
    let active_path = workspace_projection
        .as_ref()
        .map(|projection| projection.group_path.clone())
        .unwrap_or_else(|| ".".to_string());
    let active_agents = workspace_projection
        .as_ref()
        .map(|projection| projection.agents.clone())
        .unwrap_or_default();
    let active_runner = workspace_projection
        .as_ref()
        .and_then(|projection| projection.runner.clone());
    let orchestrator_snapshot: Option<GroupOrchestratorSnapshot> = workspace_projection
        .as_ref()
        .map(|projection| projection.orchestrator.clone());

    let mut ui_state = ui_state;
    let persistence_feedback_value = ui_state.read().persistence_feedback.clone();
    let mut runner_drag_start = use_signal(|| None::<(f64, i32)>);
    let mut agent_drag_start = use_signal(|| None::<(f64, f64)>);
    let mut sidebar_drag_start = use_signal(|| None::<(f64, f64)>);
    let mut renaming_header = use_signal(|| None::<SessionId>);
    let mut rename_header_draft = use_signal(String::new);
    let snippet_hotkey_state = use_signal(|| None::<SnippetHotkeyState>);
    let dragging_auxiliary_panel = use_signal(|| None::<AuxiliaryPanelKind>);
    let renaming_header_id = *renaming_header.read();
    let rename_header_draft_value = rename_header_draft.read().clone();

    let active_layout = workspace_projection
        .as_ref()
        .map(|projection| projection.layout)
        .unwrap_or_default();
    let runner_width = active_layout
        .runner_width_px
        .clamp(RUNNER_WIDTH_MIN_PX, RUNNER_WIDTH_MAX_PX);
    let agent_ratio = active_layout
        .agent_top_ratio
        .clamp(STACK_SPLIT_MIN_RATIO, STACK_SPLIT_MAX_RATIO);
    let sidebar_ratio = active_layout
        .runner_top_ratio
        .clamp(STACK_SPLIT_MIN_RATIO, STACK_SPLIT_MAX_RATIO);
    let workspace_layout_style =
        format!("--runner-width: {runner_width}px; --side-panel-width: {SIDE_PANEL_WIDTH_PX}px;");
    let sidebar_open_value = ui_state.read().sidebar_open;
    let crt_enabled = snapshot.crt_enabled();
    let workspace_layout_class = if sidebar_open_value {
        "workspace-layout with-side-panel"
    } else {
        "workspace-layout"
    };
    let agent_stack_style = format!("--agent-top-ratio: {:.2}%;", agent_ratio * 100.0);
    let run_sidebar_style = run_sidebar_style(sidebar_ratio);
    let interaction = TerminalInteractionSignals {
        app_state,
        ui_state,
        snippet_hotkey_state,
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
                    if let Some(group_id) = active_group_id {
                        app_state
                            .write()
                            .set_group_runner_width_px(group_id, next_width);
                    }
                }

                if let Some((start_y, start_ratio)) = *agent_drag_start.read() {
                    let delta_y = pointer.y - start_y;
                    let next_ratio = (start_ratio + (delta_y / STACK_SPLIT_DRAG_SENSITIVITY_PX))
                        .clamp(STACK_SPLIT_MIN_RATIO, STACK_SPLIT_MAX_RATIO);
                    if let Some(group_id) = active_group_id {
                        app_state
                            .write()
                            .set_group_agent_top_ratio(group_id, next_ratio);
                    }
                }

                if let Some((start_y, start_ratio)) = *sidebar_drag_start.read() {
                    let delta_y = pointer.y - start_y;
                    let next_ratio = (start_ratio + (delta_y / STACK_SPLIT_DRAG_SENSITIVITY_PX))
                        .clamp(STACK_SPLIT_MIN_RATIO, STACK_SPLIT_MAX_RATIO);
                    if let Some(group_id) = active_group_id {
                        app_state
                            .write()
                            .set_group_runner_top_ratio(group_id, next_ratio);
                    }
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
                    if !persistence_feedback_value.is_empty() {
                        p { class: "meta-tip", "{persistence_feedback_value}" }
                    }
                }

                div { class: "workspace-head-controls",
                    div { class: "status-summary",
                        span { class: "badge idle", "Idle {idle_count}" }
                        span { class: "badge busy", "Busy {busy_count}" }
                        span { class: "badge error", "Error {error_count}" }
                        {
                            let cpu_badge_class = metric_badge_class(resource_snapshot_value.system_cpu_percent);
                            let memory_percent = percent_used(
                                resource_snapshot_value.memory_used_bytes,
                                resource_snapshot_value.memory_total_bytes,
                            );
                            let memory_badge_class = metric_badge_class(memory_percent);
                            let memory_used = format_bytes_compact(resource_snapshot_value.memory_used_bytes);
                            let memory_total = format_bytes_compact(resource_snapshot_value.memory_total_bytes);
                            rsx! {
                                span {
                                    class: "badge {cpu_badge_class}",
                                    title: "System CPU usage sampled from native OS counters",
                                    "CPU {resource_snapshot_value.system_cpu_percent:.0}%"
                                }
                                span {
                                    class: "badge {memory_badge_class}",
                                    title: "System memory usage sampled from native OS counters",
                                    "RAM {memory_used}/{memory_total}"
                                }
                                span {
                                    class: "badge {vectorization_badge_class(&vectorization_status_value)}",
                                    title: "Emily vectorization status",
                                    "{vectorization_badge_label(&vectorization_status_value)}"
                                }
                            }
                        }
                    }
                    button {
                        class: "side-panel-toggle",
                        r#type: "button",
                        aria_pressed: crt_enabled,
                        title: "Toggle CRT mode (Ctrl+1)",
                        onclick: move |_| {
                            let next = !app_state.read().crt_enabled();
                            app_state.write().set_crt_enabled(next);
                        },
                        if crt_enabled { "CRT On" } else { "CRT Off" }
                    }
                    button {
                        class: "icon-button",
                        r#type: "button",
                        title: "Open embedding settings",
                        aria_label: "Open embedding settings",
                        onclick: move |_| {
                            on_open_embedding_settings.call(());
                        },
                        "⚙"
                    }
                    button {
                        class: "side-panel-toggle",
                        r#type: "button",
                        aria_controls: "workspace-right-panel",
                        aria_expanded: sidebar_open_value,
                        onclick: move |_| {
                            let next = !ui_state.read().sidebar_open;
                            ui_state.write().sidebar_open = next;
                        },
                        if sidebar_open_value { "Hide Panels" } else { "Show Panels" }
                    }
                }
            }

            if let Some(group_id) = active_group_id {
                {
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
                                        let session_id = session.session.id;
                                        let selected = session.is_selected;
                                        let terminal_is_focused = session.is_focused;
                                        let pane_class = if selected {
                                            "terminal-card agent selected"
                                        } else {
                                            "terminal-card agent"
                                        };
                                        let card_style = format!(
                                            "border-top-color: var({});",
                                            session.session.status.css_var()
                                        );
                                        let terminal = session.terminal.clone();
                                        let cwd = format_cwd_for_display(
                                            &session.cwd,
                                            active_path.as_str(),
                                        );
                                        let terminal_manager_for_input = terminal_manager.read().clone();
                                        let emily_bridge_for_history = emily_bridge.read().clone();
                                        let is_renaming_header = renaming_header_id == Some(session_id);
                                        let title_for_header_start = session.session.title.clone();
                                        let rename_header_aria =
                                            format!("Rename terminal {}", session.session.title);

                                        rsx! {
                                            article {
                                                class: "{pane_class}",
                                                key: "agent-card-{session_id}",
                                                style: "{card_style}",
                                                onclick: move |_| app_state.write().select_session(session_id),

                                                div { class: "terminal-head",
                                                    div {
                                                        if is_renaming_header {
                                                            input {
                                                                class: "terminal-title-input",
                                                                aria_label: "{rename_header_aria}",
                                                                value: "{rename_header_draft_value}",
                                                                oninput: move |event| rename_header_draft.set(event.value()),
                                                                onkeydown: move |event| {
                                                                    match event.key() {
                                                                        Key::Enter => {
                                                                            event.prevent_default();
                                                                            let title = rename_header_draft.read().trim().to_string();
                                                                            if !title.is_empty() {
                                                                                app_state.write().rename_session(session_id, title);
                                                                            }
                                                                            renaming_header.set(None);
                                                                        }
                                                                        Key::Escape => {
                                                                            event.prevent_default();
                                                                            renaming_header.set(None);
                                                                        }
                                                                        _ => {}
                                                                    }
                                                                },
                                                                onblur: move |_| {
                                                                    let was_editing = *renaming_header.read() == Some(session_id);
                                                                    if was_editing {
                                                                        let title = rename_header_draft.read().trim().to_string();
                                                                        if !title.is_empty() {
                                                                            app_state.write().rename_session(session_id, title);
                                                                        }
                                                                        renaming_header.set(None);
                                                                    }
                                                                },
                                                                oncontextmenu: move |event| {
                                                                    event.stop_propagation();
                                                                },
                                                            }
                                                        } else {
                                                            h4 {
                                                                ondoubleclick: move |event| {
                                                                    event.stop_propagation();
                                                                    renaming_header.set(Some(session_id));
                                                                    rename_header_draft.set(title_for_header_start.clone());
                                                                },
                                                                "{session.session.title}"
                                                            }
                                                        }
                                                        p { class: "terminal-meta", "cwd: {cwd}" }
                                                    }
                                                }

                                                {terminal_shell(
                                                    session_id,
                                                    cwd.clone(),
                                                    terminal_is_focused,
                                                    terminal,
                                                    terminal_manager_for_input,
                                                    emily_bridge_for_history,
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
                                                        let start_ratio = app_state
                                                            .read()
                                                            .group_layout(group_id)
                                                            .agent_top_ratio;
                                                        agent_drag_start
                                                            .set(Some((start_y, start_ratio)));
                                                    },
                                                    onkeydown: move |event| {
                                                        match event.key() {
                                                            Key::ArrowUp => {
                                                                event.prevent_default();
                                                                let next = app_state
                                                                    .read()
                                                                    .group_layout(group_id)
                                                                    .agent_top_ratio
                                                                    - STACK_SPLIT_STEP_RATIO;
                                                                app_state
                                                                    .write()
                                                                    .set_group_agent_top_ratio(
                                                                        group_id, next,
                                                                    );
                                                            }
                                                            Key::ArrowDown => {
                                                                event.prevent_default();
                                                                let next = app_state
                                                                    .read()
                                                                    .group_layout(group_id)
                                                                    .agent_top_ratio
                                                                    + STACK_SPLIT_STEP_RATIO;
                                                                app_state
                                                                    .write()
                                                                    .set_group_agent_top_ratio(
                                                                        group_id, next,
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
                                    let start_width =
                                        app_state.read().group_layout(group_id).runner_width_px;
                                    runner_drag_start.set(Some((start_x, start_width)));
                                },
                                onkeydown: move |event| {
                                    match event.key() {
                                        Key::ArrowLeft => {
                                            event.prevent_default();
                                            let next = app_state
                                                .read()
                                                .group_layout(group_id)
                                                .runner_width_px
                                                + RUNNER_WIDTH_STEP_PX;
                                            app_state
                                                .write()
                                                .set_group_runner_width_px(group_id, next);
                                        }
                                        Key::ArrowRight => {
                                            event.prevent_default();
                                            let next = app_state
                                                .read()
                                                .group_layout(group_id)
                                                .runner_width_px
                                                - RUNNER_WIDTH_STEP_PX;
                                            app_state
                                                .write()
                                                .set_group_runner_width_px(group_id, next);
                                        }
                                        _ => {}
                                    }
                                },
                            }

                            aside { class: "run-sidebar split-enabled", style: "{run_sidebar_style}",
                                if let Some(session) = active_runner {
                                    {
                                        let session_id = session.session.id;
                                        let selected = session.is_selected;
                                        let terminal_is_focused = session.is_focused;
                                        let pane_class = if selected {
                                            "terminal-card runner selected"
                                        } else {
                                            "terminal-card runner"
                                        };
                                        let card_style = format!(
                                            "border-top-color: var({});",
                                            session.session.status.css_var()
                                        );
                                        let terminal = session.terminal.clone();
                                        let cwd = format_cwd_for_display(
                                            &session.cwd,
                                            active_path.as_str(),
                                        );
                                        let terminal_manager_for_input = terminal_manager.read().clone();
                                        let emily_bridge_for_history = emily_bridge.read().clone();
                                        let is_renaming_header = renaming_header_id == Some(session_id);
                                        let title_for_header_start = session.session.title.clone();
                                        let rename_header_aria =
                                            format!("Rename terminal {}", session.session.title);

                                        rsx! {
                                            article {
                                                class: "{pane_class}",
                                                key: "runner-card-{session_id}",
                                                style: "{card_style}",
                                                onclick: move |_| app_state.write().select_session(session_id),

                                                div { class: "terminal-head",
                                                    div {
                                                        if is_renaming_header {
                                                            input {
                                                                class: "terminal-title-input",
                                                                aria_label: "{rename_header_aria}",
                                                                value: "{rename_header_draft_value}",
                                                                oninput: move |event| rename_header_draft.set(event.value()),
                                                                onkeydown: move |event| {
                                                                    match event.key() {
                                                                        Key::Enter => {
                                                                            event.prevent_default();
                                                                            let title = rename_header_draft.read().trim().to_string();
                                                                            if !title.is_empty() {
                                                                                app_state.write().rename_session(session_id, title);
                                                                            }
                                                                            renaming_header.set(None);
                                                                        }
                                                                        Key::Escape => {
                                                                            event.prevent_default();
                                                                            renaming_header.set(None);
                                                                        }
                                                                        _ => {}
                                                                    }
                                                                },
                                                                onblur: move |_| {
                                                                    let was_editing = *renaming_header.read() == Some(session_id);
                                                                    if was_editing {
                                                                        let title = rename_header_draft.read().trim().to_string();
                                                                        if !title.is_empty() {
                                                                            app_state.write().rename_session(session_id, title);
                                                                        }
                                                                        renaming_header.set(None);
                                                                    }
                                                                },
                                                                oncontextmenu: move |event| {
                                                                    event.stop_propagation();
                                                                },
                                                            }
                                                        } else {
                                                            h4 {
                                                                ondoubleclick: move |event| {
                                                                    event.stop_propagation();
                                                                    renaming_header.set(Some(session_id));
                                                                    rename_header_draft.set(title_for_header_start.clone());
                                                                },
                                                                "{session.session.title}"
                                                            }
                                                        }
                                                        p { class: "terminal-meta", "cwd: {cwd}" }
                                                    }
                                                }

                                                {terminal_shell(
                                                    session_id,
                                                    cwd.clone(),
                                                    terminal_is_focused,
                                                    terminal,
                                                    terminal_manager_for_input,
                                                    emily_bridge_for_history,
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
                                        let start_ratio = app_state
                                            .read()
                                            .group_layout(group_id)
                                            .runner_top_ratio;
                                        sidebar_drag_start.set(Some((start_y, start_ratio)));
                                    },
                                    onkeydown: move |event| {
                                        match event.key() {
                                            Key::ArrowUp => {
                                                event.prevent_default();
                                                let next = app_state
                                                    .read()
                                                    .group_layout(group_id)
                                                    .runner_top_ratio
                                                    - STACK_SPLIT_STEP_RATIO;
                                                app_state
                                                    .write()
                                                    .set_group_runner_top_ratio(group_id, next);
                                            }
                                            Key::ArrowDown => {
                                                event.prevent_default();
                                                let next = app_state
                                                    .read()
                                                    .group_layout(group_id)
                                                    .runner_top_ratio
                                                    + STACK_SPLIT_STEP_RATIO;
                                                app_state
                                                    .write()
                                                    .set_group_runner_top_ratio(group_id, next);
                                            }
                                            _ => {}
                                        }
                                    },
                                }

                                RunSidebarPanelHost {
                                    app_state: app_state,
                                    ui_state: ui_state,
                                    terminal_manager: terminal_manager,
                                    emily_bridge: emily_bridge,
                                    group_id: group_id,
                                    active_group_path: active_path.clone(),
                                    group_orchestrator: orchestrator_snapshot.clone(),
                                    repo_context: git_context,
                                    repo_loading: git_context_loading,
                                    git_refresh_nonce: git_refresh_nonce,
                                    dragging_panel: dragging_auxiliary_panel,
                                }
                            }

                            if sidebar_open_value {
                                div { class: "workspace-divider workspace-divider-static", aria_hidden: "true" }

                                aside { id: "workspace-right-panel", class: "workspace-side-panel",
                                    SidebarPanelHost {
                                        app_state: app_state,
                                        ui_state: ui_state,
                                        terminal_manager: terminal_manager,
                                        emily_bridge: emily_bridge,
                                        group_id: group_id,
                                        group_orchestrator: orchestrator_snapshot.clone(),
                                        active_group_path: active_path.clone(),
                                        repo_context: git_context,
                                        repo_loading: git_context_loading,
                                        git_refresh_nonce: git_refresh_nonce,
                                        dragging_panel: dragging_auxiliary_panel,
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

fn metric_badge_class(percent: f32) -> &'static str {
    if percent >= 90.0 {
        "error"
    } else if percent >= 70.0 {
        "busy"
    } else {
        "idle"
    }
}

fn vectorization_badge_class(status: &VectorizationStatus) -> &'static str {
    if status.active_job.is_some() {
        "busy"
    } else if status.config.enabled && status.provider_available {
        "idle"
    } else {
        "error"
    }
}

fn vectorization_badge_label(status: &VectorizationStatus) -> String {
    if let Some(job) = status.active_job.as_ref() {
        return format!("EMB RUN {}/{}", job.vectorized, job.processed);
    }
    if status.config.enabled && status.provider_available {
        return format!("EMB ON {}", status.config.profile_id);
    }
    if status.config.enabled {
        return "EMB ON (NO PROVIDER)".to_string();
    }
    "EMB OFF".to_string()
}

fn apply_status_updates(
    mut app_state: Signal<AppState>,
    updates: Vec<orchestrator::SessionStatusUpdate>,
) {
    if updates.is_empty() {
        return;
    }

    let mut state = app_state.write();
    for update in updates {
        state.set_session_status(update.session_id, update.status);
    }
}

fn percent_used(used: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    ((used as f64 / total as f64) * 100.0) as f32
}

fn format_bytes_compact(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    const TIB: f64 = GIB * 1024.0;
    let bytes_f64 = bytes as f64;

    if bytes_f64 >= TIB {
        format!("{:.1} TiB", bytes_f64 / TIB)
    } else if bytes_f64 >= GIB {
        format!("{:.1} GiB", bytes_f64 / GIB)
    } else if bytes_f64 >= MIB {
        format!("{:.1} MiB", bytes_f64 / MIB)
    } else if bytes_f64 >= KIB {
        format!("{:.1} KiB", bytes_f64 / KIB)
    } else {
        format!("{bytes} B")
    }
}

fn format_cwd_for_display(cwd: &str, workspace_path: &str) -> String {
    let cwd_path = Path::new(cwd);
    let workspace_root = Path::new(workspace_path);
    match cwd_path.strip_prefix(workspace_root) {
        Ok(relative) if relative.as_os_str().is_empty() => ".".to_string(),
        Ok(relative) => relative.to_string_lossy().into_owned(),
        Err(_) => cwd.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{format_bytes_compact, format_cwd_for_display, percent_used, run_sidebar_style};

    #[test]
    fn sidebar_layout_style_uses_only_ratio() {
        let ratio = 0.57;
        assert_eq!(run_sidebar_style(ratio), "--runner-top-ratio: 57.00%;");
    }

    #[test]
    fn format_bytes_compact_uses_binary_units() {
        assert_eq!(format_bytes_compact(500), "500 B");
        assert_eq!(format_bytes_compact(2_048), "2.0 KiB");
        assert_eq!(format_bytes_compact(1_572_864), "1.5 MiB");
    }

    #[test]
    fn format_cwd_for_display_returns_dot_for_workspace_root() {
        let root = std::env::temp_dir().join("gestalt-cwd-root");
        let root_str = root.to_string_lossy().into_owned();
        assert_eq!(format_cwd_for_display(&root_str, &root_str), ".");
    }

    #[test]
    fn format_cwd_for_display_strips_workspace_prefix() {
        let root = std::env::temp_dir().join("gestalt-cwd-root");
        let cwd = root.join("src").join("ui");
        let expected = std::path::PathBuf::from("src")
            .join("ui")
            .to_string_lossy()
            .into_owned();
        let root_str = root.to_string_lossy().into_owned();
        let cwd_str = cwd.to_string_lossy().into_owned();
        assert_eq!(format_cwd_for_display(&cwd_str, &root_str), expected);
    }

    #[test]
    fn format_cwd_for_display_keeps_absolute_for_non_descendant() {
        let root = std::env::temp_dir().join("gestalt-cwd-root");
        let outside = std::env::temp_dir().join("gestalt-cwd-outside");
        let root_str = root.to_string_lossy().into_owned();
        let outside_str = outside.to_string_lossy().into_owned();
        assert_eq!(format_cwd_for_display(&outside_str, &root_str), outside_str);
    }

    #[test]
    fn percent_used_returns_zero_for_missing_total() {
        assert_eq!(percent_used(1024, 0), 0.0);
    }
}
