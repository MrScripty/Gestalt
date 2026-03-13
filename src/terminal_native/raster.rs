use font8x8::{BASIC_FONTS, UnicodeFonts};

use super::{
    TerminalCell, TerminalCellFlags, TerminalColor, TerminalCursor, TerminalCursorShape,
    TerminalDamage, TerminalFrame,
};

const DEFAULT_BACKGROUND: [u8; 4] = [8, 12, 16, 255];
const DEFAULT_FOREGROUND: [u8; 4] = [222, 226, 230, 255];
const CURSOR_COLOR: [u8; 4] = [255, 244, 163, 255];

pub struct TerminalRaster {
    width: u32,
    height: u32,
    cell_width: u32,
    cell_height: u32,
    rows: u16,
    cols: u16,
    pixels: Vec<u8>,
    last_cursor: Option<TerminalCursor>,
}

impl TerminalRaster {
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            cell_width: 1,
            cell_height: 1,
            rows: 0,
            cols: 0,
            pixels: Vec::new(),
            last_cursor: None,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.pixels
            .resize((width as usize) * (height as usize) * 4, 0);
    }

    pub fn dimensions_changed(&self, width: u32, height: u32) -> bool {
        self.width != width || self.height != height
    }

    pub fn update(&mut self, frame: &TerminalFrame, width: u32, height: u32) {
        if self.dimensions_changed(width, height) {
            self.resize(width, height);
        }

        let geometry_changed = self.rows != frame.rows || self.cols != frame.cols;
        self.rows = frame.rows;
        self.cols = frame.cols;
        self.cell_width = cell_extent(width, frame.cols);
        self.cell_height = cell_extent(height, frame.rows);

        if geometry_changed || matches!(frame.damage, TerminalDamage::Full) {
            self.clear(DEFAULT_BACKGROUND);
            self.redraw_full(frame);
            self.last_cursor = Some(frame.cursor);
            return;
        }

        self.redraw_partial(frame);
        self.last_cursor = Some(frame.cursor);
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    fn clear(&mut self, color: [u8; 4]) {
        for pixel in self.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&color);
        }
    }

    fn redraw_full(&mut self, frame: &TerminalFrame) {
        for row in 0..frame.rows {
            for col in 0..frame.cols {
                self.redraw_cell(frame, row, col);
            }
        }
    }

    fn redraw_partial(&mut self, frame: &TerminalFrame) {
        match &frame.damage {
            TerminalDamage::Full => self.redraw_full(frame),
            TerminalDamage::Partial(lines) => {
                for span in lines.iter() {
                    let right = span.right.min(frame.cols.saturating_sub(1));
                    for col in span.left..=right {
                        self.redraw_cell(frame, span.row, col);
                    }
                }
            }
        }

        if let Some(previous) = self.last_cursor
            && (previous.row != frame.cursor.row || previous.col != frame.cursor.col)
            && previous.row < frame.rows
            && previous.col < frame.cols
        {
            self.redraw_cell(frame, previous.row, previous.col);
        }

        if frame.cursor.shape != TerminalCursorShape::Hidden
            && frame.cursor.row < frame.rows
            && frame.cursor.col < frame.cols
        {
            self.redraw_cell(frame, frame.cursor.row, frame.cursor.col);
        }
    }

    fn redraw_cell(&mut self, frame: &TerminalFrame, row: u16, col: u16) {
        let Some(cell) = frame.cell(row, col) else {
            return;
        };

        let cursor_here = frame.cursor.row == row && frame.cursor.col == col;
        let (bg, fg) = resolved_colors(cell, cursor_here);
        self.fill_rect(row, col, bg);

        if cell.flags.contains(TerminalCellFlags::HIDDEN)
            || cell.flags.contains(TerminalCellFlags::WIDE_CHAR_SPACER)
        {
            if cursor_here {
                self.draw_cursor(frame.cursor, row, col, fg);
            }
            return;
        }

        if let Some(glyph) = BASIC_FONTS.get(cell.codepoint) {
            self.draw_glyph(row, col, glyph, fg);
        } else if !cell.codepoint.is_whitespace() {
            self.draw_missing_glyph(row, col, fg);
        }

        if cursor_here {
            self.draw_cursor(frame.cursor, row, col, fg);
        }
    }

    fn fill_rect(&mut self, row: u16, col: u16, color: [u8; 4]) {
        let start_x = u32::from(col) * self.cell_width;
        let start_y = u32::from(row) * self.cell_height;
        let end_x = (start_x + self.cell_width).min(self.width);
        let end_y = (start_y + self.cell_height).min(self.height);

        for y in start_y..end_y {
            for x in start_x..end_x {
                let index = ((y * self.width + x) * 4) as usize;
                self.pixels[index..index + 4].copy_from_slice(&color);
            }
        }
    }

    fn draw_glyph(&mut self, row: u16, col: u16, glyph: [u8; 8], color: [u8; 4]) {
        let scale_x = self.cell_width.max(1) / 8;
        let scale_y = self.cell_height.max(1) / 8;
        let glyph_width = 8 * scale_x.max(1);
        let glyph_height = 8 * scale_y.max(1);
        let offset_x = (self.cell_width.saturating_sub(glyph_width)) / 2;
        let offset_y = (self.cell_height.saturating_sub(glyph_height)) / 2;
        let origin_x = u32::from(col) * self.cell_width + offset_x;
        let origin_y = u32::from(row) * self.cell_height + offset_y;

        for (glyph_row, bits) in glyph.iter().enumerate() {
            for glyph_col in 0..8_u32 {
                if bits & (1 << glyph_col) == 0 {
                    continue;
                }

                for y in 0..scale_y.max(1) {
                    for x in 0..scale_x.max(1) {
                        let pixel_x = origin_x + glyph_col * scale_x.max(1) + x;
                        let pixel_y = origin_y + (glyph_row as u32) * scale_y.max(1) + y;
                        self.write_pixel(pixel_x, pixel_y, color);
                    }
                }
            }
        }
    }

    fn draw_missing_glyph(&mut self, row: u16, col: u16, color: [u8; 4]) {
        let inset_x = self.cell_width.saturating_div(4).max(1);
        let inset_y = self.cell_height.saturating_div(4).max(1);
        let start_x = u32::from(col) * self.cell_width + inset_x;
        let end_x = ((u32::from(col) + 1) * self.cell_width).saturating_sub(inset_x);
        let start_y = u32::from(row) * self.cell_height + inset_y;
        let end_y = ((u32::from(row) + 1) * self.cell_height).saturating_sub(inset_y);

        for x in start_x..end_x {
            self.write_pixel(x, start_y, color);
            self.write_pixel(x, end_y.saturating_sub(1), color);
        }
        for y in start_y..end_y {
            self.write_pixel(start_x, y, color);
            self.write_pixel(end_x.saturating_sub(1), y, color);
        }
    }

    fn draw_cursor(&mut self, cursor: TerminalCursor, row: u16, col: u16, color: [u8; 4]) {
        match cursor.shape {
            TerminalCursorShape::Block => self.fill_cursor_rect(row, col, CURSOR_COLOR),
            TerminalCursorShape::Underline => {
                self.fill_cursor_line(row, col, self.cell_height, 2, color)
            }
            TerminalCursorShape::Beam => self.fill_cursor_line(row, col, self.cell_width, 2, color),
            TerminalCursorShape::HollowBlock => self.draw_missing_glyph(row, col, CURSOR_COLOR),
            TerminalCursorShape::Hidden => {}
        }
    }

    fn fill_cursor_rect(&mut self, row: u16, col: u16, color: [u8; 4]) {
        let start_x = u32::from(col) * self.cell_width;
        let start_y = u32::from(row) * self.cell_height;
        let end_x = (start_x + self.cell_width).min(self.width);
        let end_y = (start_y + self.cell_height).min(self.height);

        for y in start_y..end_y {
            for x in start_x..end_x {
                let index = ((y * self.width + x) * 4) as usize;
                let source = &mut self.pixels[index..index + 4];
                source[0] = (u16::from(source[0]) / 2 + u16::from(color[0]) / 2) as u8;
                source[1] = (u16::from(source[1]) / 2 + u16::from(color[1]) / 2) as u8;
                source[2] = (u16::from(source[2]) / 2 + u16::from(color[2]) / 2) as u8;
                source[3] = 255;
            }
        }
    }

    fn fill_cursor_line(
        &mut self,
        row: u16,
        col: u16,
        extent: u32,
        thickness: u32,
        color: [u8; 4],
    ) {
        let start_x = u32::from(col) * self.cell_width;
        let start_y = u32::from(row) * self.cell_height;
        if extent == self.cell_height {
            let y = (start_y + self.cell_height.saturating_sub(thickness)).min(self.height);
            for yy in y..(y + thickness).min(self.height) {
                for x in start_x..(start_x + self.cell_width).min(self.width) {
                    self.write_pixel(x, yy, color);
                }
            }
        } else {
            for y in start_y..(start_y + self.cell_height).min(self.height) {
                for x in start_x..(start_x + thickness).min(self.width) {
                    self.write_pixel(x, y, color);
                }
            }
        }
    }

    fn write_pixel(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }

        let index = ((y * self.width + x) * 4) as usize;
        self.pixels[index..index + 4].copy_from_slice(&color);
    }
}

