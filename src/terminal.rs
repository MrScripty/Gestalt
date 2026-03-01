use crate::state::SessionId;
use parking_lot::{Mutex, RwLock};
use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use vt100::Parser;

const DEFAULT_ROWS: u16 = 42;
const DEFAULT_COLS: u16 = 140;
const DEFAULT_SCROLLBACK: usize = 12_000;
const MIN_ROWS: u16 = 2;
const MIN_COLS: u16 = 8;
const MAX_PERSISTED_HISTORY_LINES: usize = 20_000;

/// Render-ready terminal frame data extracted from VT state.
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

/// Serializable terminal state used for workspace persistence.
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

/// Timings captured during a terminal input send.
#[derive(Debug, Clone, Copy)]
pub struct SendInputProfile {
    pub lock_wait: Duration,
    pub total: Duration,
}

/// Manages PTY-backed terminal runtimes indexed by session ID.
pub struct TerminalManager {
    shell: String,
    sessions: RwLock<HashMap<SessionId, Arc<TerminalRuntime>>>,
    pending_restore: Mutex<HashMap<SessionId, PersistedTerminalState>>,
}

struct TerminalRuntime {
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn portable_pty::Child + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    parser: Arc<Mutex<Parser>>,
    scrollback: Arc<RwLock<ScrollbackBuffer>>,
    cwd: RwLock<String>,
    snapshot_cache: Arc<RwLock<Arc<TerminalSnapshot>>>,
    snapshot_revision: Arc<AtomicU64>,
}

#[derive(Debug, Clone)]
struct ScrollbackBuffer {
    lines: Vec<String>,
    pending: Vec<u8>,
    escape_state: EscapeParseState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum EscapeParseState {
    #[default]
    Normal,
    Esc,
    Csi,
    Osc,
    OscEsc,
}

impl TerminalManager {
    /// Creates a manager configured for the detected user shell.
    pub fn new() -> Self {
        Self {
            shell: detect_shell(),
            sessions: RwLock::new(HashMap::new()),
            pending_restore: Mutex::new(HashMap::new()),
        }
    }

    /// Registers restored terminal history for deferred session startup.
    pub fn seed_restored_terminal(&self, state: PersistedTerminalState) {
        self.pending_restore.lock().insert(state.session_id, state);
    }

    /// Ensures a runtime exists for the requested session.
    pub fn ensure_session(&self, session_id: SessionId, cwd: &str) -> Result<(), String> {
        if self.sessions.read().contains_key(&session_id) {
            return Ok(());
        }

        let restored = self.pending_restore.lock().remove(&session_id);
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

        let restored_lines = restored
            .as_ref()
            .map(|state| normalized_history_lines(&state.lines))
            .unwrap_or_default();

        let mut parser = Parser::new(rows, cols, DEFAULT_SCROLLBACK);
        for line in &restored_lines {
            parser.process(line.as_bytes());
            parser.process(b"\r\n");
        }

        let parser = Arc::new(Mutex::new(parser));
        let scrollback = Arc::new(RwLock::new(ScrollbackBuffer::from_restored(restored_lines)));
        let initial_snapshot = {
            let parser = parser.lock();
            let scrollback_lines = scrollback.read().lines.clone();
            terminal_snapshot_from_parser(&parser, &scrollback_lines)
        };
        let snapshot_cache = Arc::new(RwLock::new(Arc::new(initial_snapshot)));
        let snapshot_revision = Arc::new(AtomicU64::new(1));
        spawn_reader_thread(
            reader,
            Arc::clone(&parser),
            Arc::clone(&scrollback),
            Arc::clone(&snapshot_cache),
            Arc::clone(&snapshot_revision),
        );

        let runtime = Arc::new(TerminalRuntime {
            master: Mutex::new(master),
            child: Mutex::new(child),
            writer: Mutex::new(writer),
            parser,
            scrollback,
            cwd: RwLock::new(session_cwd),
            snapshot_cache,
            snapshot_revision,
        });

        let mut sessions = self.sessions.write();
        if sessions.contains_key(&session_id) {
            let mut child = runtime.child.lock();
            let _ = child.kill();
            let _ = child.wait();
            return Ok(());
        }
        sessions.insert(session_id, runtime);

        Ok(())
    }

