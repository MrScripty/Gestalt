use dioxus::events::KeyboardEvent;
use dioxus::prelude::*;
use dioxus_native::{CustomPaintCtx, CustomPaintSource, DeviceHandle, TextureHandle, use_wgpu};
use wgpu::{
    Device, Extent3d, Origin3d, Queue, TexelCopyBufferLayout, TexelCopyTextureInfo, Texture,
    TextureAspect, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};

use super::controller::NativeTerminalController;
use super::raster::TerminalRaster;

const CELL_WIDTH_PX: u32 = 9;
const CELL_HEIGHT_PX: u32 = 18;
const MIN_ROWS: u16 = 8;
const MIN_COLS: u16 = 20;

#[component]
pub fn TerminalNativeDemo() -> Element {
    let controller = use_context::<NativeTerminalController>();
    let mut input_buffer = use_signal(String::new);
    let paint_controller = controller.clone();
    let key_controller = controller.clone();
    let input_controller = controller.clone();
    let paint_source_id =
        use_wgpu(move || NativeTerminalPaintSource::new(paint_controller.clone()));
    let input_buffer_value = input_buffer.read().clone();

    rsx! {
        div {
            style: "width: 100vw; height: 100vh; display: flex; flex-direction: column; background: #080c10; position: relative;",
            div {
                style: "padding: 10px 14px; color: #dfe7ee; background: rgba(12,18,24,0.96); font-family: monospace; font-size: 13px; display: flex; gap: 12px; align-items: center; flex-wrap: wrap;",
                div {
                    style: "font-weight: 700;",
                    "terminal_native_spike"
                }
                div {
                    "{status_line(&controller)}"
                }
                div {
                    style: "margin-left: auto; color: #90a4b8;",
                    "click terminal and type"
                }
            }
            div {
                style: "position: relative; flex: 1 1 auto; width: 100%; height: 100%;",
                canvas {
                    style: "position: absolute; inset: 0; width: 100%; height: 100%;",
                    "src": paint_source_id,
                }
                input {
                    r#type: "text",
                    tabindex: "0",
                    autofocus: "true",
                    spellcheck: "false",
                    value: "{input_buffer_value}",
                    style: "position: absolute; inset: 0; width: 100%; height: 100%; opacity: 0; background: transparent; color: transparent; caret-color: transparent; border: none; outline: none;",
                    onkeydown: move |event| {
                        if let Some(bytes) = special_key_event_to_bytes(&event) {
                            event.prevent_default();
                            key_controller.send_input(&bytes);
                        }
                    },
                    oninput: move |event| {
                        let value = event.value();
                        if !value.is_empty() {
                            input_controller.send_input(value.as_bytes());
                        }
                        input_buffer.set(String::new());
                    },
                }
            }
        }
    }
}

fn status_line(controller: &NativeTerminalController) -> String {
    let frame = controller.frame();
    format!(
        "{}x{}  revision={}  closed={}",
        frame.cols,
        frame.rows,
        controller.revision(),
        controller.is_closed()
    )
}

struct NativeTerminalPaintSource {
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
    fn new(controller: NativeTerminalController) -> Self {
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

        ensure_session_size(&self.controller, width, height);
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
        label: Some("gestalt-terminal-native-spike-texture"),
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

fn ensure_session_size(controller: &NativeTerminalController, width: u32, height: u32) {
    let rows = ((height / CELL_HEIGHT_PX).max(u32::from(MIN_ROWS))) as u16;
    let cols = ((width / CELL_WIDTH_PX).max(u32::from(MIN_COLS))) as u16;
    controller.resize_cells(rows, cols);
}

fn special_key_event_to_bytes(event: &KeyboardEvent) -> Option<Vec<u8>> {
    let data = event.data();
    let key = data.key();
    let modifiers = data.modifiers();
    let ctrl = modifiers.ctrl();
    let meta = modifiers.meta();
    let alt = modifiers.alt();
    let shift = modifiers.shift();

    let mut bytes = match key {
        Key::Enter => vec![b'\r'],
        Key::Tab => {
            if shift {
                b"\x1b[Z".to_vec()
            } else {
                vec![b'\t']
            }
        }
        Key::Backspace => vec![0x7f],
        Key::Escape => vec![0x1b],
        Key::ArrowUp => b"\x1b[A".to_vec(),
        Key::ArrowDown => b"\x1b[B".to_vec(),
        Key::ArrowRight => b"\x1b[C".to_vec(),
        Key::ArrowLeft => b"\x1b[D".to_vec(),
        Key::Home => b"\x1b[H".to_vec(),
        Key::End => b"\x1b[F".to_vec(),
        Key::Delete => b"\x1b[3~".to_vec(),
        Key::Insert => b"\x1b[2~".to_vec(),
        Key::PageUp => b"\x1b[5~".to_vec(),
        Key::PageDown => b"\x1b[6~".to_vec(),
        Key::Character(text) => {
            if text.is_empty() {
                return None;
            }

            if ctrl || meta {
                vec![control_byte(text.chars().next()?)?]
            } else if alt {
                text.as_bytes().to_vec()
            } else {
                return None;
            }
        }
        _ => return None,
    };

    if alt {
        let mut prefixed = Vec::with_capacity(bytes.len() + 1);
        prefixed.push(0x1b);
        prefixed.extend(bytes);
        bytes = prefixed;
    }

    Some(bytes)
}

fn control_byte(input: char) -> Option<u8> {
    let lower = input.to_ascii_lowercase();
    let byte = match lower {
        '@' | ' ' | '2' => 0,
        'a'..='z' => (lower as u8) - b'a' + 1,
        '[' | '3' => 27,
        '\\' | '4' => 28,
        ']' | '5' => 29,
        '^' | '6' => 30,
        '_' | '7' => 31,
        '8' | '?' => 127,
        _ => return None,
    };

    Some(byte)
}
