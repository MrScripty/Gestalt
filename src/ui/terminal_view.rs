use crate::commands::CommandId;
use crate::state::{AppState, SessionId, SessionStatus};
use crate::terminal::{TerminalManager, TerminalSnapshot};
use crate::ui::command_palette::{InsertCommandPalette, PaletteRow};
use crate::ui::insert_command_mode::{
    InsertModeOutcome, InsertModeSelection, InsertModeState, KeyModifiers, TerminalKeyRoute,
    command_matches, mode_after_blur, mode_after_focus, route_terminal_key, selected_command_id,
};
use crate::ui::terminal_input::{
    COPY_SELECTION_JS, READ_CLIPBOARD_JS, cursor_move_bytes, install_terminal_paste_bridge,
    install_terminal_scroll_behavior, key_event_to_bytes, map_click_to_terminal_cell,
    select_terminal_round, take_terminal_paste_buffer,
};
use dioxus::document;
use dioxus::prelude::*;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub(crate) struct TerminalInteractionSignals {
    pub app_state: Signal<AppState>,
    pub focused_terminal: Signal<Option<SessionId>>,
    pub round_anchor: Signal<Option<(SessionId, u16)>>,
    pub insert_mode_state: Signal<Option<InsertModeState>>,
}

pub(crate) fn terminal_shell(
    session_id: SessionId,
    terminal_is_focused: bool,
    terminal: Arc<TerminalSnapshot>,
    terminal_manager: Arc<TerminalManager>,
    interaction: TerminalInteractionSignals,
) -> Element {
    let app_state = interaction.app_state;
    let mut focused_terminal = interaction.focused_terminal;
    let mut round_anchor = interaction.round_anchor;
    let mut insert_mode_state = interaction.insert_mode_state;
    let shell_class = if terminal_is_focused {
        "terminal-shell focused"
    } else {
        "terminal-shell"
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
    let shell_id_for_mount = terminal_shell_id.clone();
    let shell_id_for_paste_event = terminal_shell_id.clone();
    let terminal_manager_for_click = terminal_manager.clone();
    let terminal_manager_for_keydown = terminal_manager;
    let terminal_manager_for_paste = terminal_manager_for_keydown.clone();
    let round_anchor_row_global = match *round_anchor.read() {
        Some((anchor_session, row)) if anchor_session == session_id => row,
        _ => cursor_row,
    };
    let current_insert_mode = insert_mode_state.read().clone();
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

    rsx! {
        div {
            class: "{shell_class}",
            id: "{terminal_shell_id}",
            tabindex: "0",
            onfocus: move |_| {
                focused_terminal.set(Some(session_id));
                let mode_snapshot = insert_mode_state.read().clone();
                insert_mode_state.set(mode_after_focus(mode_snapshot, session_id));
            },
            onblur: move |_| {
                let is_current = *focused_terminal.read() == Some(session_id);
                if is_current {
                    focused_terminal.set(None);
                }
                let mode_snapshot = insert_mode_state.read().clone();
                insert_mode_state.set(mode_after_blur(mode_snapshot, session_id));
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

                    let target_row_global = click_window_start.saturating_add(usize::from(target_row));
                    let target_row_global = u16::try_from(target_row_global).unwrap_or(u16::MAX);

                    round_anchor.set(Some((session_id, target_row_global)));
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

                let active_insert_mode = insert_mode_state
                    .read()
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
                        insert_mode_state.set(Some(InsertModeState {
                            session_id,
                            query: String::new(),
                            highlighted_index: 0,
                        }));
                        return;
                    }
                    TerminalKeyRoute::HandleMode(outcome) => {
                        event.prevent_default();
                        event.stop_propagation();
                        match outcome {
                            InsertModeOutcome::Keep(next_mode) => {
                                insert_mode_state.set(Some(next_mode));
                            }
                            InsertModeOutcome::Close => {
                                insert_mode_state.set(None);
                            }
                            InsertModeOutcome::Submit(command_id) => {
                                submit_insert_command(
                                    command_id,
                                    &terminal_manager_for_keydown,
                                    app_state,
                                    session_id,
                                );
                                insert_mode_state.set(None);
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
                if let Some(mode) = insert_mode_state.read().clone()
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
                class: "terminal-body",
                id: "{terminal_body_id}",
                style: "{body_style}",
                onmounted: move |_| {
                    let body_id = body_id_for_mount.clone();
                    let shell_id = shell_id_for_mount.clone();
                    spawn(async move {
                        let _ = install_terminal_scroll_behavior(body_id).await;
                        let _ = install_terminal_paste_bridge(shell_id).await;
                    });
                },
                div { class: "terminal-grid",
                    for row_idx in 0..rendered_lines.len() {
                        {
                            let line = rendered_lines
                                .get(row_idx)
                                .map(|line| line.as_str())
                                .unwrap_or_default();
                            let actual_row_idx = window_start.saturating_add(row_idx);
                            rsx! {
                                div {
                                    class: "terminal-line",
                                    key: "line-{session_id}-{actual_row_idx}",
                                    "data-row": "{actual_row_idx}",
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

pub(crate) fn pending_terminal_snapshot() -> TerminalSnapshot {
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

fn is_paste_shortcut(key: &Key, ctrl: bool, alt: bool, shift: bool, meta: bool) -> bool {
    match key {
        Key::Character(text) => text.eq_ignore_ascii_case("v") && (ctrl || meta) && !alt,
        Key::Insert => shift && !ctrl && !alt && !meta,
        _ => false,
    }
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

#[cfg(test)]
mod tests {
    use super::is_paste_shortcut;
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
}
