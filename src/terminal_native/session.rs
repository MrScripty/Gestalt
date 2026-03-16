use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU16, AtomicU64, Ordering};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::{Mutex, RwLock};
use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use thiserror::Error;

use super::emulator::{AlacrittyEmulator, AlacrittyEmulatorConfig};
use super::model::TerminalFrame;

type FramePublishedCallback = Arc<dyn Fn(Arc<TerminalFrame>) + Send + Sync>;

/// Startup parameters for the single-session native terminal spike runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeTerminalSessionConfig {
    pub cwd: String,
    pub rows: u16,
    pub cols: u16,
    pub scrollback: usize,
}

impl NativeTerminalSessionConfig {
    pub fn emulator_config(&self) -> AlacrittyEmulatorConfig {
        AlacrittyEmulatorConfig {
            rows: self.rows,
            cols: self.cols,
            scrollback: self.scrollback,
        }
    }
}

/// Errors surfaced by the feature-gated PTY runtime.
#[derive(Debug, Error)]
pub enum NativeTerminalError {
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

/// Single-session PTY runtime that feeds frames to the native renderer spike.
pub struct NativeTerminalSession {
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn portable_pty::Child + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    shared: Arc<SharedSessionState>,
}

struct SharedSessionState {
    emulator: Mutex<AlacrittyEmulator>,
    frame: RwLock<Arc<TerminalFrame>>,
    rows: AtomicU16,
    cols: AtomicU16,
    revision: AtomicU64,
    last_activity_unix_ms: AtomicI64,
    closed: AtomicBool,
    frame_published: Option<FramePublishedCallback>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeTerminalSessionSummary {
    pub rows: u16,
    pub cols: u16,
    pub revision: u64,
    pub closed: bool,
}

impl NativeTerminalSession {
    pub fn spawn(config: NativeTerminalSessionConfig) -> Result<Self, NativeTerminalError> {
        Self::spawn_with_callback(config, None)
    }

    pub fn spawn_with_callback(
        config: NativeTerminalSessionConfig,
        frame_published: Option<FramePublishedCallback>,
    ) -> Result<Self, NativeTerminalError> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows: config.rows,
                cols: config.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| NativeTerminalError::CreatePty(error.to_string()))?;

        let mut command = CommandBuilder::new_default_prog();
        command.cwd(config.cwd.clone());
        command.env("TERM", "xterm-256color");

        let child =
            pair.slave
                .spawn_command(command)
                .map_err(|error| NativeTerminalError::StartShell {
                    cwd: config.cwd.clone(),
                    details: error.to_string(),
                })?;

        let master = pair.master;
        let writer = master
            .take_writer()
            .map_err(|error| NativeTerminalError::OpenWriter(error.to_string()))?;
        let reader = master
            .try_clone_reader()
            .map_err(|error| NativeTerminalError::OpenReader(error.to_string()))?;

        let mut emulator = AlacrittyEmulator::new(config.emulator_config());
        let initial_frame = Arc::new(emulator.snapshot());
        let shared = Arc::new(SharedSessionState {
            emulator: Mutex::new(emulator),
            frame: RwLock::new(initial_frame),
            rows: AtomicU16::new(config.rows),
            cols: AtomicU16::new(config.cols),
            revision: AtomicU64::new(1),
            last_activity_unix_ms: AtomicI64::new(0),
            closed: AtomicBool::new(false),
            frame_published,
        });

        spawn_reader_thread(reader, Arc::clone(&shared));

        Ok(Self {
            master: Mutex::new(master),
            child: Mutex::new(child),
            writer: Mutex::new(writer),
            shared,
        })
    }

    pub fn current_frame(&self) -> Arc<TerminalFrame> {
        Arc::clone(&self.shared.frame.read())
    }

    pub fn revision(&self) -> u64 {
        self.shared.revision.load(Ordering::Acquire)
    }

    pub fn last_activity_unix_ms(&self) -> i64 {
        self.shared.last_activity_unix_ms.load(Ordering::Acquire)
    }

    pub fn summary(&self) -> NativeTerminalSessionSummary {
        NativeTerminalSessionSummary {
            rows: self.shared.rows.load(Ordering::Acquire),
            cols: self.shared.cols.load(Ordering::Acquire),
            revision: self.shared.revision.load(Ordering::Acquire),
            closed: self.shared.closed.load(Ordering::Acquire),
        }
    }

    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire)
    }

    pub fn process_id(&self) -> Option<u32> {
        self.child.lock().process_id()
    }

    pub fn send_input(&self, bytes: &[u8]) -> Result<(), NativeTerminalError> {
        let mut writer = self.writer.lock();
        writer
            .write_all(bytes)
            .map_err(|error| NativeTerminalError::WriteInput(error.to_string()))?;
        writer
            .flush()
            .map_err(|error| NativeTerminalError::FlushInput(error.to_string()))?;
        self.shared
            .last_activity_unix_ms
            .store(current_unix_ms(), Ordering::Release);
        Ok(())
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<bool, NativeTerminalError> {
        self.master
            .lock()
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| NativeTerminalError::ResizePty(error.to_string()))?;

        let mut emulator = self.shared.emulator.lock();
        let changed = emulator.resize(rows, cols);
        if changed {
            publish_frame(&self.shared, emulator.snapshot());
        }

        Ok(changed)
    }

    pub fn scroll_display_delta(&self, delta_lines: i32) -> bool {
        let mut emulator = self.shared.emulator.lock();
        let changed = emulator.scroll_display_delta(delta_lines);
        if changed {
            publish_frame(&self.shared, emulator.snapshot());
        }
        changed
    }
}

impl Drop for NativeTerminalSession {
    fn drop(&mut self) {
        let _ = self.child.lock().kill();
        self.shared.closed.store(true, Ordering::Release);
    }
}

fn spawn_reader_thread(
    mut reader: Box<dyn Read + Send>,
    shared: Arc<SharedSessionState>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 16_384];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    shared.closed.store(true, Ordering::Release);
                    break;
                }
                Ok(count) => {
                    let mut emulator = shared.emulator.lock();
                    emulator.ingest(&buffer[..count]);
                    shared
                        .last_activity_unix_ms
                        .store(current_unix_ms(), Ordering::Release);
                    let frame = publish_frame(&shared, emulator.snapshot());
                    if let Some(callback) = shared.frame_published.as_ref() {
                        callback(frame);
                    }
                }
                Err(_) => {
                    shared.closed.store(true, Ordering::Release);
                    break;
                }
            }
        }
    })
}

fn publish_frame(shared: &SharedSessionState, frame: TerminalFrame) -> Arc<TerminalFrame> {
    shared.rows.store(frame.rows, Ordering::Release);
    shared.cols.store(frame.cols, Ordering::Release);
    let frame = Arc::new(frame);
    *shared.frame.write() = Arc::clone(&frame);
    shared.revision.fetch_add(1, Ordering::AcqRel);
    frame
}

fn current_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
