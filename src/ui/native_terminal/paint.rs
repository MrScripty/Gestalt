use std::sync::Arc;

use dioxus_native::{CustomPaintCtx, CustomPaintSource, DeviceHandle, TextureHandle};
use parking_lot::Mutex;

use super::frame::NativeTerminalFrame;
use super::renderer::NativeTerminalGpuRenderer;
use super::scene::surface_cells;

#[derive(Clone)]
pub(crate) struct NativeTerminalPaintBridge {
    inner: Arc<Mutex<PaintState>>,
}

#[derive(Clone)]
struct PaintState {
    revision: u64,
    frame: NativeTerminalFrame,
    ui_scale: f32,
    surface_cells: Option<(u16, u16)>,
    surface_size_px: Option<(f64, f64)>,
}

pub(crate) struct NativeTerminalPaintSource {
    bridge: NativeTerminalPaintBridge,
    state: RendererState,
}

enum RendererState {
    Active(Box<NativeTerminalGpuRenderer>),
    Suspended,
}

impl NativeTerminalPaintBridge {
    pub(crate) fn new(frame: NativeTerminalFrame, ui_scale: f32) -> Self {
        Self {
            inner: Arc::new(Mutex::new(PaintState {
                revision: 1,
                frame,
                ui_scale,
                surface_cells: None,
                surface_size_px: None,
            })),
        }
    }

    pub(crate) fn update_frame(&self, frame: NativeTerminalFrame, ui_scale: f32) {
        let mut state = self.inner.lock();
        if state.frame != frame || state.ui_scale != ui_scale {
            state.revision = state.revision.saturating_add(1);
            state.frame = frame;
            state.ui_scale = ui_scale;
        }
    }

    fn snapshot(&self) -> PaintState {
        self.inner.lock().clone()
    }

    pub(crate) fn update_surface_cells(&self, rows: u16, cols: u16) {
        let mut state = self.inner.lock();
        state.surface_cells = Some((rows, cols));
    }

    pub(crate) fn surface_cells(&self) -> Option<(u16, u16)> {
        self.inner.lock().surface_cells
    }

    pub(crate) fn update_surface_size_px(&self, width: f64, height: f64) {
        let mut state = self.inner.lock();
        state.surface_size_px = Some((width, height));
    }

    pub(crate) fn surface_size_px(&self) -> Option<(f64, f64)> {
        self.inner.lock().surface_size_px
    }
}

impl NativeTerminalPaintSource {
    pub(crate) fn new(bridge: NativeTerminalPaintBridge) -> Self {
        Self {
            bridge,
            state: RendererState::Suspended,
        }
    }
}

impl CustomPaintSource for NativeTerminalPaintSource {
    fn resume(&mut self, device_handle: &DeviceHandle) {
        let renderer = NativeTerminalGpuRenderer::new(device_handle);
        self.state = RendererState::Active(Box::new(renderer));
    }

    fn suspend(&mut self) {
        self.state = RendererState::Suspended;
    }

    fn render(
        &mut self,
        mut ctx: CustomPaintCtx<'_>,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Option<TextureHandle> {
        let RendererState::Active(renderer) = &mut self.state else {
            return None;
        };

        let snapshot = self.bridge.snapshot();
        let (rows, cols) = surface_cells(width, height, snapshot.ui_scale);
        self.bridge.update_surface_cells(rows, cols);
        let logical_scale = scale.max(0.1);
        self.bridge.update_surface_size_px(width as f64 / logical_scale, height as f64 / logical_scale);
        if let Some(handle) = renderer.cached_handle_if_unchanged(snapshot.revision, width, height)
        {
            return Some(handle);
        }

        renderer.render(
            &mut ctx,
            &snapshot.frame,
            snapshot.revision,
            width,
            height,
            snapshot.ui_scale,
        )
    }
}