    /// Sends raw bytes to a session PTY.
    pub fn send_input(&self, session_id: SessionId, input: &[u8]) -> Result<(), String> {
        self.send_input_profiled(session_id, input).map(|_| ())
    }

    /// Sends raw bytes to a session PTY and records lock/total timings.
    pub fn send_input_profiled(
        &self,
        session_id: SessionId,
        input: &[u8],
    ) -> Result<SendInputProfile, String> {
        let started = Instant::now();
        let runtime = self
            .session_runtime(session_id)
            .ok_or_else(|| "session does not exist".to_string())?;
        let lock_started = Instant::now();
        let mut writer = runtime.writer.lock();
        let lock_wait = lock_started.elapsed();

        writer
            .write_all(input)
            .map_err(|error| format!("Failed writing input: {error}"))?;
        writer
            .flush()
            .map_err(|error| format!("Failed flushing input: {error}"))?;

        Ok(SendInputProfile {
            lock_wait,
            total: started.elapsed(),
        })
    }

    /// Sends a line terminated with carriage return.
    pub fn send_line(&self, session_id: SessionId, line: &str) -> Result<(), String> {
        let mut bytes = line.as_bytes().to_vec();
        bytes.push(b'\r');
        self.send_input(session_id, &bytes)
    }

    /// Updates tracked working directory metadata for a session.
    pub fn set_cwd(&self, session_id: SessionId, cwd: &str) -> Result<(), String> {
        self.send_line(session_id, &format!("cd {}", shell_quote(cwd)))?;

        if let Some(runtime) = self.session_runtime(session_id) {
            *runtime.cwd.write() = cwd.to_string();
        }

        Ok(())
    }

    /// Returns the latest cached terminal snapshot for a session.
    pub fn snapshot(&self, session_id: SessionId) -> Option<TerminalSnapshot> {
        self.snapshot_shared(session_id)
            .map(|snapshot| snapshot.as_ref().clone())
    }

    /// Returns a shared reference-counted terminal snapshot for a session.
    pub fn snapshot_shared(&self, session_id: SessionId) -> Option<Arc<TerminalSnapshot>> {
        let runtime = self.session_runtime(session_id)?;
        Some(runtime.snapshot_cache.read().clone())
    }

    /// Returns a persistence-friendly snapshot for the given session.
    pub fn snapshot_for_persist(&self, session_id: SessionId) -> Option<PersistedTerminalState> {
        self.snapshot_for_persist_limited(session_id, MAX_PERSISTED_HISTORY_LINES)
    }

    /// Returns a persistence snapshot with a caller-provided history cap.
    pub fn snapshot_for_persist_limited(
        &self,
        session_id: SessionId,
        max_history_lines: usize,
    ) -> Option<PersistedTerminalState> {
        if let Some(runtime) = self.session_runtime(session_id) {
            let snapshot = runtime.snapshot_cache.read().clone();
            let lines = normalized_history_lines_limited(&snapshot.lines, max_history_lines);

            return Some(PersistedTerminalState {
                session_id,
                cwd: runtime.cwd.read().clone(),
                rows: snapshot.rows,
                cols: snapshot.cols,
                cursor_row: snapshot.cursor_row,
                cursor_col: snapshot.cursor_col,
                hide_cursor: snapshot.hide_cursor,
                bracketed_paste: snapshot.bracketed_paste,
                lines,
            });
        }

        self.pending_restore
            .lock()
            .get(&session_id)
            .cloned()
            .map(|mut persisted| {
                persisted.lines =
                    normalized_history_lines_limited(&persisted.lines, max_history_lines);
                persisted
            })
    }

