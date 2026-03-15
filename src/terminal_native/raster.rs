use std::collections::HashMap;

use font8x8::{BASIC_FONTS, UnicodeFonts};

use super::{
    TerminalCell, TerminalCellFlags, TerminalColor, TerminalCursor, TerminalCursorShape,
    TerminalDamage, TerminalFrame,
};

const DEFAULT_BACKGROUND: [u8; 4] = [8, 12, 16, 255];
const DEFAULT_FOREGROUND: [u8; 4] = [222, 226, 230, 255];
const CURSOR_COLOR: [u8; 4] = [255, 244, 163, 255];
const MAX_SCROLL_REUSE_ROWS: u16 = 4;

pub struct TerminalRaster {
    width: u32,
    height: u32,
    cell_width: u32,
    cell_height: u32,
    rows: u16,
    cols: u16,
    pixels: Vec<u8>,
    last_cursor: Option<TerminalCursor>,
    cells: Vec<TerminalCell>,
    last_cells: Option<Vec<TerminalCell>>,
    tile_cache: HashMap<TileCacheKey, Vec<u8>>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct TileCacheKey {
    codepoint: char,
    fg: [u8; 4],
    bg: [u8; 4],
    hidden: bool,
    spacer: bool,
    missing: bool,
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
            cells: Vec::new(),
            last_cells: None,
            tile_cache: HashMap::new(),
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
        if geometry_changed {
            self.tile_cache.clear();
            self.cells.resize(
                usize::from(frame.rows) * usize::from(frame.cols),
                TerminalCell::default(),
            );
            self.last_cells = None;
        }
        self.apply_frame(frame);

        if geometry_changed || matches!(frame.damage, TerminalDamage::Full) {
            self.clear(DEFAULT_BACKGROUND);
            self.redraw_full(frame);
            self.last_cursor = Some(frame.cursor);
            self.last_cells = Some(self.cells.clone());
            return;
        }

        if let Some(scroll_rows) = self.detect_scroll_reuse_rows(frame) {
            self.redraw_scrolled(frame, scroll_rows);
        } else {
            self.redraw_partial(frame);
        }
        self.last_cursor = Some(frame.cursor);
        self.last_cells = Some(self.cells.clone());
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

    fn redraw_scrolled(&mut self, frame: &TerminalFrame, scroll_rows: i16) {
        self.shift_pixels(scroll_rows);
        self.redraw_exposed_rows(frame, scroll_rows);

        if let Some(previous) = self.last_cursor
            && previous.col < frame.cols
            && let Some(row) = shifted_row(previous.row, scroll_rows, frame.rows)
        {
            self.redraw_cell(frame, row, previous.col);
        }

        if frame.cursor.row < frame.rows && frame.cursor.col < frame.cols {
            self.redraw_cell(frame, frame.cursor.row, frame.cursor.col);
        }
    }

    fn redraw_cell(&mut self, frame: &TerminalFrame, row: u16, col: u16) {
        let Some(cell) = self.cell(row, col) else {
            return;
        };

        let cursor_here = frame.cursor.row == row && frame.cursor.col == col;
        let (bg, fg) = resolved_colors(cell, cursor_here);
        let hidden = cell.flags.contains(TerminalCellFlags::HIDDEN);
        let spacer = cell.flags.contains(TerminalCellFlags::WIDE_CHAR_SPACER);
        let missing = BASIC_FONTS.get(cell.codepoint).is_none() && !cell.codepoint.is_whitespace();
        let key = TileCacheKey {
            codepoint: cell.codepoint,
            fg,
            bg,
            hidden,
            spacer,
            missing,
        };
        let cell_width = self.cell_width;
        let cell_height = self.cell_height;
        let frame_width = self.width;
        let tile = self
            .tile_cache
            .entry(key)
            .or_insert_with(|| build_tile(cell_width, cell_height, key))
            .as_slice();
        blit_tile(
            &mut self.pixels,
            frame_width,
            cell_width,
            cell_height,
            row,
            col,
            tile,
        );

        if hidden || spacer {
            if cursor_here {
                self.draw_cursor(frame.cursor, row, col, fg);
            }
            return;
        }

        if cursor_here {
            self.draw_cursor(frame.cursor, row, col, fg);
        }
    }

    fn detect_scroll_reuse_rows(&self, frame: &TerminalFrame) -> Option<i16> {
        if !damage_covers_full_rows(&frame.damage, frame.rows, frame.cols) {
            return None;
        }

        let previous = self.last_cells.as_deref()?;
        if previous.len() != self.cells.len() || frame.rows <= 1 {
            return None;
        }

        let max_shift = MAX_SCROLL_REUSE_ROWS.min(frame.rows.saturating_sub(1));
        for delta in 1..=max_shift {
            if rows_match_upward_shift(previous, &self.cells, frame.rows, frame.cols, delta) {
                return Some(delta as i16);
            }
            if rows_match_downward_shift(previous, &self.cells, frame.rows, frame.cols, delta) {
                return Some(-(delta as i16));
            }
        }

        None
    }

    fn shift_pixels(&mut self, scroll_rows: i16) {
        let row_stride = self.terminal_row_stride();
        let shift = row_stride * usize::from(scroll_rows.unsigned_abs());
        if shift == 0 || shift >= self.pixels.len() {
            self.clear(DEFAULT_BACKGROUND);
            return;
        }

        if scroll_rows > 0 {
            self.pixels.copy_within(shift.., 0);
            self.clear_pixel_range(self.pixels.len() - shift..self.pixels.len());
        } else {
            let retain_len = self.pixels.len() - shift;
            self.pixels.copy_within(..retain_len, shift);
            self.clear_pixel_range(0..shift);
        }
    }

    fn redraw_exposed_rows(&mut self, frame: &TerminalFrame, scroll_rows: i16) {
        let exposed = scroll_rows.unsigned_abs();
        if scroll_rows > 0 {
            let start = frame.rows.saturating_sub(exposed);
            for row in start..frame.rows {
                self.redraw_row(frame, row);
            }
        } else {
            for row in 0..exposed.min(frame.rows) {
                self.redraw_row(frame, row);
            }
        }
    }

    fn redraw_row(&mut self, frame: &TerminalFrame, row: u16) {
        for col in 0..frame.cols {
            self.redraw_cell(frame, row, col);
        }
    }

    fn terminal_row_stride(&self) -> usize {
        (self.width as usize) * (self.cell_height as usize) * 4
    }

    fn clear_pixel_range(&mut self, range: std::ops::Range<usize>) {
        for pixel in self.pixels[range].chunks_exact_mut(4) {
            pixel.copy_from_slice(&DEFAULT_BACKGROUND);
        }
    }

    fn apply_frame(&mut self, frame: &TerminalFrame) {
        if let Some(cells) = frame.full_cells() {
            self.cells.clear();
            self.cells.extend_from_slice(cells);
            return;
        }

        if self.cells.len() != usize::from(frame.rows) * usize::from(frame.cols) {
            self.cells.resize(
                usize::from(frame.rows) * usize::from(frame.cols),
                TerminalCell::default(),
            );
        }

        if let Some(changes) = frame.changed_spans() {
            for change in changes.spans() {
                let start =
                    usize::from(change.row) * usize::from(self.cols) + usize::from(change.left);
                let source = changes.cells_for_span(change);
                let end = start + source.len();
                if let Some(target) = self.cells.get_mut(start..end) {
                    target.clone_from_slice(source);
                }
            }
        }
    }

    fn cell(&self, row: u16, col: u16) -> Option<&TerminalCell> {
        let index = usize::from(row)
            .checked_mul(usize::from(self.cols))?
            .checked_add(usize::from(col))?;
        self.cells.get(index)
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

fn blit_tile(
    pixels: &mut [u8],
    frame_width: u32,
    cell_width: u32,
    cell_height: u32,
    row: u16,
    col: u16,
    tile: &[u8],
) {
    let start_x = u32::from(col) * cell_width;
    let start_y = u32::from(row) * cell_height;
    let row_bytes = (cell_width as usize) * 4;

    for tile_row in 0..cell_height as usize {
        let dest_y = start_y as usize + tile_row;
        let dest_index = ((dest_y * frame_width as usize) + start_x as usize) * 4;
        let src_index = tile_row * row_bytes;
        pixels[dest_index..dest_index + row_bytes]
            .copy_from_slice(&tile[src_index..src_index + row_bytes]);
    }
}

fn damage_covers_full_rows(damage: &TerminalDamage, rows: u16, cols: u16) -> bool {
    let TerminalDamage::Partial(lines) = damage else {
        return false;
    };
    if rows == 0 || cols == 0 {
        return false;
    }

    let mut covered = vec![false; usize::from(rows)];
    for span in lines.iter() {
        if span.row >= rows || span.left != 0 {
            continue;
        }

        let right = span.right.min(cols.saturating_sub(1));
        if right + 1 == cols {
            covered[usize::from(span.row)] = true;
        }
    }

    covered.into_iter().all(|row| row)
}

fn rows_match_upward_shift(
    previous: &[TerminalCell],
    current: &[TerminalCell],
    rows: u16,
    cols: u16,
    delta: u16,
) -> bool {
    let width = usize::from(cols);
    for row in 0..rows.saturating_sub(delta) {
        let current_start = usize::from(row) * width;
        let previous_start = usize::from(row + delta) * width;
        if current[current_start..current_start + width]
            != previous[previous_start..previous_start + width]
        {
            return false;
        }
    }

    true
}

fn rows_match_downward_shift(
    previous: &[TerminalCell],
    current: &[TerminalCell],
    rows: u16,
    cols: u16,
    delta: u16,
) -> bool {
    let width = usize::from(cols);
    for row in delta..rows {
        let current_start = usize::from(row) * width;
        let previous_start = usize::from(row - delta) * width;
        if current[current_start..current_start + width]
            != previous[previous_start..previous_start + width]
        {
            return false;
        }
    }

    true
}

fn shifted_row(row: u16, scroll_rows: i16, total_rows: u16) -> Option<u16> {
    if scroll_rows > 0 {
        row.checked_sub(scroll_rows as u16)
    } else {
        let shifted = row.checked_add(scroll_rows.unsigned_abs())?;
        (shifted < total_rows).then_some(shifted)
    }
}

fn build_tile(cell_width: u32, cell_height: u32, key: TileCacheKey) -> Vec<u8> {
    let mut tile = vec![0; (cell_width as usize) * (cell_height as usize) * 4];
    fill_tile(&mut tile, cell_width, cell_height, key.bg);

    if key.hidden || key.spacer {
        return tile;
    }

    if let Some(glyph) = BASIC_FONTS.get(key.codepoint) {
        draw_tile_glyph(&mut tile, cell_width, cell_height, glyph, key.fg);
    } else if key.missing {
        draw_tile_missing_glyph(&mut tile, cell_width, cell_height, key.fg);
    }

    tile
}

fn fill_tile(tile: &mut [u8], cell_width: u32, cell_height: u32, color: [u8; 4]) {
    let width = cell_width as usize;
    for y in 0..cell_height as usize {
        let row_start = y * width * 4;
        for x in 0..width {
            let index = row_start + x * 4;
            tile[index..index + 4].copy_from_slice(&color);
        }
    }
}

fn draw_tile_glyph(
    tile: &mut [u8],
    cell_width: u32,
    cell_height: u32,
    glyph: [u8; 8],
    color: [u8; 4],
) {
    let scale_x = (cell_width.max(1) / 8).max(1);
    let scale_y = (cell_height.max(1) / 8).max(1);
    let glyph_width = 8 * scale_x;
    let glyph_height = 8 * scale_y;
    let offset_x = (cell_width.saturating_sub(glyph_width)) / 2;
    let offset_y = (cell_height.saturating_sub(glyph_height)) / 2;

    for (glyph_row, bits) in glyph.iter().enumerate() {
        for glyph_col in 0..8_u32 {
            if bits & (1 << glyph_col) == 0 {
                continue;
            }

            for y in 0..scale_y {
                for x in 0..scale_x {
                    let pixel_x = offset_x + glyph_col * scale_x + x;
                    let pixel_y = offset_y + (glyph_row as u32) * scale_y + y;
                    write_tile_pixel(tile, cell_width, cell_height, pixel_x, pixel_y, color);
                }
            }
        }
    }
}

fn draw_tile_missing_glyph(tile: &mut [u8], cell_width: u32, cell_height: u32, color: [u8; 4]) {
    let inset_x = cell_width.saturating_div(4).max(1);
    let inset_y = cell_height.saturating_div(4).max(1);
    let start_x = inset_x;
    let end_x = cell_width.saturating_sub(inset_x);
    let start_y = inset_y;
    let end_y = cell_height.saturating_sub(inset_y);

    for x in start_x..end_x {
        write_tile_pixel(tile, cell_width, cell_height, x, start_y, color);
        write_tile_pixel(
            tile,
            cell_width,
            cell_height,
            x,
            end_y.saturating_sub(1),
            color,
        );
    }
    for y in start_y..end_y {
        write_tile_pixel(tile, cell_width, cell_height, start_x, y, color);
        write_tile_pixel(
            tile,
            cell_width,
            cell_height,
            end_x.saturating_sub(1),
            y,
            color,
        );
    }
}

fn write_tile_pixel(
    tile: &mut [u8],
    cell_width: u32,
    cell_height: u32,
    x: u32,
    y: u32,
    color: [u8; 4],
) {
    if x >= cell_width || y >= cell_height {
        return;
    }

    let index = ((y * cell_width + x) * 4) as usize;
    tile[index..index + 4].copy_from_slice(&color);
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
