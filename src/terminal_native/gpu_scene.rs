use std::sync::Arc;
use std::time::Instant;

use bytemuck::{Pod, Zeroable};

use super::glyph_atlas::{GlyphAtlas, SharedGlyphAtlas};
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

pub struct TerminalGpuScene<'a> {
    pub background_instances: &'a [QuadInstance],
    pub glyph_instances: &'a [QuadInstance],
    pub overlay_instances: &'a [QuadInstance],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TerminalGpuSceneProfile {
    pub apply_frame_us: u128,
    pub row_rebuild_us: u128,
    pub flatten_us: u128,
    pub overlay_us: u128,
    pub rows_rebuilt: u32,
    pub cells_rebuilt: u32,
}

pub struct TerminalGpuSceneCache {
    atlas: SharedGlyphAtlas,
    cells: Vec<TerminalCell>,
    shared_full_cells: Option<Arc<Vec<TerminalCell>>>,
    rows: u16,
    cols: u16,
    background_rows: Vec<Vec<QuadInstance>>,
    glyph_rows: Vec<Vec<QuadInstance>>,
    dirty_rows: Vec<bool>,
    flat_backgrounds: Vec<QuadInstance>,
    flat_glyphs: Vec<QuadInstance>,
    flat_overlays: Vec<QuadInstance>,
    last_cursor: Option<TerminalCursor>,
    cell_width: u32,
    cell_height: u32,
}

impl TerminalGpuSceneCache {
    pub fn new() -> Self {
        Self::with_shared_atlas(SharedGlyphAtlas::new())
    }

    pub fn with_shared_atlas(atlas: SharedGlyphAtlas) -> Self {
        Self {
            atlas,
            cells: Vec::new(),
            shared_full_cells: None,
            rows: 0,
            cols: 0,
            background_rows: Vec::new(),
            glyph_rows: Vec::new(),
            dirty_rows: Vec::new(),
            flat_backgrounds: Vec::new(),
            flat_glyphs: Vec::new(),
            flat_overlays: Vec::new(),
            last_cursor: None,
            cell_width: 1,
            cell_height: 1,
        }
    }

