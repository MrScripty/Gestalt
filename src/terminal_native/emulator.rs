use std::sync::Arc;
use std::time::Instant;

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::{Dimensions, Grid};
use alacritty_terminal::index::{Column, Point};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{
    Config, MIN_COLUMNS, MIN_SCREEN_LINES, RenderableCursor, Term, TermDamage, TermMode,
    point_to_viewport, viewport_to_point,
};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Processor};

use super::model::{
    TerminalCell, TerminalCellFlags, TerminalCellPublication, TerminalCellUpdate, TerminalColor,
    TerminalCursor, TerminalCursorShape, TerminalDamage as FrameDamage, TerminalDamageSpan,
    TerminalFrame,
};

const DEFAULT_ROWS: u16 = 42;
const DEFAULT_COLS: u16 = 140;
const DEFAULT_SCROLLBACK: usize = 10_000;

/// Construction parameters for the feature-gated Alacritty-backed emulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlacrittyEmulatorConfig {
    pub rows: u16,
    pub cols: u16,
    pub scrollback: usize,
}

impl Default for AlacrittyEmulatorConfig {
    fn default() -> Self {
        Self {
            rows: DEFAULT_ROWS,
            cols: DEFAULT_COLS,
            scrollback: DEFAULT_SCROLLBACK,
        }
    }
}

/// Local terminal emulator that projects Alacritty grid state into a renderer-facing frame.
pub struct AlacrittyEmulator {
    parser: Processor,
    term: Term<VoidListener>,
    size: ViewportSize,
    projected_cells: Vec<TerminalCell>,
    last_display_offset: usize,
}

/// Fine-grained timing for one native terminal snapshot build.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmulatorSnapshotProfile {
    pub damage_collect_us: u128,
    pub projection_update_us: u128,
    pub publication_build_us: u128,
}

impl AlacrittyEmulator {
    pub fn new(config: AlacrittyEmulatorConfig) -> Self {
        let size = ViewportSize::new(config.rows, config.cols);
        let mut term_config = Config::default();
        term_config.scrolling_history = config.scrollback;

        Self {
            parser: Processor::default(),
            term: Term::new(term_config, &size, VoidListener),
            size,
            projected_cells: vec![TerminalCell::default(); size.cell_count()],
            last_display_offset: 0,
        }
    }

    pub fn ingest(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }

