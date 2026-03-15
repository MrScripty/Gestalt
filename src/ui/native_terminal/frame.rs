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

    pub(crate) fn from_snapshot(snapshot: &TerminalSnapshot, show_cursor: bool) -> Self {
        let rows = snapshot.rows.max(1);
        let cols = snapshot.cols.max(1);
        let visible_lines = visible_window(snapshot, rows);
        let mut cells = vec![NativeTerminalCell::default(); usize::from(rows) * usize::from(cols)];

        for (row_index, line) in visible_lines.iter().enumerate() {
            for (col_index, codepoint) in line.chars().take(usize::from(cols)).enumerate() {
                let cell_index = row_index * usize::from(cols) + col_index;
                cells[cell_index] = NativeTerminalCell { codepoint };
            }
        }

        let cursor = show_cursor.then(|| {
            let window_start = snapshot.lines.len().saturating_sub(usize::from(rows));
            let cursor_row = usize::from(snapshot.cursor_row).saturating_sub(window_start);
            NativeTerminalCursor {
                row: u16::try_from(cursor_row)
                    .unwrap_or(rows.saturating_sub(1))
                    .min(rows.saturating_sub(1)),
                col: snapshot.cursor_col.min(cols.saturating_sub(1)),
            }
        });

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

    pub(crate) fn from_native_frame(frame: &TerminalFrame, show_cursor: bool) -> Self {
        let rows = frame.rows.max(1);
        let cols = frame.cols.max(1);
        let mut cells = vec![NativeTerminalCell::default(); usize::from(rows) * usize::from(cols)];

        if let Some(full_cells) = full_cells(frame) {
            for row in 0..rows {
                let row_offset = usize::from(row) * usize::from(cols);
                for col in 0..cols {
                    let index = row_offset + usize::from(col);
                    if let Some(cell) = full_cells.get(index) {
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
            .map(|cursor| NativeTerminalCursor {
                row: cursor.row.min(rows.saturating_sub(1)),
                col: cursor.col.min(cols.saturating_sub(1)),
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
    ) -> Self {
        let native = Self::from_native_frame(frame, show_cursor);
        if native.is_visibly_blank() && snapshot.lines.iter().any(|line| !line.is_empty()) {
            return Self::from_snapshot(snapshot, show_cursor);
        }
        native
    }
}

fn visible_window(snapshot: &TerminalSnapshot, rows: u16) -> &[String] {
    let line_count = snapshot.lines.len();
    let window_start = line_count.saturating_sub(usize::from(rows));
    &snapshot.lines[window_start..]
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
    use super::NativeTerminalFrame;
    use crate::terminal::TerminalSnapshot;
    use crate::terminal_native::{
        TerminalCell, TerminalCellFlags, TerminalCellPublication, TerminalCursor,
        TerminalCursorShape, TerminalDamage, TerminalFrame,
    };
    use std::sync::Arc;

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

        let frame = NativeTerminalFrame::from_snapshot(&snapshot, true);

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

        let frame = NativeTerminalFrame::from_snapshot(&snapshot, true);
        let cursor = frame.cursor.unwrap();
        assert_eq!(cursor.row, 0);
        assert_eq!(cursor.col, 2);
    }

    #[test]
    fn frame_builds_cells_from_native_frame() {
        let frame = TerminalFrame {
            rows: 2,
            cols: 3,
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

        let native = NativeTerminalFrame::from_native_frame(&frame, true);

        assert_eq!(native.cell(0, 0).codepoint, 'a');
        assert_eq!(native.cell(1, 1).codepoint, ' ');
        assert_eq!(native.cursor.unwrap().row, 1);
        assert_eq!(native.cursor.unwrap().col, 2);
    }
}
