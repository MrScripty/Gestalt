use std::sync::Arc;

/// Immutable terminal frame published to native renderers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalFrame {
    pub rows: u16,
    pub cols: u16,
    pub history_size: usize,
    pub cursor: TerminalCursor,
    pub bracketed_paste: bool,
    pub display_offset: usize,
    pub damage: TerminalDamage,
    pub publication: TerminalCellPublication,
}

impl TerminalFrame {
    pub fn full_cells_shared(&self) -> Option<&Arc<Vec<TerminalCell>>> {
        match &self.publication {
            TerminalCellPublication::Full(cells) => Some(cells),
            TerminalCellPublication::Partial(_) => None,
        }
    }

    pub fn full_cells(&self) -> Option<&[TerminalCell]> {
        match &self.publication {
            TerminalCellPublication::Full(cells) => Some(cells.as_ref()),
            TerminalCellPublication::Partial(_) => None,
        }
    }

    pub fn changed_spans(&self) -> Option<&TerminalCellSpanBatch> {
        match &self.publication {
            TerminalCellPublication::Full(_) => None,
            TerminalCellPublication::Partial(changes) => Some(changes),
        }
    }

    pub fn cell(&self, row: u16, col: u16) -> Option<&TerminalCell> {
        let width = usize::from(self.cols);
        let index = usize::from(row)
            .checked_mul(width)?
            .checked_add(usize::from(col))?;
        self.full_cells()?.get(index)
    }
}

/// Immutable terminal cell publication for a single frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalCellPublication {
    Full(Arc<Vec<TerminalCell>>),
    Partial(TerminalCellSpanBatch),
}

/// Renderable terminal cell projected from the emulator grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCell {
    pub codepoint: char,
    pub zerowidth: Option<Arc<[char]>>,
    pub fg: TerminalColor,
    pub bg: TerminalColor,
    pub flags: TerminalCellFlags,
}

impl Default for TerminalCell {
    fn default() -> Self {
        Self {
            codepoint: ' ',
            zerowidth: None,
            fg: TerminalColor::DefaultForeground,
            bg: TerminalColor::DefaultBackground,
            flags: TerminalCellFlags::NONE,
        }
    }
}

/// Cursor state projected from the terminal core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCursor {
    pub row: u16,
    pub col: u16,
    pub shape: TerminalCursorShape,
}

/// Renderer-facing cursor shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalCursorShape {
    #[default]
    Block,
    Underline,
    Beam,
    HollowBlock,
    Hidden,
}

/// Renderer-facing terminal color values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalColor {
    #[default]
    DefaultForeground,
    DefaultBackground,
    Cursor,
    Palette(u8),
    Rgb {
        r: u8,
        g: u8,
        b: u8,
    },
}

/// Terminal cell decoration and layout flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TerminalCellFlags(u16);

impl TerminalCellFlags {
    pub const NONE: Self = Self(0);
    pub const INVERSE: Self = Self(1 << 0);
    pub const BOLD: Self = Self(1 << 1);
    pub const ITALIC: Self = Self(1 << 2);
    pub const UNDERLINE: Self = Self(1 << 3);
    pub const WRAPLINE: Self = Self(1 << 4);
    pub const WIDE_CHAR: Self = Self(1 << 5);
    pub const WIDE_CHAR_SPACER: Self = Self(1 << 6);
    pub const DIM: Self = Self(1 << 7);
    pub const HIDDEN: Self = Self(1 << 8);
    pub const STRIKEOUT: Self = Self(1 << 9);
    pub const LEADING_WIDE_CHAR_SPACER: Self = Self(1 << 10);
    pub const DOUBLE_UNDERLINE: Self = Self(1 << 11);
    pub const UNDERCURL: Self = Self(1 << 12);
    pub const DOTTED_UNDERLINE: Self = Self(1 << 13);
    pub const DASHED_UNDERLINE: Self = Self(1 << 14);

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Visible terminal damage since the last published frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalDamage {
    Full,
    Partial(Arc<[TerminalDamageSpan]>),
}

/// Single damaged span within the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalDamageSpan {
    pub row: u16,
    pub left: u16,
    pub right: u16,
}

/// Single renderer-facing contiguous row span within the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCellSpanUpdate {
    pub row: u16,
    pub left: u16,
    pub len: u16,
    pub cells_start: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCellSpanBatch {
    spans: Box<[TerminalCellSpanUpdate]>,
    cells: Box<[TerminalCell]>,
}

impl TerminalCellSpanBatch {
    pub fn new(spans: Box<[TerminalCellSpanUpdate]>, cells: Box<[TerminalCell]>) -> Self {
        Self { spans, cells }
    }

    pub fn spans(&self) -> &[TerminalCellSpanUpdate] {
        self.spans.as_ref()
    }

    pub fn span_count(&self) -> usize {
        self.spans.len()
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    pub fn cells_for_span(&self, span: &TerminalCellSpanUpdate) -> &[TerminalCell] {
        let start = span.cells_start as usize;
        let end = start + usize::from(span.len);
        &self.cells[start..end]
    }
}