    pub fn prepare(
        &mut self,
        frame: &TerminalFrame,
        width: u32,
        height: u32,
    ) -> TerminalGpuScene<'_> {
        self.prepare_profiled(frame, width, height).0
    }

    pub fn prepare_profiled(
        &mut self,
        frame: &TerminalFrame,
        width: u32,
        height: u32,
    ) -> (TerminalGpuScene<'_>, TerminalGpuSceneProfile) {
        let apply_frame_started = Instant::now();
        self.apply_frame(frame);
        let apply_frame_us = apply_frame_started.elapsed().as_micros();

        let cell_width = cell_extent(width, frame.cols);
        let cell_height = cell_extent(height, frame.rows);
        let geometry_changed = self.cell_width != cell_width || self.cell_height != cell_height;
        self.cell_width = cell_width;
        self.cell_height = cell_height;
        self.atlas
            .with_mut(|atlas| atlas.set_cell_size(cell_width, cell_height));
        self.ensure_row_cache();

        let row_rebuild_started = Instant::now();
        let atlas = self.atlas.clone();
        let (rows_rebuilt, cells_rebuilt) = atlas.with_mut(|atlas| {
            let rows_rebuilt =
                if geometry_changed || matches!(frame.damage, super::TerminalDamage::Full) {
                    self.rebuild_all_rows(frame, atlas)
                } else {
                    self.rebuild_dirty_rows(frame, atlas)
                };
            let cells_rebuilt = rows_rebuilt.saturating_mul(u32::from(self.cols));
            (rows_rebuilt, cells_rebuilt)
        });
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
        self.flat_backgrounds.clear();
        self.flat_glyphs.clear();
        self.flat_overlays.clear();
        self.flat_backgrounds.reserve(background_count);
        self.flat_glyphs.reserve(glyph_count);
        self.flat_overlays.reserve(8);

        for row in &self.background_rows {
            self.flat_backgrounds.extend_from_slice(row);
        }
        for row in &self.glyph_rows {
            self.flat_glyphs.extend_from_slice(row);
        }
        let flatten_us = flatten_started.elapsed().as_micros();

        let overlay_started = Instant::now();
        extend_cursor_instances(
            &mut self.flat_overlays,
            frame,
            cell_width as f32,
            cell_height as f32,
        );
        let overlay_us = overlay_started.elapsed().as_micros();
        self.last_cursor = Some(frame.cursor);

        (
            TerminalGpuScene {
                background_instances: self.flat_backgrounds.as_slice(),
                glyph_instances: self.flat_glyphs.as_slice(),
                overlay_instances: self.flat_overlays.as_slice(),
            },
            TerminalGpuSceneProfile {
                apply_frame_us,
                row_rebuild_us,
                flatten_us,
                overlay_us,
                rows_rebuilt,
                cells_rebuilt,
            },
        )
    }

    pub fn shared_atlas(&self) -> SharedGlyphAtlas {
        self.atlas.clone()
    }

    pub fn cached_glyph_count(&self) -> usize {
        self.atlas.cached_glyph_count()
    }

    fn apply_frame(&mut self, frame: &TerminalFrame) {
        if self.rows != frame.rows || self.cols != frame.cols {
            self.rows = frame.rows;
            self.cols = frame.cols;
            self.shared_full_cells = None;
            self.cells
                .resize(cell_count(frame.rows, frame.cols), TerminalCell::default());
            self.background_rows
                .resize_with(usize::from(frame.rows), std::vec::Vec::new);
            self.glyph_rows
                .resize_with(usize::from(frame.rows), std::vec::Vec::new);
            self.dirty_rows.resize(usize::from(frame.rows), false);
        }

        if let Some(cells) = frame.full_cells_shared() {
            self.shared_full_cells = Some(Arc::clone(cells));
            return;
        }

        self.materialize_shared_full_cells();
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

    fn ensure_row_cache(&mut self) {
        let row_count = usize::from(self.rows);
        self.background_rows
            .resize_with(row_count, std::vec::Vec::new);
        self.glyph_rows.resize_with(row_count, std::vec::Vec::new);
        self.dirty_rows.resize(row_count, false);
    }

    fn materialize_shared_full_cells(&mut self) {
        let Some(cells) = self.shared_full_cells.take() else {
            return;
        };

        if self.cells.len() != cells.len() {
            self.cells.resize(cells.len(), TerminalCell::default());
        }
        self.cells.clone_from_slice(cells.as_slice());
    }

    fn rebuild_all_rows(&mut self, frame: &TerminalFrame, atlas: &mut GlyphAtlas) -> u32 {
        let row_count = usize::from(self.rows);
        let cols = usize::from(self.cols);
        let cell_width = self.cell_width as f32;
        let cell_height = self.cell_height as f32;
        let cells = self
            .shared_full_cells
            .as_deref()
            .map(std::vec::Vec::as_slice)
            .unwrap_or(self.cells.as_slice());

        for (row_index, ((backgrounds, glyphs), row_cells)) in self
            .background_rows
            .iter_mut()
            .zip(self.glyph_rows.iter_mut())
            .zip(cells.chunks_exact(cols))
            .enumerate()
            .take(row_count)
        {
            rebuild_row_cells(
                backgrounds,
                glyphs,
                row_index as u16,
                row_cells,
                frame.cursor,
                cell_width,
                cell_height,
                atlas,
            );
        }
        u32::from(self.rows)
    }

    fn rebuild_dirty_rows(&mut self, frame: &TerminalFrame, atlas: &mut GlyphAtlas) -> u32 {
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

        let mut rebuilt_rows = 0u32;
        for row in 0..self.dirty_rows.len() {
            if self.dirty_rows[row] {
                self.rebuild_row(row as u16, frame.cursor, atlas);
                rebuilt_rows += 1;
            }
        }
        rebuilt_rows
    }

    fn rebuild_row(&mut self, row: u16, cursor: TerminalCursor, atlas: &mut GlyphAtlas) {
        let row_index = usize::from(row);
        let Some(backgrounds) = self.background_rows.get_mut(row_index) else {
            return;
        };
        let Some(glyphs) = self.glyph_rows.get_mut(row_index) else {
            return;
        };
        let row_start = usize::from(row) * usize::from(self.cols);
        let row_end = row_start + usize::from(self.cols);
        let Some(row_cells) = self
            .shared_full_cells
            .as_deref()
            .map(std::vec::Vec::as_slice)
            .unwrap_or(self.cells.as_slice())
            .get(row_start..row_end)
        else {
            return;
        };
        rebuild_row_cells(
            backgrounds,
            glyphs,
            row,
            row_cells,
            cursor,
            self.cell_width as f32,
            self.cell_height as f32,
            atlas,
        );
    }
}

fn rebuild_row_cells(
    backgrounds: &mut Vec<QuadInstance>,
    glyphs: &mut Vec<QuadInstance>,
    row: u16,
    row_cells: &[TerminalCell],
    cursor: TerminalCursor,
    cell_width: f32,
    cell_height: f32,
    atlas: &mut GlyphAtlas,
) {
    backgrounds.clear();
    glyphs.clear();
    let row_y = f32::from(row) * cell_height;
    let mut x = 0.0_f32;

    for (col, cell) in row_cells.iter().enumerate() {
        let col = col as u16;

        let cursor_here = cursor.row == row && cursor.col == col;
        if is_plain_blank_cell(cell, cursor_here) {
            x += cell_width;
            continue;
        }

        let rect = [x, row_y, cell_width, cell_height];
        if is_default_visible_cell(cell, cursor_here) {
            let tile = atlas.ensure_glyph(cell.codepoint);
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
            let tile = atlas.ensure_glyph(cell.codepoint);
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

        let tile = atlas.ensure_glyph(cell.codepoint);
        if !tile.empty {
            glyphs.push(QuadInstance::glyph(rect, rgba(fg), tile.uv_rect));
        }
        x += cell_width;
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
