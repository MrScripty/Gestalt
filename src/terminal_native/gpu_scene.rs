use std::time::Instant;

use bytemuck::{Pod, Zeroable};

use super::glyph_atlas::GlyphAtlas;
use super::{
    TerminalCell, TerminalCellFlags, TerminalColor, TerminalCursor, TerminalCursorShape,
    TerminalFrame,
};

const DEFAULT_BACKGROUND: [u8; 4] = [8, 12, 16, 255];
const DEFAULT_FOREGROUND: [u8; 4] = [222, 226, 230, 255];
const CURSOR_COLOR: [u8; 4] = [255, 244, 163, 160];
const CURSOR_LINE_THICKNESS_PX: f32 = 2.0;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct QuadInstance {
    pub rect: [f32; 4],
    pub color: [f32; 4],
    pub uv_rect: [f32; 4],
}

pub struct TerminalGpuScene {
    pub background_instances: Vec<QuadInstance>,
    pub glyph_instances: Vec<QuadInstance>,
    pub overlay_instances: Vec<QuadInstance>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TerminalGpuSceneProfile {
    pub apply_frame_us: u128,
    pub row_rebuild_us: u128,
    pub flatten_us: u128,
    pub overlay_us: u128,
}

pub struct TerminalGpuSceneCache {
    atlas: GlyphAtlas,
    cells: Vec<TerminalCell>,
    rows: u16,
    cols: u16,
    background_rows: Vec<Vec<QuadInstance>>,
    glyph_rows: Vec<Vec<QuadInstance>>,
    dirty_rows: Vec<bool>,
    last_cursor: Option<TerminalCursor>,
    cell_width: u32,
    cell_height: u32,
}

impl TerminalGpuSceneCache {
    pub fn new() -> Self {
        Self {
            atlas: GlyphAtlas::new(),
            cells: Vec::new(),
            rows: 0,
            cols: 0,
            background_rows: Vec::new(),
            glyph_rows: Vec::new(),
            dirty_rows: Vec::new(),
            last_cursor: None,
            cell_width: 1,
            cell_height: 1,
        }
    }

    pub fn prepare(&mut self, frame: &TerminalFrame, width: u32, height: u32) -> TerminalGpuScene {
        self.prepare_profiled(frame, width, height).0
    }

    pub fn prepare_profiled(
        &mut self,
        frame: &TerminalFrame,
        width: u32,
        height: u32,
    ) -> (TerminalGpuScene, TerminalGpuSceneProfile) {
        let apply_frame_started = Instant::now();
        self.apply_frame(frame);
        let apply_frame_us = apply_frame_started.elapsed().as_micros();

        let cell_width = cell_extent(width, frame.cols);
        let cell_height = cell_extent(height, frame.rows);
        let geometry_changed = self.cell_width != cell_width || self.cell_height != cell_height;
        self.cell_width = cell_width;
        self.cell_height = cell_height;
        self.atlas.set_cell_size(cell_width, cell_height);
        self.ensure_row_cache();

        let row_rebuild_started = Instant::now();
        if geometry_changed || matches!(frame.damage, super::TerminalDamage::Full) {
            self.rebuild_all_rows(frame);
        } else {
            self.rebuild_dirty_rows(frame);
        }
        let row_rebuild_us = row_rebuild_started.elapsed().as_micros();

        let flatten_started = Instant::now();
        let background_count = self
            .background_rows
            .iter()
            .map(std::vec::Vec::len)
            .sum::<usize>();
        let glyph_count = self
            .glyph_rows
            .iter()
            .map(std::vec::Vec::len)
            .sum::<usize>();
        let mut backgrounds = Vec::with_capacity(background_count);
        let mut glyphs = Vec::with_capacity(glyph_count);
        let mut overlays = Vec::with_capacity(8);

        for row in &self.background_rows {
            backgrounds.extend_from_slice(row);
        }
        for row in &self.glyph_rows {
            glyphs.extend_from_slice(row);
        }
        let flatten_us = flatten_started.elapsed().as_micros();

        let overlay_started = Instant::now();
        extend_cursor_instances(&mut overlays, frame, cell_width as f32, cell_height as f32);
        let overlay_us = overlay_started.elapsed().as_micros();
        self.last_cursor = Some(frame.cursor);

        (
            TerminalGpuScene {
                background_instances: backgrounds,
                glyph_instances: glyphs,
                overlay_instances: overlays,
            },
            TerminalGpuSceneProfile {
                apply_frame_us,
                row_rebuild_us,
                flatten_us,
                overlay_us,
            },
        )
    }

    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }

    pub fn atlas_mut(&mut self) -> &mut GlyphAtlas {
        &mut self.atlas
    }

