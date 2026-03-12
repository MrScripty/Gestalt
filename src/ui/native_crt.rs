use dioxus::prelude::*;
use dioxus_native::{CustomPaintCtx, CustomPaintSource, DeviceHandle, TextureHandle, use_wgpu};
use std::time::Instant;
use wgpu::{
    CommandEncoderDescriptor, Device, Extent3d, FragmentState, LoadOp, MultisampleState,
    Operations, PipelineLayoutDescriptor, PrimitiveState, PushConstantRange, Queue,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, StoreOp, Texture, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor, VertexState,
};

#[component]
pub(crate) fn NativeCrtOverlay() -> Element {
    let paint_source_id = use_wgpu(NativeCrtPaintSource::new);

    rsx! {
        canvas {
            class: "crt-native-overlay",
            "src": paint_source_id,
        }
    }
}

struct NativeCrtPaintSource {
    state: NativeCrtRendererState,
    started_at: Instant,
}

enum NativeCrtRendererState {
    Active(Box<ActiveNativeCrtRenderer>),
    Suspended,
}

#[derive(Clone)]
struct TextureAndHandle {
    texture: Texture,
    handle: TextureHandle,
}

struct ActiveNativeCrtRenderer {
    device: Device,
    queue: Queue,
    pipeline: RenderPipeline,
    displayed_texture: Option<TextureAndHandle>,
    next_texture: Option<TextureAndHandle>,
}

impl NativeCrtPaintSource {
    fn new() -> Self {
        Self {
            state: NativeCrtRendererState::Suspended,
            started_at: Instant::now(),
        }
    }
}

impl CustomPaintSource for NativeCrtPaintSource {
    fn resume(&mut self, device_handle: &DeviceHandle) {
        let active_state =
            ActiveNativeCrtRenderer::new(&device_handle.device, &device_handle.queue);
        self.state = NativeCrtRendererState::Active(Box::new(active_state));
    }

    fn suspend(&mut self) {
        self.state = NativeCrtRendererState::Suspended;
    }

    fn render(
        &mut self,
        ctx: CustomPaintCtx<'_>,
        width: u32,
        height: u32,
        _scale: f64,
    ) -> Option<TextureHandle> {
        let NativeCrtRendererState::Active(state) = &mut self.state else {
            return None;
        };

        state.render(ctx, width, height, self.started_at.elapsed().as_secs_f32())
    }
}

impl ActiveNativeCrtRenderer {
    fn new(device: &Device, queue: &Queue) -> Self {
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("gestalt-native-crt-shader"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!("native_crt.wgsl"))),
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("gestalt-native-crt-layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[PushConstantRange {
                stages: ShaderStages::FRAGMENT,
                range: 0..16,
            }],
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("gestalt-native-crt-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(TextureFormat::Rgba8Unorm.into())],
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            device: device.clone(),
            queue: queue.clone(),
            pipeline,
            displayed_texture: None,
            next_texture: None,
        }
    }

    fn render(
        &mut self,
        mut ctx: CustomPaintCtx<'_>,
        width: u32,
        height: u32,
        elapsed_seconds: f32,
    ) -> Option<TextureHandle> {
        if width == 0 || height == 0 {
            return None;
        }

        if let Some(next) = &self.next_texture {
            if next.texture.width() != width || next.texture.height() != height {
                ctx.unregister_texture(self.next_texture.take().unwrap().handle);
            }
        }

        let texture_and_handle = match &self.next_texture {
            Some(next) => next,
            None => {
                let texture = create_texture(&self.device, width, height);
                let handle = ctx.register_texture(texture.clone());
                self.next_texture = Some(TextureAndHandle { texture, handle });
                self.next_texture.as_ref().unwrap()
            }
        };

        let next_texture = &texture_and_handle.texture;
        let next_texture_handle = texture_and_handle.handle.clone();
        let push_constants = NativeCrtPushConstants {
            params: [width as f32, height as f32, elapsed_seconds, 1.0],
        };

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("gestalt-native-crt-encoder"),
            });
        {
            let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("gestalt-native-crt-pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &next_texture.create_view(&TextureViewDescriptor::default()),
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_push_constants(
                ShaderStages::FRAGMENT,
                0,
                bytemuck::bytes_of(&push_constants),
            );
            render_pass.draw(0..3, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
        std::mem::swap(&mut self.next_texture, &mut self.displayed_texture);
        Some(next_texture_handle)
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct NativeCrtPushConstants {
    params: [f32; 4],
}

fn create_texture(device: &Device, width: u32, height: u32) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("gestalt-native-crt-texture"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
