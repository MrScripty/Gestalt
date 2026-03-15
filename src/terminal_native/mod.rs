//! Feature-gated terminal semantics and PTY runtime for the native renderer spike.

mod app;
mod constants;
mod controller;
mod demo;
mod emulator;
mod glyph_atlas;
mod gpu_renderer;
mod gpu_scene;
mod input;
mod model;
mod paint;
mod raster;
mod session;

pub use app::launch_terminal_native_spike;
pub(crate) use constants::{
    APP_ROOT_STYLE, CANVAS_STYLE, INPUT_OVERLAY_STYLE, STATUS_BAR_STYLE, STATUS_HINT_STYLE,
    STATUS_HINT_TEXT, STATUS_TITLE_STYLE, STATUS_TITLE_TEXT, TERMINAL_SURFACE_STYLE,
};
pub use emulator::{AlacrittyEmulator, AlacrittyEmulatorConfig};
pub use gpu_scene::TerminalGpuSceneCache;
pub use model::{
    TerminalCell, TerminalCellFlags, TerminalColor, TerminalCursor, TerminalCursorShape,
    TerminalDamage, TerminalDamageSpan, TerminalFrame,
};
pub use raster::TerminalRaster;
pub use session::{NativeTerminalError, NativeTerminalSession, NativeTerminalSessionConfig};
