use super::controller::NativeTerminalController;
use super::gpu_renderer::NativeTerminalGpuShared;
use super::input::special_key_event_to_bytes;
use super::paint::NativeTerminalPaintSource;
use super::{
    ACTIVE_PANE_STACK_STYLE, APP_ROOT_STYLE, BACKGROUND_PANE_BODY_STYLE,
    BACKGROUND_PANE_BUTTON_STYLE, BACKGROUND_PANE_LIST_STYLE, CANVAS_STYLE, INPUT_OVERLAY_STYLE,
    PANE_CARD_STYLE, PANE_HEADER_STYLE, PANE_LAYOUT_STYLE, PANE_META_STYLE,
    PANE_SWITCH_BUTTON_ACTIVE_STYLE, PANE_SWITCH_BUTTON_STYLE, PANE_SWITCHER_STYLE,
    PANE_TITLE_STYLE, STATUS_BAR_STYLE, STATUS_HINT_STYLE, STATUS_HINT_TEXT, STATUS_TITLE_STYLE,
    STATUS_TITLE_TEXT, TERMINAL_SURFACE_STYLE,
};
use dioxus::prelude::*;
use dioxus_native::use_wgpu;

#[component]
pub fn TerminalNativeDemo(
    panes: Vec<NativeTerminalController>,
    shared_gpu: NativeTerminalGpuShared,
) -> Element {
    let mut active_index = use_signal(|| 0_usize);
    let pane_summaries = panes
        .iter()
        .enumerate()
        .map(|(index, controller)| PaneSummary {
            index,
            title: pane_title(index),
            status_line: status_line(controller),
            summary: controller.summary(),
        })
        .collect::<Vec<_>>();
    let pane_count = pane_summaries.len();
    let active = (*active_index.read()).min(pane_count.saturating_sub(1));
    let active_summary = pane_summaries.get(active).cloned();
    let status = if let Some(summary) = &active_summary {
        format!(
            "visible={}  hidden={}  active:{}",
            1,
            pane_count.saturating_sub(1),
            summary.status_line
        )
    } else {
        "no panes".to_string()
    };

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
            div { style: PANE_LAYOUT_STYLE,
                div { style: ACTIVE_PANE_STACK_STYLE,
                    div { style: PANE_SWITCHER_STYLE,
                        for pane in &pane_summaries {
                            button {
                                key: "{pane.index}",
                                style: if pane.index == active {
                                    PANE_SWITCH_BUTTON_ACTIVE_STYLE
                                } else {
                                    PANE_SWITCH_BUTTON_STYLE
                                },
                                onclick: {
                                    let index = pane.index;
                                    move |_| active_index.set(index)
                                },
                                "{pane.title}"
                            }
                        }
                    }
                    if let Some(controller) = panes.get(active).cloned() {
                        if let Some(summary) = active_summary.clone() {
                            TerminalNativePane {
                                title: summary.title,
                                controller,
                                shared_gpu: shared_gpu.clone(),
                            }
                        }
                    }
                }
                div { style: BACKGROUND_PANE_LIST_STYLE,
                    for pane in &pane_summaries {
                        BackgroundPaneCard {
                            key: "{pane.index}",
                            title: pane.title.clone(),
                            summary: pane.summary,
                            is_visible: pane.index == active,
                            on_show: {
                                let index = pane.index;
                                move |_| active_index.set(index)
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TerminalNativePane(
    title: String,
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

#[component]
fn BackgroundPaneCard(
    title: String,
    summary: super::NativeTerminalSessionSummary,
    is_visible: bool,
    on_show: EventHandler<MouseEvent>,
) -> Element {
    let state_text = if is_visible {
        "visible"
    } else if summary.closed {
        "closed"
    } else {
        "running in background"
    };

    rsx! {
        div { style: PANE_CARD_STYLE,
            div { style: PANE_HEADER_STYLE,
                div { style: PANE_TITLE_STYLE, "{title}" }
                div { style: PANE_META_STYLE, "{summary.cols}x{summary.rows}  revision={summary.revision}" }
            }
            div { style: BACKGROUND_PANE_BODY_STYLE,
                div { "{state_text}" }
                if !is_visible {
                    button {
                        style: BACKGROUND_PANE_BUTTON_STYLE,
                        onclick: move |event| on_show.call(event),
                        "show pane"
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
struct PaneSummary {
    index: usize,
    title: String,
    status_line: String,
    summary: super::NativeTerminalSessionSummary,
}

fn pane_title(index: usize) -> String {
    format!("pane-{}", index + 1)
}

fn status_line(controller: &NativeTerminalController) -> String {
    let summary = controller.summary();
    format!(
        "{}x{}  revision={}  closed={}",
        summary.cols, summary.rows, summary.revision, summary.closed
    )
}
