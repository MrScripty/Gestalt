use std::sync::Arc;

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{
    Config, MIN_COLUMNS, MIN_SCREEN_LINES, RenderableCursor, Term, TermDamage, TermMode,
    point_to_viewport,
};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Processor};

use super::model::{
    TerminalCell, TerminalCellFlags, TerminalColor, TerminalCursor, TerminalCursorShape,
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
        true
    }

    pub fn snapshot(&mut self) -> TerminalFrame {
        let damage = collect_damage(&mut self.term);
        let content = self.term.renderable_content();
        let cursor = project_cursor(content.cursor, content.display_offset);
        let cells = collect_cells(content, self.size);
        let bracketed_paste = self.term.mode().contains(TermMode::BRACKETED_PASTE);

        self.term.reset_damage();

        TerminalFrame {
            rows: self.size.rows,
            cols: self.size.cols,
            cursor,
            bracketed_paste,
            display_offset: self.term.grid().display_offset(),
            damage,
            cells,
        }
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

fn collect_cells(
    content: alacritty_terminal::term::RenderableContent<'_>,
    size: ViewportSize,
) -> Arc<[TerminalCell]> {
    let mut cells = vec![TerminalCell::default(); size.cell_count()];
    let width = usize::from(size.cols);
    let display_offset = content.display_offset;

    for indexed in content.display_iter {
        let Some(point) = point_to_viewport(display_offset, indexed.point) else {
            continue;
        };

        let index = point.line * width + point.column.0;
        if let Some(cell) = cells.get_mut(index) {
            *cell = project_cell(indexed.cell);
        }
    }

    cells.into()
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

fn project_cell(cell: &Cell) -> TerminalCell {
    let zerowidth = cell
        .zerowidth()
        .map(|value| Arc::<[char]>::from(value.to_vec()))
        .unwrap_or_else(|| Arc::<[char]>::from([]));

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
        assert_eq!(frame.cell(0, 0).unwrap().codepoint, 'h');
        assert_eq!(frame.cell(0, 1).unwrap().codepoint, 'i');
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
        assert_eq!(frame.cell(0, 0).unwrap().fg, TerminalColor::Palette(1));
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
        assert_eq!(frame.cell(0, 0).unwrap().codepoint, 'a');
    }
}
