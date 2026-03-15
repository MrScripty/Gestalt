use std::sync::Arc;

use gestalt::terminal_native::{
    TerminalCell, TerminalCursor, TerminalCursorShape, TerminalDamage, TerminalDamageSpan,
    TerminalFrame, TerminalRaster,
};

const TEST_ROWS: u16 = 3;
const TEST_COLS: u16 = 2;
const TEST_WIDTH: u32 = 18;
const TEST_HEIGHT: u32 = 24;

#[test]
fn scroll_reuse_matches_full_redraw_for_upward_shift() {
    let initial = frame(['a', 'b', 'c', 'd', 'e', 'f']);
    let scrolled = frame(['c', 'd', 'e', 'f', 'g', 'h']);

    let mut optimized = TerminalRaster::new();
    optimized.update(&initial, TEST_WIDTH, TEST_HEIGHT);
    optimized.update(&scrolled, TEST_WIDTH, TEST_HEIGHT);

    let mut full_redraw = TerminalRaster::new();
    full_redraw.update(&scrolled, TEST_WIDTH, TEST_HEIGHT);

    assert_eq!(optimized.pixels(), full_redraw.pixels());
}

fn frame(chars: [char; (TEST_ROWS as usize) * (TEST_COLS as usize)]) -> TerminalFrame {
    let cells = chars
        .into_iter()
        .map(|codepoint| TerminalCell {
            codepoint,
            ..TerminalCell::default()
        })
        .collect::<Vec<_>>();

    let damage = TerminalDamage::Partial(
        (0..TEST_ROWS)
            .map(|row| TerminalDamageSpan {
                row,
                left: 0,
                right: TEST_COLS - 1,
            })
            .collect::<Vec<_>>()
            .into(),
    );

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
        damage,
        cells: Arc::<[TerminalCell]>::from(cells),
    }
}
