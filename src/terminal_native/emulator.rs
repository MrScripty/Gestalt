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
    TerminalCell, TerminalCellFlags, TerminalCellPublication, TerminalCellSpanBatch,
    TerminalCellSpanUpdate, TerminalColor, TerminalCursor, TerminalCursorShape,
    TerminalDamage as FrameDamage, TerminalDamageSpan, TerminalFrame,
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
            let mut spans: Vec<_> = lines
                .map(|line| TerminalDamageSpan {
                    row: line.line as u16,
                    left: line.left as u16,
                    right: line.right as u16,
                })
                .collect();
            coalesce_damage_spans(&mut spans);
            FrameDamage::Partial(spans.into())
        }
    }
}

fn coalesce_damage_spans(spans: &mut Vec<TerminalDamageSpan>) {
    if spans.len() <= 1 {
        return;
    }

    let mut write_index = 0;
    for read_index in 1..spans.len() {
        let current = spans[read_index];
        let last = &mut spans[write_index];
        if last.row == current.row && current.left <= last.right.saturating_add(1) {
            last.right = last.right.max(current.right);
            continue;
        }

        write_index += 1;
        spans[write_index] = current;
    }
    spans.truncate(write_index + 1);
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
        FrameDamage::Full => TerminalCellPublication::Full(cells.to_vec().into_boxed_slice()),
        FrameDamage::Partial(spans) => {
            TerminalCellPublication::Partial(collect_changed_spans(cells, size, spans))
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
    let row_start = usize::from(row) * width + usize::from(left);
    let row_end = row_start + usize::from(right - left) + 1;
    let Some(target_cells) = cells.get_mut(row_start..row_end) else {
        return;
    };
    let source_cells = &grid[line][Column(usize::from(left))..Column(usize::from(right) + 1)];

    for (target, source) in target_cells.iter_mut().zip(source_cells.iter()) {
        *target = project_cell(source);
    }
}

fn collect_changed_spans(
    cells: &[TerminalCell],
    size: ViewportSize,
    spans: &[TerminalDamageSpan],
) -> TerminalCellSpanBatch {
    let width = usize::from(size.cols);
    let mut change_spans = Vec::with_capacity(spans.len());
    let mut changed_cells = Vec::new();

    for span in spans {
        if span.row >= size.rows || span.left > span.right || span.left >= size.cols {
            continue;
        }

        let right = span.right.min(size.cols.saturating_sub(1));
        let start = usize::from(span.row) * width + usize::from(span.left);
        let end = usize::from(span.row) * width + usize::from(right) + 1;
        let Some(row_cells) = cells.get(start..end) else {
            continue;
        };
        let cells_start = changed_cells.len() as u32;
        changed_cells.extend_from_slice(row_cells);
        change_spans.push(TerminalCellSpanUpdate {
            row: span.row,
            left: span.left,
            len: row_cells.len() as u16,
            cells_start,
        });
    }

    TerminalCellSpanBatch::new(
        change_spans.into_boxed_slice(),
        changed_cells.into_boxed_slice(),
    )
}

fn project_cell(cell: &Cell) -> TerminalCell {
    if cell.flags.is_empty()
        && cell.zerowidth().is_none()
        && is_default_foreground(cell.fg)
        && is_default_background(cell.bg)
    {
        return TerminalCell {
            codepoint: cell.c,
            zerowidth: None,
            fg: TerminalColor::DefaultForeground,
            bg: TerminalColor::DefaultBackground,
            flags: TerminalCellFlags::NONE,
        };
    }

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

fn is_default_foreground(color: Color) -> bool {
    matches!(
        color,
        Color::Named(NamedColor::Foreground) | Color::Named(NamedColor::BrightForeground)
    )
}

fn is_default_background(color: Color) -> bool {
    matches!(
        color,
        Color::Named(NamedColor::Background) | Color::Named(NamedColor::DimForeground)
    )
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
    if flags.is_empty() {
        return TerminalCellFlags::NONE;
    }

    let mut mapped = TerminalCellFlags::NONE;

    if flags.contains(Flags::INVERSE) {
        mapped = mapped.union(TerminalCellFlags::INVERSE);
    }
    if flags.contains(Flags::BOLD) {
        mapped = mapped.union(TerminalCellFlags::BOLD);
    }
    if flags.contains(Flags::ITALIC) {
        mapped = mapped.union(TerminalCellFlags::ITALIC);
    }
    if flags.contains(Flags::UNDERLINE) {
        mapped = mapped.union(TerminalCellFlags::UNDERLINE);
    }
    if flags.contains(Flags::WRAPLINE) {
        mapped = mapped.union(TerminalCellFlags::WRAPLINE);
    }
    if flags.contains(Flags::WIDE_CHAR) {
        mapped = mapped.union(TerminalCellFlags::WIDE_CHAR);
    }
    if flags.contains(Flags::WIDE_CHAR_SPACER) {
        mapped = mapped.union(TerminalCellFlags::WIDE_CHAR_SPACER);
    }
    if flags.contains(Flags::DIM) {
        mapped = mapped.union(TerminalCellFlags::DIM);
    }
    if flags.contains(Flags::HIDDEN) {
        mapped = mapped.union(TerminalCellFlags::HIDDEN);
    }
    if flags.contains(Flags::STRIKEOUT) {
        mapped = mapped.union(TerminalCellFlags::STRIKEOUT);
    }
    if flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
        mapped = mapped.union(TerminalCellFlags::LEADING_WIDE_CHAR_SPACER);
    }
    if flags.contains(Flags::DOUBLE_UNDERLINE) {
        mapped = mapped.union(TerminalCellFlags::DOUBLE_UNDERLINE);
    }
    if flags.contains(Flags::UNDERCURL) {
        mapped = mapped.union(TerminalCellFlags::UNDERCURL);
    }
    if flags.contains(Flags::DOTTED_UNDERLINE) {
        mapped = mapped.union(TerminalCellFlags::DOTTED_UNDERLINE);
    }
    if flags.contains(Flags::DASHED_UNDERLINE) {
        mapped = mapped.union(TerminalCellFlags::DASHED_UNDERLINE);
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
        let Some(changes) = frame.changed_spans() else {
            return Vec::new();
        };

        changes
            .spans()
            .iter()
            .flat_map(|change| {
                changes
                    .cells_for_span(change)
                    .iter()
                    .enumerate()
                    .map(|(offset, cell)| (change.row, change.left + offset as u16, cell.clone()))
                    .collect::<Vec<_>>()
            })
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
