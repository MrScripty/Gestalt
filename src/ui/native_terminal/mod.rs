mod component;
mod constants;
mod frame;
mod glyph_atlas;
mod paint;
mod renderer;
mod scene;
mod scroll;
mod surface_sync;
mod viewport;
mod wrap_policy;

pub(crate) use component::NativeTerminalBody;
pub(crate) use scene::scaled_cell_height_px;
pub(crate) use scene::scaled_cell_width_px;
pub(crate) use scroll::{
    apply_native_scroll_delta, apply_native_scroll_to, native_offset_from_horizontal_track,
    native_offset_from_vertical_track, native_scroll_track_height_px,
};
pub(crate) use viewport::native_terminal_viewport_metrics;
pub(crate) use wrap_policy::default_unwrapped_terminal_cols;
