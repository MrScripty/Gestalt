use crate::state::SessionId;
use crate::terminal::TerminalManager;
use dioxus::prelude::{Signal, WritableExt};
use std::collections::HashMap;
use std::sync::Arc;

use super::scaled_cell_height_px;

pub(crate) fn native_scrollbar_thumb_metrics(
    visible_rows: u16,
    total_scroll_range: usize,
    effective_offset: usize,
) -> (f64, f64) {
    if total_scroll_range == 0 {
        return (0.0, 100.0);
    }

    let total_rows = total_scroll_range as f64 + f64::from(visible_rows.max(1));
    let thumb_height = (f64::from(visible_rows.max(1)) / total_rows * 100.0).clamp(8.0, 100.0);
    let track_travel = 100.0 - thumb_height;
    let progress = (effective_offset as f64 / total_scroll_range as f64).clamp(0.0, 1.0);
    let thumb_top = track_travel * (1.0 - progress);
    (thumb_top, thumb_height)
}

pub(crate) fn native_horizontal_scrollbar_thumb_metrics(
    visible_cols: u16,
    total_scroll_range: usize,
    horizontal_offset: usize,
) -> (f64, f64) {
    if total_scroll_range == 0 {
        return (0.0, 100.0);
    }

    let total_cols = total_scroll_range as f64 + f64::from(visible_cols.max(1));
    let thumb_width = (f64::from(visible_cols.max(1)) / total_cols * 100.0).clamp(8.0, 100.0);
    let track_travel = 100.0 - thumb_width;
    let progress = (horizontal_offset as f64 / total_scroll_range as f64).clamp(0.0, 1.0);
    let thumb_left = track_travel * progress;
    (thumb_left, thumb_width)
}

pub(crate) fn native_scroll_track_height_px(visible_rows: u16, ui_scale: f64) -> f64 {
    f64::from(visible_rows.max(1)) * scaled_cell_height_px(ui_scale)
}

pub(crate) fn native_offset_from_vertical_track(
    click_y: f64,
    track_height_px: f64,
    thumb_height_px: f64,
    total_scroll_range: usize,
) -> usize {
    if total_scroll_range == 0 {
        return 0;
    }

    let thumb_travel = (track_height_px - thumb_height_px).max(1.0);
    let thumb_top = (click_y - (thumb_height_px / 2.0)).clamp(0.0, thumb_travel);
    let progress = 1.0 - (thumb_top / thumb_travel);
    (progress * total_scroll_range as f64)
        .round()
        .clamp(0.0, total_scroll_range as f64) as usize
}

pub(crate) fn native_offset_from_horizontal_track(
    click_x: f64,
    track_width_px: f64,
    thumb_width_px: f64,
    total_scroll_range: usize,
) -> usize {
    if total_scroll_range == 0 {
        return 0;
    }

    let thumb_travel = (track_width_px - thumb_width_px).max(1.0);
    let thumb_left = (click_x - (thumb_width_px / 2.0)).clamp(0.0, thumb_travel);
    let progress = thumb_left / thumb_travel;
    (progress * total_scroll_range as f64)
        .round()
        .clamp(0.0, total_scroll_range as f64) as usize
}

#[cfg(feature = "terminal-native-spike")]
pub(crate) fn apply_native_scroll_delta(
    session_id: SessionId,
    delta_lines: i32,
    hidden_rows: usize,
    history_size: usize,
    display_offset: usize,
    local_offset: usize,
    local_offsets: &mut Signal<HashMap<SessionId, usize>>,
    terminal_manager: &Arc<TerminalManager>,
) {
    let current = display_offset + local_offset;
    let max_offset = history_size + hidden_rows;
    let desired = if delta_lines >= 0 {
        current.saturating_add(delta_lines as usize)
    } else {
        current.saturating_sub(delta_lines.unsigned_abs() as usize)
    }
    .min(max_offset);
    let _ = apply_native_scroll_to(
        session_id,
        desired,
        hidden_rows,
        history_size,
        display_offset,
        local_offset,
        local_offsets,
        terminal_manager,
    );
}

#[cfg(feature = "terminal-native-spike")]
pub(crate) fn apply_native_scroll_to(
    session_id: SessionId,
    desired_effective_offset: usize,
    hidden_rows: usize,
    history_size: usize,
    display_offset: usize,
    local_offset: usize,
    local_offsets: &mut Signal<HashMap<SessionId, usize>>,
    terminal_manager: &Arc<TerminalManager>,
) -> bool {
    let max_effective_offset = history_size + hidden_rows;
    let desired_effective_offset = desired_effective_offset.min(max_effective_offset);
    let desired_local = desired_effective_offset.min(hidden_rows);
    let desired_backend = desired_effective_offset.saturating_sub(desired_local);
    let backend_delta = desired_backend as i32 - display_offset as i32;
    if backend_delta != 0 && terminal_manager.scroll_viewport(session_id, backend_delta).is_err() {
        return false;
    }
    if desired_local == 0 {
        local_offsets.write().remove(&session_id);
    } else {
        local_offsets.write().insert(session_id, desired_local);
    }
    backend_delta != 0 || desired_local != local_offset
}
