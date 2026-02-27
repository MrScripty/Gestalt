use crate::state::SessionId;
use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use vt100::Parser;

const DEFAULT_ROWS: u16 = 42;
const DEFAULT_COLS: u16 = 140;
const DEFAULT_SCROLLBACK: usize = 12_000;
const MIN_ROWS: u16 = 2;
const MIN_COLS: u16 = 8;
const MAX_PERSISTED_HISTORY_LINES: usize = 20_000;

#[derive(Debug, Clone)]
pub struct TerminalSnapshot {
    pub lines: Vec<String>,
    pub rows: u16,
    pub cols: u16,
    pub cursor_row: u16,
    pub cursor_col: u16,
    pub hide_cursor: bool,
    pub bracketed_paste: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedTerminalState {
    pub session_id: SessionId,
    pub cwd: String,
    pub rows: u16,
    pub cols: u16,
    pub cursor_row: u16,
    pub cursor_col: u16,
    pub hide_cursor: bool,
    pub bracketed_paste: bool,
    pub lines: Vec<String>,
}

pub struct TerminalManager {
    shell: String,
    sessions: HashMap<SessionId, TerminalRuntime>,
    pending_restore: HashMap<SessionId, PersistedTerminalState>,
}

struct TerminalRuntime {
    _master: Box<dyn MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    parser: Arc<Mutex<Parser>>,
    cwd: String,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            shell: detect_shell(),
            sessions: HashMap::new(),
            pending_restore: HashMap::new(),
        }
    }

    pub fn seed_restored_terminal(&mut self, state: PersistedTerminalState) {
        self.pending_restore.insert(state.session_id, state);
    }

    pub fn ensure_session(&mut self, session_id: SessionId, cwd: &str) -> Result<(), String> {
        if self.sessions.contains_key(&session_id) {
            return Ok(());
        }

        let restored = self.pending_restore.remove(&session_id);
        let session_cwd = restored
            .as_ref()
            .map(|state| state.cwd.clone())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| cwd.to_string());
        let rows = restored
            .as_ref()
            .map_or(DEFAULT_ROWS, |state| state.rows.max(MIN_ROWS));
        let cols = restored
            .as_ref()
            .map_or(DEFAULT_COLS, |state| state.cols.max(MIN_COLS));

        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("Failed to create PTY: {error}"))?;

        let mut command = CommandBuilder::new(&self.shell);
        command.cwd(&session_cwd);
        command.env("TERM", "xterm-256color");

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| format!("Failed to start shell in {cwd}: {error}"))?;

        let master = pair.master;
        let writer = master
            .take_writer()
            .map_err(|error| format!("Failed to open PTY writer: {error}"))?;
        let reader = master
            .try_clone_reader()
            .map_err(|error| format!("Failed to open PTY reader: {error}"))?;

        let mut parser = Parser::new(rows, cols, DEFAULT_SCROLLBACK);
        if let Some(restored) = restored {
            for line in normalized_history_lines(&restored.lines) {
                parser.process(line.as_bytes());
                parser.process(b"\r\n");
            }
        }

        let parser = Arc::new(Mutex::new(parser));
        spawn_reader_thread(reader, Arc::clone(&parser));

        self.sessions.insert(
            session_id,
            TerminalRuntime {
                _master: master,
                child,
                writer: Arc::new(Mutex::new(writer)),
                parser,
                cwd: session_cwd,
            },
        );

        Ok(())
    }

    pub fn send_input(&mut self, session_id: SessionId, input: &[u8]) -> Result<(), String> {
        let Some(runtime) = self.sessions.get(&session_id) else {
            return Err("session does not exist".to_string());
        };

        let mut writer = runtime
            .writer
            .lock()
            .map_err(|_| "terminal writer lock poisoned".to_string())?;

        writer
            .write_all(input)
            .map_err(|error| format!("Failed writing input: {error}"))?;
        writer
            .flush()
            .map_err(|error| format!("Failed flushing input: {error}"))?;

        Ok(())
    }

    pub fn send_line(&mut self, session_id: SessionId, line: &str) -> Result<(), String> {
        let mut bytes = line.as_bytes().to_vec();
        bytes.push(b'\r');
        self.send_input(session_id, &bytes)
    }

    pub fn set_cwd(&mut self, session_id: SessionId, cwd: &str) -> Result<(), String> {
        self.send_line(session_id, &format!("cd {}", shell_quote(cwd)))?;

        if let Some(runtime) = self.sessions.get_mut(&session_id) {
            runtime.cwd = cwd.to_string();
        }

        Ok(())
    }

    pub fn snapshot(&self, session_id: SessionId) -> Option<TerminalSnapshot> {
        let runtime = self.sessions.get(&session_id)?;
        let parser = runtime.parser.lock().ok()?;
        let screen = parser.screen();
        let (rows, cols) = screen.size();
        let (cursor_row, cursor_col) = screen.cursor_position();
        let lines = screen.rows(0, cols).collect();

        Some(TerminalSnapshot {
            lines,
            rows,
            cols,
            cursor_row,
            cursor_col,
            hide_cursor: screen.hide_cursor(),
            bracketed_paste: screen.bracketed_paste(),
        })
    }

    pub fn snapshot_for_persist(&self, session_id: SessionId) -> Option<PersistedTerminalState> {
        if let Some(runtime) = self.sessions.get(&session_id) {
            let parser = runtime.parser.lock().ok()?;
            let screen = parser.screen();
            let (rows, cols) = screen.size();
            let (cursor_row, cursor_col) = screen.cursor_position();
            let lines = normalized_history_lines(
                &screen
                    .contents()
                    .lines()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            );

            return Some(PersistedTerminalState {
                session_id,
                cwd: runtime.cwd.clone(),
                rows,
                cols,
                cursor_row,
                cursor_col,
                hide_cursor: screen.hide_cursor(),
                bracketed_paste: screen.bracketed_paste(),
                lines,
            });
        }

        self.pending_restore.get(&session_id).cloned()
    }

    pub fn resize_session(
        &mut self,
        session_id: SessionId,
        rows: u16,
        cols: u16,
    ) -> Result<(), String> {
        let Some(runtime) = self.sessions.get_mut(&session_id) else {
            return Err("session does not exist".to_string());
        };

        let rows = rows.max(MIN_ROWS);
        let cols = cols.max(MIN_COLS);
        runtime
            ._master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("Failed to resize PTY: {error}"))?;

        let mut parser = runtime
            .parser
            .lock()
            .map_err(|_| "terminal parser lock poisoned".to_string())?;
        parser.set_size(rows, cols);

        Ok(())
    }

    pub fn session_cwd(&self, session_id: SessionId) -> Option<&str> {
        self.sessions
            .get(&session_id)
            .map(|runtime| runtime.cwd.as_str())
    }
}

impl Drop for TerminalManager {
    fn drop(&mut self) {
        for runtime in self.sessions.values_mut() {
            let _ = runtime.child.kill();
            let _ = runtime.child.wait();
        }
    }
}

fn spawn_reader_thread(mut reader: Box<dyn Read + Send>, parser: Arc<Mutex<Parser>>) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    if let Ok(mut parser) = parser.lock() {
                        parser.process(&buffer[..read]);
                    }
                }
                Err(_) => break,
            }
        }
    });
}

fn detect_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "/bin/bash".to_string())
}

fn shell_quote(input: &str) -> String {
    let escaped = input.replace('\'', "'\\''");
    format!("'{}'", escaped)
}

fn normalized_history_lines(lines: &[String]) -> Vec<String> {
    let start = lines.len().saturating_sub(MAX_PERSISTED_HISTORY_LINES);
    lines
        .iter()
        .skip(start)
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect()
}
