use crate::commands::CommandId;
use crate::emily_bridge::{EmilyBridge, SnippetIngestRequest};
use crate::state::{AppState, NewSnippet, SessionId, SessionStatus};
use crate::terminal::{TerminalManager, TerminalSnapshot};
use crate::ui::command_palette::{InsertCommandPalette, PaletteRow};
use crate::ui::insert_command_mode::{
    InsertModeOutcome, InsertModeSelection, InsertModeState, KeyModifiers, TerminalKeyRoute,
    command_matches, mode_after_blur, mode_after_focus, route_terminal_key, selected_command_id,
};
use crate::ui::terminal_input::{
    COPY_SELECTION_JS, READ_CLIPBOARD_JS, cursor_move_bytes, install_terminal_paste_bridge,
    install_terminal_scroll_behavior, is_terminal_scrolled_near_top, key_event_to_bytes,
    map_click_to_terminal_cell, read_terminal_selection, select_terminal_round,
    take_terminal_paste_buffer,
};
use crate::ui::{EMILY_HISTORY_BACKFILL_PAGE_LINES, TerminalHistoryState, UiState};
use dioxus::document;
use dioxus::prelude::*;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const INSERT_CHORD_TIMEOUT_MS: u64 = 1_000;
const MAX_SNIPPET_TEXT_BYTES: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SnippetHotkeyState {
    pub session_id: SessionId,
    pub armed_at_unix_ms: i64,
}

#[derive(Clone, Copy)]
pub(crate) struct TerminalInteractionSignals {
    pub app_state: Signal<AppState>,
    pub ui_state: Signal<UiState>,
    pub snippet_hotkey_state: Signal<Option<SnippetHotkeyState>>,
}