        self.parser.advance(&mut self.term, bytes);
    }

    pub fn resize(&mut self, rows: u16, cols: u16) -> bool {
        let next = ViewportSize::new(rows, cols);
        if self.size == next {
            return false;
        }

        self.term.resize(next);
        self.size = next;
        self.projected_cells
            .resize(self.size.cell_count(), TerminalCell::default());
        true
    }

    pub fn snapshot(&mut self) -> TerminalFrame {
        self.snapshot_profiled().0
    }

    pub fn snapshot_profiled(&mut self) -> (TerminalFrame, EmulatorSnapshotProfile) {
        let damage_collect_started = Instant::now();
        let mut damage = collect_damage(&mut self.term);
        let (cursor, display_offset) = {
            let content = self.term.renderable_content();
            (
                project_cursor(content.cursor, content.display_offset),
                content.display_offset,
            )
        };
        if self.last_display_offset != display_offset {
            damage = FrameDamage::Full;
        }
        let damage_collect_us = damage_collect_started.elapsed().as_micros();

        let projection_update_started = Instant::now();
        project_damage_into(
            &mut self.projected_cells,
            self.term.grid(),
            self.size,
            display_offset,
            &damage,
        );
        let projection_update_us = projection_update_started.elapsed().as_micros();
        let bracketed_paste = self.term.mode().contains(TermMode::BRACKETED_PASTE);

        let publication_build_started = Instant::now();
        let publication = build_publication(&self.projected_cells, self.size, &damage);
        let publication_build_us = publication_build_started.elapsed().as_micros();
        self.last_display_offset = display_offset;

        self.term.reset_damage();

        (
            TerminalFrame {
                rows: self.size.rows,
                cols: self.size.cols,
                cursor,
                bracketed_paste,
                display_offset,
                damage,
                publication,
            },
            EmulatorSnapshotProfile {
                damage_collect_us,
                projection_update_us,
                publication_build_us,
            },
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViewportSize {
    rows: u16,
    cols: u16,
}

impl ViewportSize {
    fn new(rows: u16, cols: u16) -> Self {
        Self {
            rows: rows.max(MIN_SCREEN_LINES as u16),
            cols: cols.max(MIN_COLUMNS as u16),
        }
    }

    fn cell_count(self) -> usize {
        usize::from(self.rows) * usize::from(self.cols)
    }
}

impl Dimensions for ViewportSize {
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }

    fn screen_lines(&self) -> usize {
        usize::from(self.rows)
    }

    fn columns(&self) -> usize {
        usize::from(self.cols)
    }
}

fn collect_damage(term: &mut Term<VoidListener>) -> FrameDamage {
    match term.damage() {
        TermDamage::Full => FrameDamage::Full,
        TermDamage::Partial(lines) => {
            let spans: Vec<_> = lines
                .map(|line| TerminalDamageSpan {
                    row: line.line as u16,
                    left: line.left as u16,
                    right: line.right as u16,
                })
                .collect();
            FrameDamage::Partial(spans.into())
        }
    }
}

fn project_damage_into(
    cells: &mut [TerminalCell],
    grid: &Grid<Cell>,
    size: ViewportSize,
    display_offset: usize,
    damage: &FrameDamage,
) {
    match damage {
        FrameDamage::Full => rebuild_projected_cells(cells, grid, size, display_offset),
        FrameDamage::Partial(spans) => {
            update_damage_spans(cells, grid, size, display_offset, spans)
        }
    }
}

fn build_publication(
    cells: &[TerminalCell],
    size: ViewportSize,
    damage: &FrameDamage,
) -> TerminalCellPublication {
    match damage {
        FrameDamage::Full => {
            TerminalCellPublication::Full(Arc::<[TerminalCell]>::from(cells.to_vec()))
        }
        FrameDamage::Partial(spans) => {
            TerminalCellPublication::Partial(collect_changed_cells(cells, size, spans).into())
        }
    }
}

fn project_cursor(cursor: RenderableCursor, display_offset: usize) -> TerminalCursor {
    let Some(point) = point_to_viewport(display_offset, cursor.point) else {
        return TerminalCursor {
            row: 0,
            col: 0,
            shape: TerminalCursorShape::Hidden,
        };
    };

    TerminalCursor {
        row: point.line as u16,
        col: point.column.0 as u16,
        shape: map_cursor_shape(cursor.shape),
    }
}

fn rebuild_projected_cells(
    cells: &mut [TerminalCell],
    grid: &Grid<Cell>,
    size: ViewportSize,
    display_offset: usize,
) {
    for row in 0..size.rows {
        update_projected_row(
            cells,
            grid,
            size,
            display_offset,
            row,
            0,
            size.cols.saturating_sub(1),
        );
    }
}

fn update_damage_spans(
    cells: &mut [TerminalCell],
    grid: &Grid<Cell>,
    size: ViewportSize,
    display_offset: usize,
    spans: &[TerminalDamageSpan],
) {
    for span in spans {
        if span.row >= size.rows || span.left >= size.cols {
            continue;
        }

        let right = span.right.min(size.cols.saturating_sub(1));
        if span.left > right {
            continue;
        }

        update_projected_row(
            cells,
            grid,
            size,
            display_offset,
            span.row,
            span.left,
            right,
        );
    }
}

fn update_projected_row(
    cells: &mut [TerminalCell],
    grid: &Grid<Cell>,
    size: ViewportSize,
    display_offset: usize,
    row: u16,
    left: u16,
    right: u16,
) {
    let line = viewport_to_point(display_offset, Point::new(usize::from(row), Column(0))).line;
    let width = usize::from(size.cols);
    let mut index = usize::from(row) * width + usize::from(left);

    for col in left..=right {
        cells[index] = project_cell(&grid[line][Column(usize::from(col))]);
        index += 1;
    }
}

fn collect_changed_cells(
    cells: &[TerminalCell],
    size: ViewportSize,
    spans: &[TerminalDamageSpan],
) -> Vec<TerminalCellUpdate> {
    let width = usize::from(size.cols);
    let mut changes = Vec::new();

    for span in spans {
        if span.left > span.right {
            continue;
        }

        for col in span.left..=span.right {
            let index = usize::from(span.row)
                .saturating_mul(width)
                .saturating_add(usize::from(col));
            if let Some(cell) = cells.get(index) {
                changes.push(TerminalCellUpdate {
                    row: span.row,
                    col,
                    cell: cell.clone(),
                });
            }
        }
    }

    changes
}

fn project_cell(cell: &Cell) -> TerminalCell {
    let zerowidth = cell
        .zerowidth()
        .map(|value| Arc::<[char]>::from(value.to_vec()));

    TerminalCell {
        codepoint: cell.c,
        zerowidth,
        fg: map_color(cell.fg),
        bg: map_color(cell.bg),
        flags: map_flags(cell.flags),
    }
}

fn map_cursor_shape(shape: CursorShape) -> TerminalCursorShape {
    match shape {
        CursorShape::Block => TerminalCursorShape::Block,
        CursorShape::Underline => TerminalCursorShape::Underline,
        CursorShape::Beam => TerminalCursorShape::Beam,
        CursorShape::HollowBlock => TerminalCursorShape::HollowBlock,
        CursorShape::Hidden => TerminalCursorShape::Hidden,
    }
}

fn map_color(color: Color) -> TerminalColor {
    match color {
        Color::Named(NamedColor::Foreground) | Color::Named(NamedColor::BrightForeground) => {
            TerminalColor::DefaultForeground
        }
        Color::Named(NamedColor::Background) | Color::Named(NamedColor::DimForeground) => {
            TerminalColor::DefaultBackground
        }
        Color::Named(NamedColor::Cursor) => TerminalColor::Cursor,
        Color::Named(named) => TerminalColor::Palette(named as u8),
        Color::Indexed(index) => TerminalColor::Palette(index),
        Color::Spec(rgb) => TerminalColor::Rgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        },
    }
}

