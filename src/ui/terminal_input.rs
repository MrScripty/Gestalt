use dioxus::events::KeyboardEvent;
use dioxus::html::geometry::PixelsVector2D;
use dioxus::prelude::{Key, ModifiersInteraction, MountedData, ScrollBehavior};
use serde::Deserialize;
use std::rc::Rc;

#[cfg(feature = "native-renderer")]
const TERM_LINE_HEIGHT_PX: f64 = 21.0;
#[cfg(not(feature = "native-renderer"))]
const TERM_LINE_HEIGHT_PX: f64 = 17.0;

#[cfg(feature = "native-renderer")]
const TERM_CHAR_WIDTH_PX: f64 = 11.4;
#[cfg(not(feature = "native-renderer"))]
const TERM_CHAR_WIDTH_PX: f64 = 8.4;
const TERM_PAD_X_PX: f64 = 12.0;
const TERM_PAD_Y_PX: f64 = 12.0;
#[cfg(feature = "native-renderer")]
const NATIVE_SCROLLBAR_WIDTH_PX: f64 = 12.0;
#[cfg(feature = "native-renderer")]
const NATIVE_SCROLLBAR_GAP_PX: f64 = 8.0;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TerminalSelectionSnapshot {
    pub text: String,
    pub start_row: u32,
    pub end_row: u32,
    pub start_col: u32,
    pub end_col: u32,
}

pub(crate) async fn read_clipboard_text() -> Option<String> {
    tokio::task::spawn_blocking(|| {
        let mut clipboard = arboard::Clipboard::new().ok()?;
        clipboard.get_text().ok()
    })
    .await
    .ok()
    .flatten()
}

pub(crate) async fn write_clipboard_text(text: String) -> bool {
    tokio::task::spawn_blocking(move || {
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(clipboard) => clipboard,
            Err(_) => return false,
        };
        clipboard.set_text(text).is_ok()
    })
    .await
    .unwrap_or(false)
}

pub(crate) async fn map_click_to_terminal_cell(
    terminal_body: Rc<MountedData>,
    client_x: f64,
    client_y: f64,
    max_row_count: u16,
    cols: u16,
    ui_scale: f64,
) -> Option<(u16, u16)> {
    let client_rect = terminal_body.get_client_rect().await.ok()?;
    let scroll_offset = terminal_body
        .get_scroll_offset()
        .await
        .unwrap_or_else(|_| PixelsVector2D::new(0.0, 0.0));
    let pad_x = term_pad_x(ui_scale);
    let pad_y = term_pad_y(ui_scale);
    let relative_x = client_x - client_rect.origin.x - pad_x;
    let relative_y = client_y - client_rect.origin.y - pad_y;

    if relative_x < 0.0 || relative_y < 0.0 {
        return None;
    }

    let row = ((relative_y + scroll_offset.y) / term_line_height(ui_scale)).floor() as u16;
    let col = ((relative_x + scroll_offset.x) / term_char_width(ui_scale)).floor() as u16;
    let clamped_row = row.min(max_row_count.saturating_sub(1));
    let clamped_col = col.min(cols.saturating_sub(1));
    Some((clamped_row, clamped_col))
}

pub(crate) async fn scroll_terminal_to_bottom(terminal_body: Rc<MountedData>) -> bool {
    let scroll_size = match terminal_body.get_scroll_size().await {
        Ok(size) => size,
        Err(_) => return false,
    };

    terminal_body
        .scroll(
            PixelsVector2D::new(0.0, scroll_size.height.max(0.0)),
            ScrollBehavior::Instant,
        )
        .await
        .is_ok()
}

