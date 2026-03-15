use crate::commands::CommandId;
use crate::emily_bridge::{EmilyBridge, SnippetIngestRequest};
use crate::state::{AppState, NewSnippet, SessionId, SessionStatus};
use crate::terminal::{TerminalManager, TerminalSnapshot};
use crate::ui::command_palette::{InsertCommandPalette, PaletteRow};
use crate::ui::insert_command_mode::{
    InsertModeOutcome, InsertModeSelection, InsertModeState, KeyModifiers, TerminalKeyRoute,
    command_matches, mode_after_blur, mode_after_focus, route_terminal_key, selected_command_id,
};
#[cfg(feature = "native-renderer")]
use crate::ui::native_terminal::{NativeTerminalBody, native_terminal_pilot_active_for_pane};
use crate::ui::terminal_input::{
    cursor_move_bytes, key_event_to_bytes, map_click_to_terminal_cell, read_clipboard_text,
    read_terminal_selection, scroll_terminal_to_bottom, select_terminal_round,
    write_clipboard_text,
};
use crate::ui::{EMILY_HISTORY_BACKFILL_PAGE_LINES, TerminalHistoryState, UiState};
use dioxus::prelude::*;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const INSERT_CHORD_TIMEOUT_MS: u64 = 1_000;
const MAX_SNIPPET_TEXT_BYTES: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SnippetHotkeyState {
    pub session_id: SessionId,
    pub armed_at_unix_ms: i64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalInteractionSignals {
    pub app_state: Signal<AppState>,
    pub ui_state: Signal<UiState>,
    pub terminal_body_mounts: Signal<HashMap<SessionId, Rc<MountedData>>>,
    pub terminal_body_stick_bottom: Signal<HashMap<SessionId, bool>>,
    pub snippet_hotkey_state: Signal<Option<SnippetHotkeyState>>,
}

pub(crate) fn terminal_shell(
    session_id: SessionId,
    source_cwd: String,
    terminal_is_focused: bool,
    terminal_is_selected: bool,
    terminal: Arc<TerminalSnapshot>,
    terminal_manager: Arc<TerminalManager>,
    emily_bridge: Arc<EmilyBridge>,
    interaction: TerminalInteractionSignals,
) -> Element {
    let app_state = interaction.app_state;
    let mut ui_state = interaction.ui_state;
    let mut terminal_body_mounts = interaction.terminal_body_mounts;
    let mut terminal_body_stick_bottom = interaction.terminal_body_stick_bottom;
    let snippet_hotkey_state = interaction.snippet_hotkey_state;
    let mut shell_mount = use_signal(|| None::<Rc<MountedData>>);
    #[cfg(not(feature = "native-renderer"))]
    let _ = terminal_is_selected;
    let crt_enabled = app_state.read().crt_enabled();
    let shell_class = match (terminal_is_focused, crt_enabled) {
        (true, true) => "terminal-shell focused crt-enabled",
        (true, false) => "terminal-shell focused",
        (false, true) => "terminal-shell crt-enabled",
        (false, false) => "terminal-shell",
    };
    #[cfg(feature = "native-renderer")]
    let native_terminal_active =
        native_terminal_pilot_active_for_pane(terminal_is_selected) && !crt_enabled;
    #[cfg(not(feature = "native-renderer"))]
    let native_terminal_active = false;
    let body_class = if crt_enabled {
        "terminal-body crt-enabled"
    } else if native_terminal_active {
        "terminal-body native-terminal-active"
    } else {
        "terminal-body"
    };
    let body_style = format!(
        "--term-rows: {}; --term-cols: {};",
        terminal.rows, terminal.cols
    );
    #[cfg(feature = "native-renderer")]
    const RENDER_WINDOW_MULTIPLIER: usize = 2;
    #[cfg(not(feature = "native-renderer"))]
    const RENDER_WINDOW_MULTIPLIER: usize = 8;
    #[cfg(feature = "native-renderer")]
    const RENDER_WINDOW_MIN_ROWS: usize = 48;
    #[cfg(not(feature = "native-renderer"))]
    const RENDER_WINDOW_MIN_ROWS: usize = 256;
    let line_count = terminal.lines.len().max(1);
    let max_render_rows_u16 = u16::try_from(line_count).unwrap_or(u16::MAX);
    let cursor_row = terminal
        .cursor_row
        .min(max_render_rows_u16.saturating_sub(1));
    let cursor_col = terminal.cursor_col.min(terminal.cols.saturating_sub(1));
    let render_window_rows = if native_terminal_active {
        usize::from(terminal.rows.max(1))
    } else {
        usize::from(terminal.rows)
            .saturating_mul(RENDER_WINDOW_MULTIPLIER)
            .max(RENDER_WINDOW_MIN_ROWS)
    };
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
    let body_id_for_round_select = terminal_body_id.clone();
    let body_id_for_snippet_capture = terminal_body_id.clone();
    let body_id_for_copy = terminal_body_id.clone();
    let terminal_manager_for_click = terminal_manager.clone();
    let terminal_manager_for_keydown = terminal_manager;
    let terminal_manager_for_paste_shortcut = terminal_manager_for_keydown.clone();
    let terminal_manager_for_paste_event = terminal_manager_for_keydown.clone();
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
    #[cfg(not(feature = "native-renderer"))]
    let snippet_row_ranges = {
        let state = app_state.read();
        state
            .snippets_for_session(session_id)
            .into_iter()
            .map(|snippet| (snippet.log_ref.start_row, snippet.log_ref.end_row))
            .collect::<Vec<_>>()
    };
    let body_mount = terminal_body_mounts.read().get(&session_id).cloned();
    let stick_to_bottom = terminal_body_stick_bottom
        .read()
        .get(&session_id)
        .copied()
        .unwrap_or(true);
    let ui_scale = app_state.read().ui_scale();
    #[cfg(feature = "native-renderer")]
    let native_frame = if native_terminal_active {
        terminal_manager_for_keydown.native_frame_shared(session_id)
    } else {
        None
    };
    #[cfg(feature = "native-renderer")]
    let mut native_input_buffer = use_signal(String::new);
    #[cfg(feature = "native-renderer")]
    let native_input_value = native_input_buffer.read().clone();

    {
        let body_mount = body_mount.clone();
        let rendered_line_count = rendered_lines.len();
        use_effect(move || {
            if !stick_to_bottom || rendered_line_count == 0 {
                return;
            }
            let Some(body_mount) = body_mount.clone() else {
                return;
            };
            spawn(async move {
                let _ = scroll_terminal_to_bottom(body_mount).await;
            });
        });
    }

    let shell_terminal_manager_for_keydown = terminal_manager_for_keydown.clone();
    let native_terminal_manager_for_keydown = shell_terminal_manager_for_keydown.clone();
    let native_terminal_manager_for_input = native_terminal_manager_for_keydown.clone();
    let shell_terminal_manager_for_paste_shortcut = terminal_manager_for_paste_shortcut.clone();
    let native_terminal_manager_for_paste_shortcut =
        shell_terminal_manager_for_paste_shortcut.clone();
    let shell_terminal_manager_for_paste_event = terminal_manager_for_paste_event.clone();
    let native_terminal_manager_for_paste_event = shell_terminal_manager_for_paste_event.clone();
    let shell_emily_bridge_for_snippet = emily_bridge_for_snippet.clone();
    let native_emily_bridge_for_snippet = shell_emily_bridge_for_snippet.clone();
    let shell_terminal_snapshot = terminal.clone();
    let native_terminal_snapshot = shell_terminal_snapshot.clone();
    let shell_source_cwd = source_cwd.clone();
    let native_source_cwd = shell_source_cwd.clone();
    let shell_body_id_for_snippet_capture = body_id_for_snippet_capture.clone();
    let native_body_id_for_snippet_capture = shell_body_id_for_snippet_capture.clone();
    let shell_body_id_for_round_select = body_id_for_round_select.clone();
    let native_body_id_for_round_select = shell_body_id_for_round_select.clone();
    let shell_body_id_for_copy = body_id_for_copy.clone();
    let native_body_id_for_copy = shell_body_id_for_copy.clone();
    let shell_terminal_manager_for_click = terminal_manager_for_click.clone();
    let body_terminal_manager_for_click = terminal_manager_for_click.clone();
    let shell_click_mount = shell_mount;
    #[cfg(feature = "native-renderer")]
    let native_terminal_body = rsx! {
        NativeTerminalBody {
            key: "native-terminal-{session_id}",
            terminal: terminal.clone(),
            native_frame: native_frame.clone(),
            show_caret: show_caret,
            input_value: native_input_value.clone(),
            onclick: move |event| {
                handle_terminal_click(
                    event,
                    session_id,
                    app_state,
                    ui_state,
                    terminal_body_mounts,
                    &body_terminal_manager_for_click,
                    ui_scale,
                    click_rows,
                    click_cols,
                    click_window_start,
                    click_cursor_row_local,
                    click_cursor_row,
                    click_cursor_col,
                );
            },
            onfocus: move |_| {
                focus_terminal_session(ui_state, session_id);
            },
            onblur: move |_| {
                blur_terminal_session(ui_state, session_id);
            },
            onkeydown: move |event: KeyboardEvent| {
                handle_terminal_keydown(
                    event,
                    session_id,
                    app_state,
                    ui_state,
                    snippet_hotkey_state,
                    &native_terminal_manager_for_keydown,
                    &native_terminal_manager_for_paste_shortcut,
                    &native_emily_bridge_for_snippet,
                    &native_terminal_snapshot,
                    &native_source_cwd,
                    &native_body_id_for_snippet_capture,
                    &native_body_id_for_round_select,
                    &native_body_id_for_copy,
                    round_anchor_row_global,
                    bracketed_paste,
                );
            },
            oninput: move |event: FormEvent| {
                let value = event.value();
                if value.is_empty() {
                    return;
                }
                let mode_snapshot = ui_state.read().insert_mode_state.clone();
                if let Some(mode) = mode_snapshot
                    && mode.session_id == session_id
                {
                    let mut next_mode = mode;
                    next_mode.query.push_str(&value);
                    next_mode.highlighted_index = 0;
                    ui_state.write().insert_mode_state = Some(next_mode);
                } else {
                    send_input_to_session(
                        &native_terminal_manager_for_input,
                        app_state,
                        session_id,
                        value.as_bytes(),
                    );
                }
                native_input_buffer.set(String::new());
            },
            onpaste: move |event| {
                handle_terminal_paste(
                    event,
                    session_id,
                    app_state,
                    ui_state,
                    &native_terminal_manager_for_paste_event,
                    bracketed_paste,
                );
            },
        }
    };
    #[cfg(not(feature = "native-renderer"))]
    let native_terminal_body = rsx! { div {} };

    rsx! {
        div {
            class: "{shell_class}",
            id: "{terminal_shell_id}",
            tabindex: "0",
            onmounted: move |event| shell_mount.set(Some(event.data())),
            onfocus: move |_| {
                if native_terminal_active {
                    return;
                }
                focus_terminal_session(ui_state, session_id);
            },
            onblur: move |_| {
                if native_terminal_active {
                    return;
                }
                blur_terminal_session(ui_state, session_id);
            },
            onclick: move |event| {
                if native_terminal_active {
                    return;
                }
                handle_terminal_click(
                    event,
                    session_id,
                    app_state,
                    ui_state,
                    terminal_body_mounts,
                    &shell_terminal_manager_for_click,
                    ui_scale,
                    click_rows,
                    click_cols,
                    click_window_start,
                    click_cursor_row_local,
                    click_cursor_row,
                    click_cursor_col,
                );
                if let Some(shell_mount) = shell_click_mount.read().clone() {
                    spawn(async move {
                        let _ = shell_mount.set_focus(true).await;
                    });
                }
            },
            onkeydown: move |event| {
                handle_terminal_keydown(
                    event,
                    session_id,
                    app_state,
                    ui_state,
                    snippet_hotkey_state,
                    &shell_terminal_manager_for_keydown,
                    &shell_terminal_manager_for_paste_shortcut,
                    &shell_emily_bridge_for_snippet,
                    &shell_terminal_snapshot,
                    &shell_source_cwd,
                    &shell_body_id_for_snippet_capture,
                    &shell_body_id_for_round_select,
                    &shell_body_id_for_copy,
                    round_anchor_row_global,
                    bracketed_paste,
                );
            },
            onpaste: move |event| {
                handle_terminal_paste(
                    event,
                    session_id,
                    app_state,
                    ui_state,
                    &shell_terminal_manager_for_paste_event,
                    bracketed_paste,
                );
            },

            div {
                class: "{body_class}",
                id: "{terminal_body_id}",
                style: "{body_style}",
                onscroll: move |event| {
                    if native_terminal_active {
                        return;
                    }
                    let scroll = event.data();
                    let distance_from_bottom = f64::from(scroll.scroll_height() - scroll.client_height())
                        - scroll.scroll_top();
                    terminal_body_stick_bottom
                        .write()
                        .insert(session_id, distance_from_bottom <= 24.0);
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

                    let emily_bridge = emily_bridge_for_scroll.clone();
                    let terminal_manager = terminal_manager_for_scroll.clone();
                    let scroll_top = scroll.scroll_top();
                    spawn(async move {
                        const TOP_THRESHOLD_PX: u32 = 20;
                        if scroll_top > f64::from(TOP_THRESHOLD_PX) {
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
                onmounted: move |event| {
                    terminal_body_mounts
                        .write()
                        .insert(session_id, event.data());
                    terminal_body_stick_bottom.write().insert(session_id, true);
                },
                if native_terminal_active {
                    {native_terminal_body}
                } else {
                    div { class: "terminal-grid",
                        for row_idx in 0..rendered_lines.len() {
                            {
                                let line = rendered_lines
                                    .get(row_idx)
                                    .map(|line| line.as_str())
                                    .unwrap_or_default();
                                let actual_row_idx = window_start.saturating_add(row_idx);
                                #[cfg(feature = "native-renderer")]
                                let has_snippet = false;
                                #[cfg(not(feature = "native-renderer"))]
                                let has_snippet = snippet_row_ranges.iter().any(|(start, end)| {
                                    actual_row_idx >= usize::try_from(*start).unwrap_or(usize::MAX)
                                        && actual_row_idx <= usize::try_from(*end).unwrap_or(0)
                                });
                                let line_class = if has_snippet {
                                    "terminal-line snippet-annotated"
                                } else {
                                    "terminal-line"
                                };
                                rsx! {
                                    div {
                                        class: "{line_class}",
                                        key: "line-{session_id}-{actual_row_idx}",
                                        "data-row": "{actual_row_idx}",
                                        if cfg!(feature = "native-renderer") {
                                            {
                                                let native_line = if show_caret
                                                    && actual_row_idx == usize::from(cursor_row)
                                                {
                                                    render_native_terminal_line_with_caret(
                                                        line,
                                                        cursor_col,
                                                    )
                                                } else {
                                                    line.to_string()
                                                };
                                                rsx!(span { class: "terminal-text", "{native_line}" })
                                            }
                                        } else if show_caret && actual_row_idx == usize::from(cursor_row) {
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

fn render_native_terminal_line_with_caret(line: &str, cursor_col: u16) -> String {
    let caret_byte = char_index_to_byte(line, usize::from(cursor_col));
    let mut rendered = String::with_capacity(line.len().saturating_add(1));
    rendered.push_str(&line[..caret_byte]);
    rendered.push('|');
    rendered.push_str(&line[caret_byte..]);
    rendered
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

#[allow(clippy::too_many_arguments)]
fn handle_terminal_click(
    event: MouseEvent,
    session_id: SessionId,
    app_state: Signal<AppState>,
    mut ui_state: Signal<UiState>,
    terminal_body_mounts: Signal<HashMap<SessionId, Rc<MountedData>>>,
    terminal_manager: &Arc<TerminalManager>,
    ui_scale: f64,
    click_rows: u16,
    click_cols: u16,
    click_window_start: usize,
    click_cursor_row_local: usize,
    click_cursor_row: u16,
    click_cursor_col: u16,
) {
    ui_state.write().focused_terminal = Some(session_id);
    let click_position = event.data().client_coordinates();
    let click_x = click_position.x;
    let click_y = click_position.y;
    let body_mount = terminal_body_mounts.read().get(&session_id).cloned();
    let terminal_manager = terminal_manager.clone();
    spawn(async move {
        let Some(body_mount) = body_mount else {
            return;
        };
        let Some((target_row, target_col)) = map_click_to_terminal_cell(
            body_mount, click_x, click_y, click_rows, click_cols, ui_scale,
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
}

fn focus_terminal_session(mut ui_state: Signal<UiState>, session_id: SessionId) {
    let mode_snapshot = ui_state.read().insert_mode_state.clone();
    let mut state = ui_state.write();
    state.focused_terminal = Some(session_id);
    state.insert_mode_state = mode_after_focus(mode_snapshot, session_id);
}

fn blur_terminal_session(mut ui_state: Signal<UiState>, session_id: SessionId) {
    let mode_snapshot = ui_state.read().insert_mode_state.clone();
    let mut state = ui_state.write();
    if state.focused_terminal == Some(session_id) {
        state.focused_terminal = None;
    }
    state.insert_mode_state = mode_after_blur(mode_snapshot, session_id);
}

#[allow(clippy::too_many_arguments)]
fn handle_terminal_keydown(
    event: KeyboardEvent,
    session_id: SessionId,
    mut app_state: Signal<AppState>,
    mut ui_state: Signal<UiState>,
    mut snippet_hotkey_state: Signal<Option<SnippetHotkeyState>>,
    terminal_manager_for_keydown: &Arc<TerminalManager>,
    terminal_manager_for_paste_shortcut: &Arc<TerminalManager>,
    emily_bridge_for_snippet: &Arc<EmilyBridge>,
    terminal: &Arc<TerminalSnapshot>,
    source_cwd: &str,
    body_id_for_snippet_capture: &str,
    body_id_for_round_select: &str,
    body_id_for_copy: &str,
    round_anchor_row_global: u16,
    bracketed_paste: bool,
) {
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
                        let body_id = body_id_for_snippet_capture.to_string();
                        let terminal_lines = terminal.lines.clone();
                        let source_cwd = source_cwd.to_string();
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
                tokio::time::sleep(Duration::from_millis(INSERT_CHORD_TIMEOUT_MS)).await;
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
                        terminal_manager_for_keydown,
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

    if is_paste_shortcut(&key, ctrl, alt, shift, meta) {
        event.prevent_default();
        event.stop_propagation();
        if let Some(mode) = ui_state.read().insert_mode_state.clone()
            && mode.session_id == session_id
        {
            return;
        }
        let terminal_manager = terminal_manager_for_paste_shortcut.clone();
        paste_clipboard_into_terminal(terminal_manager, app_state, session_id, bracketed_paste);
        return;
    }

    if ctrl
        && !alt
        && let Key::Character(text) = &key
    {
        if text.eq_ignore_ascii_case("a") {
            event.prevent_default();
            event.stop_propagation();
            if let Some((start_row, end_row)) =
                terminal_round_bounds(&terminal.lines, round_anchor_row_global)
            {
                let body_id = body_id_for_round_select.to_string();
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
            let body_id = body_id_for_copy.to_string();
            spawn(async move {
                let copied = if let Some(selection) = read_terminal_selection(body_id).await {
                    write_clipboard_text(selection.text).await
                } else {
                    false
                };
                if !copied {
                    send_input_to_session(&terminal_manager, app_state, session_id, &[0x03]);
                }
            });
            return;
        }
    }

    if let Some(input) = key_event_to_bytes(&event) {
        event.prevent_default();
        event.stop_propagation();
        send_input_to_session(terminal_manager_for_keydown, app_state, session_id, &input);
    }
}

fn handle_terminal_paste(
    event: ClipboardEvent,
    session_id: SessionId,
    app_state: Signal<AppState>,
    ui_state: Signal<UiState>,
    terminal_manager_for_paste_event: &Arc<TerminalManager>,
    bracketed_paste: bool,
) {
    event.prevent_default();
    event.stop_propagation();
    if let Some(mode) = ui_state.read().insert_mode_state.clone()
        && mode.session_id == session_id
    {
        return;
    }
    let terminal_manager = terminal_manager_for_paste_event.clone();
    paste_clipboard_into_terminal(terminal_manager, app_state, session_id, bracketed_paste);
}

fn paste_clipboard_into_terminal(
    terminal_manager: Arc<TerminalManager>,
    app_state: Signal<AppState>,
    session_id: SessionId,
    bracketed_paste: bool,
) {
    spawn(async move {
        let clipboard_text = read_clipboard_text().await.unwrap_or_default();

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
    use super::{is_paste_shortcut, is_snippet_hotkey_trigger, split_prompt_prefix};
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
