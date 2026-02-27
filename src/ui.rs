use crate::state::{AppState, SessionId, SessionStatus};
use crate::terminal::{TerminalManager, TerminalSnapshot};
use dioxus::document;
use dioxus::events::KeyboardEvent;
use dioxus::prelude::*;
use dioxus::prelude::{InteractionLocation, Key, ModifiersInteraction};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const STYLE: &str = include_str!("style.css");
const READ_CLIPBOARD_JS: &str = r#"
if (navigator.clipboard && navigator.clipboard.readText) {
    try {
        return await navigator.clipboard.readText();
    } catch (_) {}
}
return "";
"#;
const COPY_SELECTION_JS: &str = r#"
const selected = window.getSelection ? window.getSelection().toString() : "";
if (!selected) {
    return false;
}
if (navigator.clipboard && navigator.clipboard.writeText) {
    try {
        await navigator.clipboard.writeText(selected);
        return true;
    } catch (_) {}
}
return false;
"#;
const TERMINAL_RESIZE_POLL_MS: u64 = 180;

#[derive(Clone)]
struct TerminalPaneData {
    terminal: TerminalSnapshot,
    cwd: String,
}

#[component]
pub fn App() -> Element {
    let mut app_state = use_signal(AppState::default);
    let mut dragging_tab = use_signal(|| None::<SessionId>);
    let terminal_manager = use_signal(|| Arc::new(Mutex::new(TerminalManager::new())));
    let mut new_group_path = use_signal(String::new);
    let refresh_tick = use_signal(|| 0_u64);
    let focused_terminal = use_signal(|| None::<SessionId>);
    let round_anchor = use_signal(|| None::<(SessionId, u16)>);
    let mut renaming_tab = use_signal(|| None::<SessionId>);
    let mut rename_draft = use_signal(String::new);

    {
        let mut refresh_tick = refresh_tick;
        use_future(move || async move {
            loop {
                tokio::time::sleep(Duration::from_millis(33)).await;
                let next = *refresh_tick.read() + 1;
                refresh_tick.set(next);
            }
        });
    }

    {
        let app_state = app_state;
        let terminal_manager = terminal_manager.read().clone();
        use_future(move || {
            let terminal_manager = terminal_manager.clone();
            async move {
                let mut last_sizes: HashMap<SessionId, (u16, u16)> = HashMap::new();

                loop {
                    tokio::time::sleep(Duration::from_millis(TERMINAL_RESIZE_POLL_MS)).await;

                    let snapshot = app_state.read().clone();
                    let Some(group_id) = snapshot.active_group_id() else {
                        last_sizes.clear();
                        continue;
                    };

                    let (agents, runner) = snapshot.workspace_sessions_for_group(group_id);
                    let mut active_session_ids: Vec<SessionId> =
                        agents.into_iter().map(|session| session.id).collect();
                    if let Some(runner) = runner {
                        active_session_ids.push(runner.id);
                    }

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

                        if let Ok(mut runtime) = terminal_manager.lock() {
                            if runtime.resize_session(session_id, rows, cols).is_ok() {
                                last_sizes.insert(session_id, (rows, cols));
                            }
                        }
                    }
                }
            }
        });
    }

    let _ = *refresh_tick.read();

    let snapshot = app_state.read().clone();

    let failed_starts = {
        let mut failures = Vec::new();
        if let Ok(mut runtime) = terminal_manager.read().lock() {
            for session in &snapshot.sessions {
                if let Some(path) = snapshot.group_path(session.group_id) {
                    if runtime.ensure_session(session.id, path).is_err() {
                        failures.push(session.id);
                    }
                }
            }
        }
        failures
    };

    if !failed_starts.is_empty() {
        let mut state = app_state.write();
        for session_id in failed_starts {
            state.set_session_status(session_id, SessionStatus::Error);
        }
    }

    let busy_count = snapshot.session_count_by_status(SessionStatus::Busy);
    let error_count = snapshot.session_count_by_status(SessionStatus::Error);
    let idle_count = snapshot.session_count_by_status(SessionStatus::Idle);
    let focused_terminal_id = *focused_terminal.read();
    let renaming_tab_id = *renaming_tab.read();
    let rename_draft_value = rename_draft.read().clone();

    let active_group_id = snapshot.active_group_id();
    let (active_agents, active_runner) = active_group_id
        .map(|group_id| snapshot.workspace_sessions_for_group(group_id))
        .unwrap_or_default();
    let active_path = active_group_id
        .and_then(|group_id| snapshot.group_path(group_id))
        .unwrap_or(".")
        .to_string();

    let mut workspace_sessions = active_agents.clone();
    if let Some(runner) = active_runner.clone() {
        workspace_sessions.push(runner);
    }

    let terminal_snapshot_by_id: HashMap<SessionId, TerminalPaneData> = {
        let mut panes = HashMap::new();
        if let Ok(runtime) = terminal_manager.read().lock() {
            for session in &workspace_sessions {
                let terminal = runtime
                    .snapshot(session.id)
                    .unwrap_or_else(pending_terminal_snapshot);
                let cwd = runtime
                    .session_cwd(session.id)
                    .unwrap_or_else(|| snapshot.group_path(session.group_id).unwrap_or("."))
                    .to_string();
                panes.insert(session.id, TerminalPaneData { terminal, cwd });
            }
        }
        panes
    };

    let new_group_path_value = new_group_path.read().clone();

    rsx! {
        style { "{STYLE}" }

        div { class: "shell",
            aside { class: "tab-rail",
                div { class: "brand",
                    h1 { "Gestalt" }
                    p { "Path-grouped terminal fleet manager" }
                }

                div { class: "group-list",
                    for group in snapshot.groups.clone() {
                        {
                            let group_id = group.id;
                            let group_color = group.color.clone();
                            let group_path = group.path.clone();
                            let group_label = group.label();
                            let terminal_manager_for_add = terminal_manager.read().clone();
                            let terminal_manager_for_drop = terminal_manager.read().clone();

                            let sessions_in_group: Vec<_> = snapshot
                                .sessions
                                .iter()
                                .filter(|session| session.group_id == group_id)
                                .cloned()
                                .collect();

                            rsx! {
                                section {
                                    class: "group",
                                    key: "group-{group_id}",

                                    div {
                                        class: "group-header",
                                        style: "border-left-color: {group_color};",
                                        div {
                                            h3 { "{group_label}" }
                                            p { class: "group-path", "{group_path}" }
                                        }
                                        button {
                                            class: "pill-btn",
                                            onclick: move |_| {
                                                let (session_id, path) = {
                                                    let mut state = app_state.write();
                                                    let id = state.add_session(group_id);
                                                    state.select_session(id);
                                                    let path = state.group_path(group_id).unwrap_or(".").to_string();
                                                    (id, path)
                                                };

                                                let start_error = if let Ok(mut runtime) = terminal_manager_for_add.lock() {
                                                    runtime.ensure_session(session_id, &path).err()
                                                } else {
                                                    Some("terminal manager lock failed".to_string())
                                                };

                                                if start_error.is_some() {
                                                    app_state.write().set_session_status(session_id, SessionStatus::Error);
                                                }
                                            },
                                            "+ tab"
                                        }
                                    }

                                    ul { class: "tab-list",
                                        for session in sessions_in_group {
                                            {
                                                let session_id = session.id;
                                                let selected = snapshot.selected_session == Some(session_id);
                                                let is_runner = session.role.is_runner();
                                                let tab_class = if selected {
                                                    if is_runner { "tab active role-run" } else { "tab active role-agent" }
                                                } else if is_runner {
                                                    "tab role-run"
                                                } else {
                                                    "tab role-agent"
                                                };
                                                let status_style = format!("background: var({});", session.status.css_var());
                                                let target_path = snapshot.group_path(session.group_id).unwrap_or(".").to_string();
                                                let terminal_manager_for_reorder = terminal_manager.read().clone();
                                                let is_renaming = renaming_tab_id == Some(session_id);
                                                let title_for_start = session.title.clone();

                                                rsx! {
                                                    li {
                                                        class: "{tab_class}",
                                                        key: "session-{session_id}",
                                                        draggable: "true",
                                                        ondragstart: move |_| {
                                                            dragging_tab.set(Some(session_id));
                                                        },
                                                        ondragend: move |_| {
                                                            dragging_tab.set(None);
                                                        },
                                                        ondragover: move |event| {
                                                            event.prevent_default();
                                                        },
                                                        ondrop: move |event| {
                                                            event.prevent_default();
                                                            if let Some(source_id) = *dragging_tab.read() {
                                                                app_state.write().move_session_before(source_id, session_id);
                                                                if let Ok(mut runtime) = terminal_manager_for_reorder.lock() {
                                                                    let _ = runtime.set_cwd(source_id, &target_path);
                                                                }
                                                            }
                                                            dragging_tab.set(None);
                                                        },
                                                        onclick: move |_| app_state.write().select_session(session_id),
                                                        ondoubleclick: move |_| {
                                                            renaming_tab.set(Some(session_id));
                                                            rename_draft.set(title_for_start.clone());
                                                        },

                                                        span { class: "status-dot", style: "{status_style}" }

                                                        if is_renaming {
                                                            input {
                                                                class: "tab-rename-input",
                                                                value: "{rename_draft_value}",
                                                                oninput: move |event| rename_draft.set(event.value()),
                                                                onkeydown: move |event| {
                                                                    match event.key() {
                                                                        Key::Enter => {
                                                                            event.prevent_default();
                                                                            let title = rename_draft.read().trim().to_string();
                                                                            if !title.is_empty() {
                                                                                app_state.write().rename_session(session_id, title);
                                                                            }
                                                                            renaming_tab.set(None);
                                                                        }
                                                                        Key::Escape => {
                                                                            event.prevent_default();
                                                                            renaming_tab.set(None);
                                                                        }
                                                                        _ => {}
                                                                    }
                                                                },
                                                                onblur: move |_| {
                                                                    let was_editing = *renaming_tab.read() == Some(session_id);
                                                                    if was_editing {
                                                                        let title = rename_draft.read().trim().to_string();
                                                                        if !title.is_empty() {
                                                                            app_state.write().rename_session(session_id, title);
                                                                        }
                                                                        renaming_tab.set(None);
                                                                    }
                                                                }
                                                            }
                                                        } else {
                                                            div { class: "tab-main",
                                                                span { class: "title", "{session.title}" }
                                                                span { class: "role-pill", "{session.role.badge()}" }
                                                            }
                                                        }

                                                        div { class: "tab-actions",
                                                            span { class: "status-text", "{session.status.label()}" }
                                                            button {
                                                                class: "rename-btn",
                                                                onclick: move |event| {
                                                                    event.stop_propagation();
                                                                    renaming_tab.set(Some(session_id));
                                                                    rename_draft.set(session.title.clone());
                                                                },
                                                                "rename"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    div {
                                        class: "group-drop-target",
                                        ondragover: move |event| {
                                            event.prevent_default();
                                        },
                                        ondrop: move |event| {
                                            event.prevent_default();
                                            if let Some(source_id) = *dragging_tab.read() {
                                                app_state.write().move_session_to_group_end(source_id, group_id);
                                                if let Ok(mut runtime) = terminal_manager_for_drop.lock() {
                                                    let _ = runtime.set_cwd(source_id, &group_path);
                                                }
                                            }
                                            dragging_tab.set(None);
                                        },
                                        "Drop tab here to move into this path"
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "rail-footer",
                    p { class: "meta-label", "New Path Group" }
                    input {
                        class: "path-input",
                        placeholder: "/abs/path/to/project",
                        value: "{new_group_path_value}",
                        oninput: move |event| new_group_path.set(event.value()),
                    }
                    button {
                        class: "primary-btn",
                        onclick: move |_| {
                            let path = new_group_path.read().trim().to_string();
                            if path.is_empty() {
                                return;
                            }

                            let default_sessions = {
                                let mut state = app_state.write();
                                let (_group_id, ids) = state.create_group_with_defaults(path.clone());
                                if let Some(first) = ids.first().copied() {
                                    state.select_session(first);
                                }
                                ids
                            };

                            new_group_path.set(String::new());

                            let failed = if let Ok(mut runtime) = terminal_manager.read().lock() {
                                default_sessions
                                    .iter()
                                    .filter_map(|session_id| {
                                        runtime
                                            .ensure_session(*session_id, &path)
                                            .err()
                                            .map(|_| *session_id)
                                    })
                                    .collect::<Vec<_>>()
                            } else {
                                default_sessions
                            };

                            if !failed.is_empty() {
                                let mut state = app_state.write();
                                for session_id in failed {
                                    state.set_session_status(session_id, SessionStatus::Error);
                                }
                            }
                        },
                        "Create Path Group"
                    }
                }
            }

            main { class: "workspace",
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
                    }

                    div { class: "status-summary",
                        span { class: "badge idle", "Idle {idle_count}" }
                        span { class: "badge busy", "Busy {busy_count}" }
                        span { class: "badge error", "Error {error_count}" }
                    }
                }

                if let Some(group_id) = active_group_id {
                    {
                        let group_name = snapshot.group_label(group_id);

                        rsx! {
                            div { class: "workspace-layout",
                                div { class: "agent-stack",
                                    for session in active_agents {
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
                                                        app_state,
                                                        focused_terminal,
                                                        round_anchor,
                                                    )}
                                                }
                                            }
                                        }
                                    }
                                }

                                aside { class: "run-sidebar",
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
                                                        app_state,
                                                        focused_terminal,
                                                        round_anchor,
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
}

fn terminal_shell(
    session_id: SessionId,
    terminal_is_focused: bool,
    terminal: TerminalSnapshot,
    terminal_manager: Arc<Mutex<TerminalManager>>,
    app_state: Signal<AppState>,
    mut focused_terminal: Signal<Option<SessionId>>,
    mut round_anchor: Signal<Option<(SessionId, u16)>>,
) -> Element {
    let shell_class = if terminal_is_focused {
        "terminal-shell focused"
    } else {
        "terminal-shell"
    };
    let body_style = format!(
        "--term-rows: {}; --term-cols: {};",
        terminal.rows, terminal.cols
    );
    let cursor_row = terminal.cursor_row.min(terminal.rows.saturating_sub(1));
    let cursor_col = terminal.cursor_col.min(terminal.cols.saturating_sub(1));
    let click_rows = terminal.rows;
    let click_cols = terminal.cols;
    let click_cursor_row = terminal.cursor_row;
    let click_cursor_col = terminal.cursor_col;
    let bracketed_paste = terminal.bracketed_paste;
    let show_caret = terminal_is_focused && !terminal.hide_cursor;
    let terminal_body_id = format!("terminal-body-{session_id}");
    let body_id_for_click = terminal_body_id.clone();
    let body_id_for_round_select = terminal_body_id.clone();
    let terminal_manager_for_click = terminal_manager.clone();
    let terminal_manager_for_keydown = terminal_manager;
    let round_anchor_row = match *round_anchor.read() {
        Some((anchor_session, row)) if anchor_session == session_id => row,
        _ => cursor_row,
    };
    let round_bounds = terminal_round_bounds(&terminal.lines, round_anchor_row);

    rsx! {
        div {
            class: "{shell_class}",
            tabindex: "0",
            onfocus: move |_| {
                focused_terminal.set(Some(session_id));
            },
            onblur: move |_| {
                let is_current = *focused_terminal.read() == Some(session_id);
                if is_current {
                    focused_terminal.set(None);
                }
            },
            onclick: move |event| {
                focused_terminal.set(Some(session_id));
                let click_position = event.data().client_coordinates();
                let click_x = click_position.x;
                let click_y = click_position.y;
                let body_id = body_id_for_click.clone();
                let terminal_manager = terminal_manager_for_click.clone();
                spawn(async move {
                    let Some((target_row, target_col)) = map_click_to_terminal_cell(
                        body_id,
                        click_x,
                        click_y,
                        click_rows,
                        click_cols,
                    )
                    .await
                    else {
                        return;
                    };

                    round_anchor.set(Some((session_id, target_row)));
                    if target_row != click_cursor_row {
                        return;
                    }

                    let movement = cursor_move_bytes(
                        click_cursor_row,
                        click_cursor_col,
                        target_row,
                        target_col,
                    );

                    if !movement.is_empty() {
                        send_input_to_session(
                            &terminal_manager,
                            app_state,
                            session_id,
                            &movement,
                        );
                    }
                });
            },
            onkeydown: move |event| {
                let data = event.data();
                let key = data.key();
                let modifiers = data.modifiers();
                if modifiers.ctrl() && !modifiers.alt() {
                    if let Key::Character(text) = &key {
                        if text.eq_ignore_ascii_case("a") {
                            event.prevent_default();
                            event.stop_propagation();
                            if let Some((start_row, end_row)) = round_bounds {
                                let body_id = body_id_for_round_select.clone();
                                spawn(async move {
                                    let _ = select_terminal_round(body_id, start_row, end_row).await;
                                });
                            }
                            return;
                        }

                        if text.eq_ignore_ascii_case("v") {
                            event.prevent_default();
                            event.stop_propagation();
                            let terminal_manager = terminal_manager_for_keydown.clone();
                            spawn(async move {
                                let clipboard_text = document::eval(READ_CLIPBOARD_JS)
                                    .join::<String>()
                                    .await
                                    .unwrap_or_default();
                                if clipboard_text.is_empty() {
                                    return;
                                }

                                let payload = if bracketed_paste {
                                    wrap_bracketed_paste(clipboard_text.as_bytes())
                                } else {
                                    clipboard_text.into_bytes()
                                };
                                send_input_to_session(
                                    &terminal_manager,
                                    app_state,
                                    session_id,
                                    &payload,
                                );
                            });
                            return;
                        }

                        if text.eq_ignore_ascii_case("c") {
                            event.prevent_default();
                            event.stop_propagation();
                            let terminal_manager = terminal_manager_for_keydown.clone();
                            spawn(async move {
                                let copied = document::eval(COPY_SELECTION_JS)
                                    .join::<bool>()
                                    .await
                                    .unwrap_or(false);
                                if !copied {
                                    send_input_to_session(
                                        &terminal_manager,
                                        app_state,
                                        session_id,
                                        &[0x03],
                                    );
                                }
                            });
                            return;
                        }
                    }
                }

                if let Some(input) = key_event_to_bytes(&event) {
                    event.prevent_default();
                    event.stop_propagation();
                    send_input_to_session(
                        &terminal_manager_for_keydown,
                        app_state,
                        session_id,
                        &input,
                    );
                }
            },

            div {
                class: "terminal-body",
                id: "{terminal_body_id}",
                style: "{body_style}",
                div { class: "terminal-grid",
                    for row_idx in 0..usize::from(terminal.rows) {
                        {
                            let line = terminal.lines.get(row_idx).cloned().unwrap_or_default();
                            rsx! {
                                div {
                                    class: "terminal-line",
                                    key: "line-{session_id}-{row_idx}",
                                    "data-row": "{row_idx}",
                                    if show_caret && row_idx == usize::from(cursor_row) {
                                        {render_terminal_line_with_caret(line, cursor_col)}
                                    } else {
                                        {render_terminal_line(line)}
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

fn render_terminal_line(line: String) -> Element {
    if let Some((prompt, rest)) = split_prompt_prefix(&line) {
        rsx! {
            span {
                class: "terminal-prompt",
                "{prompt}"
            }
            span { class: "terminal-text", "{rest}" }
        }
    } else {
        rsx! {
            span { class: "terminal-text", "{line}" }
        }
    }
}

fn render_terminal_line_with_caret(line: String, cursor_col: u16) -> Element {
    let split_idx = char_index_to_byte(&line, usize::from(cursor_col));
    let before = &line[..split_idx];
    let after = &line[split_idx..];

    if let Some((prompt, rest)) = split_prompt_prefix(&line) {
        let prompt_chars = prompt.chars().count();
        let before_chars = before.chars().count();

        if before_chars <= prompt_chars {
            let prompt_split = char_index_to_byte(prompt, before_chars);
            let prompt_before = &prompt[..prompt_split];
            let prompt_after = &prompt[prompt_split..];

            rsx! {
                span { class: "terminal-prompt", "{prompt_before}" }
                span { class: "terminal-caret-inline", " " }
                span { class: "terminal-prompt", "{prompt_after}" }
                span { class: "terminal-text", "{rest}" }
            }
        } else {
            let rest_before_chars = before_chars - prompt_chars;
            let rest_split = char_index_to_byte(rest, rest_before_chars);
            let rest_before = &rest[..rest_split];
            let rest_after = &rest[rest_split..];

            rsx! {
                span { class: "terminal-prompt", "{prompt}" }
                span { class: "terminal-text", "{rest_before}" }
                span { class: "terminal-caret-inline", " " }
                span { class: "terminal-text", "{rest_after}" }
            }
        }
    } else {
        rsx! {
            span { class: "terminal-text", "{before}" }
            span { class: "terminal-caret-inline", " " }
            span { class: "terminal-text", "{after}" }
        }
    }
}

fn char_index_to_byte(input: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }

    input
        .char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(input.len())
}

fn split_prompt_prefix(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return None;
    }

    let leading = line.len().saturating_sub(trimmed.len());

    // When the terminal gets narrow, bash/zsh prompts can wrap and leave the
    // final "$ " or "# " marker at the start of a new visual line.
    if trimmed.starts_with("$ ") || trimmed.starts_with("# ") {
        let end = leading + 2;
        return Some((&line[..end], &line[end..]));
    }

    // Wrapped prompts can also leave only "$" or "#" at end/start of row.
    if trimmed == "$" || trimmed == "#" {
        return Some((line, ""));
    }

    if (trimmed.ends_with('$') || trimmed.ends_with('#'))
        && (trimmed.contains('@') || trimmed.contains(':'))
    {
        return Some((line, ""));
    }

    let marker = trimmed.find("$ ").or_else(|| trimmed.find("# "))?;
    let end = leading + marker + 2;
    let prefix = &line[..end];
    if !prefix.contains('@') || !prefix.contains(':') {
        return None;
    }

    Some((prefix, &line[end..]))
}

fn terminal_round_bounds(lines: &[String], cursor_row: u16) -> Option<(u16, u16)> {
    if lines.is_empty() {
        return None;
    }

    let cursor_idx = usize::from(cursor_row).min(lines.len().saturating_sub(1));

    let start_idx = (0..=cursor_idx)
        .rev()
        .find(|idx| {
            is_prompt_row(
                lines
                    .get(*idx)
                    .map(|line| line.as_str())
                    .unwrap_or_default(),
            )
        })
        .unwrap_or(0);

    let next_prompt_idx = (start_idx + 1..lines.len()).find(|idx| {
        is_prompt_row(
            lines
                .get(*idx)
                .map(|line| line.as_str())
                .unwrap_or_default(),
        )
    });

    let last_non_empty = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .unwrap_or(cursor_idx);

    let mut end_idx = next_prompt_idx
        .map(|idx| idx.saturating_sub(1))
        .unwrap_or(last_non_empty.max(cursor_idx));
    if end_idx < start_idx {
        end_idx = start_idx;
    }

    let start = u16::try_from(start_idx).ok()?;
    let end = u16::try_from(end_idx).ok()?;
    Some((start, end))
}

fn is_prompt_row(line: &str) -> bool {
    split_prompt_prefix(line).is_some()
}

fn pending_terminal_snapshot() -> TerminalSnapshot {
    let rows = 42_u16;
    let cols = 140_u16;
    let mut lines = vec![String::new(); usize::from(rows)];
    lines[0] = "# Terminal pending startup".to_string();
    TerminalSnapshot {
        lines,
        rows,
        cols,
        cursor_row: 0,
        cursor_col: 0,
        hide_cursor: false,
        bracketed_paste: false,
    }
}

fn wrap_bracketed_paste(payload: &[u8]) -> Vec<u8> {
    let mut wrapped = Vec::with_capacity(payload.len() + 12);
    wrapped.extend_from_slice(b"\x1b[200~");
    wrapped.extend_from_slice(payload);
    wrapped.extend_from_slice(b"\x1b[201~");
    wrapped
}

fn send_input_to_session(
    terminal_manager: &Arc<Mutex<TerminalManager>>,
    mut app_state: Signal<AppState>,
    session_id: SessionId,
    input: &[u8],
) {
    let send_error = if let Ok(mut runtime) = terminal_manager.lock() {
        runtime.send_input(session_id, input).err()
    } else {
        Some("terminal manager lock failed".to_string())
    };

    if send_error.is_none() {
        app_state
            .write()
            .set_session_status(session_id, SessionStatus::Busy);
    } else {
        app_state
            .write()
            .set_session_status(session_id, SessionStatus::Error);
    }
}

async fn map_click_to_terminal_cell(
    terminal_body_id: String,
    client_x: f64,
    client_y: f64,
    rows: u16,
    cols: u16,
) -> Option<(u16, u16)> {
    let script = format!(
        r#"
const root = document.getElementById({terminal_body_id:?});
if (!root) return "";

const el = document.elementFromPoint({client_x}, {client_y});
if (!el || !root.contains(el)) return "";

const line = el.closest(".terminal-line");
if (!line || !root.contains(line)) return "";

const row = Number.parseInt(line.dataset.row ?? "0", 10);
if (Number.isNaN(row)) return "";

let col = 0;
let node = null;
let offset = 0;

if (document.caretPositionFromPoint) {{
    const pos = document.caretPositionFromPoint({client_x}, {client_y});
    if (pos) {{
        node = pos.offsetNode;
        offset = pos.offset;
    }}
}} else if (document.caretRangeFromPoint) {{
    const range = document.caretRangeFromPoint({client_x}, {client_y});
    if (range) {{
        node = range.startContainer;
        offset = range.startOffset;
    }}
}}

if (node && line.contains(node)) {{
    try {{
        const range = document.createRange();
        range.setStart(line, 0);
        range.setEnd(node, offset);
        col = range.toString().length;
    }} catch (_) {{
        col = line.textContent ? line.textContent.length : 0;
    }}
}} else {{
    col = line.textContent ? line.textContent.length : 0;
}}

return `${{row}},${{Math.max(0, col)}}`;
"#
    );
    let mapped = document::eval(&script).join::<String>().await.ok()?;
    let (row, col) = parse_row_col(&mapped)?;
    let clamped_row = row.min(rows.saturating_sub(1));
    let clamped_col = col.min(cols.saturating_sub(1));
    Some((clamped_row, clamped_col))
}

async fn select_terminal_round(terminal_body_id: String, start_row: u16, end_row: u16) -> bool {
    let script = format!(
        r#"
const root = document.getElementById({terminal_body_id:?});
if (!root) return false;

const start = root.querySelector('.terminal-line[data-row="{start_row}"]');
const end = root.querySelector('.terminal-line[data-row="{end_row}"]');
if (!start || !end) return false;

const selection = window.getSelection ? window.getSelection() : null;
if (!selection) return false;

const range = document.createRange();
range.setStartBefore(start);
range.setEndAfter(end);
selection.removeAllRanges();
selection.addRange(range);
return true;
"#
    );

    document::eval(&script)
        .join::<bool>()
        .await
        .unwrap_or(false)
}

async fn measure_terminal_viewport(terminal_body_id: String) -> Option<(u16, u16)> {
    let script = format!(
        r#"
const root = document.getElementById({terminal_body_id:?});
if (!root) return "";

const style = window.getComputedStyle(root);
const parsePx = (value, fallback) => {{
    const parsed = Number.parseFloat(value);
    return Number.isFinite(parsed) ? parsed : fallback;
}};

const paddingX = parsePx(style.paddingLeft, 0) + parsePx(style.paddingRight, 0);
const paddingY = parsePx(style.paddingTop, 0) + parsePx(style.paddingBottom, 0);
const lineHeight = Math.max(1, parsePx(style.lineHeight, 17));

let charWidth = parsePx(style.getPropertyValue("--term-char-width"), 8.4);
const probe = document.createElement("span");
probe.textContent = "MMMMMMMMMM";
probe.style.position = "absolute";
probe.style.visibility = "hidden";
probe.style.pointerEvents = "none";
probe.style.whiteSpace = "pre";
probe.style.font = style.font;
probe.style.letterSpacing = style.letterSpacing;
root.appendChild(probe);
const probeWidth = probe.getBoundingClientRect().width / 10;
root.removeChild(probe);
if (Number.isFinite(probeWidth) && probeWidth > 0) {{
    charWidth = probeWidth;
}}
charWidth = Math.max(1, charWidth);

const viewportWidth = Math.max(0, root.clientWidth - paddingX);
const viewportHeight = Math.max(0, root.clientHeight - paddingY);
const cols = Math.max(8, Math.floor(viewportWidth / charWidth));
const rows = Math.max(2, Math.floor(viewportHeight / lineHeight));

return `${{rows}},${{cols}}`;
"#
    );

    let measured = document::eval(&script).join::<String>().await.ok()?;
    parse_row_col(&measured)
}

fn parse_row_col(input: &str) -> Option<(u16, u16)> {
    let (row, col) = input.trim().split_once(',')?;
    let row = row.trim().parse::<u16>().ok()?;
    let col = col.trim().parse::<u16>().ok()?;
    Some((row, col))
}

fn cursor_move_bytes(from_row: u16, from_col: u16, target_row: u16, target_col: u16) -> Vec<u8> {
    let mut bytes = Vec::new();

    if target_row > from_row {
        for _ in 0..(target_row - from_row) {
            bytes.extend_from_slice(b"\x1b[B");
        }
    } else {
        for _ in 0..(from_row - target_row) {
            bytes.extend_from_slice(b"\x1b[A");
        }
    }

    if target_col > from_col {
        for _ in 0..(target_col - from_col) {
            bytes.extend_from_slice(b"\x1b[C");
        }
    } else {
        for _ in 0..(from_col - target_col) {
            bytes.extend_from_slice(b"\x1b[D");
        }
    }

    bytes
}

fn key_event_to_bytes(event: &KeyboardEvent) -> Option<Vec<u8>> {
    let data = event.data();
    let key = data.key();
    let modifiers = data.modifiers();
    let ctrl = modifiers.ctrl();
    let alt = modifiers.alt();
    let shift = modifiers.shift();

    let mut bytes = match key {
        Key::Enter => vec![b'\r'],
        Key::Tab => {
            if shift {
                b"\x1b[Z".to_vec()
            } else {
                vec![b'\t']
            }
        }
        Key::Backspace => vec![0x7f],
        Key::Escape => vec![0x1b],
        Key::ArrowUp => b"\x1b[A".to_vec(),
        Key::ArrowDown => b"\x1b[B".to_vec(),
        Key::ArrowRight => b"\x1b[C".to_vec(),
        Key::ArrowLeft => b"\x1b[D".to_vec(),
        Key::Home => b"\x1b[H".to_vec(),
        Key::End => b"\x1b[F".to_vec(),
        Key::Delete => b"\x1b[3~".to_vec(),
        Key::Insert => b"\x1b[2~".to_vec(),
        Key::PageUp => b"\x1b[5~".to_vec(),
        Key::PageDown => b"\x1b[6~".to_vec(),
        Key::Character(text) => {
            if text.is_empty() {
                return None;
            }

            if ctrl {
                let first = text.chars().next()?;
                vec![control_byte(first)?]
            } else {
                text.as_bytes().to_vec()
            }
        }
        _ => return None,
    };

    if alt {
        let mut prefixed = Vec::with_capacity(bytes.len() + 1);
        prefixed.push(0x1b);
        prefixed.extend(bytes);
        bytes = prefixed;
    }

    Some(bytes)
}

fn control_byte(input: char) -> Option<u8> {
    let lower = input.to_ascii_lowercase();

    let byte = match lower {
        '@' | ' ' | '2' => 0,
        'a'..='z' => (lower as u8) - b'a' + 1,
        '[' | '3' => 27,
        '\\' | '4' => 28,
        ']' | '5' => 29,
        '^' | '6' => 30,
        '_' | '7' => 31,
        '8' | '?' => 127,
        _ => return None,
    };

    Some(byte)
}
