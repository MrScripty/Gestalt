use bytemuck::{Pod, Zeroable};

use super::glyph_atlas::GlyphAtlas;
use super::{TerminalCell, TerminalCellFlags, TerminalColor, TerminalCursorShape, TerminalFrame};

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

pub struct TerminalGpuSceneCache {
    atlas: GlyphAtlas,
}

impl TerminalGpuSceneCache {
    pub fn new() -> Self {
        Self {
            atlas: GlyphAtlas::new(),
        }
    }

    pub fn prepare(&mut self, frame: &TerminalFrame, width: u32, height: u32) -> TerminalGpuScene {
        let cell_width = cell_extent(width, frame.cols);
        let cell_height = cell_extent(height, frame.rows);
        self.atlas.set_cell_size(cell_width, cell_height);

        let total_cells = usize::from(frame.rows) * usize::from(frame.cols);
        let mut backgrounds = Vec::with_capacity(total_cells / 4);
        let mut glyphs = Vec::with_capacity(total_cells);
        let mut overlays = Vec::with_capacity(8);

        for row in 0..frame.rows {
            for col in 0..frame.cols {
                let Some(cell) = frame.cell(row, col) else {
                    continue;
                };

                let cursor_here = frame.cursor.row == row && frame.cursor.col == col;
                let (bg, fg) = resolved_colors(cell, cursor_here);
                let rect = cell_rect(row, col, cell_width, cell_height);

                if bg != DEFAULT_BACKGROUND {
                    backgrounds.push(QuadInstance::solid(rect, rgba(bg)));
                }

                if cell.flags.contains(TerminalCellFlags::HIDDEN)
                    || cell.flags.contains(TerminalCellFlags::WIDE_CHAR_SPACER)
                    || cell.codepoint.is_whitespace()
                {
                    continue;
                }

                let tile = self.atlas.ensure_glyph(cell.codepoint);
                if !tile.empty {
                    glyphs.push(QuadInstance::glyph(rect, rgba(fg), tile.uv_rect));
                }
            }
        }

        extend_cursor_instances(&mut overlays, frame, cell_width as f32, cell_height as f32);

        TerminalGpuScene {
            background_instances: backgrounds,
            glyph_instances: glyphs,
            overlay_instances: overlays,
        }
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

fn cell_rect(row: u16, col: u16, cell_width: u32, cell_height: u32) -> [f32; 4] {
    [
        (u32::from(col) * cell_width) as f32,
        (u32::from(row) * cell_height) as f32,
        cell_width as f32,
        cell_height as f32,
    ]
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
