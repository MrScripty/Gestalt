use gestalt::terminal_native::{
    AlacrittyEmulator, AlacrittyEmulatorConfig, TerminalCellPublication, TerminalCellSpanBatch,
    TerminalCellSpanUpdate, TerminalColor, TerminalCursorShape, TerminalDamage, TerminalFrame,
};

fn emulator(rows: u16, cols: u16) -> AlacrittyEmulator {
    AlacrittyEmulator::new(AlacrittyEmulatorConfig {
        rows,
        cols,
        scrollback: 128,
    })
}

#[test]
fn projects_text_cursor_and_partial_damage() {
    let mut emulator = emulator(4, 8);

    let initial = emulator.snapshot();
    assert_eq!(initial.damage, TerminalDamage::Full);

    emulator.ingest(b"hi");
    let frame = emulator.snapshot();

    assert_eq!(frame.cursor.row, 0);
    assert_eq!(frame.cursor.col, 2);
    let changes = changed_cells(&frame);
    assert_eq!(change_at(&changes, 0, 0).unwrap().codepoint, 'h');
    assert_eq!(change_at(&changes, 0, 1).unwrap().codepoint, 'i');
    match &frame.damage {
        TerminalDamage::Full => panic!("expected partial damage after steady-state update"),
        TerminalDamage::Partial(lines) => assert!(lines.iter().any(|line| line.row == 0)),
    }
}

#[test]
fn projects_modes_and_colors() {
    let mut emulator = emulator(4, 8);

    let _ = emulator.snapshot();
    emulator.ingest(b"\x1b[31mR\x1b[0m\x1b[?2004h\x1b[?25l");
    let frame = emulator.snapshot();

    assert!(frame.bracketed_paste);
    assert_eq!(frame.cursor.shape, TerminalCursorShape::Hidden);
    assert_eq!(
        change_at(&changed_cells(&frame), 0, 0).unwrap().fg,
        TerminalColor::Palette(1)
    );
}

#[test]
fn resize_marks_terminal_fully_damaged() {
    let mut emulator = emulator(2, 4);

    let _ = emulator.snapshot();
    emulator.ingest(b"abcd");
    let _ = emulator.snapshot();

    assert!(emulator.resize(3, 6));
    let frame = emulator.snapshot();

    assert_eq!(frame.rows, 3);
    assert_eq!(frame.cols, 6);
    assert_eq!(frame.damage, TerminalDamage::Full);
    assert!(matches!(
        frame.publication,
        TerminalCellPublication::Full(_)
    ));
    assert_eq!(frame.cell(0, 0).unwrap().codepoint, 'a');
}

#[test]
fn partial_updates_preserve_undamaged_cells() {
    let mut emulator = emulator(4, 8);

    let _ = emulator.snapshot();
    emulator.ingest(b"hi");
    let _ = emulator.snapshot();

    emulator.ingest(b"!");
    let frame = emulator.snapshot();

    let changes = changed_cells(&frame);
    assert!(change_at(&changes, 0, 0).is_none());
    assert!(change_at(&changes, 0, 1).is_none());
    assert_eq!(change_at(&changes, 0, 2).unwrap().codepoint, '!');
    assert!(matches!(frame.damage, TerminalDamage::Partial(_)));
}

#[test]
fn scrolling_display_changes_visible_offset() {
    let mut emulator = emulator(3, 4);

    let _ = emulator.snapshot();
    emulator.ingest(b"1\r\n2\r\n3\r\n4\r\n5\r\n");
    let _ = emulator.snapshot();

    assert!(emulator.scroll_display_delta(2));
    let frame = emulator.snapshot();

    assert_eq!(frame.display_offset, 2);
    assert!(matches!(frame.damage, TerminalDamage::Full));
    assert_eq!(frame.cell(0, 0).unwrap().codepoint, '2');
}

fn changed_cells(frame: &TerminalFrame) -> Vec<(u16, u16, gestalt::terminal_native::TerminalCell)> {
    let Some(changes) = frame.changed_spans() else {
        return Vec::new();
    };

    changes
        .spans()
        .iter()
        .flat_map(|change| span_cells(changes, change))
        .collect()
}

fn span_cells<'a>(
    changes: &'a TerminalCellSpanBatch,
    change: &'a TerminalCellSpanUpdate,
) -> impl Iterator<Item = (u16, u16, gestalt::terminal_native::TerminalCell)> + 'a {
    changes
        .cells_for_span(change)
        .iter()
        .enumerate()
        .map(|(offset, cell)| (change.row, change.left + offset as u16, cell.clone()))
}

fn change_at(
    changes: &[(u16, u16, gestalt::terminal_native::TerminalCell)],
    row: u16,
    col: u16,
) -> Option<&gestalt::terminal_native::TerminalCell> {
    changes
        .iter()
        .find(|(candidate_row, candidate_col, _)| *candidate_row == row && *candidate_col == col)
        .map(|(_, _, cell)| cell)
}