    /// Resizes a running PTY and updates parser dimensions.
    pub fn resize_session(
        &self,
        session_id: SessionId,
        rows: u16,
        cols: u16,
    ) -> Result<(), String> {
        let runtime = self
            .session_runtime(session_id)
            .ok_or_else(|| "session does not exist".to_string())?;

        let rows = rows.max(MIN_ROWS);
        let cols = cols.max(MIN_COLS);
        runtime
            .master
            .lock()
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("Failed to resize PTY: {error}"))?;

        let mut parser = runtime.parser.lock();
        parser.set_size(rows, cols);
        let scrollback_lines = runtime.scrollback.read().lines.clone();
        let snapshot = terminal_snapshot_from_parser(&parser, &scrollback_lines);
        *runtime.snapshot_cache.write() = Arc::new(snapshot);
        runtime.snapshot_revision.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Returns the tracked session working directory.
    pub fn session_cwd(&self, session_id: SessionId) -> Option<String> {
        self.session_runtime(session_id)
            .map(|runtime| runtime.cwd.read().clone())
    }

    /// Terminates and unregisters a session runtime if it exists.
    pub fn terminate_session(&self, session_id: SessionId) -> bool {
        self.pending_restore.lock().remove(&session_id);

        let runtime = self.sessions.write().remove(&session_id);
        let Some(runtime) = runtime else {
            return false;
        };

        let mut child = runtime.child.lock();
        let _ = child.kill();
        let _ = child.wait();
        true
    }

    /// Returns the monotonic snapshot revision for change detection.
    pub fn session_snapshot_revision(&self, session_id: SessionId) -> Option<u64> {
        self.session_runtime(session_id)
            .map(|runtime| runtime.snapshot_revision.load(Ordering::Relaxed))
    }

    fn session_runtime(&self, session_id: SessionId) -> Option<Arc<TerminalRuntime>> {
        self.sessions.read().get(&session_id).cloned()
    }
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TerminalManager {
    fn drop(&mut self) {
        let runtimes = self
            .sessions
            .get_mut()
            .drain()
            .map(|(_, runtime)| runtime)
            .collect::<Vec<_>>();
        for runtime in runtimes {
            let mut child = runtime.child.lock();
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn spawn_reader_thread(
    mut reader: Box<dyn Read + Send>,
    parser: Arc<Mutex<Parser>>,
    scrollback: Arc<RwLock<ScrollbackBuffer>>,
    snapshot_cache: Arc<RwLock<Arc<TerminalSnapshot>>>,
    snapshot_revision: Arc<AtomicU64>,
) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let snapshot = {
                        let mut parser = parser.lock();
                        parser.process(&buffer[..read]);

                        let scrollback_lines = {
                            let mut scrollback = scrollback.write();
                            scrollback.process_bytes(&buffer[..read]);
                            scrollback.lines.clone()
                        };

                        terminal_snapshot_from_parser(&parser, &scrollback_lines)
                    };
                    *snapshot_cache.write() = Arc::new(snapshot);
                    snapshot_revision.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => break,
            }
        }
    });
}

fn terminal_snapshot_from_parser(parser: &Parser, scrollback_lines: &[String]) -> TerminalSnapshot {
    let screen = parser.screen();
    let (rows, cols) = screen.size();
    let (cursor_row_rel, cursor_col) = screen.cursor_position();
    let visible_lines = screen.rows(0, cols).collect::<Vec<_>>();
    let lines = merge_scrollback_with_visible(scrollback_lines, &visible_lines);
    let visible_start = lines.len().saturating_sub(visible_lines.len());
    let cursor_row = visible_start
        .saturating_add(usize::from(cursor_row_rel))
        .min(lines.len().saturating_sub(1));

    TerminalSnapshot {
        lines,
        rows,
        cols,
        cursor_row: u16::try_from(cursor_row).unwrap_or(u16::MAX),
        cursor_col,
        hide_cursor: screen.hide_cursor(),
        bracketed_paste: screen.bracketed_paste(),
    }
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
    normalized_history_lines_limited(lines, MAX_PERSISTED_HISTORY_LINES)
}

