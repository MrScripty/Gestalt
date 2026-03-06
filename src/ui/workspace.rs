use crate::emily_bridge::EmilyBridge;
use crate::orchestrator::{self, GroupOrchestratorSnapshot};
use crate::resource_monitor::{RESOURCE_POLL_MS, ResourceSnapshot, sample_resource_snapshot};
use crate::state::{AppState, GroupLayout, SessionId, SessionStatus};
use crate::terminal::{TerminalManager, TerminalSnapshot};
use crate::ui::TerminalHistoryState;
use crate::ui::insert_command_mode::InsertModeState;
use crate::ui::local_agent_panel::LocalAgentPanel;
use crate::ui::sidebar_panel_host::{SidebarPanelHost, SidebarPanelKind};
use crate::ui::terminal_view::{
    TerminalInteractionSignals, pending_terminal_snapshot, terminal_shell,
};
use dioxus::prelude::*;
use emily::model::VectorizationStatus;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone)]
struct TerminalPaneData {
    terminal: Arc<TerminalSnapshot>,
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
const STATUS_RECONCILE_POLL_MS: u64 = 150;
const STATUS_BUSY_ACTIVITY_WINDOW_MS: i64 = 900;

fn run_sidebar_style_for_panel(_panel: SidebarPanelKind, ratio: f64) -> String {
    format!("--runner-top-ratio: {:.2}%;", ratio * 100.0)
}

#[component]
pub(crate) fn WorkspaceMain(
    app_state: Signal<AppState>,
    emily_bridge: Signal<Arc<EmilyBridge>>,
    vectorization_status: Signal<VectorizationStatus>,
    terminal_manager: Signal<Arc<TerminalManager>>,
    focused_terminal: Signal<Option<SessionId>>,
    round_anchor: Signal<Option<(SessionId, u16)>>,
    terminal_history_state: Signal<HashMap<SessionId, TerminalHistoryState>>,
    local_agent_command: Signal<String>,
    local_agent_feedback: Signal<String>,
    persistence_feedback: Signal<String>,
    refresh_tick: Signal<u64>,
    git_context: Signal<Option<crate::git::RepoContext>>,
    git_context_loading: Signal<bool>,
    git_refresh_nonce: Signal<u64>,
    sidebar_panel: Signal<SidebarPanelKind>,
    sidebar_open: Signal<bool>,
    insert_mode_state: Signal<Option<InsertModeState>>,
    on_open_embedding_settings: EventHandler<()>,
) -> Element {
    let _ = *refresh_tick.read();
    {
        let terminal_manager = terminal_manager.read().clone();
        use_future(move || {
            let terminal_manager = terminal_manager.clone();
            async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(STATUS_RECONCILE_POLL_MS)).await;

                    let now_ms = unix_now_ms();
                    let pending_updates = {
                        let state = app_state.read();
                        state
                            .sessions
                            .iter()
                            .filter_map(|session| {
                                let next = derive_session_status_from_activity(
                                    session.status,
                                    terminal_manager.session_last_activity_unix_ms(session.id),
                                    now_ms,
                                );
                                if next == session.status {
                                    None
                                } else {
                                    Some((session.id, next))
                                }
                            })
                            .collect::<Vec<_>>()
                    };

                    if pending_updates.is_empty() {
                        continue;
                    }

                    let mut state = app_state.write();
                    for (session_id, status) in pending_updates {
                        state.set_session_status(session_id, status);
                    }
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
    let resource_snapshot_value = resource_snapshot.read().clone();
    let vectorization_status_value = vectorization_status.read().clone();
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
            let runtime_snapshot = runtime.snapshot_shared(session.id);
            let is_runtime_ready = runtime_snapshot.is_some();
            let terminal =
                runtime_snapshot.unwrap_or_else(|| Arc::new(pending_terminal_snapshot()));
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
                    lines: &pane.terminal.lines,
                    cwd: pane.cwd.as_str(),
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

    let mut sidebar_open = sidebar_open;
    let persistence_feedback_value = persistence_feedback.read().clone();
    let mut runner_drag_start = use_signal(|| None::<(f64, i32)>);
    let mut agent_drag_start = use_signal(|| None::<(f64, f64)>);
    let mut sidebar_drag_start = use_signal(|| None::<(f64, f64)>);
    let mut renaming_header = use_signal(|| None::<SessionId>);
    let mut rename_header_draft = use_signal(String::new);
    let renaming_header_id = *renaming_header.read();
    let rename_header_draft_value = rename_header_draft.read().clone();

    let active_layout = active_group_id
        .map(|group_id| snapshot.group_layout(group_id))
        .unwrap_or_else(GroupLayout::default);
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
                            let next = !*sidebar_open.read();
                            sidebar_open.set(next);
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
                                        let session_id = session.id;
                                        let selected = snapshot.selected_session == Some(session_id);
                                        let terminal_is_focused = focused_terminal_id == Some(session_id);
                                        let pane_class = if selected {
                                            "terminal-card agent selected"
                                        } else {
                                            "terminal-card agent"
                                        };
                                        let card_style = format!("border-top-color: var({});", session.status.css_var());
                                        let pane = terminal_snapshot_by_id
                                            .get(&session_id)
                                            .cloned()
                                            .unwrap_or_else(|| TerminalPaneData {
                                                terminal: Arc::new(pending_terminal_snapshot()),
                                                cwd: snapshot
                                                    .group_path(session.group_id)
                                                    .unwrap_or(".")
                                                    .to_string(),
                                                is_runtime_ready: false,
                                            });
                                        let terminal = pane.terminal;
                                        let cwd = pane.cwd;
                                        let terminal_manager_for_input = terminal_manager.read().clone();
                                        let emily_bridge_for_history = emily_bridge.read().clone();
                                        let is_renaming_header = renaming_header_id == Some(session_id);
                                        let title_for_header_start = session.title.clone();
                                        let rename_header_aria = format!("Rename terminal {}", session.title);

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
                                                                "{session.title}"
                                                            }
                                                        }
                                                        p { class: "terminal-meta", "cwd: {cwd}" }
                                                    }
                                                }

                                                {terminal_shell(
                                                    session_id,
                                                    terminal_is_focused,
                                                    terminal,
                                                    terminal_manager_for_input,
                                                    emily_bridge_for_history,
                                                    terminal_history_state,
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
                                        let session_id = session.id;
                                        let selected = snapshot.selected_session == Some(session_id);
                                        let terminal_is_focused = focused_terminal_id == Some(session_id);
                                        let pane_class = if selected {
                                            "terminal-card runner selected"
                                        } else {
                                            "terminal-card runner"
                                        };
                                        let card_style = format!("border-top-color: var({});", session.status.css_var());
                                        let pane = terminal_snapshot_by_id
                                            .get(&session_id)
                                            .cloned()
                                            .unwrap_or_else(|| TerminalPaneData {
                                                terminal: Arc::new(pending_terminal_snapshot()),
                                                cwd: snapshot
                                                    .group_path(session.group_id)
                                                    .unwrap_or(".")
                                                    .to_string(),
                                                is_runtime_ready: false,
                                            });
                                        let terminal = pane.terminal;
                                        let cwd = pane.cwd;
                                        let terminal_manager_for_input = terminal_manager.read().clone();
                                        let emily_bridge_for_history = emily_bridge.read().clone();
                                        let is_renaming_header = renaming_header_id == Some(session_id);
                                        let title_for_header_start = session.title.clone();
                                        let rename_header_aria = format!("Rename terminal {}", session.title);

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
                                                                "{session.title}"
                                                            }
                                                        }
                                                        p { class: "terminal-meta", "cwd: {cwd}" }
                                                    }
                                                }

                                                {terminal_shell(
                                                    session_id,
                                                    terminal_is_focused,
                                                    terminal,
                                                    terminal_manager_for_input,
                                                    emily_bridge_for_history,
                                                    terminal_history_state,
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

fn derive_session_status_from_activity(
    current_status: SessionStatus,
    last_activity_unix_ms: Option<i64>,
    now_unix_ms: i64,
) -> SessionStatus {
    let is_recent = is_recent_activity(last_activity_unix_ms, now_unix_ms);
    match current_status {
        SessionStatus::Error => {
            if is_recent {
                SessionStatus::Busy
            } else if last_activity_unix_ms.unwrap_or(0) > 0 {
                SessionStatus::Idle
            } else {
                SessionStatus::Error
            }
        }
        SessionStatus::Busy | SessionStatus::Idle => {
            if is_recent {
                SessionStatus::Busy
            } else {
                SessionStatus::Idle
            }
        }
    }
}

fn is_recent_activity(last_activity_unix_ms: Option<i64>, now_unix_ms: i64) -> bool {
    let Some(last_activity_unix_ms) = last_activity_unix_ms else {
        return false;
    };
    if last_activity_unix_ms <= 0 {
        return false;
    }
    if now_unix_ms < last_activity_unix_ms {
        return true;
    }
    now_unix_ms - last_activity_unix_ms <= STATUS_BUSY_ACTIVITY_WINDOW_MS
}

fn unix_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
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

#[cfg(test)]
mod tests {
    use super::{
        derive_session_status_from_activity, format_bytes_compact, percent_used,
        run_sidebar_style_for_panel,
    };
    use crate::state::SessionStatus;
    use crate::ui::sidebar_panel_host::SidebarPanelKind;

    #[test]
    fn sidebar_layout_style_is_panel_agnostic() {
        let ratio = 0.57;
        let local = run_sidebar_style_for_panel(SidebarPanelKind::LocalAgent, ratio);
        let commands = run_sidebar_style_for_panel(SidebarPanelKind::Commands, ratio);
        let git = run_sidebar_style_for_panel(SidebarPanelKind::Git, ratio);
        let files = run_sidebar_style_for_panel(SidebarPanelKind::FileBrowser, ratio);

        assert_eq!(local, commands);
        assert_eq!(local, git);
        assert_eq!(local, files);
    }

    #[test]
    fn format_bytes_compact_uses_binary_units() {
        assert_eq!(format_bytes_compact(500), "500 B");
        assert_eq!(format_bytes_compact(2_048), "2.0 KiB");
        assert_eq!(format_bytes_compact(1_572_864), "1.5 MiB");
    }

    #[test]
    fn percent_used_returns_zero_for_missing_total() {
        assert_eq!(percent_used(1024, 0), 0.0);
    }

    #[test]
    fn active_runtime_marks_session_busy() {
        let now = 5_000;
        let status = derive_session_status_from_activity(SessionStatus::Idle, Some(now - 200), now);
        assert_eq!(status, SessionStatus::Busy);
    }

    #[test]
    fn stale_activity_marks_session_idle() {
        let now = 10_000;
        let status =
            derive_session_status_from_activity(SessionStatus::Busy, Some(now - 5_000), now);
        assert_eq!(status, SessionStatus::Idle);
    }

    #[test]
    fn error_without_runtime_activity_stays_error() {
        let now = 10_000;
        let status = derive_session_status_from_activity(SessionStatus::Error, None, now);
        assert_eq!(status, SessionStatus::Error);
    }

    #[test]
    fn error_clears_after_runtime_activity() {
        let now = 10_000;
        let status =
            derive_session_status_from_activity(SessionStatus::Error, Some(now - 100), now);
        assert_eq!(status, SessionStatus::Busy);
    }
}
