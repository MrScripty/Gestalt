use dioxus_native::{CustomPaintCtx, CustomPaintSource, DeviceHandle, TextureHandle};
use wgpu::{
    Device, Extent3d, Origin3d, Queue, TexelCopyBufferLayout, TexelCopyTextureInfo, Texture,
    TextureAspect, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};

use super::constants::TEXTURE_LABEL;
use super::controller::NativeTerminalController;
use super::raster::TerminalRaster;

pub struct NativeTerminalPaintSource {
    controller: NativeTerminalController,
    state: RendererState,
}

enum RendererState {
    Active(Box<ActiveNativeTerminalRenderer>),
    Suspended,
}

#[derive(Clone)]
struct TextureAndHandle {
    texture: Texture,
    handle: TextureHandle,
}

struct ActiveNativeTerminalRenderer {
    device: Device,
    queue: Queue,
    texture: Option<TextureAndHandle>,
    raster: TerminalRaster,
    last_revision: u64,
    last_size: (u32, u32),
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
        let renderer = ActiveNativeTerminalRenderer {
            device: device_handle.device.clone(),
            queue: device_handle.queue.clone(),
            texture: None,
            raster: TerminalRaster::new(),
            last_revision: 0,
            last_size: (0, 0),
        };
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
        let needs_full_upload = state.last_size != (width, height)
            || state.raster.dimensions_changed(width, height)
            || revision != state.last_revision;

        let texture = ensure_texture(&mut ctx, &state.device, &mut state.texture, width, height);
        if needs_full_upload {
            state.raster.update(&frame, width, height);
            state.queue.write_texture(
                TexelCopyTextureInfo {
                    texture: &texture.texture,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                state.raster.pixels(),
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width),
                    rows_per_image: Some(height),
                },
                Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            state.last_revision = revision;
            state.last_size = (width, height);
        }

        Some(texture.handle.clone())
    }
}

fn ensure_texture(
    ctx: &mut CustomPaintCtx<'_>,
    device: &Device,
    slot: &mut Option<TextureAndHandle>,
    width: u32,
    height: u32,
) -> TextureAndHandle {
    let replace = slot
        .as_ref()
        .map(|entry| entry.texture.width() != width || entry.texture.height() != height)
        .unwrap_or(true);

    if replace {
        if let Some(existing) = slot.take() {
            ctx.unregister_texture(existing.handle);
        }

        let texture = create_texture(device, width, height, ctx);
        *slot = Some(texture);
    }

    slot.as_ref()
        .expect("texture slot should be populated")
        .clone()
}

fn create_texture(
    device: &Device,
    width: u32,
    height: u32,
    ctx: &mut CustomPaintCtx<'_>,
) -> TextureAndHandle {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some(TEXTURE_LABEL),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::COPY_DST
            | TextureUsages::COPY_SRC
            | TextureUsages::TEXTURE_BINDING
            | TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let handle = ctx.register_texture(texture.clone());
    TextureAndHandle { texture, handle }
}
