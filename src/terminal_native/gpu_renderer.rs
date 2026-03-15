use bytemuck::cast_slice;
use dioxus_native::{CustomPaintCtx, DeviceHandle, TextureHandle};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferAddress,
    BufferBindingType, BufferDescriptor, BufferUsages, Color, ColorTargetState, ColorWrites,
    CommandEncoderDescriptor, Device, Extent3d, FilterMode, FragmentState, FrontFace,
    MultisampleState, Origin3d, PipelineCompilationOptions, PipelineLayoutDescriptor, PolygonMode,
    PrimitiveState, PrimitiveTopology, Queue, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, SamplerBindingType, SamplerDescriptor,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, TexelCopyBufferLayout,
    TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor, TextureDimension,
    TextureFormat, TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor,
    TextureViewDimension, VertexAttribute, VertexBufferLayout, VertexState, VertexStepMode,
    util::DeviceExt,
};

use super::constants::{
    ATLAS_TEXTURE_LABEL, GLYPH_PIPELINE_LABEL, GLYPH_SHADER_LABEL, INSTANCE_BUFFER_LABEL,
    TEXTURE_LABEL, UNIFORM_BUFFER_LABEL,
};
use super::gpu_scene::{QuadInstance, TerminalGpuSceneCache};

const DEFAULT_CLEAR: Color = Color {
    r: 8.0 / 255.0,
    g: 12.0 / 255.0,
    b: 16.0 / 255.0,
    a: 1.0,
};

pub struct NativeTerminalGpuRenderer {
    device: Device,
    queue: Queue,
    pipeline: RenderPipeline,
    bind_group: BindGroup,
    uniform_buffer: Buffer,
    atlas_texture: Texture,
    output: Option<OutputTexture>,
    background_buffer: Option<InstanceBuffer>,
    glyph_buffer: Option<InstanceBuffer>,
    overlay_buffer: Option<InstanceBuffer>,
    scene_cache: TerminalGpuSceneCache,
    last_revision: u64,
    last_size: (u32, u32),
}

struct OutputTexture {
    texture: Texture,
    view: TextureView,
    handle: TextureHandle,
}

struct InstanceBuffer {
    buffer: Buffer,
    capacity: usize,
}

impl NativeTerminalGpuRenderer {
    pub fn new(device_handle: &DeviceHandle) -> Self {
        let device = device_handle.device.clone();
        let queue = device_handle.queue.clone();
        let scene_cache = TerminalGpuSceneCache::new();
        let atlas_texture = create_atlas_texture(&device, scene_cache.atlas().atlas_size());
        let atlas_view = atlas_texture.create_view(&TextureViewDescriptor::default());
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some(ATLAS_TEXTURE_LABEL),
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..Default::default()
        });
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(UNIFORM_BUFFER_LABEL),
            contents: cast_slice(&[[1.0f32, 1.0, 0.0, 0.0]]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some(GLYPH_PIPELINE_LABEL),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some(GLYPH_PIPELINE_LABEL),
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&atlas_view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some(GLYPH_SHADER_LABEL),
            source: ShaderSource::Wgsl(GLYPH_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some(GLYPH_PIPELINE_LABEL),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(GLYPH_PIPELINE_LABEL),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[VertexBufferLayout {
                    array_stride: std::mem::size_of::<QuadInstance>() as BufferAddress,
                    step_mode: VertexStepMode::Instance,
                    attributes: &[
                        VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 0,
                            shader_location: 0,
                        },
                        VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 16,
                            shader_location: 1,
                        },
                        VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 32,
                            shader_location: 2,
                        },
                    ],
                }],
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: PolygonMode::Fill,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        Self {
            device,
            queue,
            pipeline,
            bind_group,
            uniform_buffer,
            atlas_texture,
            output: None,
            background_buffer: None,
            glyph_buffer: None,
            overlay_buffer: None,
            scene_cache,
            last_revision: 0,
            last_size: (0, 0),
        }
    }

    pub fn render(
        &mut self,
        ctx: &mut CustomPaintCtx<'_>,
        frame: &super::TerminalFrame,
        revision: u64,
        width: u32,
        height: u32,
    ) -> Option<TextureHandle> {
        if width == 0 || height == 0 {
            return None;
        }

        let output = ensure_output_texture(ctx, &self.device, &mut self.output, width, height);
        let needs_redraw = self.last_revision != revision || self.last_size != (width, height);
        if !needs_redraw {
            return Some(output.handle.clone());
        }

        let scene = self.scene_cache.prepare(frame, width, height);
        if self.scene_cache.atlas().is_dirty() {
            self.queue.write_texture(
                TexelCopyTextureInfo {
                    texture: &self.atlas_texture,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                self.scene_cache.atlas().pixels(),
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.scene_cache.atlas().atlas_size()),
                    rows_per_image: Some(self.scene_cache.atlas().atlas_size()),
                },
                Extent3d {
                    width: self.scene_cache.atlas().atlas_size(),
                    height: self.scene_cache.atlas().atlas_size(),
                    depth_or_array_layers: 1,
                },
            );
            self.scene_cache.atlas_mut().mark_uploaded();
        }

        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            cast_slice(&[[width as f32, height as f32, 0.0, 0.0]]),
        );
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some(TEXTURE_LABEL),
            });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some(TEXTURE_LABEL),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &output.view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(DEFAULT_CLEAR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            draw_instances(
                &self.device,
                &self.queue,
                &mut self.background_buffer,
                &scene.background_instances,
                &mut pass,
            );
            draw_instances(
                &self.device,
                &self.queue,
                &mut self.glyph_buffer,
                &scene.glyph_instances,
                &mut pass,
            );
            draw_instances(
                &self.device,
                &self.queue,
                &mut self.overlay_buffer,
                &scene.overlay_instances,
                &mut pass,
            );
        }

        self.queue.submit(Some(encoder.finish()));
        self.last_revision = revision;
        self.last_size = (width, height);

        Some(output.handle.clone())
    }
}

