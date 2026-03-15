use std::sync::Arc;

use gestalt::terminal_native::{
    TerminalCell, TerminalCellPublication, TerminalCellSpanBatch, TerminalCellSpanUpdate,
    TerminalCursor, TerminalCursorShape, TerminalDamage, TerminalDamageSpan, TerminalFrame,
    TerminalGpuSceneCache,
};

const TEST_ROWS: u16 = 2;
const TEST_COLS: u16 = 3;
const TEST_WIDTH: u32 = 27;
const TEST_HEIGHT: u32 = 36;

#[test]
fn gpu_scene_reuses_cached_glyphs_across_frames() {
    let frame = full_frame(['a', 'b', ' ', 'a', 'b', ' ']);
    let mut cache = TerminalGpuSceneCache::new();

    let first_glyph_len = {
        let first = cache.prepare(&frame, TEST_WIDTH, TEST_HEIGHT);
        first.glyph_instances.len()
    };
    let first_count = cache.cached_glyph_count();
    let second_glyph_len = {
        let second = cache.prepare(&frame, TEST_WIDTH, TEST_HEIGHT);
        second.glyph_instances.len()
    };

    assert_eq!(first_glyph_len, 4);
    assert_eq!(second_glyph_len, 4);
    assert_eq!(first_count, 2);
    assert_eq!(cache.cached_glyph_count(), 2);
}

#[test]
fn gpu_scene_applies_partial_updates_over_cached_cells() {
    let full = full_frame(['h', 'i', ' ', ' ', ' ', ' ']);
    let partial = partial_frame(
        vec![TerminalCellSpanUpdate {
            row: 0,
            left: 2,
            len: 1,
            cells_start: 0,
        }],
        vec![TerminalCell {
            codepoint: '!',
            ..TerminalCell::default()
        }],
    );
    let mut cache = TerminalGpuSceneCache::new();

    {
        let _ = cache.prepare(&full, TEST_WIDTH, TEST_HEIGHT);
    }
    let updated_glyph_len = {
        let updated = cache.prepare(&partial, TEST_WIDTH, TEST_HEIGHT);
        updated.glyph_instances.len()
    };

    assert_eq!(updated_glyph_len, 3);
    assert_eq!(cache.cached_glyph_count(), 3);
}

#[test]
fn gpu_scene_rebuilds_cursor_rows_without_cell_damage() {
    let initial = frame_with_cursor(['a', ' ', ' ', ' ', ' ', ' '], 0, 0);
    let moved = frame_with_cursor(['a', ' ', ' ', ' ', ' ', ' '], 0, 1);
    let mut cache = TerminalGpuSceneCache::new();

    let (first_len, first_color) = {
        let first = cache.prepare(&initial, TEST_WIDTH, TEST_HEIGHT);
        (first.glyph_instances.len(), first.glyph_instances[0].color)
    };
    let (second_len, second_color) = {
        let second = cache.prepare(&moved, TEST_WIDTH, TEST_HEIGHT);
        (
            second.glyph_instances.len(),
            second.glyph_instances[0].color,
        )
    };

    assert_eq!(first_len, 1);
    assert_eq!(second_len, 1);
    assert_ne!(first_color, second_color);
}

fn full_frame(chars: [char; (TEST_ROWS as usize) * (TEST_COLS as usize)]) -> TerminalFrame {
    frame_with_cursor(chars, 0, 0)
}

fn frame_with_cursor(
    chars: [char; (TEST_ROWS as usize) * (TEST_COLS as usize)],
    cursor_row: u16,
    cursor_col: u16,
) -> TerminalFrame {
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
            row: cursor_row,
            col: cursor_col,
            shape: TerminalCursorShape::Block,
        },
        bracketed_paste: false,
        display_offset: 0,
        damage: TerminalDamage::Full,
        publication: TerminalCellPublication::Full(Arc::new(cells)),
    }
}

fn partial_frame(spans: Vec<TerminalCellSpanUpdate>, cells: Vec<TerminalCell>) -> TerminalFrame {
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
        damage: TerminalDamage::Partial(
            [TerminalDamageSpan {
                row: 0,
                left: 2,
                right: 2,
            }]
            .into(),
        ),
        publication: TerminalCellPublication::Partial(TerminalCellSpanBatch::new(
            spans.into_boxed_slice(),
            cells.into_boxed_slice(),
        )),
    }
}