fn normalized_history_lines_limited(lines: &[String], max_history_lines: usize) -> Vec<String> {
    let start = lines.len().saturating_sub(max_history_lines);
    lines
        .iter()
        .skip(start)
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect()
}

fn merge_scrollback_with_visible(scrollback: &[String], visible: &[String]) -> Vec<String> {
    let max_overlap = scrollback.len().min(visible.len());
    let overlap = (0..=max_overlap)
        .rev()
        .find(|overlap_len| {
            scrollback[scrollback.len().saturating_sub(*overlap_len)..] == visible[..*overlap_len]
        })
        .unwrap_or(0);

    let keep = scrollback.len().saturating_sub(overlap);
    let mut lines = Vec::with_capacity(keep + visible.len());
    lines.extend(scrollback.iter().take(keep).cloned());
    lines.extend(visible.iter().cloned());
    lines
}

impl ScrollbackBuffer {
    fn from_restored(restored: Vec<String>) -> Self {
        Self {
            lines: normalized_history_lines(&restored),
            pending: Vec::new(),
            escape_state: EscapeParseState::Normal,
        }
    }

    fn process_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            match self.escape_state {
                EscapeParseState::Normal => match byte {
                    0x1b => self.escape_state = EscapeParseState::Esc,
                    b'\n' => self.finish_line(),
                    b'\r' => {}
                    0x08 => {
                        let _ = self.pending.pop();
                    }
                    byte if byte >= 0x20 || byte == b'\t' => self.pending.push(byte),
                    _ => {}
                },
                EscapeParseState::Esc => match byte {
                    b'[' => self.escape_state = EscapeParseState::Csi,
                    b']' => self.escape_state = EscapeParseState::Osc,
                    _ => self.escape_state = EscapeParseState::Normal,
                },
                EscapeParseState::Csi => {
                    if (0x40..=0x7e).contains(&byte) {
                        self.escape_state = EscapeParseState::Normal;
                    }
                }
                EscapeParseState::Osc => match byte {
                    0x07 => self.escape_state = EscapeParseState::Normal,
                    0x1b => self.escape_state = EscapeParseState::OscEsc,
                    _ => {}
                },
                EscapeParseState::OscEsc => {
                    self.escape_state = EscapeParseState::Normal;
                }
            }
        }
    }

    fn finish_line(&mut self) {
        let line = String::from_utf8_lossy(&self.pending)
            .trim_end_matches('\r')
            .to_string();
        self.pending.clear();
        self.push_line(line);
    }

    fn push_line(&mut self, line: String) {
        self.lines.push(line);
        let overflow = self.lines.len().saturating_sub(MAX_PERSISTED_HISTORY_LINES);
        if overflow > 0 {
            self.lines.drain(0..overflow);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_scrollback_deduplicates_visible_suffix() {
        let scrollback = vec![
            "line 1".to_string(),
            "line 2".to_string(),
            "line 3".to_string(),
        ];
        let visible = vec!["line 3".to_string(), "line 4".to_string()];

        let merged = merge_scrollback_with_visible(&scrollback, &visible);
        assert_eq!(
            merged,
            vec![
                "line 1".to_string(),
                "line 2".to_string(),
                "line 3".to_string(),
                "line 4".to_string(),
            ]
        );
    }

    #[test]
    fn scrollback_buffer_strips_escape_sequences_and_tracks_lines() {
        let mut scrollback = ScrollbackBuffer::from_restored(Vec::new());
        scrollback.process_bytes(b"\x1b[31mred\x1b[0m\nplain\n");

        assert_eq!(
            scrollback.lines,
            vec!["red".to_string(), "plain".to_string()]
        );
    }
}
