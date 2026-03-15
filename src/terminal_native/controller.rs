use std::env;
use std::sync::Arc;

use super::constants::{
    CELL_HEIGHT_PX, CELL_WIDTH_PX, DEFAULT_SCROLLBACK, DEFAULT_SESSION_COLS, DEFAULT_SESSION_ROWS,
    MIN_TERMINAL_COLS, MIN_TERMINAL_ROWS,
};
use super::{
    NativeTerminalSession, NativeTerminalSessionConfig, NativeTerminalSessionSummary, TerminalFrame,
};

#[derive(Clone)]
pub struct NativeTerminalController {
    session: Arc<NativeTerminalSession>,
}

impl PartialEq for NativeTerminalController {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.session, &other.session)
    }
}

impl Eq for NativeTerminalController {}

impl NativeTerminalController {
    pub fn surface_cells(width: u32, height: u32) -> (u16, u16) {
        let rows = ((height / CELL_HEIGHT_PX).max(u32::from(MIN_TERMINAL_ROWS))) as u16;
        let cols = ((width / CELL_WIDTH_PX).max(u32::from(MIN_TERMINAL_COLS))) as u16;
        (rows, cols)
    }

    pub fn spawn_for_current_dir() -> Self {
        let cwd = env::current_dir()
            .ok()
            .and_then(|path| path.to_str().map(str::to_owned))
            .unwrap_or_else(|| ".".to_string());

        let session = NativeTerminalSession::spawn(NativeTerminalSessionConfig {
            cwd,
            rows: DEFAULT_SESSION_ROWS,
            cols: DEFAULT_SESSION_COLS,
            scrollback: DEFAULT_SCROLLBACK,
        })
        .expect("native terminal spike session should spawn");

        Self {
            session: Arc::new(session),
        }
    }

    pub fn frame(&self) -> Arc<TerminalFrame> {
        self.session.current_frame()
    }

    pub fn revision(&self) -> u64 {
        self.session.revision()
    }

    pub fn summary(&self) -> NativeTerminalSessionSummary {
        self.session.summary()
    }

    pub fn send_input(&self, bytes: &[u8]) {
        let _ = self.session.send_input(bytes);
    }

    pub fn resize_cells(&self, rows: u16, cols: u16) -> bool {
        self.session.resize(rows, cols).unwrap_or(false)
    }
}