pub(crate) async fn select_terminal_round(
    terminal_body_id: String,
    start_row: u16,
    end_row: u16,
) -> bool {
    #[cfg(feature = "native-renderer")]
    {
        let _ = (terminal_body_id, start_row, end_row);
        false
    }

    #[cfg(not(feature = "native-renderer"))]
    {
        use dioxus::document;

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
}

pub(crate) async fn read_terminal_selection(
    terminal_body_id: String,
) -> Option<TerminalSelectionSnapshot> {
    #[cfg(feature = "native-renderer")]
    {
        let _ = terminal_body_id;
        None
    }

    #[cfg(not(feature = "native-renderer"))]
    {
        use dioxus::document;

        let script = format!(
            r#"
const root = document.getElementById({terminal_body_id:?});
if (!root) return "";
const selection = window.getSelection ? window.getSelection() : null;
if (!selection || selection.rangeCount === 0) return "";
const range = selection.getRangeAt(0);
if (!range || range.collapsed) return "";

const withinRoot = (node) => node && (node === root || root.contains(node));
if (!withinRoot(range.commonAncestorContainer)) return "";

const nearestLine = (node) => {{
    if (!node) return null;
    let element = node.nodeType === Node.ELEMENT_NODE ? node : node.parentElement;
    if (!element) return null;
    if (element.classList && element.classList.contains("terminal-line")) return element;
    return element.closest ? element.closest(".terminal-line") : null;
}};

const startLine = nearestLine(range.startContainer);
const endLine = nearestLine(range.endContainer);
if (!startLine || !endLine) return "";
if (!withinRoot(startLine) || !withinRoot(endLine)) return "";

const parseRow = (line) => {{
    const rowValue = Number.parseInt(line.dataset.row ?? "", 10);
    return Number.isFinite(rowValue) ? Math.max(0, rowValue) : null;
}};
const startRow = parseRow(startLine);
const endRow = parseRow(endLine);
if (startRow === null || endRow === null) return "";

const linePrefixLength = (line, container, offset) => {{
    try {{
        const prefix = document.createRange();
        prefix.setStart(line, 0);
        prefix.setEnd(container, offset);
        return Math.max(0, prefix.toString().length);
    }} catch (_) {{
        return 0;
    }}
}};

const startCol = linePrefixLength(startLine, range.startContainer, range.startOffset);
const endCol = linePrefixLength(endLine, range.endContainer, range.endOffset);
const text = selection.toString();
if (!text) return "";

return JSON.stringify({{
    text,
    start_row: startRow,
    end_row: endRow,
    start_col: startCol,
    end_col: endCol,
}});
"#
        );

        let payload = document::eval(&script).join::<String>().await.ok()?;
        if payload.is_empty() {
            return None;
        }
        serde_json::from_str::<TerminalSelectionSnapshot>(&payload).ok()
    }
}

pub(crate) async fn measure_terminal_viewport(
    terminal_body: Rc<MountedData>,
    ui_scale: f64,
    native_terminal_active: bool,
) -> Option<(u16, u16)> {
    let client_rect = terminal_body.get_client_rect().await.ok()?;
    let viewport_width = (client_rect.size.width
        - (term_pad_x(ui_scale) * 2.0)
        - native_scrollbar_chrome_width(ui_scale, native_terminal_active))
        .max(0.0);
    let viewport_height = (client_rect.size.height - (term_pad_y(ui_scale) * 2.0)).max(0.0);
    let cols = (viewport_width / term_char_width(ui_scale))
        .floor()
        .max(8.0) as u16;
    let rows = (viewport_height / term_line_height(ui_scale))
        .floor()
        .max(2.0) as u16;
    Some((rows, cols))
}

#[cfg(feature = "native-renderer")]
pub(crate) async fn measure_native_terminal_viewport(
    viewport_mount: Rc<MountedData>,
    ui_scale: f64,
) -> Option<(u16, u16)> {
    let client_rect = viewport_mount.get_client_rect().await.ok()?;
    let viewport_width = client_rect.size.width.max(0.0);
    let viewport_height = client_rect.size.height.max(0.0);
    let cols = (viewport_width / term_char_width(ui_scale))
        .floor()
        .max(8.0) as u16;
    let rows = (viewport_height / term_line_height(ui_scale))
        .floor()
        .max(2.0) as u16;
    let rows = rows.saturating_sub(1).max(2);
    Some((rows, cols))
}

pub(crate) fn terminal_line_height_px(ui_scale: f64) -> f64 {
    term_line_height(ui_scale)
}

pub(crate) fn cursor_move_bytes(
    from_row: u16,
    from_col: u16,
    target_row: u16,
    target_col: u16,
) -> Vec<u8> {
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

pub(crate) fn key_event_to_bytes(event: &KeyboardEvent) -> Option<Vec<u8>> {
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

fn term_line_height(ui_scale: f64) -> f64 {
    TERM_LINE_HEIGHT_PX * ui_scale
}

fn term_char_width(ui_scale: f64) -> f64 {
    TERM_CHAR_WIDTH_PX * ui_scale
}

fn term_pad_x(ui_scale: f64) -> f64 {
    TERM_PAD_X_PX * ui_scale
}

fn term_pad_y(ui_scale: f64) -> f64 {
    TERM_PAD_Y_PX * ui_scale
}

fn native_scrollbar_chrome_width(ui_scale: f64, native_terminal_active: bool) -> f64 {
    #[cfg(feature = "native-renderer")]
    {
        if native_terminal_active {
            return (NATIVE_SCROLLBAR_WIDTH_PX + NATIVE_SCROLLBAR_GAP_PX) * ui_scale;
        }
    }

    let _ = (ui_scale, native_terminal_active);
    0.0
}
