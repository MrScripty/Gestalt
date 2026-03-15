//! Feature-gated terminal semantics and PTY runtime for the native renderer spike.

mod app;
mod constants;
mod controller;
mod demo;
mod emulator;
mod model;
mod raster;
mod session;

pub use app::launch_terminal_native_spike;
pub use emulator::{AlacrittyEmulator, AlacrittyEmulatorConfig};
pub use model::{
    TerminalCell, TerminalCellFlags, TerminalColor, TerminalCursor, TerminalCursorShape,
    TerminalDamage, TerminalDamageSpan, TerminalFrame,
};
pub use session::{NativeTerminalError, NativeTerminalSession, NativeTerminalSessionConfig};
