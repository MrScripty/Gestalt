use gestalt::terminal_native::{
    AlacrittyEmulator, AlacrittyEmulatorConfig, TerminalColor, TerminalCursorShape, TerminalDamage,
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
    assert_eq!(frame.cell(0, 0).unwrap().codepoint, 'h');
    assert_eq!(frame.cell(0, 1).unwrap().codepoint, 'i');
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
    assert_eq!(frame.cell(0, 0).unwrap().fg, TerminalColor::Palette(1));
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

    assert_eq!(frame.cell(0, 0).unwrap().codepoint, 'h');
    assert_eq!(frame.cell(0, 1).unwrap().codepoint, 'i');
    assert_eq!(frame.cell(0, 2).unwrap().codepoint, '!');
    assert!(matches!(frame.damage, TerminalDamage::Partial(_)));
}