fn cell_extent(size: u32, cells: u16) -> u32 {
    if cells == 0 {
        return 1;
    }

    (size / u32::from(cells)).max(1)
}

fn resolved_colors(cell: &TerminalCell, cursor_here: bool) -> ([u8; 4], [u8; 4]) {
    let mut fg = resolve_color(cell.fg);
    let mut bg = resolve_color(cell.bg);

    if cell.flags.contains(TerminalCellFlags::INVERSE) {
        std::mem::swap(&mut fg, &mut bg);
    }

    if cell.flags.contains(TerminalCellFlags::DIM) {
        fg[0] /= 2;
        fg[1] /= 2;
        fg[2] /= 2;
    }

    if cursor_here && matches!(cell.fg, TerminalColor::DefaultForeground) {
        fg = DEFAULT_BACKGROUND;
    }

    (bg, fg)
}

fn resolve_color(color: TerminalColor) -> [u8; 4] {
    match color {
        TerminalColor::DefaultForeground => DEFAULT_FOREGROUND,
        TerminalColor::DefaultBackground => DEFAULT_BACKGROUND,
        TerminalColor::Cursor => CURSOR_COLOR,
        TerminalColor::Palette(index) => resolve_palette(index),
        TerminalColor::Rgb { r, g, b } => [r, g, b, 255],
    }
}

fn resolve_palette(index: u8) -> [u8; 4] {
    const ANSI: [[u8; 4]; 16] = [
        [18, 20, 22, 255],
        [204, 36, 29, 255],
        [152, 151, 26, 255],
        [215, 153, 33, 255],
        [69, 133, 136, 255],
        [177, 98, 134, 255],
        [104, 157, 106, 255],
        [235, 219, 178, 255],
        [102, 92, 84, 255],
        [251, 73, 52, 255],
        [184, 187, 38, 255],
        [250, 189, 47, 255],
        [131, 165, 152, 255],
        [211, 134, 155, 255],
        [142, 192, 124, 255],
        [251, 241, 199, 255],
    ];

    if let Some(color) = ANSI.get(index as usize) {
        return *color;
    }

    if (16..=231).contains(&index) {
        let index = index - 16;
        let r = index / 36;
        let g = (index / 6) % 6;
        let b = index % 6;
        return [cube_component(r), cube_component(g), cube_component(b), 255];
    }

    let gray = 8 + (index.saturating_sub(232) * 10);
    [gray, gray, gray, 255]
}

fn cube_component(component: u8) -> u8 {
    match component {
        0 => 0,
        _ => 55 + component * 40,
    }
}
