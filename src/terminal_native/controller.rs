use std::env;
use std::sync::Arc;

use super::constants::{DEFAULT_SCROLLBACK, DEFAULT_SESSION_COLS, DEFAULT_SESSION_ROWS};
use super::{NativeTerminalSession, NativeTerminalSessionConfig, TerminalFrame};

#[derive(Clone)]
pub struct NativeTerminalController {
    session: Arc<NativeTerminalSession>,
}

impl NativeTerminalController {
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

    pub fn is_closed(&self) -> bool {
        self.session.is_closed()
    }

    pub fn send_input(&self, bytes: &[u8]) {
        let _ = self.session.send_input(bytes);
    }

    pub fn resize_cells(&self, rows: u16, cols: u16) {
        let _ = self.session.resize(rows, cols);
    }
}
