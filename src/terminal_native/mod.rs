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
    ACTIVE_PANE_STACK_STYLE, APP_ROOT_STYLE, BACKGROUND_PANE_BODY_STYLE,
    BACKGROUND_PANE_BUTTON_STYLE, BACKGROUND_PANE_LIST_STYLE, CANVAS_STYLE, INPUT_OVERLAY_STYLE,
    PANE_CARD_STYLE, PANE_HEADER_STYLE, PANE_LAYOUT_STYLE, PANE_META_STYLE,
    PANE_SWITCH_BUTTON_ACTIVE_STYLE, PANE_SWITCH_BUTTON_STYLE, PANE_SWITCHER_STYLE,
    PANE_TITLE_STYLE, STATUS_BAR_STYLE, STATUS_HINT_STYLE, STATUS_HINT_TEXT, STATUS_TITLE_STYLE,
    STATUS_TITLE_TEXT, TERMINAL_SURFACE_STYLE,
};
pub use emulator::{AlacrittyEmulator, AlacrittyEmulatorConfig, EmulatorSnapshotProfile};
pub use glyph_atlas::SharedGlyphAtlas;
pub use gpu_scene::{TerminalGpuSceneCache, TerminalGpuSceneProfile};
pub use model::{
    TerminalCell, TerminalCellFlags, TerminalCellPublication, TerminalCellSpanBatch,
    TerminalCellSpanUpdate, TerminalColor, TerminalCursor, TerminalCursorShape, TerminalDamage,
    TerminalDamageSpan, TerminalFrame,
};
pub use raster::TerminalRaster;
pub use session::{
    NativeTerminalError, NativeTerminalSession, NativeTerminalSessionConfig,
    NativeTerminalSessionSummary,
};