pub(crate) fn terminal_shell(
    session_id: SessionId,
    source_cwd: String,
    terminal_is_focused: bool,
    terminal: Arc<TerminalSnapshot>,
    terminal_manager: Arc<TerminalManager>,
    emily_bridge: Arc<EmilyBridge>,
    interaction: TerminalInteractionSignals,
) -> Element {
    let mut app_state = interaction.app_state;
    let mut ui_state = interaction.ui_state;
    let mut snippet_hotkey_state = interaction.snippet_hotkey_state;
    let crt_enabled = app_state.read().crt_enabled();
    let shell_class = match (terminal_is_focused, crt_enabled) {
        (true, true) => "terminal-shell focused crt-enabled",
        (true, false) => "terminal-shell focused",
        (false, true) => "terminal-shell crt-enabled",
        (false, false) => "terminal-shell",
    };
    let body_class = if crt_enabled {
        "terminal-body crt-enabled"
    } else {
        "terminal-body"
    };
    let grid_class = if crt_enabled {
        "terminal-grid crt-enabled"
    } else {
        "terminal-grid"
    };
    let body_style = format!(
        "--term-rows: {}; --term-cols: {};",
        terminal.rows, terminal.cols
    );
    const RENDER_WINDOW_MULTIPLIER: usize = 8;
    const RENDER_WINDOW_MIN_ROWS: usize = 256;
    let line_count = terminal.lines.len().max(1);
    let max_render_rows_u16 = u16::try_from(line_count).unwrap_or(u16::MAX);
    let cursor_row = terminal
        .cursor_row
        .min(max_render_rows_u16.saturating_sub(1));
    let cursor_col = terminal.cursor_col.min(terminal.cols.saturating_sub(1));
    let render_window_rows = usize::from(terminal.rows)
        .saturating_mul(RENDER_WINDOW_MULTIPLIER)
        .max(RENDER_WINDOW_MIN_ROWS);
    let window_start = terminal.lines.len().saturating_sub(render_window_rows);
    let rendered_lines = &terminal.lines[window_start..];
    let click_rows = u16::try_from(rendered_lines.len().max(1)).unwrap_or(u16::MAX);
    let click_cols = terminal.cols;
    let click_cursor_row = cursor_row;
    let click_cursor_row_local = usize::from(click_cursor_row).saturating_sub(window_start);
    let click_window_start = window_start;
    let click_cursor_col = cursor_col;
    let bracketed_paste = terminal.bracketed_paste;
    let show_caret = terminal_is_focused && !terminal.hide_cursor;
    let terminal_shell_id = format!("terminal-shell-{session_id}");
    let terminal_body_id = format!("terminal-body-{session_id}");
    let body_id_for_click = terminal_body_id.clone();
    let body_id_for_round_select = terminal_body_id.clone();
    let body_id_for_mount = terminal_body_id.clone();
    let body_id_for_snippet_capture = terminal_body_id.clone();
    let shell_id_for_mount = terminal_shell_id.clone();
    let shell_id_for_paste_event = terminal_shell_id.clone();
    let body_id_for_scroll = terminal_body_id.clone();
    let terminal_manager_for_click = terminal_manager.clone();
    let terminal_manager_for_keydown = terminal_manager;
    let terminal_manager_for_paste = terminal_manager_for_keydown.clone();
    let terminal_manager_for_scroll = terminal_manager_for_keydown.clone();
    let emily_bridge_for_scroll = emily_bridge.clone();
    let emily_bridge_for_snippet = emily_bridge.clone();
    let round_anchor_row_global = match ui_state.read().round_anchor {
        Some((anchor_session, row)) if anchor_session == session_id => row,
        _ => cursor_row,
    };
    let current_insert_mode = ui_state.read().insert_mode_state.clone();
    let insert_mode_for_session = current_insert_mode
        .as_ref()
        .filter(|mode| mode.session_id == session_id)
        .cloned();
    let command_matches_for_palette = insert_mode_for_session
        .as_ref()
        .map(|mode| command_matches(app_state.read().commands(), &mode.query))
        .unwrap_or_default();
    let palette_highlight = insert_mode_for_session
        .as_ref()
        .map(|mode| {
            mode.highlighted_index
                .min(command_matches_for_palette.len().saturating_sub(1))
        })
        .unwrap_or(0);
    let palette_rows = command_matches_for_palette
        .iter()
        .filter_map(|entry| {
            app_state
                .read()
                .command_by_id(entry.command_id)
                .map(|command| PaletteRow {
                    name: command.name.clone(),
                    description: command.description.clone(),
                    prompt_preview: prompt_preview(&command.prompt),
                })
        })
        .collect::<Vec<_>>();
    let snippet_row_ranges = {
        let state = app_state.read();
        state
            .snippets_for_session(session_id)
            .into_iter()
            .map(|snippet| (snippet.log_ref.start_row, snippet.log_ref.end_row))
            .collect::<Vec<_>>()
    };

    rsx! {
        div {
            class: "{shell_class}",
            id: "{terminal_shell_id}",
            tabindex: "0",
            onfocus: move |_| {
                let mode_snapshot = ui_state.read().insert_mode_state.clone();
                let mut state = ui_state.write();
                state.focused_terminal = Some(session_id);
                state.insert_mode_state = mode_after_focus(mode_snapshot, session_id);
            },
            onblur: move |_| {
                let mode_snapshot = ui_state.read().insert_mode_state.clone();
                let mut state = ui_state.write();
                if state.focused_terminal == Some(session_id) {
                    state.focused_terminal = None;
                }
                state.insert_mode_state = mode_after_blur(mode_snapshot, session_id);
            },
            onclick: move |event| {
                ui_state.write().focused_terminal = Some(session_id);
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

                    let target_row_global = click_window_start.saturating_add(usize::from(target_row));
                    let target_row_global = u16::try_from(target_row_global).unwrap_or(u16::MAX);

                    ui_state.write().round_anchor = Some((session_id, target_row_global));
                    if usize::from(target_row) != click_cursor_row_local {
                        return;
                    }

                    let movement = cursor_move_bytes(
                        click_cursor_row,
                        click_cursor_col,
                        target_row_global,
                        target_col,
                    );

                    if !movement.is_empty() {
                        send_input_to_session(&terminal_manager, app_state, session_id, &movement);
                    }
                });
            },
            onkeydown: move |event| {
                let data = event.data();
                let key = data.key();
                let modifiers = data.modifiers();
                let ctrl = modifiers.ctrl();
                let alt = modifiers.alt();
                let shift = modifiers.shift();
                let meta = modifiers.meta();

                if crt_toggle_requested(&key, ctrl, alt, shift, meta) {
                    event.prevent_default();
                    event.stop_propagation();
                    let enabled = app_state.read().crt_enabled();
                    app_state.write().set_crt_enabled(!enabled);
                    return;
                }

                let active_insert_mode = ui_state
                    .read()
                    .insert_mode_state
                    .as_ref()
                    .filter(|mode| mode.session_id == session_id)
                    .cloned();
                let command_matches = active_insert_mode
                    .as_ref()
                    .map(|mode| command_matches(app_state.read().commands(), &mode.query))
                    .unwrap_or_default();
                let selected_id = active_insert_mode
                    .as_ref()
                    .and_then(|mode| selected_command_id(&command_matches, mode.highlighted_index));

                if active_insert_mode.is_none() {
                    let chord_state = *snippet_hotkey_state.read();
                    if let Some(chord) = chord_state
                        && chord.session_id == session_id
                    {
                        if unix_now_ms().saturating_sub(chord.armed_at_unix_ms)
                            > i64::try_from(INSERT_CHORD_TIMEOUT_MS).unwrap_or(i64::MAX)
                        {
                            snippet_hotkey_state.set(None);
                        } else if !ctrl && !alt && !shift && !meta {
                            match &key {
                                Key::Character(text) if text.eq_ignore_ascii_case("s") => {
                                    event.prevent_default();
                                    event.stop_propagation();
                                    snippet_hotkey_state.set(None);
                                    let body_id = body_id_for_snippet_capture.clone();
                                    let terminal_lines = terminal.lines.clone();
                                    let source_cwd = source_cwd.clone();
                                    let emily_bridge = emily_bridge_for_snippet.clone();
                                    spawn(async move {
                                        save_selection_as_snippet(
                                            app_state,
                                            emily_bridge,
                                            session_id,
                                            source_cwd,
                                            body_id,
                                            terminal_lines,
                                        )
                                        .await;
                                    });
                                    return;
                                }
                                Key::Character(text) if text.eq_ignore_ascii_case("c") => {
                                    event.prevent_default();
                                    event.stop_propagation();
                                    snippet_hotkey_state.set(None);
                                    ui_state.write().insert_mode_state = Some(InsertModeState {
                                        session_id,
                                        query: String::new(),
                                        highlighted_index: 0,
                                    });
                                    return;
                                }
                                Key::Escape | Key::Insert => {
                                    event.prevent_default();
                                    event.stop_propagation();
                                    snippet_hotkey_state.set(None);
                                    return;
                                }
                                _ => {
                                    snippet_hotkey_state.set(None);
                                }
                            }
                        } else {
                            snippet_hotkey_state.set(None);
                        }
                    }

                    if is_snippet_hotkey_trigger(&key, ctrl, alt, shift, meta) {
                        event.prevent_default();
                        event.stop_propagation();
                        let armed = SnippetHotkeyState {
                            session_id,
                            armed_at_unix_ms: unix_now_ms(),
                        };
                        snippet_hotkey_state.set(Some(armed));
                        spawn(async move {
                            tokio::time::sleep(Duration::from_millis(INSERT_CHORD_TIMEOUT_MS))
                                .await;
                            if snippet_hotkey_state.read().as_ref() == Some(&armed) {
                                snippet_hotkey_state.set(None);
                            }
                        });
                        return;
                    }
                }

                match route_terminal_key(
                    active_insert_mode.as_ref(),
                    &key,
                    KeyModifiers {
                        ctrl,
                        alt,
                        shift,
                        meta,
                    },
                    InsertModeSelection {
                        selected_command_id: selected_id,
                        match_count: command_matches.len(),
                    },
                ) {
                    TerminalKeyRoute::OpenMode => {
                        event.prevent_default();
                        event.stop_propagation();
                        ui_state.write().insert_mode_state = Some(InsertModeState {
                            session_id,
                            query: String::new(),
                            highlighted_index: 0,
                        });
                        return;
                    }
                    TerminalKeyRoute::HandleMode(outcome) => {
                        event.prevent_default();
                        event.stop_propagation();
                        match outcome {
                            InsertModeOutcome::Keep(next_mode) => {
                                ui_state.write().insert_mode_state = Some(next_mode);
                            }
                            InsertModeOutcome::Close => {
                                ui_state.write().insert_mode_state = None;
                            }
                            InsertModeOutcome::Submit(command_id) => {
                                submit_insert_command(
                                    command_id,
                                    &terminal_manager_for_keydown,
                                    app_state,
                                    session_id,
                                );
                                ui_state.write().insert_mode_state = None;
                            }
                            InsertModeOutcome::Ignore => {}
                        }
                        return;
                    }
                    TerminalKeyRoute::Passthrough => {}
                }

                if is_paste_shortcut(
                    &key,
                    ctrl,
                    alt,
                    shift,
                    meta,
                ) {
                    // Let the platform dispatch `paste` so clipboard data is available via the
                    // trusted paste event path.
                    return;
                }

                if ctrl && !alt && let Key::Character(text) = &key {
                    if text.eq_ignore_ascii_case("a") {
                        event.prevent_default();
                        event.stop_propagation();
                        if let Some((start_row, end_row)) =
                            terminal_round_bounds(&terminal.lines, round_anchor_row_global)
                        {
                            let body_id = body_id_for_round_select.clone();
                            spawn(async move {
                                let _ = select_terminal_round(body_id, start_row, end_row).await;
                            });
                        }
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
            onpaste: move |event| {
                event.prevent_default();
                event.stop_propagation();
                if let Some(mode) = ui_state.read().insert_mode_state.clone()
                    && mode.session_id == session_id
                {
                    return;
                }
                let terminal_manager = terminal_manager_for_paste.clone();
                let shell_id = shell_id_for_paste_event.clone();
                paste_clipboard_into_terminal(
                    terminal_manager,
                    app_state,
                    session_id,
                    bracketed_paste,
                    shell_id,
                );
            },

            div {
                class: "{body_class}",
                id: "{terminal_body_id}",
                style: "{body_style}",
                onscroll: move |_| {
                    let history_snapshot = ui_state
                        .read()
                        .terminal_history_by_session
                        .get(&session_id)
                        .copied()
                        .unwrap_or_default();
                    if history_snapshot.exhausted || history_snapshot.is_loading {
                        return;
                    }

                    ui_state
                        .write()
                        .terminal_history_by_session
                        .entry(session_id)
                        .and_modify(|state| state.is_loading = true)
                        .or_insert(TerminalHistoryState {
                            before_sequence: None,
                            is_loading: true,
                            exhausted: true,
                        });

                    let body_id = body_id_for_scroll.clone();
                    let emily_bridge = emily_bridge_for_scroll.clone();
                    let terminal_manager = terminal_manager_for_scroll.clone();
                    spawn(async move {
                        const TOP_THRESHOLD_PX: u32 = 20;
                        if !is_terminal_scrolled_near_top(body_id, TOP_THRESHOLD_PX).await {
                            if let Some(state) = ui_state
                                .write()
                                .terminal_history_by_session
                                .get_mut(&session_id)
                            {
                                state.is_loading = false;
                            }
                            return;
                        }

                        let before_sequence = ui_state
                            .read()
                            .terminal_history_by_session
                            .get(&session_id)
                            .and_then(|state| state.before_sequence);
                        let Some(before_sequence) = before_sequence else {
                            if let Some(state) = ui_state
                                .write()
                                .terminal_history_by_session
                                .get_mut(&session_id)
                            {
                                state.is_loading = false;
                                state.exhausted = true;
                            }
                            return;
                        };

                        let result = emily_bridge
                            .page_history_before_async(
                                session_id,
                                Some(before_sequence),
                                EMILY_HISTORY_BACKFILL_PAGE_LINES,
                            )
                            .await;

                        match result {
                            Ok(chunk) => {
                                let older_lines = chunk.lines.into_iter().rev().collect::<Vec<_>>();
                                let inserted = terminal_manager
                                    .prepend_history_lines(session_id, &older_lines)
                                    .unwrap_or(0);
                                if let Some(state) = ui_state
                                    .write()
                                    .terminal_history_by_session
                                    .get_mut(&session_id)
                                {
                                    state.before_sequence = chunk.next_before_sequence;
                                    state.exhausted =
                                        chunk.next_before_sequence.is_none() || inserted == 0;
                                    state.is_loading = false;
                                }
                            }
                            Err(_) => {
                                if let Some(state) = ui_state
                                    .write()
                                    .terminal_history_by_session
                                    .get_mut(&session_id)
                                {
                                    state.is_loading = false;
                                }
                            }
                        }
                    });
                },
                onmounted: move |_| {
                    let body_id = body_id_for_mount.clone();
                    let shell_id = shell_id_for_mount.clone();
                    spawn(async move {
                        let _ = install_terminal_scroll_behavior(body_id).await;
                        let _ = install_terminal_paste_bridge(shell_id).await;
                    });
                },
                div { class: "{grid_class}",
                    for row_idx in 0..rendered_lines.len() {
                        {
                            let line = rendered_lines
                                .get(row_idx)
                                .map(|line| line.as_str())
                                .unwrap_or_default();
                            let actual_row_idx = window_start.saturating_add(row_idx);
                            let has_snippet = snippet_row_ranges.iter().any(|(start, end)| {
                                actual_row_idx >= usize::try_from(*start).unwrap_or(usize::MAX)
                                    && actual_row_idx <= usize::try_from(*end).unwrap_or(0)
                            });
                            let line_class = if has_snippet {
                                "terminal-line snippet-annotated"
                            } else {
                                "terminal-line"
                            };
                            let line_style = crt_line_style(row_idx, rendered_lines.len());
                            let line_chars = line.chars().count();
                            rsx! {
                                div {
                                    class: "{line_class}",
                                    key: "line-{session_id}-{actual_row_idx}",
                                    "data-row": "{actual_row_idx}",
                                    "data-line-chars": "{line_chars}",
                                    style: "{line_style}",
                                    if show_caret && actual_row_idx == usize::from(cursor_row) {
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

            if let Some(mode) = insert_mode_for_session {
                InsertCommandPalette {
                    query: mode.query,
                    highlighted_index: palette_highlight,
                    rows: palette_rows,
                }
            }
        }
    }
}

fn crt_line_style(row_idx: usize, total_rows: usize) -> String {
    format!(
        "--crt-line-bend: {:.4};",
        crt_line_bend(row_idx, total_rows)
    )
}

fn crt_line_bend(row_idx: usize, total_rows: usize) -> f64 {
    if total_rows <= 1 {
        return 0.0;
    }

    let progress = (row_idx as f64 + 0.5) / total_rows as f64;
    let normalized = (progress * 2.0) - 1.0;
    normalized * normalized
}

fn crt_toggle_requested(key: &Key, ctrl: bool, alt: bool, shift: bool, meta: bool) -> bool {
    if !ctrl || alt || shift || meta {
        return false;
    }

    matches!(key, Key::Character(text) if text == "1")
}

fn render_terminal_line(line: &str) -> Element {
    if let Some((prompt, rest)) = split_prompt_prefix(line) {
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

fn render_terminal_line_with_caret(line: &str, cursor_col: u16) -> Element {
    let split_idx = char_index_to_byte(line, usize::from(cursor_col));
    let before = &line[..split_idx];
    let after = &line[split_idx..];

    if let Some((prompt, rest)) = split_prompt_prefix(line) {
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

    if trimmed.starts_with("$ ") || trimmed.starts_with("# ") {
        let end = leading + 2;
        return Some((&line[..end], &line[end..]));
    }

    if trimmed == "$" || trimmed == "#" {
        return Some((line, ""));
    }

    if trimmed.ends_with('$') || trimmed.ends_with('#') {
        return Some((line, ""));
    }

    if trimmed.contains('@')
        && trimmed.contains(':')
        && (trimmed.contains('/') || trimmed.contains('\\'))
    {
        return Some((line, ""));
    }

    let marker = trimmed.rfind("$ ").or_else(|| trimmed.rfind("# "))?;
    let end = leading + marker + 2;
    let prefix = &line[..end];
    if !(prefix.contains('@') && prefix.contains(':'))
        && marker + 2 != trimmed.len()
        && !prefix.contains('/')
    {
        return None;
    }

    Some((prefix, &line[end..]))
}

fn is_paste_shortcut(key: &Key, ctrl: bool, alt: bool, shift: bool, meta: bool) -> bool {
    match key {
        Key::Character(text) => text.eq_ignore_ascii_case("v") && (ctrl || meta) && !alt,
        Key::Insert => shift && !ctrl && !alt && !meta,
        _ => false,
    }
}

fn is_snippet_hotkey_trigger(key: &Key, ctrl: bool, alt: bool, shift: bool, meta: bool) -> bool {
    matches!(key, Key::Insert) && alt && !ctrl && !shift && !meta
}

fn paste_clipboard_into_terminal(
    terminal_manager: Arc<TerminalManager>,
    app_state: Signal<AppState>,
    session_id: SessionId,
    bracketed_paste: bool,
    terminal_body_id: String,
) {
    spawn(async move {
        let bridged_text = take_terminal_paste_buffer(terminal_body_id)
            .await
            .unwrap_or_default();
        let clipboard_text = if bridged_text.is_empty() {
            document::eval(READ_CLIPBOARD_JS)
                .join::<String>()
                .await
                .unwrap_or_default()
        } else {
            bridged_text
        };

        if clipboard_text.is_empty() {
            return;
        }

        let payload = if bracketed_paste {
            wrap_bracketed_paste(clipboard_text.as_bytes())
        } else {
            clipboard_text.into_bytes()
        };
        send_input_to_session(&terminal_manager, app_state, session_id, &payload);
    });
}

fn submit_insert_command(
    command_id: CommandId,
    terminal_manager: &Arc<TerminalManager>,
    app_state: Signal<AppState>,
    session_id: SessionId,
) {
    let command_prompt = app_state
        .read()
        .command_by_id(command_id)
        .map(|command| command.prompt.clone())
        .unwrap_or_default();
    if command_prompt.is_empty() {
        return;
    }

    send_input_to_session(
        terminal_manager,
        app_state,
        session_id,
        command_prompt.as_bytes(),
    );
}

fn prompt_preview(prompt: &str) -> String {
    let normalized = prompt.trim().replace('\n', " ");
    let mut chars = normalized.chars();
    let mut preview = String::new();
    for _ in 0..72 {
        let Some(ch) = chars.next() else {
            return normalized;
        };
        preview.push(ch);
    }
    if chars.next().is_some() {
        preview.push_str("...");
    }
    preview
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

fn wrap_bracketed_paste(payload: &[u8]) -> Vec<u8> {
    let mut wrapped = Vec::with_capacity(payload.len() + 12);
    wrapped.extend_from_slice(b"\x1b[200~");
    wrapped.extend_from_slice(payload);
    wrapped.extend_from_slice(b"\x1b[201~");
    wrapped
}

fn send_input_to_session(
    terminal_manager: &Arc<TerminalManager>,
    mut app_state: Signal<AppState>,
    session_id: SessionId,
    input: &[u8],
) {
    let send_error = terminal_manager.send_input(session_id, input).err();

    if send_error.is_some() {
        app_state
            .write()
            .set_session_status(session_id, SessionStatus::Error);
    }
}

fn unix_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

async fn save_selection_as_snippet(
    mut app_state: Signal<AppState>,
    emily_bridge: Arc<EmilyBridge>,
    session_id: SessionId,
    source_cwd: String,
    terminal_body_id: String,
    terminal_lines: Vec<String>,
) {
    let Some(selection) = read_terminal_selection(terminal_body_id).await else {
        return;
    };
    let normalized_text = normalize_snippet_text(&selection.text);
    if normalized_text.trim().is_empty() {
        return;
    }
    let Some((start_offset, end_offset, start_row, end_row)) = selection_offsets(
        &terminal_lines,
        selection.start_row,
        selection.start_col,
        selection.end_row,
        selection.end_col,
    ) else {
        return;
    };
    let now_ms = unix_now_ms();
    let snippet_id = {
        let mut state = app_state.write();
        let snippet_id = state.create_snippet(NewSnippet {
            source_session_id: session_id,
            source_stream_id: format!("terminal:{session_id}"),
            source_cwd: source_cwd.clone(),
            text_snapshot_plain: normalized_text.clone(),
            start_offset,
            end_offset,
            start_row,
            end_row,
            created_at_unix_ms: now_ms,
        });
        state.set_snippet_embedding_processing(snippet_id);
        snippet_id
    };
    let ingest_result = emily_bridge
        .ingest_snippet_async(SnippetIngestRequest {
            snippet_id,
            source_session_id: session_id,
            source_stream_id: format!("terminal:{session_id}"),
            source_cwd,
            source_start_offset: start_offset,
            source_end_offset: end_offset,
            source_start_row: start_row,
            source_end_row: end_row,
            text: normalized_text,
            ts_unix_ms: now_ms,
        })
        .await;
    let mut state = app_state.write();
    match ingest_result {
        Ok(result) => {
            state.set_snippet_embedding_ready(
                snippet_id,
                result.object_id,
                result.embedding_profile_id,
                result.embedding_dimensions,
            );
        }
        Err(error) => {
            state.set_snippet_embedding_failed(snippet_id, error);
        }
    }
}

fn normalize_snippet_text(input: &str) -> String {
    let normalized = input.replace('\r', "");
    if normalized.len() <= MAX_SNIPPET_TEXT_BYTES {
        return normalized;
    }
    truncate_utf8(&normalized, MAX_SNIPPET_TEXT_BYTES)
}

fn truncate_utf8(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }
    let mut boundary = 0_usize;
    for (idx, _) in input.char_indices() {
        if idx > max_bytes {
            break;
        }
        boundary = idx;
    }
    input[..boundary].to_string()
}

fn selection_offsets(
    lines: &[String],
    start_row: u32,
    start_col: u32,
    end_row: u32,
    end_col: u32,
) -> Option<(u64, u64, u32, u32)> {
    if lines.is_empty() {
        return None;
    }
    let max_row = lines.len().saturating_sub(1);
    let start_row = usize::try_from(start_row).unwrap_or(0).min(max_row);
    let end_row = usize::try_from(end_row).unwrap_or(0).min(max_row);
    let start_line_chars = lines.get(start_row)?.chars().count();
    let end_line_chars = lines.get(end_row)?.chars().count();
    let start_col = usize::try_from(start_col)
        .unwrap_or(start_line_chars)
        .min(start_line_chars);
    let end_col = usize::try_from(end_col)
        .unwrap_or(end_line_chars)
        .min(end_line_chars);
    let start_offset = row_char_offset(lines, start_row)? + u64::try_from(start_col).ok()?;
    let end_offset = row_char_offset(lines, end_row)? + u64::try_from(end_col).ok()?;
    let (start_offset, end_offset, start_row, end_row) = if start_offset <= end_offset {
        (
            start_offset,
            end_offset,
            u32::try_from(start_row).ok()?,
            u32::try_from(end_row).ok()?,
        )
    } else {
        (
            end_offset,
            start_offset,
            u32::try_from(end_row).ok()?,
            u32::try_from(start_row).ok()?,
        )
    };
    Some((start_offset, end_offset, start_row, end_row))
}

fn row_char_offset(lines: &[String], row: usize) -> Option<u64> {
    let mut offset = 0_u64;
    for line in lines.iter().take(row) {
        offset = offset
            .saturating_add(u64::try_from(line.chars().count()).ok()?)
            .saturating_add(1);
    }
    Some(offset)
}

#[cfg(test)]
mod tests {
    use super::crt_toggle_requested;
    use super::{crt_line_bend, is_paste_shortcut, is_snippet_hotkey_trigger, split_prompt_prefix};
    use dioxus::prelude::Key;

    #[test]
    fn recognizes_ctrl_v_as_paste() {
        assert!(is_paste_shortcut(
            &Key::Character("v".to_string()),
            true,
            false,
            false,
            false,
        ));
    }

    #[test]
    fn rejects_ctrl_alt_v_as_paste() {
        assert!(!is_paste_shortcut(
            &Key::Character("v".to_string()),
            true,
            true,
            false,
            false,
        ));
    }

    #[test]
    fn recognizes_shift_insert_as_paste() {
        assert!(is_paste_shortcut(&Key::Insert, false, false, true, false));
    }

    #[test]
    fn snippet_hotkey_requires_alt_insert() {
        assert!(is_snippet_hotkey_trigger(
            &Key::Insert,
            false,
            true,
            false,
            false,
        ));
        assert!(!is_snippet_hotkey_trigger(
            &Key::Insert,
            false,
            false,
            false,
            false,
        ));
    }

    #[test]
    fn recognizes_wrapped_prompt_suffix() {
        let line = "re/Gestalt$ ";
        assert!(split_prompt_prefix(line).is_some());
    }

    #[test]
    fn recognizes_prompt_fragment_with_user_host_prefix() {
        let line = "jeremy@FizzyPop:/media/jeremy/OrangeCream/Linux Softwa";
        assert!(split_prompt_prefix(line).is_some());
    }

    #[test]
    fn crt_line_bend_is_zero_in_the_middle() {
        assert!(crt_line_bend(1, 3) < 0.01);
    }

    #[test]
    fn crt_line_bend_increases_toward_the_edges() {
        assert!(crt_line_bend(0, 5) > crt_line_bend(2, 5));
        assert!(crt_line_bend(4, 5) > crt_line_bend(2, 5));
    }

    #[test]
    fn recognizes_ctrl_one_for_crt_toggle() {
        assert!(crt_toggle_requested(
            &Key::Character("1".to_string()),
            true,
            false,
            false,
            false
        ));
    }
}
