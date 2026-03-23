use crate::terminal::TerminalSnapshot;
use crate::terminal_native::{
    TerminalCell, TerminalCellFlags, TerminalCellPublication, TerminalCursorShape, TerminalFrame,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeTerminalFrame {
    pub rows: u16,
    pub cols: u16,
    pub cells: Vec<NativeTerminalCell>,
    pub cursor: Option<NativeTerminalCursor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeTerminalCell {
    pub codepoint: char,
}

impl Default for NativeTerminalCell {
    fn default() -> Self {
        Self { codepoint: ' ' }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeTerminalCursor {
    pub row: u16,
    pub col: u16,
}

impl NativeTerminalFrame {
    pub(crate) fn is_visibly_blank(&self) -> bool {
        self.cells.iter().all(|cell| cell.codepoint == ' ')
    }

    pub(crate) fn from_snapshot(
        snapshot: &TerminalSnapshot,
        show_cursor: bool,
        visible_rows: u16,
        local_scroll_offset: u16,
        visible_cols: u16,
        horizontal_scroll_offset: u16,
    ) -> Self {
        let rows = visible_rows.max(1).min(snapshot.rows.max(1));
        let cols = visible_cols.max(1).min(snapshot.cols.max(1));
        let left_col = horizontal_scroll_offset.min(snapshot.cols.saturating_sub(cols));
        let visible_lines = visible_window(snapshot, rows, local_scroll_offset);
        let mut cells = vec![NativeTerminalCell::default(); usize::from(rows) * usize::from(cols)];

        for (row_index, line) in visible_lines.iter().enumerate() {
            for (col_index, codepoint) in line
                .chars()
                .skip(usize::from(left_col))
                .take(usize::from(cols))
                .enumerate()
            {
                let cell_index = row_index * usize::from(cols) + col_index;
                cells[cell_index] = NativeTerminalCell { codepoint };
            }
        }

        let cursor = show_cursor.then(|| {
            let window_start = visible_window_start(snapshot, rows, local_scroll_offset);
            let cursor_row = usize::from(snapshot.cursor_row).saturating_sub(window_start);
            if snapshot.cursor_col < left_col || snapshot.cursor_col >= left_col.saturating_add(cols)
            {
                return None;
            }
            NativeTerminalCursor {
                row: u16::try_from(cursor_row)
                    .unwrap_or(rows.saturating_sub(1))
                    .min(rows.saturating_sub(1)),
                col: snapshot
                    .cursor_col
                    .saturating_sub(left_col)
                    .min(cols.saturating_sub(1)),
            }
            .into()
        })
        .flatten();

        Self {
            rows,
            cols,
            cells,
            cursor,
        }
    }

    pub(crate) fn cell(&self, row: u16, col: u16) -> NativeTerminalCell {
        let index = usize::from(row) * usize::from(self.cols) + usize::from(col);
        self.cells.get(index).copied().unwrap_or_default()
    }

    pub(crate) fn from_native_frame(
        frame: &TerminalFrame,
        show_cursor: bool,
        visible_rows: u16,
        local_scroll_offset: u16,
        visible_cols: u16,
        horizontal_scroll_offset: u16,
    ) -> Self {
        let rows = visible_rows.max(1).min(frame.rows.max(1));
        let cols = visible_cols.max(1).min(frame.cols.max(1));
        let left_col = horizontal_scroll_offset.min(frame.cols.saturating_sub(cols));
        let mut cells = vec![NativeTerminalCell::default(); usize::from(rows) * usize::from(cols)];
        let frame_row_offset = usize::from(frame.rows.max(rows) - rows) - usize::from(local_scroll_offset.min(frame.rows.saturating_sub(rows)));

        if let Some(full_cells) = full_cells(frame) {
            for row in 0..rows {
                let source_row = usize::from(row) + frame_row_offset;
                let source_row_offset = source_row * usize::from(frame.cols);
                let row_offset = usize::from(row) * usize::from(cols);
                for col in 0..cols {
                    let source_index =
                        source_row_offset + usize::from(left_col) + usize::from(col);
                    let index = row_offset + usize::from(col);
                    if let Some(cell) = full_cells.get(source_index) {
                        cells[index] = NativeTerminalCell {
                            codepoint: native_codepoint(cell),
                        };
                    }
                }
            }
        }

        let cursor = show_cursor
            .then_some(frame.cursor)
            .filter(|cursor| !matches!(cursor.shape, TerminalCursorShape::Hidden))
            .and_then(|cursor| {
                let top_row = frame_row_offset as u16;
                if cursor.row < top_row {
                    return None;
                }
                if cursor.col < left_col || cursor.col >= left_col.saturating_add(cols) {
                    return None;
                }
                Some(NativeTerminalCursor {
                    row: cursor
                        .row
                        .saturating_sub(top_row)
                        .min(rows.saturating_sub(1)),
                    col: cursor
                        .col
                        .saturating_sub(left_col)
                        .min(cols.saturating_sub(1)),
                })
            });

        Self {
            rows,
            cols,
            cells,
            cursor,
        }
    }

    pub(crate) fn from_native_or_snapshot(
        frame: &TerminalFrame,
        snapshot: &TerminalSnapshot,
        show_cursor: bool,
        visible_rows: u16,
        local_scroll_offset: u16,
        visible_cols: u16,
        horizontal_scroll_offset: u16,
    ) -> Self {
        let native = Self::from_native_frame(
            frame,
            show_cursor,
            visible_rows,
            local_scroll_offset,
            visible_cols,
            horizontal_scroll_offset,
        );
        if native.is_visibly_blank() && snapshot.lines.iter().any(|line| !line.is_empty()) {
            return Self::from_snapshot(
                snapshot,
                show_cursor,
                visible_rows,
                local_scroll_offset,
                visible_cols,
                horizontal_scroll_offset,
            );
        }
        native
    }
}

pub(crate) fn snapshot_content_cols(
    snapshot: &TerminalSnapshot,
    visible_rows: u16,
    local_scroll_offset: u16,
) -> u16 {
    let rows = visible_rows.max(1).min(snapshot.rows.max(1));
    let window_start = visible_window_start(snapshot, rows, local_scroll_offset);
    let mut max_cols = visible_window(snapshot, rows, local_scroll_offset)
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let cursor_row = usize::from(snapshot.cursor_row);
    if cursor_row >= window_start && cursor_row < window_start.saturating_add(usize::from(rows)) {
        max_cols = max_cols.max(usize::from(snapshot.cursor_col.saturating_add(1)));
    }
    u16::try_from(max_cols)
        .unwrap_or(u16::MAX)
        .max(1)
        .min(snapshot.cols.max(1))
}

pub(crate) fn native_frame_content_cols(
    frame: &TerminalFrame,
    visible_rows: u16,
    local_scroll_offset: u16,
) -> u16 {
    let rows = visible_rows.max(1).min(frame.rows.max(1));
    let frame_row_offset = usize::from(frame.rows.max(rows) - rows)
        - usize::from(local_scroll_offset.min(frame.rows.saturating_sub(rows)));
    let mut max_cols = 0usize;

    if let Some(full_cells) = full_cells(frame) {
        let width = usize::from(frame.cols);
        for row in 0..rows {
            let source_row = usize::from(row) + frame_row_offset;
            let start = source_row * width;
            let end = start + width;
            let row_cells = match full_cells.get(start..end) {
                Some(row_cells) => row_cells,
                None => continue,
            };
            let occupied_cols = row_cells
                .iter()
                .rposition(|cell| native_codepoint(cell) != ' ')
                .map(|index| index + 1)
                .unwrap_or(0);
            max_cols = max_cols.max(occupied_cols);
        }
    }

    if !matches!(frame.cursor.shape, TerminalCursorShape::Hidden) {
        let top_row = frame_row_offset as u16;
        let bottom_row = top_row.saturating_add(rows);
        if frame.cursor.row >= top_row && frame.cursor.row < bottom_row {
            max_cols = max_cols.max(usize::from(frame.cursor.col.saturating_add(1)));
        }
    }

    u16::try_from(max_cols)
        .unwrap_or(u16::MAX)
        .max(1)
        .min(frame.cols.max(1))
}

fn visible_window(snapshot: &TerminalSnapshot, rows: u16, local_scroll_offset: u16) -> &[String] {
    let window_start = visible_window_start(snapshot, rows, local_scroll_offset);
    &snapshot.lines[window_start..]
}

fn visible_window_start(snapshot: &TerminalSnapshot, rows: u16, local_scroll_offset: u16) -> usize {
    let bottom_start = snapshot.lines.len().saturating_sub(usize::from(rows));
    bottom_start.saturating_sub(usize::from(local_scroll_offset))
}

fn full_cells(frame: &TerminalFrame) -> Option<&[TerminalCell]> {
    match &frame.publication {
        TerminalCellPublication::Full(cells) => Some(cells.as_ref()),
        TerminalCellPublication::Partial(_) => None,
    }
}

fn native_codepoint(cell: &TerminalCell) -> char {
    if cell.flags.contains(TerminalCellFlags::HIDDEN)
        || cell.flags.contains(TerminalCellFlags::WIDE_CHAR_SPACER)
        || cell
            .flags
            .contains(TerminalCellFlags::LEADING_WIDE_CHAR_SPACER)
    {
        ' '
    } else {
        cell.codepoint
    }
}

#[cfg(test)]
mod tests {
    use super::{NativeTerminalFrame, native_frame_content_cols, snapshot_content_cols};
    use crate::terminal::TerminalSnapshot;
    use crate::terminal_native::{
        TerminalCell, TerminalCellFlags, TerminalCellPublication, TerminalCursor,
        TerminalCursorShape, TerminalDamage, TerminalFrame,
    };
    use std::sync::Arc;

    fn snapshot(
        lines: &[&str],
        rows: u16,
        cols: u16,
        cursor_row: u16,
        cursor_col: u16,
    ) -> TerminalSnapshot {
        TerminalSnapshot {
            lines: lines.iter().map(|line| (*line).to_string()).collect(),
            rows,
            cols,
            cursor_row,
            cursor_col,
            hide_cursor: false,
            bracketed_paste: false,
        }
    }

    fn full_frame(
        lines: &[&str],
        rows: u16,
        cols: u16,
        cursor_row: u16,
        cursor_col: u16,
    ) -> TerminalFrame {
        let mut cells = vec![TerminalCell::default(); usize::from(rows) * usize::from(cols)];
        for (row_index, line) in lines.iter().enumerate() {
            for (col_index, codepoint) in line.chars().enumerate() {
                let index = row_index * usize::from(cols) + col_index;
                cells[index].codepoint = codepoint;
            }
        }

        TerminalFrame {
            rows,
            cols,
            history_size: 0,
            cursor: TerminalCursor {
                row: cursor_row,
                col: cursor_col,
                shape: TerminalCursorShape::Block,
            },
            bracketed_paste: false,
            display_offset: 0,
            damage: TerminalDamage::Full,
            publication: TerminalCellPublication::Full(Arc::new(cells)),
        }
    }

    #[test]
    fn frame_uses_last_visible_rows() {
        let snapshot = TerminalSnapshot {
            lines: vec!["one".into(), "two".into(), "three".into()],
            rows: 2,
            cols: 5,
            cursor_row: 2,
            cursor_col: 2,
            hide_cursor: false,
            bracketed_paste: false,
        };

        let frame = NativeTerminalFrame::from_snapshot(&snapshot, true, 2, 0, 5, 0);

        assert_eq!(frame.rows, 2);
        assert_eq!(frame.cell(0, 0).codepoint, 't');
        assert_eq!(frame.cell(1, 0).codepoint, 't');
        assert_eq!(frame.cursor.unwrap().row, 1);
    }

    #[test]
    fn frame_clamps_cursor_inside_viewport() {
        let snapshot = TerminalSnapshot {
            lines: vec!["prompt".into()],
            rows: 1,
            cols: 3,
            cursor_row: 0,
            cursor_col: 99,
            hide_cursor: false,
            bracketed_paste: false,
        };

        let frame = NativeTerminalFrame::from_snapshot(&snapshot, true, 1, 0, 3, 0);
        let cursor = frame.cursor.unwrap();
        assert_eq!(cursor.row, 0);
        assert_eq!(cursor.col, 2);
    }

    #[test]
    fn frame_crops_snapshot_columns_for_horizontal_scroll() {
        let snapshot = TerminalSnapshot {
            lines: vec!["abcdef".into()],
            rows: 1,
            cols: 6,
            cursor_row: 0,
            cursor_col: 4,
            hide_cursor: false,
            bracketed_paste: false,
        };

        let frame = NativeTerminalFrame::from_snapshot(&snapshot, true, 1, 0, 3, 2);

        assert_eq!(frame.cols, 3);
        assert_eq!(frame.cell(0, 0).codepoint, 'c');
        assert_eq!(frame.cell(0, 2).codepoint, 'e');
        assert_eq!(frame.cursor.unwrap().col, 2);
    }

    #[test]
    fn frame_builds_cells_from_native_frame() {
        let frame = TerminalFrame {
            rows: 2,
            cols: 3,
            history_size: 0,
            cursor: TerminalCursor {
                row: 1,
                col: 2,
                shape: TerminalCursorShape::Block,
            },
            bracketed_paste: false,
            display_offset: 0,
            damage: TerminalDamage::Full,
            publication: TerminalCellPublication::Full(Arc::new(vec![
                TerminalCell {
                    codepoint: 'a',
                    ..TerminalCell::default()
                },
                TerminalCell {
                    codepoint: 'b',
                    ..TerminalCell::default()
                },
                TerminalCell {
                    codepoint: 'c',
                    ..TerminalCell::default()
                },
                TerminalCell {
                    codepoint: 'd',
                    ..TerminalCell::default()
                },
                TerminalCell {
                    codepoint: 'e',
                    flags: TerminalCellFlags::HIDDEN,
                    ..TerminalCell::default()
                },
                TerminalCell {
                    codepoint: 'f',
                    ..TerminalCell::default()
                },
            ])),
        };

        let native = NativeTerminalFrame::from_native_frame(&frame, true, 2, 0, 3, 0);

        assert_eq!(native.cell(0, 0).codepoint, 'a');
        assert_eq!(native.cell(1, 1).codepoint, ' ');
        assert_eq!(native.cursor.unwrap().row, 1);
        assert_eq!(native.cursor.unwrap().col, 2);
    }

    #[test]
    fn frame_crops_native_columns_for_horizontal_scroll() {
        let frame = TerminalFrame {
            rows: 1,
            cols: 5,
            history_size: 0,
            cursor: TerminalCursor {
                row: 0,
                col: 3,
                shape: TerminalCursorShape::Block,
            },
            bracketed_paste: false,
            display_offset: 0,
            damage: TerminalDamage::Full,
            publication: TerminalCellPublication::Full(Arc::new(vec![
                TerminalCell {
                    codepoint: 'a',
                    ..TerminalCell::default()
                },
                TerminalCell {
                    codepoint: 'b',
                    ..TerminalCell::default()
                },
                TerminalCell {
                    codepoint: 'c',
                    ..TerminalCell::default()
                },
                TerminalCell {
                    codepoint: 'd',
                    ..TerminalCell::default()
                },
                TerminalCell {
                    codepoint: 'e',
                    ..TerminalCell::default()
                },
            ])),
        };

        let native = NativeTerminalFrame::from_native_frame(&frame, true, 1, 0, 2, 2);

        assert_eq!(native.cols, 2);
        assert_eq!(native.cell(0, 0).codepoint, 'c');
        assert_eq!(native.cell(0, 1).codepoint, 'd');
        assert_eq!(native.cursor.unwrap().col, 1);
    }

    #[test]
    fn snapshot_content_cols_uses_visible_window_only() {
        let snapshot = snapshot(&["abcdefghij", "zz", "1234"], 2, 20, 2, 1);

        assert_eq!(snapshot_content_cols(&snapshot, 2, 0), 4);
    }

    #[test]
    fn snapshot_content_cols_includes_cursor_beyond_text() {
        let snapshot = snapshot(&[""], 1, 20, 0, 5);

        assert_eq!(snapshot_content_cols(&snapshot, 1, 0), 6);
    }

    #[test]
    fn native_frame_content_cols_uses_visible_rows_only() {
        let frame = full_frame(&["abcdefghij", "zz", "1234"], 3, 20, 2, 1);

        assert_eq!(native_frame_content_cols(&frame, 2, 0), 4);
    }

    #[test]
    fn native_frame_content_cols_includes_cursor_beyond_text() {
        let frame = full_frame(&[""], 1, 20, 0, 7);

        assert_eq!(native_frame_content_cols(&frame, 1, 0), 8);
    }
}
