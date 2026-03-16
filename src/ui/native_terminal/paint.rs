use std::sync::Arc;

use dioxus_native::{CustomPaintCtx, CustomPaintSource, DeviceHandle, TextureHandle};
use parking_lot::Mutex;

use super::frame::NativeTerminalFrame;
use super::renderer::NativeTerminalGpuRenderer;

#[derive(Clone)]
pub(crate) struct NativeTerminalPaintBridge {
    inner: Arc<Mutex<PaintState>>,
}

#[derive(Clone)]
struct PaintState {
    revision: u64,
    frame: NativeTerminalFrame,
    ui_scale: f32,
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
        _scale: f64,
    ) -> Option<TextureHandle> {
        let RendererState::Active(renderer) = &mut self.state else {
            return None;
        };

        let snapshot = self.bridge.snapshot();
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
