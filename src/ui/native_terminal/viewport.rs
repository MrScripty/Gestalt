use crate::terminal::TerminalSnapshot;
use crate::terminal_native::TerminalFrame;

use super::frame::{native_frame_content_cols, snapshot_content_cols};
use super::scroll::{native_horizontal_scrollbar_thumb_metrics, native_scrollbar_thumb_metrics};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NativeTerminalViewportMetrics {
    pub visible_rows: u16,
    pub visible_cols: u16,
    pub frame_rows: u16,
    pub hidden_rows: usize,
    pub local_offset: usize,
    pub content_cols: u16,
    pub history_size: usize,
    pub display_offset: usize,
    pub total_scroll_range: usize,
    pub effective_offset: usize,
    pub hidden_cols: usize,
    pub horizontal_offset: usize,
    pub thumb_top_pct: f64,
    pub thumb_height_pct: f64,
    pub horizontal_thumb_left_pct: f64,
    pub horizontal_thumb_width_pct: f64,
}

pub(crate) fn native_terminal_viewport_metrics(
    snapshot: &TerminalSnapshot,
    frame: Option<&TerminalFrame>,
    viewport_size: Option<(u16, u16)>,
    surface_cells: Option<(u16, u16)>,
    local_scroll_offset: usize,
    horizontal_scroll_offset: usize,
) -> NativeTerminalViewportMetrics {
    let visible_rows = viewport_size
        .map(|(rows, _)| rows)
        .or_else(|| surface_cells.map(|(rows, _)| rows))
        .unwrap_or(snapshot.rows)
        .max(1);
    let visible_cols = viewport_size
        .map(|(_, cols)| cols)
        .or_else(|| surface_cells.map(|(_, cols)| cols))
        .unwrap_or(snapshot.cols)
        .max(1);
    let frame_rows = frame.map(|frame| frame.rows.max(1)).unwrap_or(snapshot.rows.max(1));
    let hidden_rows = usize::from(frame_rows.saturating_sub(visible_rows));
    let local_offset = local_scroll_offset.min(hidden_rows);
    let content_cols = frame
        .map(|frame| native_frame_content_cols(frame, visible_rows, local_offset as u16))
        .unwrap_or_else(|| snapshot_content_cols(snapshot, visible_rows, local_offset as u16))
        .max(1);
    let history_size = frame.map(|frame| frame.history_size).unwrap_or(0);
    let display_offset = frame.map(|frame| frame.display_offset).unwrap_or(0);
    let total_scroll_range = history_size + hidden_rows;
    let effective_offset = display_offset + local_offset;
    let hidden_cols = usize::from(content_cols.saturating_sub(visible_cols));
    let horizontal_offset = horizontal_scroll_offset.min(hidden_cols);
    let (thumb_top_pct, thumb_height_pct) =
        native_scrollbar_thumb_metrics(visible_rows, total_scroll_range, effective_offset);
    let (horizontal_thumb_left_pct, horizontal_thumb_width_pct) =
        native_horizontal_scrollbar_thumb_metrics(visible_cols, hidden_cols, horizontal_offset);

    NativeTerminalViewportMetrics {
        visible_rows,
        visible_cols,
        frame_rows,
        hidden_rows,
        local_offset,
        content_cols,
        history_size,
        display_offset,
        total_scroll_range,
        effective_offset,
        hidden_cols,
        horizontal_offset,
        thumb_top_pct,
        thumb_height_pct,
        horizontal_thumb_left_pct,
        horizontal_thumb_width_pct,
    }
}
