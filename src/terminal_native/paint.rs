use dioxus_native::{CustomPaintCtx, CustomPaintSource, DeviceHandle, TextureHandle};

use super::controller::NativeTerminalController;
use super::gpu_renderer::{NativeTerminalGpuRenderer, NativeTerminalGpuShared};

pub struct NativeTerminalPaintSource {
    controller: NativeTerminalController,
    shared_gpu: NativeTerminalGpuShared,
    last_surface_cells: Option<(u16, u16)>,
    state: RendererState,
}

enum RendererState {
    Active(Box<NativeTerminalGpuRenderer>),
    Suspended,
}

impl NativeTerminalPaintSource {
    pub fn new(controller: NativeTerminalController, shared_gpu: NativeTerminalGpuShared) -> Self {
        Self {
            controller,
            shared_gpu,
            last_surface_cells: None,
            state: RendererState::Suspended,
        }
    }
}

impl CustomPaintSource for NativeTerminalPaintSource {
    fn resume(&mut self, device_handle: &DeviceHandle) {
        let renderer = NativeTerminalGpuRenderer::with_shared(&self.shared_gpu, device_handle);
        self.state = RendererState::Active(Box::new(renderer));
    }

    fn suspend(&mut self) {
        self.last_surface_cells = None;
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

        let surface_cells = NativeTerminalController::surface_cells(width, height);
        let cells_changed = self.last_surface_cells != Some(surface_cells);
        if cells_changed {
            self.controller
                .resize_cells(surface_cells.0, surface_cells.1);
            self.last_surface_cells = Some(surface_cells);
        }

        let revision = self.controller.revision();
        if !cells_changed {
            if let Some(handle) = state.cached_handle_if_unchanged(revision, width, height) {
                return Some(handle);
            }
        }

        let frame = self.controller.frame();
        state.render(&mut ctx, &frame, revision, width, height)
    }
}
