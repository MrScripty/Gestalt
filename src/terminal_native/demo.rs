use super::controller::NativeTerminalController;
use super::gpu_renderer::NativeTerminalGpuShared;
use super::input::special_key_event_to_bytes;
use super::paint::NativeTerminalPaintSource;
use super::{
    APP_ROOT_STYLE, CANVAS_STYLE, INPUT_OVERLAY_STYLE, PANE_CARD_STYLE, PANE_GRID_STYLE,
    PANE_HEADER_STYLE, PANE_META_STYLE, PANE_TITLE_STYLE, STATUS_BAR_STYLE, STATUS_HINT_STYLE,
    STATUS_HINT_TEXT, STATUS_TITLE_STYLE, STATUS_TITLE_TEXT, TERMINAL_SURFACE_STYLE,
};
use dioxus::prelude::*;
use dioxus_native::use_wgpu;

#[component]
pub fn TerminalNativeDemo(
    left: NativeTerminalController,
    right: NativeTerminalController,
    shared_gpu: NativeTerminalGpuShared,
) -> Element {
    let status = format!("left:{}  right:{}", status_line(&left), status_line(&right));

    rsx! {
        div {
            style: APP_ROOT_STYLE,
            div {
                style: STATUS_BAR_STYLE,
                div {
                    style: STATUS_TITLE_STYLE,
                    "{STATUS_TITLE_TEXT}"
                }
                div {
                    "{status}"
                }
                div {
                    style: STATUS_HINT_STYLE,
                    "{STATUS_HINT_TEXT}"
                }
            }
            div { style: PANE_GRID_STYLE,
                TerminalNativePane {
                    title: "pane-1",
                    controller: left,
                    shared_gpu: shared_gpu.clone(),
                }
                TerminalNativePane {
                    title: "pane-2",
                    controller: right,
                    shared_gpu,
                }
            }
        }
    }
}

#[component]
fn TerminalNativePane(
    title: &'static str,
    controller: NativeTerminalController,
    shared_gpu: NativeTerminalGpuShared,
) -> Element {
    let mut input_buffer = use_signal(String::new);
    let paint_controller = controller.clone();
    let paint_gpu = shared_gpu.clone();
    let key_controller = controller.clone();
    let input_controller = controller.clone();
    let paint_source_id = use_wgpu(move || {
        NativeTerminalPaintSource::new(paint_controller.clone(), paint_gpu.clone())
    });
    let input_buffer_value = input_buffer.read().clone();

    rsx! {
        div { style: PANE_CARD_STYLE,
            div { style: PANE_HEADER_STYLE,
                div { style: PANE_TITLE_STYLE, "{title}" }
                div { style: PANE_META_STYLE, "{status_line(&controller)}" }
            }
            div {
                style: TERMINAL_SURFACE_STYLE,
                canvas {
                    style: CANVAS_STYLE,
                    "src": paint_source_id,
                }
                input {
                    r#type: "text",
                    tabindex: "0",
                    autofocus: "true",
                    spellcheck: "false",
                    value: "{input_buffer_value}",
                    style: INPUT_OVERLAY_STYLE,
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