fn draw_instances<'a>(
    device: &Device,
    queue: &Queue,
    slot: &'a mut Option<InstanceBuffer>,
    instances: &[QuadInstance],
    pass: &mut wgpu::RenderPass<'a>,
) {
    if instances.is_empty() {
        return;
    }

    let buffer = ensure_instance_buffer(device, slot, instances.len());
    queue.write_buffer(&buffer.buffer, 0, cast_slice(instances));
    pass.set_vertex_buffer(0, buffer.buffer.slice(..));
    pass.draw(0..6, 0..instances.len() as u32);
}

fn ensure_instance_buffer<'a>(
    device: &Device,
    slot: &'a mut Option<InstanceBuffer>,
    count: usize,
) -> &'a InstanceBuffer {
    let needs_new = slot
        .as_ref()
        .map(|buffer| buffer.capacity < count)
        .unwrap_or(true);

    if needs_new {
        let capacity = count.max(64).next_power_of_two();
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some(INSTANCE_BUFFER_LABEL),
            size: (std::mem::size_of::<QuadInstance>() * capacity) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        *slot = Some(InstanceBuffer { buffer, capacity });
    }

    slot.as_ref().expect("instance buffer should exist")
}

fn ensure_output_texture<'a>(
    ctx: &mut CustomPaintCtx<'_>,
    device: &Device,
    slot: &'a mut Option<OutputTexture>,
    width: u32,
    height: u32,
) -> &'a OutputTexture {
    let replace = slot
        .as_ref()
        .map(|entry| entry.texture.width() != width || entry.texture.height() != height)
        .unwrap_or(true);

    if replace {
        if let Some(existing) = slot.take() {
            ctx.unregister_texture(existing.handle);
        }

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
            usage: TextureUsages::COPY_SRC
                | TextureUsages::TEXTURE_BINDING
                | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        let handle = ctx.register_texture(texture.clone());
        *slot = Some(OutputTexture {
            texture,
            view,
            handle,
        });
    }

    slot.as_ref().expect("output texture should exist")
}

fn create_atlas_texture(device: &Device, atlas_size: u32) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some(ATLAS_TEXTURE_LABEL),
        size: Extent3d {
            width: atlas_size,
            height: atlas_size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::R8Unorm,
        usage: TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

const GLYPH_SHADER: &str = r#"
struct SurfaceUniform {
    size: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> surface: SurfaceUniform;
@group(0) @binding(1)
var atlas_tex: texture_2d<f32>;
@group(0) @binding(2)
var atlas_sampler: sampler;

struct VertexInput {
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv_rect: vec4<f32>,
    @builtin(vertex_index) vertex_index: u32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv_rect: vec4<f32>,
    @location(2) local_uv: vec2<f32>,
};

fn quad_uv(index: u32) -> vec2<f32> {
    let points = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    return points[index];
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    let uv = quad_uv(input.vertex_index);
    let pixel = input.rect.xy + uv * input.rect.zw;
    let ndc = vec2<f32>(
        (pixel.x / surface.size.x) * 2.0 - 1.0,
        1.0 - (pixel.y / surface.size.y) * 2.0,
    );

    var out: VertexOutput;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.color = input.color;
    out.uv_rect = input.uv_rect;
    out.local_uv = uv;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.uv_rect.z <= 0.0 || input.uv_rect.w <= 0.0 {
        return input.color;
    }

    let atlas_uv = input.uv_rect.xy + input.local_uv * input.uv_rect.zw;
    let alpha = textureSample(atlas_tex, atlas_sampler, atlas_uv).r;
    if alpha <= 0.001 {
        discard;
    }

    return vec4<f32>(input.color.rgb, input.color.a * alpha);
}
"#;
