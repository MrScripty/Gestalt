use super::constants::{CELL_HEIGHT_PX, CELL_WIDTH_PX};
use bytemuck::{Pod, Zeroable};

use super::frame::{NativeTerminalCell, NativeTerminalCursor, NativeTerminalFrame};
use super::glyph_atlas::GlyphAtlas;

const DEFAULT_BACKGROUND: [u8; 4] = [8, 12, 16, 255];
const DEFAULT_FOREGROUND: [u8; 4] = [222, 226, 230, 255];
const CURSOR_COLOR: [u8; 4] = [255, 244, 163, 160];

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct QuadInstance {
    pub rect: [f32; 4],
    pub color: [f32; 4],
    pub uv_rect: [f32; 4],
}

pub(crate) struct NativeTerminalScene {
    pub background_instances: Vec<QuadInstance>,
    pub glyph_instances: Vec<QuadInstance>,
    pub overlay_instances: Vec<QuadInstance>,
}

impl QuadInstance {
    pub(crate) fn solid(rect: [f32; 4], color: [u8; 4]) -> Self {
        Self {
            rect,
            color: rgba(color),
            uv_rect: [0.0; 4],
        }
    }

    pub(crate) fn glyph(rect: [f32; 4], color: [u8; 4], uv_rect: [f32; 4]) -> Self {
        Self {
            rect,
            color: rgba(color),
            uv_rect,
        }
    }
}

pub(crate) fn build_scene(
    frame: &NativeTerminalFrame,
    atlas: &mut GlyphAtlas,
    width: u32,
    height: u32,
    ui_scale: f32,
) -> NativeTerminalScene {
    let cell_width = scaled_cell_extent(CELL_WIDTH_PX, ui_scale);
    let cell_height = scaled_cell_extent(CELL_HEIGHT_PX, ui_scale);
    atlas.set_cell_size(cell_width, cell_height);

    let cell_width = cell_width as f32;
    let cell_height = cell_height as f32;
    let mut glyphs = Vec::new();
    let mut overlays = Vec::new();
    let backgrounds = vec![QuadInstance::solid(
        [0.0, 0.0, width as f32, height as f32],
        DEFAULT_BACKGROUND,
    )];

    for row in 0..frame.rows {
        for col in 0..frame.cols {
            let cell = frame.cell(row, col);
            if is_blank(cell) {
                continue;
            }

            let tile = atlas.ensure_glyph(cell.codepoint);
            if tile.empty {
                continue;
            }

            let rect = [
                f32::from(col) * cell_width,
                f32::from(row) * cell_height,
                cell_width,
                cell_height,
            ];
            glyphs.push(QuadInstance::glyph(rect, DEFAULT_FOREGROUND, tile.uv_rect));
        }
    }

    if let Some(cursor) = frame.cursor {
        overlays.push(cursor_instance(cursor, cell_width, cell_height));
    }

    NativeTerminalScene {
        background_instances: backgrounds,
        glyph_instances: glyphs,
        overlay_instances: overlays,
    }
}

pub(crate) fn surface_cells(width: u32, height: u32, ui_scale: f32) -> (u16, u16) {
    let cell_width = scaled_cell_extent(CELL_WIDTH_PX, ui_scale).max(1);
    let cell_height = scaled_cell_extent(CELL_HEIGHT_PX, ui_scale).max(1);
    let cols = (width / cell_width).max(1) as u16;
    let rows = (height / cell_height).max(1) as u16;
    (rows, cols)
}

fn cursor_instance(
    cursor: NativeTerminalCursor,
    cell_width: f32,
    cell_height: f32,
) -> QuadInstance {
    QuadInstance::solid(
        [
            f32::from(cursor.col) * cell_width,
            f32::from(cursor.row) * cell_height,
            cell_width,
            cell_height,
        ],
        CURSOR_COLOR,
    )
}

fn scaled_cell_extent(base_extent: f32, ui_scale: f32) -> u32 {
    (base_extent * ui_scale.max(0.1)).round().max(1.0) as u32
}

fn is_blank(cell: NativeTerminalCell) -> bool {
    cell.codepoint == ' '
}

fn rgba(color: [u8; 4]) -> [f32; 4] {
    [
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
        color[3] as f32 / 255.0,
    ]
}
