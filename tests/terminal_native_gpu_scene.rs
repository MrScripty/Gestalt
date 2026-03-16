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

#[test]
fn gpu_scene_skips_identical_full_frame_row_rebuilds() {
    let frame = hidden_cursor_frame(['a', 'b', ' ', ' ', ' ', ' ']);
    let mut cache = TerminalGpuSceneCache::new();

    let (_, first_profile) = cache.prepare_profiled(&frame, TEST_WIDTH, TEST_HEIGHT);
    let (_, second_profile) = cache.prepare_profiled(&frame, TEST_WIDTH, TEST_HEIGHT);

    assert!(first_profile.rows_rebuilt > 0);
    assert_eq!(second_profile.rows_rebuilt, 0);
}

#[test]
fn gpu_scene_rebuilds_only_changed_rows_for_full_frame_updates() {
    let initial = hidden_cursor_frame(['a', 'b', ' ', ' ', ' ', ' ']);
    let updated = hidden_cursor_frame(['a', 'b', ' ', 'x', ' ', ' ']);
    let mut cache = TerminalGpuSceneCache::new();

    let _ = cache.prepare_profiled(&initial, TEST_WIDTH, TEST_HEIGHT);
    let (_, profile) = cache.prepare_profiled(&updated, TEST_WIDTH, TEST_HEIGHT);

    assert_eq!(profile.rows_rebuilt, 1);
    assert_eq!(profile.cells_rebuilt, u32::from(TEST_COLS));
}

fn full_frame(chars: [char; (TEST_ROWS as usize) * (TEST_COLS as usize)]) -> TerminalFrame {
    frame_with_cursor(chars, 0, 0)
}

fn hidden_cursor_frame(
    chars: [char; (TEST_ROWS as usize) * (TEST_COLS as usize)],
) -> TerminalFrame {
    frame_with_cursor_shape(chars, 0, 0, TerminalCursorShape::Hidden)
}

fn frame_with_cursor(
    chars: [char; (TEST_ROWS as usize) * (TEST_COLS as usize)],
    cursor_row: u16,
    cursor_col: u16,
) -> TerminalFrame {
    frame_with_cursor_shape(chars, cursor_row, cursor_col, TerminalCursorShape::Block)
}

fn frame_with_cursor_shape(
    chars: [char; (TEST_ROWS as usize) * (TEST_COLS as usize)],
    cursor_row: u16,
    cursor_col: u16,
    cursor_shape: TerminalCursorShape,
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
        history_size: 0,
        cursor: TerminalCursor {
            row: cursor_row,
            col: cursor_col,
            shape: cursor_shape,
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
        history_size: 0,
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