fn map_flags(flags: Flags) -> TerminalCellFlags {
    let mut mapped = TerminalCellFlags::NONE;

    for (alacritty_flag, frame_flag) in [
        (Flags::INVERSE, TerminalCellFlags::INVERSE),
        (Flags::BOLD, TerminalCellFlags::BOLD),
        (Flags::ITALIC, TerminalCellFlags::ITALIC),
        (Flags::UNDERLINE, TerminalCellFlags::UNDERLINE),
        (Flags::WRAPLINE, TerminalCellFlags::WRAPLINE),
        (Flags::WIDE_CHAR, TerminalCellFlags::WIDE_CHAR),
        (Flags::WIDE_CHAR_SPACER, TerminalCellFlags::WIDE_CHAR_SPACER),
        (Flags::DIM, TerminalCellFlags::DIM),
        (Flags::HIDDEN, TerminalCellFlags::HIDDEN),
        (Flags::STRIKEOUT, TerminalCellFlags::STRIKEOUT),
        (
            Flags::LEADING_WIDE_CHAR_SPACER,
            TerminalCellFlags::LEADING_WIDE_CHAR_SPACER,
        ),
        (Flags::DOUBLE_UNDERLINE, TerminalCellFlags::DOUBLE_UNDERLINE),
        (Flags::UNDERCURL, TerminalCellFlags::UNDERCURL),
        (Flags::DOTTED_UNDERLINE, TerminalCellFlags::DOTTED_UNDERLINE),
        (Flags::DASHED_UNDERLINE, TerminalCellFlags::DASHED_UNDERLINE),
    ] {
        if flags.contains(alacritty_flag) {
            mapped = mapped.union(frame_flag);
        }
    }

    mapped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_projects_text_cursor_and_damage() {
        let mut emulator = AlacrittyEmulator::new(AlacrittyEmulatorConfig {
            rows: 4,
            cols: 8,
            scrollback: 128,
        });

        let initial = emulator.snapshot();
        assert_eq!(initial.damage, FrameDamage::Full);

        emulator.ingest(b"hi");
        let frame = emulator.snapshot();

        assert_eq!(frame.cursor.row, 0);
        assert_eq!(frame.cursor.col, 2);
        let changes = changed_cells(&frame);
        assert_eq!(change_at(&changes, 0, 0).unwrap().codepoint, 'h');
        assert_eq!(change_at(&changes, 0, 1).unwrap().codepoint, 'i');
        match &frame.damage {
            FrameDamage::Full => panic!("expected partial damage after steady-state update"),
            FrameDamage::Partial(lines) => assert!(lines.iter().any(|line| line.row == 0)),
        }
    }

    #[test]
    fn snapshot_projects_modes_and_colors() {
        let mut emulator = AlacrittyEmulator::new(AlacrittyEmulatorConfig {
            rows: 4,
            cols: 8,
            scrollback: 128,
        });

        let _ = emulator.snapshot();
        emulator.ingest(b"\x1b[31mR\x1b[0m\x1b[?2004h\x1b[?25l");
        let frame = emulator.snapshot();

        assert!(frame.bracketed_paste);
        assert_eq!(frame.cursor.shape, TerminalCursorShape::Hidden);
        assert_eq!(
            change_at(&changed_cells(&frame), 0, 0).unwrap().fg,
            TerminalColor::Palette(1)
        );
    }

    #[test]
    fn resize_marks_terminal_fully_damaged() {
        let mut emulator = AlacrittyEmulator::new(AlacrittyEmulatorConfig {
            rows: 2,
            cols: 4,
            scrollback: 128,
        });

        let _ = emulator.snapshot();
        emulator.ingest(b"abcd");
        let _ = emulator.snapshot();

        assert!(emulator.resize(3, 6));
        let frame = emulator.snapshot();

        assert_eq!(frame.rows, 3);
        assert_eq!(frame.cols, 6);
        assert_eq!(frame.damage, FrameDamage::Full);
        assert!(matches!(
            frame.publication,
            TerminalCellPublication::Full(_)
        ));
        assert_eq!(frame.cell(0, 0).unwrap().codepoint, 'a');
    }

    #[test]
    fn partial_damage_updates_preserve_undamaged_cells() {
        let mut emulator = AlacrittyEmulator::new(AlacrittyEmulatorConfig {
            rows: 4,
            cols: 8,
            scrollback: 128,
        });

        let _ = emulator.snapshot();
        emulator.ingest(b"hi");
        let _ = emulator.snapshot();

        emulator.ingest(b"!");
        let frame = emulator.snapshot();

        let changes = changed_cells(&frame);
        assert!(change_at(&changes, 0, 0).is_none());
        assert!(change_at(&changes, 0, 1).is_none());
        assert_eq!(change_at(&changes, 0, 2).unwrap().codepoint, '!');
        assert!(matches!(frame.damage, FrameDamage::Partial(_)));
    }

    fn changed_cells(frame: &TerminalFrame) -> Vec<(u16, u16, TerminalCell)> {
        frame
            .changed_cells()
            .unwrap_or(&[])
            .iter()
            .map(|change| (change.row, change.col, change.cell.clone()))
            .collect()
    }

    fn change_at(
        changes: &[(u16, u16, TerminalCell)],
        row: u16,
        col: u16,
    ) -> Option<&TerminalCell> {
        changes
            .iter()
            .find(|(candidate_row, candidate_col, _)| {
                *candidate_row == row && *candidate_col == col
            })
            .map(|(_, _, cell)| cell)
    }
}
