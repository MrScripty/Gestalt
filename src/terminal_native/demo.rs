use dioxus::prelude::*;
use super::controller::NativeTerminalController;
use super::input::special_key_event_to_bytes;
use super::paint::NativeTerminalPaintSource;
use super::{
    APP_ROOT_STYLE, CANVAS_STYLE, INPUT_OVERLAY_STYLE, STATUS_BAR_STYLE, STATUS_HINT_STYLE,
    STATUS_HINT_TEXT, STATUS_TITLE_STYLE, STATUS_TITLE_TEXT, TERMINAL_SURFACE_STYLE,
};
use dioxus_native::use_wgpu;

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
            style: APP_ROOT_STYLE,
            div {
                style: STATUS_BAR_STYLE,
                div {
                    style: STATUS_TITLE_STYLE,
                    "{STATUS_TITLE_TEXT}"
                }
                div {
                    "{status_line(&controller)}"
                }
                div {
                    style: STATUS_HINT_STYLE,
                    "{STATUS_HINT_TEXT}"
                }
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