    pub fn cached_glyph_count(&self) -> usize {
        self.atlas.cached_glyph_count()
    }

    fn apply_frame(&mut self, frame: &TerminalFrame) {
        if self.rows != frame.rows || self.cols != frame.cols {
            self.rows = frame.rows;
            self.cols = frame.cols;
            self.cells
                .resize(cell_count(frame.rows, frame.cols), TerminalCell::default());
            self.background_rows
                .resize_with(usize::from(frame.rows), std::vec::Vec::new);
            self.glyph_rows
                .resize_with(usize::from(frame.rows), std::vec::Vec::new);
            self.dirty_rows.resize(usize::from(frame.rows), false);
        }

        if let Some(cells) = frame.full_cells() {
            self.cells.clear();
            self.cells.extend_from_slice(cells);
            return;
        }

        if let Some(changes) = frame.changed_spans() {
            for change in changes.spans() {
                let start = match cell_index(self.cols, change.row, change.left) {
                    Some(index) => index,
                    None => continue,
                };
                let source = changes.cells_for_span(change);
                let end = start + source.len();
                if let Some(target) = self.cells.get_mut(start..end) {
                    target.clone_from_slice(source);
                }
            }
        }
    }

    fn ensure_row_cache(&mut self) {
        let row_count = usize::from(self.rows);
        self.background_rows
            .resize_with(row_count, std::vec::Vec::new);
        self.glyph_rows.resize_with(row_count, std::vec::Vec::new);
        self.dirty_rows.resize(row_count, false);
    }

    fn rebuild_all_rows(&mut self, frame: &TerminalFrame) {
        for row in 0..self.rows {
            self.rebuild_row(row, frame.cursor);
        }
    }

    fn rebuild_dirty_rows(&mut self, frame: &TerminalFrame) {
        self.dirty_rows.fill(false);

        if let super::TerminalDamage::Partial(spans) = &frame.damage {
            for span in spans.iter() {
                if let Some(dirty) = self.dirty_rows.get_mut(usize::from(span.row)) {
                    *dirty = true;
                }
            }
        }

        if let Some(previous_cursor) = self.last_cursor {
            if previous_cursor.row < self.rows {
                self.dirty_rows[usize::from(previous_cursor.row)] = true;
            }
        }
        if frame.cursor.row < self.rows {
            self.dirty_rows[usize::from(frame.cursor.row)] = true;
        }

        for row in 0..self.dirty_rows.len() {
            if self.dirty_rows[row] {
                self.rebuild_row(row as u16, frame.cursor);
            }
        }
    }

    fn rebuild_row(&mut self, row: u16, cursor: TerminalCursor) {
        let row_index = usize::from(row);
        let mut backgrounds = std::mem::take(&mut self.background_rows[row_index]);
        let mut glyphs = std::mem::take(&mut self.glyph_rows[row_index]);
        backgrounds.clear();
        glyphs.clear();
        let cell_width = self.cell_width as f32;
        let cell_height = self.cell_height as f32;
        let row_y = f32::from(row) * cell_height;
        let mut x = 0.0_f32;
        let row_start = usize::from(row) * usize::from(self.cols);
        let row_end = row_start + usize::from(self.cols);
        let Some(row_cells) = self.cells.get(row_start..row_end) else {
            return;
        };

        for (col, cell) in row_cells.iter().enumerate() {
            let col = col as u16;

            let cursor_here = cursor.row == row && cursor.col == col;
            if is_plain_blank_cell(cell, cursor_here) {
                x += cell_width;
                continue;
            }

            let rect = [x, row_y, cell_width, cell_height];
            if is_default_visible_cell(cell, cursor_here) {
                let tile = self.atlas.ensure_glyph(cell.codepoint);
                if !tile.empty {
                    glyphs.push(QuadInstance::glyph(
                        rect,
                        rgba(DEFAULT_FOREGROUND),
                        tile.uv_rect,
                    ));
                }
                x += cell_width;
                continue;
            }
            if is_simple_foreground_cell(cell, cursor_here) {
                let tile = self.atlas.ensure_glyph(cell.codepoint);
                if !tile.empty {
                    glyphs.push(QuadInstance::glyph(
                        rect,
                        rgba(resolve_color(cell.fg)),
                        tile.uv_rect,
                    ));
                }
                x += cell_width;
                continue;
            }

            let (bg, fg) = resolved_colors(cell, cursor_here);

            if bg != DEFAULT_BACKGROUND {
                backgrounds.push(QuadInstance::solid(rect, rgba(bg)));
            }

            if cell.flags.contains(TerminalCellFlags::HIDDEN)
                || cell.flags.contains(TerminalCellFlags::WIDE_CHAR_SPACER)
                || cell.codepoint.is_whitespace()
            {
                x += cell_width;
                continue;
            }

            let tile = self.atlas.ensure_glyph(cell.codepoint);
            if !tile.empty {
                glyphs.push(QuadInstance::glyph(rect, rgba(fg), tile.uv_rect));
            }
            x += cell_width;
        }

        if let Some(cached_backgrounds) = self.background_rows.get_mut(row_index) {
            *cached_backgrounds = backgrounds;
        }
        if let Some(cached_glyphs) = self.glyph_rows.get_mut(row_index) {
            *cached_glyphs = glyphs;
        }
    }
}

