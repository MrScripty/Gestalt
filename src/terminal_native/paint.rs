use dioxus_native::{CustomPaintCtx, CustomPaintSource, DeviceHandle, TextureHandle};

use super::controller::NativeTerminalController;
use super::gpu_renderer::NativeTerminalGpuRenderer;

pub struct NativeTerminalPaintSource {
    controller: NativeTerminalController,
    state: RendererState,
}

enum RendererState {
    Active(Box<NativeTerminalGpuRenderer>),
    Suspended,
}

impl NativeTerminalPaintSource {
    pub fn new(controller: NativeTerminalController) -> Self {
        Self {
            controller,
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
        let RendererState::Active(state) = &mut self.state else {
            return None;
        };

        if width == 0 || height == 0 {
            return None;
        }

        self.controller.resize_for_surface(width, height);
        let revision = self.controller.revision();
        let frame = self.controller.frame();
        state.render(&mut ctx, &frame, revision, width, height)
    }
}
