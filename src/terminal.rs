use crate::state::SessionId;
#[cfg(feature = "terminal-native-spike")]
use crate::terminal_native::{
    NativeTerminalSession, NativeTerminalSessionConfig, TerminalCell, TerminalCellFlags,
    TerminalCellPublication, TerminalCursorShape, TerminalDamage, TerminalFrame,
};
use parking_lot::{Mutex, RwLock};
use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::sync::broadcast;
use vt100::Parser;

const DEFAULT_ROWS: u16 = 42;
const DEFAULT_COLS: u16 = 140;
const DEFAULT_SCROLLBACK: usize = 12_000;
const MIN_ROWS: u16 = 2;
const MIN_COLS: u16 = 8;
const MAX_PERSISTED_HISTORY_LINES: usize = 20_000;
#[cfg(feature = "terminal-native-spike")]
const NATIVE_TERMINAL_BACKEND_ENV_VAR: &str = "GESTALT_NATIVE_TERMINAL_BACKEND";

/// Render-ready terminal frame data extracted from VT state.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    // Terminal history persistence is owned by Emily, not workspace JSON.
    #[serde(default, skip_serializing, skip_deserializing)]
    pub lines: Vec<String>,
}

/// Timings captured during a terminal input send.
#[derive(Debug, Clone, Copy)]
pub struct SendInputProfile {
    pub lock_wait: Duration,
    pub total: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalEventKind {
    Activity,
    SnapshotChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalEvent {
    pub session_id: SessionId,
    pub kind: TerminalEventKind,
}

#[derive(Debug, Clone, Error)]
pub enum TerminalError {
    #[error("session does not exist")]
    SessionMissing,
    #[error("failed to create PTY: {0}")]
    CreatePty(String),
    #[error("failed to start shell in {cwd}: {details}")]
    StartShell { cwd: String, details: String },
    #[error("failed to open PTY writer: {0}")]
    OpenWriter(String),
    #[error("failed to open PTY reader: {0}")]
    OpenReader(String),
    #[error("failed writing input: {0}")]
    WriteInput(String),
    #[error("failed flushing input: {0}")]
    FlushInput(String),
    #[error("failed to resize PTY: {0}")]
    ResizePty(String),
}

/// Manages PTY-backed terminal runtimes indexed by session ID.
pub struct TerminalManager {
    shell: String,
    memory_sink: Option<Arc<dyn TerminalMemorySink>>,
    events: broadcast::Sender<TerminalEvent>,
    sessions: RwLock<HashMap<SessionId, Arc<TerminalRuntime>>>,
    pending_restore: Mutex<HashMap<SessionId, PersistedTerminalState>>,
}

/// Non-blocking sink for terminal text history events.
pub trait TerminalMemorySink: Send + Sync {
    fn record_input_line(&self, session_id: SessionId, cwd: &str, line: String, ts_unix_ms: i64);
    fn record_output_line(&self, session_id: SessionId, cwd: &str, line: String, ts_unix_ms: i64);
}

struct TerminalRuntime {
    backend: TerminalRuntimeBackend,
    cwd: Arc<RwLock<String>>,
    input_pending: Mutex<Vec<u8>>,
    memory_sink: Option<Arc<dyn TerminalMemorySink>>,
    snapshot_cache: Arc<RwLock<Arc<TerminalSnapshot>>>,
    snapshot_revision: Arc<AtomicU64>,
    last_activity_unix_ms: Arc<AtomicI64>,
}

enum TerminalRuntimeBackend {
    Legacy(LegacyTerminalRuntime),
    #[cfg(feature = "terminal-native-spike")]
    Native(NativeTerminalRuntime),
}

struct LegacyTerminalRuntime {
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn portable_pty::Child + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    parser: Arc<Mutex<Parser>>,
    scrollback: Arc<RwLock<ScrollbackBuffer>>,
}

#[cfg(feature = "terminal-native-spike")]
struct NativeTerminalRuntime {
    session: NativeTerminalSession,
    frame_cache: Arc<RwLock<Arc<TerminalFrame>>>,
}

struct ReaderThreadContext {
    reader: Box<dyn Read + Send>,
    parser: Arc<Mutex<Parser>>,
    scrollback: Arc<RwLock<ScrollbackBuffer>>,
    snapshot_cache: Arc<RwLock<Arc<TerminalSnapshot>>>,
    snapshot_revision: Arc<AtomicU64>,
    last_activity_unix_ms: Arc<AtomicI64>,
    cwd: Arc<RwLock<String>>,
    memory_sink: Option<Arc<dyn TerminalMemorySink>>,
    event_tx: broadcast::Sender<TerminalEvent>,
    session_id: SessionId,
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

struct TerminalStartupState {
    restored: Option<PersistedTerminalState>,
    cwd: String,
    rows: u16,
    cols: u16,
}

impl TerminalManager {
    /// Creates a manager configured for the detected user shell.
    pub fn new() -> Self {
        Self::new_with_memory_sink(None)
    }

    /// Creates a manager configured for the detected user shell and optional memory sink.
    pub fn new_with_memory_sink(memory_sink: Option<Arc<dyn TerminalMemorySink>>) -> Self {
        let (events, _) = broadcast::channel(4_096);
        Self {
            shell: detect_shell(),
            memory_sink,
            events,
            sessions: RwLock::new(HashMap::new()),
            pending_restore: Mutex::new(HashMap::new()),
        }
    }

    /// Subscribes to terminal runtime activity and snapshot updates.
    pub fn subscribe_events(&self) -> broadcast::Receiver<TerminalEvent> {
        self.events.subscribe()
    }

    /// Registers restored terminal state for deferred session startup.
    pub fn seed_restored_terminal(&self, state: PersistedTerminalState) {
        self.pending_restore.lock().insert(state.session_id, state);
    }

    /// Ensures a runtime exists for the requested session.
    pub fn ensure_session(&self, session_id: SessionId, cwd: &str) -> Result<(), TerminalError> {
        if self.sessions.read().contains_key(&session_id) {
            return Ok(());
        }

        let startup = self.session_startup_state(session_id, cwd);

        #[cfg(feature = "terminal-native-spike")]
        if native_terminal_backend_enabled() {
            return self.ensure_native_session(session_id, startup);
        }

        self.ensure_legacy_session(session_id, startup)
    }

    fn session_startup_state(&self, session_id: SessionId, cwd: &str) -> TerminalStartupState {
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

        TerminalStartupState {
            restored,
            cwd: session_cwd,
            rows,
            cols,
        }
    }

    fn ensure_legacy_session(
        &self,
        session_id: SessionId,
        startup: TerminalStartupState,
    ) -> Result<(), TerminalError> {
        let TerminalStartupState {
            restored,
            cwd,
            rows,
            cols,
        } = startup;
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| TerminalError::CreatePty(error.to_string()))?;

        let mut command = CommandBuilder::new(&self.shell);
        command.cwd(&cwd);
        command.env("TERM", "xterm-256color");

        let child =
            pair.slave
                .spawn_command(command)
                .map_err(|error| TerminalError::StartShell {
                    cwd: cwd.clone(),
                    details: error.to_string(),
                })?;

        let master = pair.master;
        let writer = master
            .take_writer()
            .map_err(|error| TerminalError::OpenWriter(error.to_string()))?;
        let reader = master
            .try_clone_reader()
            .map_err(|error| TerminalError::OpenReader(error.to_string()))?;

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
        let cwd = Arc::new(RwLock::new(cwd));
        let initial_snapshot = {
            let parser = parser.lock();
            let scrollback = scrollback.read();
            terminal_snapshot_from_parser(&parser, &scrollback.lines)
        };
        let snapshot_cache = Arc::new(RwLock::new(Arc::new(initial_snapshot)));
        let snapshot_revision = Arc::new(AtomicU64::new(1));
        let last_activity_unix_ms = Arc::new(AtomicI64::new(0));
        spawn_reader_thread(ReaderThreadContext {
            reader,
            parser: Arc::clone(&parser),
            scrollback: Arc::clone(&scrollback),
            snapshot_cache: Arc::clone(&snapshot_cache),
            snapshot_revision: Arc::clone(&snapshot_revision),
            last_activity_unix_ms: Arc::clone(&last_activity_unix_ms),
            cwd: Arc::clone(&cwd),
            memory_sink: self.memory_sink.clone(),
            event_tx: self.events.clone(),
            session_id,
        });

        let runtime = Arc::new(TerminalRuntime {
            backend: TerminalRuntimeBackend::Legacy(LegacyTerminalRuntime {
                master: Mutex::new(master),
                child: Mutex::new(child),
                writer: Mutex::new(writer),
                parser,
                scrollback,
            }),
            cwd,
            input_pending: Mutex::new(Vec::new()),
            memory_sink: self.memory_sink.clone(),
            snapshot_cache,
            snapshot_revision,
            last_activity_unix_ms,
        });

        let mut sessions = self.sessions.write();
        if sessions.contains_key(&session_id) {
            if let TerminalRuntimeBackend::Legacy(runtime) = &runtime.backend {
                let mut child = runtime.child.lock();
                let _ = child.kill();
                let _ = child.wait();
            }
            return Ok(());
        }
        sessions.insert(session_id, runtime);

        Ok(())
    }

    #[cfg(feature = "terminal-native-spike")]
    fn ensure_native_session(
        &self,
        session_id: SessionId,
        startup: TerminalStartupState,
    ) -> Result<(), TerminalError> {
        let TerminalStartupState {
            restored,
            cwd,
            rows,
            cols,
        } = startup;

        let cwd = Arc::new(RwLock::new(cwd));
        let snapshot_revision = Arc::new(AtomicU64::new(1));
        let last_activity_unix_ms = Arc::new(AtomicI64::new(0));
        let initial_frame = Arc::new(TerminalFrame {
            rows,
            cols,
            cursor: crate::terminal_native::TerminalCursor {
                row: 0,
                col: 0,
                shape: TerminalCursorShape::Hidden,
            },
            bracketed_paste: false,
            display_offset: 0,
            damage: TerminalDamage::Full,
            publication: TerminalCellPublication::Full(Arc::new(vec![
                TerminalCell::default();
                usize::from(rows)
                    * usize::from(cols)
            ])),
        });
        let frame_cache = Arc::new(RwLock::new(initial_frame));
        let restored_lines = restored
            .as_ref()
            .map(|state| state.lines.clone())
            .unwrap_or_default();
        let initial_snapshot =
            compatibility_snapshot_from_native_frame(&frame_cache.read(), &restored_lines);
        let snapshot_cache = Arc::new(RwLock::new(Arc::new(initial_snapshot)));

        let frame_cache_for_callback = Arc::clone(&frame_cache);
        let snapshot_cache_for_callback = Arc::clone(&snapshot_cache);
        let snapshot_revision_for_callback = Arc::clone(&snapshot_revision);
        let last_activity_for_callback = Arc::clone(&last_activity_unix_ms);
        let event_tx = self.events.clone();
        let session = NativeTerminalSession::spawn_with_callback(
            NativeTerminalSessionConfig {
                cwd: cwd.read().clone(),
                rows,
                cols,
                scrollback: DEFAULT_SCROLLBACK,
            },
            Some(Arc::new(move |frame| {
                let full_frame =
                    materialize_native_frame(frame_cache_for_callback.read().as_ref(), frame);
                *frame_cache_for_callback.write() = Arc::clone(&full_frame);
                *snapshot_cache_for_callback.write() =
                    Arc::new(compatibility_snapshot_from_native_frame(&full_frame, &[]));
                snapshot_revision_for_callback.fetch_add(1, Ordering::Relaxed);
                last_activity_for_callback.store(current_unix_ms(), Ordering::Relaxed);
                let _ = event_tx.send(TerminalEvent {
                    session_id,
                    kind: TerminalEventKind::Activity,
                });
                let _ = event_tx.send(TerminalEvent {
                    session_id,
                    kind: TerminalEventKind::SnapshotChanged,
                });
            })),
        )
        .map_err(map_native_terminal_error)?;

        let full_frame =
            materialize_native_frame(frame_cache.read().as_ref(), session.current_frame());
        *frame_cache.write() = Arc::clone(&full_frame);
        *snapshot_cache.write() = Arc::new(compatibility_snapshot_from_native_frame(
            &full_frame,
            &restored_lines,
        ));
        last_activity_unix_ms.store(session.last_activity_unix_ms(), Ordering::Relaxed);

        let runtime = Arc::new(TerminalRuntime {
            backend: TerminalRuntimeBackend::Native(NativeTerminalRuntime {
                session,
                frame_cache,
            }),
            cwd,
            input_pending: Mutex::new(Vec::new()),
            memory_sink: self.memory_sink.clone(),
            snapshot_cache,
            snapshot_revision,
            last_activity_unix_ms,
        });

        let mut sessions = self.sessions.write();
        if sessions.contains_key(&session_id) {
            return Ok(());
        }
        sessions.insert(session_id, runtime);

        Ok(())
    }

    /// Sends raw bytes to a session PTY.
    pub fn send_input(&self, session_id: SessionId, input: &[u8]) -> Result<(), TerminalError> {
        self.send_input_profiled(session_id, input).map(|_| ())
    }

    /// Sends raw bytes to a session PTY and records lock/total timings.
    pub fn send_input_profiled(
        &self,
        session_id: SessionId,
        input: &[u8],
    ) -> Result<SendInputProfile, TerminalError> {
        let started = Instant::now();
        let runtime = self
            .session_runtime(session_id)
            .ok_or(TerminalError::SessionMissing)?;
        let lock_started = Instant::now();
        match &runtime.backend {
            TerminalRuntimeBackend::Legacy(runtime_backend) => {
                let mut writer = runtime_backend.writer.lock();
                let lock_wait = lock_started.elapsed();

                writer
                    .write_all(input)
                    .map_err(|error| TerminalError::WriteInput(error.to_string()))?;
                writer
                    .flush()
                    .map_err(|error| TerminalError::FlushInput(error.to_string()))?;
                runtime
                    .last_activity_unix_ms
                    .store(current_unix_ms(), Ordering::Relaxed);
                self.publish_event(TerminalEvent {
                    session_id,
                    kind: TerminalEventKind::Activity,
                });

                if let Some(memory_sink) = runtime.memory_sink.as_ref() {
                    let mut pending = runtime.input_pending.lock();
                    let lines = parse_input_lines(&mut pending, input);
                    if !lines.is_empty() {
                        let cwd = runtime.cwd.read().clone();
                        let now_ms = current_unix_ms();
                        for line in lines {
                            memory_sink.record_input_line(session_id, &cwd, line, now_ms);
                        }
                    }
                }

                return Ok(SendInputProfile {
                    lock_wait,
                    total: started.elapsed(),
                });
            }
            #[cfg(feature = "terminal-native-spike")]
            TerminalRuntimeBackend::Native(runtime_backend) => {
                runtime_backend
                    .session
                    .send_input(input)
                    .map_err(map_native_terminal_error)?;
            }
        }

        let lock_wait = lock_started.elapsed();
        runtime
            .last_activity_unix_ms
            .store(current_unix_ms(), Ordering::Relaxed);
        self.publish_event(TerminalEvent {
            session_id,
            kind: TerminalEventKind::Activity,
        });

        if let Some(memory_sink) = runtime.memory_sink.as_ref() {
            let mut pending = runtime.input_pending.lock();
            let lines = parse_input_lines(&mut pending, input);
            if !lines.is_empty() {
                let cwd = runtime.cwd.read().clone();
                let now_ms = current_unix_ms();
                for line in lines {
                    memory_sink.record_input_line(session_id, &cwd, line, now_ms);
                }
            }
        }

        Ok(SendInputProfile {
            lock_wait,
            total: started.elapsed(),
        })
    }

    /// Sends a line terminated with carriage return.
    pub fn send_line(&self, session_id: SessionId, line: &str) -> Result<(), TerminalError> {
        let mut bytes = line.as_bytes().to_vec();
        bytes.push(b'\r');
        self.send_input(session_id, &bytes)
    }

    /// Updates tracked working directory metadata for a session.
    pub fn set_cwd(&self, session_id: SessionId, cwd: &str) -> Result<(), TerminalError> {
        self.send_line(session_id, &format!("cd {}", shell_quote(cwd)))?;

        if let Some(runtime) = self.session_runtime(session_id) {
            *runtime.cwd.write() = cwd.to_string();
            runtime.snapshot_revision.fetch_add(1, Ordering::Relaxed);
            self.publish_event(TerminalEvent {
                session_id,
                kind: TerminalEventKind::SnapshotChanged,
            });
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

    #[cfg(feature = "terminal-native-spike")]
    pub fn native_frame_shared(&self, session_id: SessionId) -> Option<Arc<TerminalFrame>> {
        let runtime = self.session_runtime(session_id)?;
        match &runtime.backend {
            TerminalRuntimeBackend::Legacy(_) => None,
            TerminalRuntimeBackend::Native(runtime_backend) => {
                Some(runtime_backend.frame_cache.read().clone())
            }
        }
    }

    /// Returns a persistence-friendly snapshot for the given session.
    /// History lines are intentionally omitted; Emily is the source of truth.
    pub fn snapshot_for_persist(&self, session_id: SessionId) -> Option<PersistedTerminalState> {
        if let Some(runtime) = self.session_runtime(session_id) {
            let snapshot = runtime.snapshot_cache.read().clone();

            return Some(PersistedTerminalState {
                session_id,
                cwd: runtime.cwd.read().clone(),
                rows: snapshot.rows,
                cols: snapshot.cols,
                cursor_row: snapshot.cursor_row,
                cursor_col: snapshot.cursor_col,
                hide_cursor: snapshot.hide_cursor,
                bracketed_paste: snapshot.bracketed_paste,
                lines: Vec::new(),
            });
        }

        self.pending_restore
            .lock()
            .get(&session_id)
            .cloned()
            .map(|mut persisted| {
                persisted.lines.clear();
                persisted
            })
    }

    /// Returns a persistence snapshot with a caller-provided history cap.
    /// History is no longer persisted outside Emily, so this cap is ignored.
    pub fn snapshot_for_persist_limited(
        &self,
        session_id: SessionId,
        _max_history_lines: usize,
    ) -> Option<PersistedTerminalState> {
        self.snapshot_for_persist(session_id)
    }

    /// Prepends older terminal history lines into in-memory scrollback.
    pub fn prepend_history_lines(
        &self,
        session_id: SessionId,
        older_lines: &[String],
    ) -> Result<usize, TerminalError> {
        if older_lines.is_empty() {
            return Ok(0);
        }

        if let Some(runtime) = self.session_runtime(session_id) {
            let inserted = match &runtime.backend {
                TerminalRuntimeBackend::Legacy(runtime_backend) => {
                    let parser = runtime_backend.parser.lock();
                    let inserted = {
                        let mut scrollback = runtime_backend.scrollback.write();
                        prepend_history_lines_limited(
                            &mut scrollback.lines,
                            older_lines,
                            MAX_PERSISTED_HISTORY_LINES,
                        )
                    };
                    if inserted > 0 {
                        let scrollback = runtime_backend.scrollback.read();
                        let snapshot = terminal_snapshot_from_parser(&parser, &scrollback.lines);
                        *runtime.snapshot_cache.write() = Arc::new(snapshot);
                    }
                    inserted
                }
                #[cfg(feature = "terminal-native-spike")]
                TerminalRuntimeBackend::Native(_) => {
                    let mut snapshot = runtime.snapshot_cache.read().as_ref().clone();
                    let inserted = prepend_history_lines_limited(
                        &mut snapshot.lines,
                        older_lines,
                        MAX_PERSISTED_HISTORY_LINES,
                    );
                    if inserted > 0 {
                        *runtime.snapshot_cache.write() = Arc::new(snapshot);
                    }
                    inserted
                }
            };
            if inserted > 0 {
                runtime.snapshot_revision.fetch_add(1, Ordering::Relaxed);
                self.publish_event(TerminalEvent {
                    session_id,
                    kind: TerminalEventKind::SnapshotChanged,
                });
            }
            return Ok(inserted);
        }

        if let Some(persisted) = self.pending_restore.lock().get_mut(&session_id) {
            let inserted = prepend_history_lines_limited(
                &mut persisted.lines,
                older_lines,
                MAX_PERSISTED_HISTORY_LINES,
            );
            return Ok(inserted);
        }

        Err(TerminalError::SessionMissing)
    }

    /// Resizes a running PTY and updates parser dimensions.
    pub fn resize_session(
        &self,
        session_id: SessionId,
        rows: u16,
        cols: u16,
    ) -> Result<(), TerminalError> {
        let runtime = self
            .session_runtime(session_id)
            .ok_or(TerminalError::SessionMissing)?;

        let rows = rows.max(MIN_ROWS);
        let cols = cols.max(MIN_COLS);
        match &runtime.backend {
            TerminalRuntimeBackend::Legacy(runtime_backend) => {
                runtime_backend
                    .master
                    .lock()
                    .resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    })
                    .map_err(|error| TerminalError::ResizePty(error.to_string()))?;

                let mut parser = runtime_backend.parser.lock();
                parser.set_size(rows, cols);
                let scrollback = runtime_backend.scrollback.read();
                let snapshot = terminal_snapshot_from_parser(&parser, &scrollback.lines);
                *runtime.snapshot_cache.write() = Arc::new(snapshot);
            }
            #[cfg(feature = "terminal-native-spike")]
            TerminalRuntimeBackend::Native(runtime_backend) => {
                runtime_backend
                    .session
                    .resize(rows, cols)
                    .map_err(map_native_terminal_error)?;
                let full_frame = materialize_native_frame(
                    runtime_backend.frame_cache.read().as_ref(),
                    runtime_backend.session.current_frame(),
                );
                *runtime_backend.frame_cache.write() = Arc::clone(&full_frame);
                *runtime.snapshot_cache.write() =
                    Arc::new(compatibility_snapshot_from_native_frame(&full_frame, &[]));
            }
        }
        runtime.snapshot_revision.fetch_add(1, Ordering::Relaxed);
        self.publish_event(TerminalEvent {
            session_id,
            kind: TerminalEventKind::SnapshotChanged,
        });

        Ok(())
    }

    /// Returns the tracked session working directory.
    pub fn session_cwd(&self, session_id: SessionId) -> Option<String> {
        self.session_runtime(session_id)
            .map(|runtime| runtime.cwd.read().clone())
    }

    /// Returns the root shell process identifier for a running session.
    pub fn session_process_id(&self, session_id: SessionId) -> Option<u32> {
        let runtime = self.session_runtime(session_id)?;
        match &runtime.backend {
            TerminalRuntimeBackend::Legacy(runtime_backend) => {
                runtime_backend.child.lock().process_id()
            }
            #[cfg(feature = "terminal-native-spike")]
            TerminalRuntimeBackend::Native(runtime_backend) => runtime_backend.session.process_id(),
        }
    }

    /// Terminates and unregisters a session runtime if it exists.
    pub fn terminate_session(&self, session_id: SessionId) -> bool {
        self.pending_restore.lock().remove(&session_id);

        let runtime = self.sessions.write().remove(&session_id);
        let Some(runtime) = runtime else {
            return false;
        };

        if let TerminalRuntimeBackend::Legacy(runtime_backend) = &runtime.backend {
            let mut child = runtime_backend.child.lock();
            let _ = child.kill();
            let _ = child.wait();
        }
        true
    }

    /// Returns the monotonic snapshot revision for change detection.
    pub fn session_snapshot_revision(&self, session_id: SessionId) -> Option<u64> {
        self.session_runtime(session_id)
            .map(|runtime| runtime.snapshot_revision.load(Ordering::Relaxed))
    }

    /// Returns the last observed runtime I/O activity timestamp in unix milliseconds.
    pub fn session_last_activity_unix_ms(&self, session_id: SessionId) -> Option<i64> {
        self.session_runtime(session_id)
            .map(|runtime| runtime.last_activity_unix_ms.load(Ordering::Relaxed))
    }

    fn session_runtime(&self, session_id: SessionId) -> Option<Arc<TerminalRuntime>> {
        self.sessions.read().get(&session_id).cloned()
    }

    fn publish_event(&self, event: TerminalEvent) {
        let _ = self.events.send(event);
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
            if let TerminalRuntimeBackend::Legacy(runtime_backend) = &runtime.backend {
                let mut child = runtime_backend.child.lock();
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

#[cfg(feature = "terminal-native-spike")]
fn native_terminal_backend_enabled() -> bool {
    std::env::var(NATIVE_TERMINAL_BACKEND_ENV_VAR)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(feature = "terminal-native-spike")]
fn map_native_terminal_error(error: crate::terminal_native::NativeTerminalError) -> TerminalError {
    match error {
        crate::terminal_native::NativeTerminalError::CreatePty(details) => {
            TerminalError::CreatePty(details)
        }
        crate::terminal_native::NativeTerminalError::StartShell { cwd, details } => {
            TerminalError::StartShell { cwd, details }
        }
        crate::terminal_native::NativeTerminalError::OpenWriter(details) => {
            TerminalError::OpenWriter(details)
        }
        crate::terminal_native::NativeTerminalError::OpenReader(details) => {
            TerminalError::OpenReader(details)
        }
        crate::terminal_native::NativeTerminalError::WriteInput(details) => {
            TerminalError::WriteInput(details)
        }
        crate::terminal_native::NativeTerminalError::FlushInput(details) => {
            TerminalError::FlushInput(details)
        }
        crate::terminal_native::NativeTerminalError::ResizePty(details) => {
            TerminalError::ResizePty(details)
        }
    }
}

#[cfg(feature = "terminal-native-spike")]
fn materialize_native_frame(
    previous: &TerminalFrame,
    next: Arc<TerminalFrame>,
) -> Arc<TerminalFrame> {
    let publication = match &next.publication {
        TerminalCellPublication::Full(_) => return next,
        TerminalCellPublication::Partial(changes) => {
            let mut cells = previous
                .full_cells()
                .map(|cells| cells.to_vec())
                .unwrap_or_else(|| {
                    vec![TerminalCell::default(); usize::from(next.rows) * usize::from(next.cols)]
                });
            let width = usize::from(next.cols);
            for span in changes.spans() {
                let start = usize::from(span.row) * width + usize::from(span.left);
                let end = start + usize::from(span.len);
                if let Some(target) = cells.get_mut(start..end) {
                    target.clone_from_slice(changes.cells_for_span(span));
                }
            }
            TerminalCellPublication::Full(Arc::new(cells))
        }
    };

    Arc::new(TerminalFrame {
        rows: next.rows,
        cols: next.cols,
        cursor: next.cursor,
        bracketed_paste: next.bracketed_paste,
        display_offset: next.display_offset,
        damage: next.damage.clone(),
        publication,
    })
}

#[cfg(feature = "terminal-native-spike")]
fn compatibility_snapshot_from_native_frame(
    frame: &TerminalFrame,
    restored_lines: &[String],
) -> TerminalSnapshot {
    let visible_lines = frame_visible_lines(frame);
    let lines = if restored_lines.is_empty() {
        visible_lines.clone()
    } else {
        let restored = normalized_history_lines(restored_lines);
        merge_scrollback_with_visible(&restored, &visible_lines)
    };
    let visible_start = lines.len().saturating_sub(visible_lines.len());

    TerminalSnapshot {
        lines,
        rows: frame.rows,
        cols: frame.cols,
        cursor_row: u16::try_from(visible_start + usize::from(frame.cursor.row))
            .unwrap_or(u16::MAX),
        cursor_col: frame.cursor.col.min(frame.cols.saturating_sub(1)),
        hide_cursor: matches!(frame.cursor.shape, TerminalCursorShape::Hidden),
        bracketed_paste: frame.bracketed_paste,
    }
}

#[cfg(feature = "terminal-native-spike")]
fn frame_visible_lines(frame: &TerminalFrame) -> Vec<String> {
    let mut lines = Vec::with_capacity(usize::from(frame.rows));
    let width = usize::from(frame.cols);
    let Some(cells) = frame.full_cells() else {
        lines.resize_with(usize::from(frame.rows), String::new);
        return lines;
    };

    for row in 0..usize::from(frame.rows) {
        let start = row * width;
        let end = start + width;
        let Some(row_cells) = cells.get(start..end) else {
            lines.push(String::new());
            continue;
        };

        let trimmed_len = row_cells
            .iter()
            .rposition(|cell| project_native_snapshot_char(cell) != ' ')
            .map(|index| index + 1)
            .unwrap_or(0);
        let mut line = String::with_capacity(trimmed_len);
        for cell in &row_cells[..trimmed_len] {
            line.push(project_native_snapshot_char(cell));
        }
        lines.push(line);
    }

    lines
}

#[cfg(feature = "terminal-native-spike")]
fn project_native_snapshot_char(cell: &TerminalCell) -> char {
    if cell.flags.contains(TerminalCellFlags::HIDDEN)
        || cell.flags.contains(TerminalCellFlags::WIDE_CHAR_SPACER)
        || cell
            .flags
            .contains(TerminalCellFlags::LEADING_WIDE_CHAR_SPACER)
    {
        ' '
    } else {
        cell.codepoint
    }
}

fn spawn_reader_thread(context: ReaderThreadContext) {
    let ReaderThreadContext {
        mut reader,
        parser,
        scrollback,
        snapshot_cache,
        snapshot_revision,
        last_activity_unix_ms,
        cwd,
        memory_sink,
        event_tx,
        session_id,
    } = context;

    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let now_ms = current_unix_ms();
                    last_activity_unix_ms.store(now_ms, Ordering::Relaxed);
                    let snapshot = {
                        let mut parser = parser.lock();
                        parser.process(&buffer[..read]);

                        let (snapshot, emitted_lines) = {
                            let mut scrollback = scrollback.write();
                            let emitted_lines = scrollback.process_bytes(&buffer[..read]);
                            let snapshot =
                                terminal_snapshot_from_parser(&parser, &scrollback.lines);
                            (snapshot, emitted_lines)
                        };

                        if let Some(memory_sink) = memory_sink.as_ref()
                            && !emitted_lines.is_empty()
                        {
                            let cwd = cwd.read().clone();
                            for line in emitted_lines {
                                memory_sink.record_output_line(session_id, &cwd, line, now_ms);
                            }
                        }

                        snapshot
                    };
                    *snapshot_cache.write() = Arc::new(snapshot);
                    snapshot_revision.fetch_add(1, Ordering::Relaxed);
                    let _ = event_tx.send(TerminalEvent {
                        session_id,
                        kind: TerminalEventKind::Activity,
                    });
                    let _ = event_tx.send(TerminalEvent {
                        session_id,
                        kind: TerminalEventKind::SnapshotChanged,
                    });
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

fn current_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn parse_input_lines(pending: &mut Vec<u8>, bytes: &[u8]) -> Vec<String> {
    let mut lines = Vec::new();
    for &byte in bytes {
        match byte {
            b'\r' | b'\n' => {
                let line = String::from_utf8_lossy(pending)
                    .trim_end_matches('\r')
                    .to_string();
                pending.clear();
                lines.push(line);
            }
            0x08 => {
                let _ = pending.pop();
            }
            value if value >= 0x20 || value == b'\t' => pending.push(value),
            _ => {}
        }
    }
    lines
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

fn prepend_history_lines_limited(
    existing_lines: &mut Vec<String>,
    older_lines: &[String],
    max_history_lines: usize,
) -> usize {
    let normalized_older = normalized_history_lines_limited(older_lines, max_history_lines);
    if normalized_older.is_empty() {
        return 0;
    }

    let existing_len = existing_lines.len();
    let mut merged = Vec::with_capacity(normalized_older.len() + existing_len);
    merged.extend(normalized_older);
    merged.extend(existing_lines.iter().cloned());

    let overflow = merged.len().saturating_sub(max_history_lines);
    if overflow > 0 {
        merged.drain(0..overflow);
    }

    *existing_lines = merged;
    existing_lines.len().saturating_sub(existing_len)
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

    fn process_bytes(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut emitted_lines = Vec::new();
        for &byte in bytes {
            match self.escape_state {
                EscapeParseState::Normal => match byte {
                    0x1b => self.escape_state = EscapeParseState::Esc,
                    b'\n' => emitted_lines.push(self.finish_line()),
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
        emitted_lines
    }

    fn finish_line(&mut self) -> String {
        let line = String::from_utf8_lossy(&self.pending)
            .trim_end_matches('\r')
            .to_string();
        self.pending.clear();
        self.push_line(line.clone());
        line
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
    #[cfg(feature = "terminal-native-spike")]
    use crate::terminal_native::TerminalCursor;

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

    #[test]
    fn parse_input_lines_collects_completed_lines() {
        let mut pending = Vec::new();
        let lines = parse_input_lines(&mut pending, b"echo hi\rnext");
        assert_eq!(lines, vec!["echo hi".to_string()]);
        assert_eq!(pending, b"next".to_vec());
    }

    #[test]
    fn prepend_history_lines_places_older_lines_at_front() {
        let mut existing = vec!["line 3".to_string(), "line 4".to_string()];
        let inserted = prepend_history_lines_limited(
            &mut existing,
            &["line 1".to_string(), "line 2".to_string()],
            10,
        );

        assert_eq!(inserted, 2);
        assert_eq!(
            existing,
            vec![
                "line 1".to_string(),
                "line 2".to_string(),
                "line 3".to_string(),
                "line 4".to_string(),
            ]
        );
    }

    #[cfg(feature = "terminal-native-spike")]
    #[test]
    fn materialize_native_frame_reuses_full_publications() {
        let previous = Arc::new(TerminalFrame {
            rows: 1,
            cols: 2,
            cursor: TerminalCursor {
                row: 0,
                col: 0,
                shape: TerminalCursorShape::Hidden,
            },
            bracketed_paste: false,
            display_offset: 0,
            damage: TerminalDamage::Full,
            publication: TerminalCellPublication::Full(Arc::new(vec![TerminalCell::default(); 2])),
        });
        let next = Arc::new(TerminalFrame {
            rows: 1,
            cols: 2,
            cursor: TerminalCursor {
                row: 0,
                col: 1,
                shape: TerminalCursorShape::Block,
            },
            bracketed_paste: false,
            display_offset: 0,
            damage: TerminalDamage::Full,
            publication: TerminalCellPublication::Full(Arc::new(vec![
                TerminalCell {
                    codepoint: 'a',
                    ..TerminalCell::default()
                },
                TerminalCell {
                    codepoint: 'b',
                    ..TerminalCell::default()
                },
            ])),
        });

        let materialized = materialize_native_frame(previous.as_ref(), Arc::clone(&next));

        assert!(Arc::ptr_eq(&materialized, &next));
    }

    #[cfg(feature = "terminal-native-spike")]
    #[test]
    fn frame_visible_lines_trims_trailing_blank_cells_without_reallocating_rows() {
        let frame = TerminalFrame {
            rows: 2,
            cols: 4,
            cursor: TerminalCursor {
                row: 0,
                col: 0,
                shape: TerminalCursorShape::Hidden,
            },
            bracketed_paste: false,
            display_offset: 0,
            damage: TerminalDamage::Full,
            publication: TerminalCellPublication::Full(Arc::new(vec![
                TerminalCell {
                    codepoint: 'a',
                    ..TerminalCell::default()
                },
                TerminalCell {
                    codepoint: 'b',
                    ..TerminalCell::default()
                },
                TerminalCell::default(),
                TerminalCell::default(),
                TerminalCell {
                    codepoint: 'x',
                    flags: TerminalCellFlags::HIDDEN,
                    ..TerminalCell::default()
                },
                TerminalCell::default(),
                TerminalCell::default(),
                TerminalCell::default(),
            ])),
        };

        let lines = frame_visible_lines(&frame);

        assert_eq!(lines, vec!["ab".to_string(), String::new()]);
    }
}