impl QuadInstance {
    fn solid(rect: [f32; 4], color: [f32; 4]) -> Self {
        Self {
            rect,
            color,
            uv_rect: [0.0; 4],
        }
    }

    fn glyph(rect: [f32; 4], color: [f32; 4], uv_rect: [f32; 4]) -> Self {
        Self {
            rect,
            color,
            uv_rect,
        }
    }
}

fn extend_cursor_instances(
    overlays: &mut Vec<QuadInstance>,
    frame: &TerminalFrame,
    cell_width: f32,
    cell_height: f32,
) {
    if frame.cursor.row >= frame.rows || frame.cursor.col >= frame.cols {
        return;
    }

    let x = f32::from(frame.cursor.col) * cell_width;
    let y = f32::from(frame.cursor.row) * cell_height;
    match frame.cursor.shape {
        TerminalCursorShape::Block => overlays.push(QuadInstance::solid(
            [x, y, cell_width, cell_height],
            rgba(CURSOR_COLOR),
        )),
        TerminalCursorShape::Underline => overlays.push(QuadInstance::solid(
            [
                x,
                y + (cell_height - CURSOR_LINE_THICKNESS_PX).max(0.0),
                cell_width,
                CURSOR_LINE_THICKNESS_PX,
            ],
            rgba(CURSOR_COLOR),
        )),
        TerminalCursorShape::Beam => overlays.push(QuadInstance::solid(
            [x, y, CURSOR_LINE_THICKNESS_PX, cell_height],
            rgba(CURSOR_COLOR),
        )),
        TerminalCursorShape::HollowBlock => {
            overlays.push(QuadInstance::solid(
                [x, y, cell_width, 1.0],
                rgba(CURSOR_COLOR),
            ));
            overlays.push(QuadInstance::solid(
                [x, y + (cell_height - 1.0).max(0.0), cell_width, 1.0],
                rgba(CURSOR_COLOR),
            ));
            overlays.push(QuadInstance::solid(
                [x, y, 1.0, cell_height],
                rgba(CURSOR_COLOR),
            ));
            overlays.push(QuadInstance::solid(
                [x + (cell_width - 1.0).max(0.0), y, 1.0, cell_height],
                rgba(CURSOR_COLOR),
            ));
        }
        TerminalCursorShape::Hidden => {}
    }
}

fn is_plain_blank_cell(cell: &TerminalCell, cursor_here: bool) -> bool {
    !cursor_here
        && cell.codepoint.is_whitespace()
        && matches!(cell.bg, TerminalColor::DefaultBackground)
        && matches!(cell.fg, TerminalColor::DefaultForeground)
        && cell.flags == TerminalCellFlags::NONE
}

fn is_default_visible_cell(cell: &TerminalCell, cursor_here: bool) -> bool {
    !cursor_here
        && !cell.codepoint.is_whitespace()
        && matches!(cell.bg, TerminalColor::DefaultBackground)
        && matches!(cell.fg, TerminalColor::DefaultForeground)
        && cell.flags == TerminalCellFlags::NONE
}

fn is_simple_foreground_cell(cell: &TerminalCell, cursor_here: bool) -> bool {
    !cursor_here
        && !cell.codepoint.is_whitespace()
        && matches!(cell.bg, TerminalColor::DefaultBackground)
        && !matches!(cell.fg, TerminalColor::DefaultForeground)
        && cell.flags == TerminalCellFlags::NONE
}

fn cell_count(rows: u16, cols: u16) -> usize {
    usize::from(rows) * usize::from(cols)
}

fn cell_index(cols: u16, row: u16, col: u16) -> Option<usize> {
    usize::from(row)
        .checked_mul(usize::from(cols))?
        .checked_add(usize::from(col))
}

fn cell_extent(size: u32, cells: u16) -> u32 {
    if cells == 0 {
        return 1;
    }

    (size / u32::from(cells)).max(1)
}

fn rgba(color: [u8; 4]) -> [f32; 4] {
    [
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
        color[3] as f32 / 255.0,
    ]
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
        TerminalColor::Cursor => [255, 244, 163, 255],
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
