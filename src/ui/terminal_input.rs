use dioxus::document;
use dioxus::events::KeyboardEvent;
use dioxus::prelude::{Key, ModifiersInteraction};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TerminalSelectionSnapshot {
    pub text: String,
    pub start_row: u32,
    pub end_row: u32,
    pub start_col: u32,
    pub end_col: u32,
}

pub(crate) const READ_CLIPBOARD_JS: &str = r#"
if (navigator.clipboard && navigator.clipboard.readText) {
    try {
        return await navigator.clipboard.readText();
    } catch (_) {}
}

try {
    const probe = document.createElement("textarea");
    probe.style.position = "fixed";
    probe.style.opacity = "0";
    probe.style.pointerEvents = "none";
    probe.style.left = "-9999px";
    probe.style.top = "0";
    document.body.appendChild(probe);
    probe.focus();
    document.execCommand("paste");
    const text = probe.value || "";
    document.body.removeChild(probe);
    if (text) {
        return text;
    }
} catch (_) {}

return "";
"#;

pub(crate) const COPY_SELECTION_JS: &str = r#"
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

pub(crate) async fn map_click_to_terminal_cell(
    terminal_body_id: String,
    client_x: f64,
    client_y: f64,
    max_row_count: u16,
    cols: u16,
) -> Option<(u16, u16)> {
    let script = format!(
        r#"
const root = document.getElementById({terminal_body_id:?});
if (!root) return "";

const rect = root.getBoundingClientRect();
if (
    {client_x} < rect.left ||
    {client_x} > rect.right ||
    {client_y} < rect.top ||
    {client_y} > rect.bottom
) return "";

const style = window.getComputedStyle(root);
const parsePx = (value, fallback) => {{
    const parsed = Number.parseFloat(value);
    return Number.isFinite(parsed) ? parsed : fallback;
}};

const paddingLeft = parsePx(style.paddingLeft, 0);
const paddingRight = parsePx(style.paddingRight, 0);
const paddingTop = parsePx(style.paddingTop, 0);
const paddingX = paddingLeft + paddingRight;
const lineHeight = Math.max(1, parsePx(style.lineHeight, 17));
const cachedCharWidth = root._gestaltViewportMeasureCache?.charWidth;
const charWidth = Math.max(
    1,
    Number.isFinite(cachedCharWidth) && cachedCharWidth > 0
        ? cachedCharWidth
        : parsePx(style.getPropertyValue("--term-char-width"), 8.4)
);

if (root.classList.contains("crt-enabled")) {{
    const lines = Array.from(root.querySelectorAll(".terminal-line"));
    if (!lines.length) return "";

    const localX = {client_x} - rect.left - paddingLeft + root.scrollLeft;
    const localY = {client_y} - rect.top - paddingTop + root.scrollTop;
    const rowIndex = Math.min(
        lines.length - 1,
        Math.max(0, Math.floor(localY / lineHeight))
    );
    const line = lines[rowIndex];
    const row = Number.parseInt(line.dataset.row ?? `${{rowIndex}}`, 10);
    if (Number.isNaN(row)) return "";

    const lineChars = Number.parseInt(line.dataset.lineChars ?? "0", 10);
    const edgeLoss = Math.min(
        0.35,
        Math.max(0, parsePx(style.getPropertyValue("--crt-edge-loss"), 0.085))
    );
    const rowNorm = lines.length <= 1 ? 0 : ((((rowIndex + 0.5) / lines.length) * 2) - 1);
    const scale = Math.max(0.7, 1 - (edgeLoss * rowNorm * rowNorm));
    const contentWidth = Math.max(root.scrollWidth - paddingX, {cols} * charWidth);
    const centerX = contentWidth / 2;
    const undistortedX = centerX + ((localX - centerX) / scale);
    const col = Math.max(
        0,
        Math.min(
            {cols} - 1,
            Math.min(
                Number.isFinite(lineChars) ? lineChars : {cols} - 1,
                Math.floor(Math.max(0, undistortedX) / charWidth)
            )
        )
    );

    return `${{row}},${{col}}`;
}}

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
    let clamped_row = row.min(max_row_count.saturating_sub(1));
    let clamped_col = col.min(cols.saturating_sub(1));
    Some((clamped_row, clamped_col))
}

pub(crate) async fn install_terminal_scroll_behavior(terminal_body_id: String) -> bool {
    let script = format!(
        r#"
const root = document.getElementById({terminal_body_id:?});
if (!root) return false;
if (root.dataset.scrollManaged === "1") return true;

const threshold = 24;
const isNearBottom = () => (root.scrollHeight - root.clientHeight - root.scrollTop) <= threshold;
const scheduleStickBottom = () => {{
    if (root.dataset.stickBottom !== "1") return;
    if (root._gestaltScrollStickPending) return;
    root._gestaltScrollStickPending = true;
    const flush = () => {{
        root._gestaltScrollStickPending = false;
        if (root.dataset.stickBottom === "1") {{
            root.scrollTop = root.scrollHeight;
        }}
    }};
    if (window.requestAnimationFrame) {{
        window.requestAnimationFrame(flush);
    }} else {{
        setTimeout(flush, 0);
    }}
}};

root.dataset.scrollManaged = "1";
root.dataset.stickBottom = "1";
root._gestaltScrollStickPending = false;

root.addEventListener("scroll", () => {{
    root.dataset.stickBottom = isNearBottom() ? "1" : "0";
}}, {{ passive: true }});

const observer = new MutationObserver(() => {{
    scheduleStickBottom();
}});
observer.observe(root, {{ childList: true, subtree: true }});
root._gestaltScrollObserver = observer;
if (window.ResizeObserver) {{
    const resizeObserver = new ResizeObserver(() => {{
        scheduleStickBottom();
    }});
    resizeObserver.observe(root);
    root._gestaltScrollResizeObserver = resizeObserver;
}}
scheduleStickBottom();
return true;
"#
    );

    document::eval(&script)
        .join::<bool>()
        .await
        .unwrap_or(false)
}

pub(crate) async fn is_terminal_scrolled_near_top(
    terminal_body_id: String,
    threshold_px: u32,
) -> bool {
    let script = format!(
        r#"
const root = document.getElementById({terminal_body_id:?});
if (!root) return false;
return root.scrollTop <= {threshold_px};
"#
    );

    document::eval(&script)
        .join::<bool>()
        .await
        .unwrap_or(false)
}

pub(crate) async fn install_terminal_paste_bridge(terminal_body_id: String) -> bool {
    let script = format!(
        r#"
const root = document.getElementById({terminal_body_id:?});
if (!root) return false;
if (root.dataset.pasteBridgeInstalled === "1") return true;

if (!window.__gestaltPasteBuffer) {{
    window.__gestaltPasteBuffer = Object.create(null);
}}

root.dataset.pasteBridgeInstalled = "1";
root.addEventListener("paste", (event) => {{
    const clipboard = event.clipboardData || window.clipboardData;
    const text = clipboard ? (clipboard.getData("text/plain") || clipboard.getData("Text") || "") : "";
    window.__gestaltPasteBuffer[{terminal_body_id:?}] = text;
}}, true);

return true;
"#
    );

    document::eval(&script)
        .join::<bool>()
        .await
        .unwrap_or(false)
}

pub(crate) async fn take_terminal_paste_buffer(terminal_body_id: String) -> Option<String> {
    let script = format!(
        r#"
const store = window.__gestaltPasteBuffer;
if (!store) return "";
const key = {terminal_body_id:?};
const text = typeof store[key] === "string" ? store[key] : "";
delete store[key];
return text;
"#
    );

    document::eval(&script).join::<String>().await.ok()
}

pub(crate) async fn select_terminal_round(
    terminal_body_id: String,
    start_row: u16,
    end_row: u16,
) -> bool {
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

pub(crate) async fn read_terminal_selection(
    terminal_body_id: String,
) -> Option<TerminalSelectionSnapshot> {
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

pub(crate) async fn measure_terminal_viewport(terminal_body_id: String) -> Option<(u16, u16)> {
    let script = format!(
        r#"
const root = document.getElementById({terminal_body_id:?});
if (!root) return "";

if (root.dataset.viewportObserverInstalled !== "1") {{
    root.dataset.viewportObserverInstalled = "1";
    root._gestaltViewportDirty = true;
    if (window.ResizeObserver) {{
        const observer = new ResizeObserver(() => {{
            root._gestaltViewportDirty = true;
        }});
        observer.observe(root);
        root._gestaltViewportObserver = observer;
    }}
}}

const style = window.getComputedStyle(root);
const parsePx = (value, fallback) => {{
    const parsed = Number.parseFloat(value);
    return Number.isFinite(parsed) ? parsed : fallback;
}};

const paddingX = parsePx(style.paddingLeft, 0) + parsePx(style.paddingRight, 0);
const paddingY = parsePx(style.paddingTop, 0) + parsePx(style.paddingBottom, 0);
const lineHeight = Math.max(1, parsePx(style.lineHeight, 17));
const styleKey = `${{style.font}}|${{style.letterSpacing}}|${{lineHeight}}|${{paddingX}}|${{paddingY}}`;

const viewportWidth = Math.max(0, root.clientWidth - paddingX);
const viewportHeight = Math.max(0, root.clientHeight - paddingY);

const cached = root._gestaltViewportMeasureCache || null;
if (cached) {{
    const dimensionsChanged =
        cached.viewportWidth !== viewportWidth || cached.viewportHeight !== viewportHeight;
    const styleChanged = cached.styleKey !== styleKey;
    const dirty = root._gestaltViewportDirty !== false;
    if (!dirty && !dimensionsChanged && !styleChanged) {{
        return "";
    }}
}}

let charWidth = parsePx(style.getPropertyValue("--term-char-width"), 8.4);
if (!cached || cached.styleKey !== styleKey || !(cached.charWidth > 0)) {{
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
}} else {{
    charWidth = cached.charWidth;
}}
charWidth = Math.max(1, charWidth);

const cols = Math.max(8, Math.floor(viewportWidth / charWidth));
const rows = Math.max(2, Math.floor(viewportHeight / lineHeight));

root._gestaltViewportMeasureCache = {{
    styleKey,
    charWidth,
    viewportWidth,
    viewportHeight,
    rows,
    cols,
}};
root._gestaltViewportDirty = false;

return `${{rows}},${{cols}}`;
"#
    );

    let measured = document::eval(&script).join::<String>().await.ok()?;
    parse_row_col(&measured)
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

fn parse_row_col(input: &str) -> Option<(u16, u16)> {
    let (row, col) = input.trim().split_once(',')?;
    let row = row.trim().parse::<u16>().ok()?;
    let col = col.trim().parse::<u16>().ok()?;
    Some((row, col))
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
