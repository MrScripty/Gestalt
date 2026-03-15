pub const WINDOW_TITLE: &str = "Gestalt Native Terminal Spike";
pub const WINDOW_WIDTH_PX: f64 = 1360.0;
pub const WINDOW_HEIGHT_PX: f64 = 900.0;

pub const DEFAULT_SESSION_ROWS: u16 = 42;
pub const DEFAULT_SESSION_COLS: u16 = 140;
pub const DEFAULT_SCROLLBACK: usize = 20_000;

pub const CELL_WIDTH_PX: u32 = 9;
pub const CELL_HEIGHT_PX: u32 = 18;
pub const MIN_TERMINAL_ROWS: u16 = 8;
pub const MIN_TERMINAL_COLS: u16 = 20;

pub const APP_ROOT_STYLE: &str = "width: 100vw; height: 100vh; display: flex; flex-direction: column; background: #080c10; position: relative;";
pub const STATUS_BAR_STYLE: &str = "padding: 10px 14px; color: #dfe7ee; background: rgba(12,18,24,0.96); font-family: monospace; font-size: 13px; display: flex; gap: 12px; align-items: center; flex-wrap: wrap;";
pub const STATUS_TITLE_STYLE: &str = "font-weight: 700;";
pub const STATUS_HINT_STYLE: &str = "margin-left: auto; color: #90a4b8;";
pub const PANE_GRID_STYLE: &str = "flex: 1 1 auto; min-height: 0; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; padding: 12px;";
pub const PANE_CARD_STYLE: &str = "min-width: 0; min-height: 0; display: flex; flex-direction: column; border: 1px solid rgba(124, 145, 168, 0.28); background: rgba(12,18,24,0.92); box-shadow: inset 0 1px 0 rgba(255,255,255,0.03);";
pub const PANE_HEADER_STYLE: &str = "padding: 8px 10px; border-bottom: 1px solid rgba(124, 145, 168, 0.2); color: #dfe7ee; font-family: monospace; font-size: 12px; display: flex; gap: 10px; align-items: center;";
pub const PANE_TITLE_STYLE: &str = "font-weight: 700; color: #f2f6fa;";
pub const PANE_META_STYLE: &str = "color: #90a4b8;";
pub const TERMINAL_SURFACE_STYLE: &str =
    "position: relative; flex: 1 1 auto; width: 100%; height: 100%;";
pub const CANVAS_STYLE: &str = "position: absolute; inset: 0; width: 100%; height: 100%;";
pub const INPUT_OVERLAY_STYLE: &str = "position: absolute; inset: 0; width: 100%; height: 100%; opacity: 0; background: transparent; color: transparent; caret-color: transparent; border: none; outline: none;";

pub const STATUS_TITLE_TEXT: &str = "terminal_native_spike";
pub const STATUS_HINT_TEXT: &str = "click terminal and type";
pub const TEXTURE_LABEL: &str = "gestalt-terminal-native-spike-texture";
pub const ATLAS_TEXTURE_LABEL: &str = "gestalt-terminal-native-spike-glyph-atlas";
pub const GLYPH_PIPELINE_LABEL: &str = "gestalt-terminal-native-spike-glyph-pipeline";
pub const GLYPH_SHADER_LABEL: &str = "gestalt-terminal-native-spike-glyph-shader";
pub const INSTANCE_BUFFER_LABEL: &str = "gestalt-terminal-native-spike-instance-buffer";
pub const UNIFORM_BUFFER_LABEL: &str = "gestalt-terminal-native-spike-uniform-buffer";
pub const ATLAS_TEXTURE_SIZE_PX: u32 = 2048;
