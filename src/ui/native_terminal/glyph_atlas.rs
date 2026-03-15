use std::collections::HashMap;

use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};

use super::constants::ATLAS_TEXTURE_SIZE_PX;

const FONT_BYTES: &[u8] = include_bytes!("../../../assets/terminal-native/DejaVuSansMono.ttf");
const GLYPH_SCALE_HEIGHT_RATIO: f32 = 0.82;
const ASCII_CACHE_LEN: usize = 128;

#[derive(Clone, Copy, Debug)]
pub(crate) struct GlyphTile {
    pub uv_rect: [f32; 4],
    pub empty: bool,
}

pub(crate) struct GlyphAtlas {
    font: FontArc,
    atlas_size: u32,
    pixels: Vec<u8>,
    ascii_entries: [Option<GlyphTile>; ASCII_CACHE_LEN],
    entries: HashMap<char, GlyphTile>,
    cell_width: u32,
    cell_height: u32,
    columns_per_row: u32,
    next_index: u32,
    dirty: bool,
}

impl GlyphAtlas {
    pub(crate) fn new() -> Self {
        let atlas_size = ATLAS_TEXTURE_SIZE_PX;
        Self {
            font: FontArc::try_from_slice(FONT_BYTES)
                .expect("bundled terminal pilot font should parse"),
            atlas_size,
            pixels: vec![0; (atlas_size as usize) * (atlas_size as usize)],
            ascii_entries: [None; ASCII_CACHE_LEN],
            entries: HashMap::new(),
            cell_width: 1,
            cell_height: 1,
            columns_per_row: atlas_size,
            next_index: 0,
            dirty: true,
        }
    }

    pub(crate) fn atlas_size(&self) -> u32 {
        self.atlas_size
    }

    pub(crate) fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub(crate) fn mark_uploaded(&mut self) {
        self.dirty = false;
    }

    pub(crate) fn set_cell_size(&mut self, cell_width: u32, cell_height: u32) {
        let cell_width = cell_width.max(1);
        let cell_height = cell_height.max(1);
        if self.cell_width == cell_width && self.cell_height == cell_height {
            return;
        }

        self.cell_width = cell_width;
        self.cell_height = cell_height;
        self.columns_per_row = (self.atlas_size / self.cell_width).max(1);
        self.ascii_entries = [None; ASCII_CACHE_LEN];
        self.entries.clear();
        self.next_index = 0;
        self.pixels.fill(0);
        self.dirty = true;
    }

    pub(crate) fn ensure_glyph(&mut self, codepoint: char) -> GlyphTile {
        if codepoint.is_ascii() {
            let index = codepoint as usize;
            if let Some(tile) = self.ascii_entries[index] {
                return tile;
            }

            let tile = self.insert_glyph(codepoint);
            self.ascii_entries[index] = Some(tile);
            return tile;
        }

        if let Some(tile) = self.entries.get(&codepoint).copied() {
            return tile;
        }

        let tile = self.insert_glyph(codepoint);
        self.entries.insert(codepoint, tile);
        tile
    }

    fn insert_glyph(&mut self, codepoint: char) -> GlyphTile {
        let capacity = self.max_tiles();
        if self.next_index >= capacity {
            return GlyphTile {
                uv_rect: [0.0; 4],
                empty: true,
            };
        }

        let index = self.next_index;
        self.next_index += 1;

        let atlas_x = (index % self.columns_per_row) * self.cell_width;
        let atlas_y = (index / self.columns_per_row) * self.cell_height;
        let tile = self.rasterize_tile(codepoint);
        self.write_tile(atlas_x, atlas_y, &tile);
        self.dirty = true;

        GlyphTile {
            uv_rect: [
                atlas_x as f32 / self.atlas_size as f32,
                atlas_y as f32 / self.atlas_size as f32,
                self.cell_width as f32 / self.atlas_size as f32,
                self.cell_height as f32 / self.atlas_size as f32,
            ],
            empty: tile.iter().all(|value| *value == 0),
        }
    }

    fn max_tiles(&self) -> u32 {
        let rows = (self.atlas_size / self.cell_height.max(1)).max(1);
        self.columns_per_row * rows
    }

    fn rasterize_tile(&self, codepoint: char) -> Vec<u8> {
        let mut tile = vec![0; (self.cell_width as usize) * (self.cell_height as usize)];
        if codepoint.is_whitespace() {
            return tile;
        }

        let scale = PxScale::from((self.cell_height as f32 * GLYPH_SCALE_HEIGHT_RATIO).max(1.0));
        let scaled = self.font.as_scaled(scale);
        let glyph_id = self.font.glyph_id(codepoint);
        let advance = scaled.h_advance(glyph_id);
        let line_height = scaled.height().max(1.0);
        let baseline = ((self.cell_height as f32 - line_height) / 2.0).max(0.0) + scaled.ascent();
        let origin_x = ((self.cell_width as f32 - advance) / 2.0).max(0.0);
        let glyph = glyph_id.with_scale_and_position(scale, point(origin_x, baseline));

        if let Some(outline) = self.font.outline_glyph(glyph) {
            outline.draw(|x, y, alpha| {
                if x < self.cell_width && y < self.cell_height {
                    let index = (y * self.cell_width + x) as usize;
                    tile[index] = ((alpha * 255.0).round() as u32).min(255) as u8;
                }
            });
        } else {
            draw_missing_glyph(&mut tile, self.cell_width, self.cell_height);
        }

        tile
    }

    fn write_tile(&mut self, atlas_x: u32, atlas_y: u32, tile: &[u8]) {
        let tile_row_bytes = self.cell_width as usize;
        for row in 0..self.cell_height as usize {
            let src_start = row * tile_row_bytes;
            let dst_start =
                ((atlas_y as usize + row) * self.atlas_size as usize) + atlas_x as usize;
            self.pixels[dst_start..dst_start + tile_row_bytes]
                .copy_from_slice(&tile[src_start..src_start + tile_row_bytes]);
        }
    }
}

fn draw_missing_glyph(tile: &mut [u8], cell_width: u32, cell_height: u32) {
    let inset_x = cell_width.saturating_div(4).max(1);
    let inset_y = cell_height.saturating_div(4).max(1);
    let start_x = inset_x;
    let end_x = cell_width.saturating_sub(inset_x);
    let start_y = inset_y;
    let end_y = cell_height.saturating_sub(inset_y);

    for x in start_x..end_x {
        write_tile_pixel(tile, cell_width, cell_height, x, start_y, 255);
        write_tile_pixel(
            tile,
            cell_width,
            cell_height,
            x,
            end_y.saturating_sub(1),
            255,
        );
    }

    for y in start_y..end_y {
        write_tile_pixel(tile, cell_width, cell_height, start_x, y, 255);
        write_tile_pixel(
            tile,
            cell_width,
            cell_height,
            end_x.saturating_sub(1),
            y,
            255,
        );
    }
}

fn write_tile_pixel(tile: &mut [u8], cell_width: u32, cell_height: u32, x: u32, y: u32, value: u8) {
    if x >= cell_width || y >= cell_height {
        return;
    }

    let index = (y * cell_width + x) as usize;
    tile[index] = value;
}
