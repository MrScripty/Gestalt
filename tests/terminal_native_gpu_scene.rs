use std::sync::Arc;

use gestalt::terminal_native::{
    TerminalCell, TerminalCursor, TerminalCursorShape, TerminalDamage, TerminalFrame,
    TerminalGpuSceneCache,
};

const TEST_ROWS: u16 = 2;
const TEST_COLS: u16 = 3;
const TEST_WIDTH: u32 = 27;
const TEST_HEIGHT: u32 = 36;

#[test]
fn gpu_scene_reuses_cached_glyphs_across_frames() {
    let frame = frame(['a', 'b', ' ', 'a', 'b', ' ']);
    let mut cache = TerminalGpuSceneCache::new();

    let first = cache.prepare(&frame, TEST_WIDTH, TEST_HEIGHT);
    let first_count = cache.cached_glyph_count();
    let second = cache.prepare(&frame, TEST_WIDTH, TEST_HEIGHT);

    assert_eq!(first.glyph_instances.len(), 4);
    assert_eq!(second.glyph_instances.len(), 4);
    assert_eq!(first_count, 2);
    assert_eq!(cache.cached_glyph_count(), 2);
}

fn frame(chars: [char; (TEST_ROWS as usize) * (TEST_COLS as usize)]) -> TerminalFrame {
    let cells = chars
        .into_iter()
        .map(|codepoint| TerminalCell {
            codepoint,
            ..TerminalCell::default()
        })
        .collect::<Vec<_>>();

    TerminalFrame {
        rows: TEST_ROWS,
        cols: TEST_COLS,
        cursor: TerminalCursor {
            row: 0,
            col: 0,
            shape: TerminalCursorShape::Hidden,
        },
        bracketed_paste: false,
        display_offset: 0,
        damage: TerminalDamage::Full,
        cells: Arc::<[TerminalCell]>::from(cells),
    }
}
