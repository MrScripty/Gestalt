use std::sync::Arc;

use gestalt::terminal_native::{
    TerminalCell, TerminalCellPublication, TerminalCellUpdate, TerminalCursor, TerminalCursorShape,
    TerminalDamage, TerminalDamageSpan, TerminalFrame, TerminalGpuSceneCache,
};

const TEST_ROWS: u16 = 2;
const TEST_COLS: u16 = 3;
const TEST_WIDTH: u32 = 27;
const TEST_HEIGHT: u32 = 36;

#[test]
fn gpu_scene_reuses_cached_glyphs_across_frames() {
    let frame = full_frame(['a', 'b', ' ', 'a', 'b', ' ']);
    let mut cache = TerminalGpuSceneCache::new();

    let first = cache.prepare(&frame, TEST_WIDTH, TEST_HEIGHT);
    let first_count = cache.cached_glyph_count();
    let second = cache.prepare(&frame, TEST_WIDTH, TEST_HEIGHT);

    assert_eq!(first.glyph_instances.len(), 4);
    assert_eq!(second.glyph_instances.len(), 4);
    assert_eq!(first_count, 2);
    assert_eq!(cache.cached_glyph_count(), 2);
}

#[test]
fn gpu_scene_applies_partial_updates_over_cached_cells() {
    let full = full_frame(['h', 'i', ' ', ' ', ' ', ' ']);
    let partial = partial_frame(vec![TerminalCellUpdate {
        row: 0,
        col: 2,
        cell: TerminalCell {
            codepoint: '!',
            ..TerminalCell::default()
        },
    }]);
    let mut cache = TerminalGpuSceneCache::new();

    let _ = cache.prepare(&full, TEST_WIDTH, TEST_HEIGHT);
    let updated = cache.prepare(&partial, TEST_WIDTH, TEST_HEIGHT);

    assert_eq!(updated.glyph_instances.len(), 3);
    assert_eq!(cache.cached_glyph_count(), 3);
}

fn full_frame(chars: [char; (TEST_ROWS as usize) * (TEST_COLS as usize)]) -> TerminalFrame {
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
        publication: TerminalCellPublication::Full(Arc::<[TerminalCell]>::from(cells)),
    }
}

fn partial_frame(changes: Vec<TerminalCellUpdate>) -> TerminalFrame {
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
        publication: TerminalCellPublication::Partial(Arc::<[TerminalCellUpdate]>::from(changes)),
    }
}
