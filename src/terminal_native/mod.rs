//! Feature-gated terminal semantics and PTY runtime for the native renderer spike.

mod emulator;
mod model;
mod session;

pub use emulator::{AlacrittyEmulator, AlacrittyEmulatorConfig};
pub use model::{
    TerminalCell, TerminalCellFlags, TerminalColor, TerminalCursor, TerminalCursorShape,
    TerminalDamage, TerminalDamageSpan, TerminalFrame,
};
pub use session::{NativeTerminalError, NativeTerminalSession, NativeTerminalSessionConfig};
