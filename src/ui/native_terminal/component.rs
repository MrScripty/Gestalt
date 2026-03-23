use crate::terminal::TerminalSnapshot;
use crate::state::SessionId;
use crate::terminal_native::TerminalFrame;
use dioxus::prelude::*;
use dioxus_native::use_wgpu;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use super::frame::NativeTerminalFrame;
use super::paint::{NativeTerminalPaintBridge, NativeTerminalPaintSource};
use super::surface_sync::use_native_surface_metric_sync;

const INPUT_SINK_STYLE: &str = "position: absolute; inset: 0; width: 100%; height: 100%; opacity: 0; background: transparent; color: transparent; caret-color: transparent; border: none; outline: none;";

#[component]
fn NativeTerminalPaintHost(
    session_id: SessionId,
    terminal: Arc<TerminalSnapshot>,
    native_frame: Option<Arc<TerminalFrame>>,
    show_caret: bool,
    ui_scale: f64,
    visible_rows: u16,
    visible_cols: u16,
    local_scroll_offset: u16,
    horizontal_scroll_offset: u16,
    native_terminal_surface_cells: Signal<HashMap<SessionId, (u16, u16)>>,
    native_terminal_surface_sizes: Signal<HashMap<SessionId, (f64, f64)>>,
) -> Element {
    let initial_frame = native_frame
        .as_ref()
        .map(|frame| {
            NativeTerminalFrame::from_native_or_snapshot(
                frame.as_ref(),
                &terminal,
                show_caret,
                visible_rows,
                local_scroll_offset,
                visible_cols,
                horizontal_scroll_offset,
            )
        })
        .unwrap_or_else(|| {
            NativeTerminalFrame::from_snapshot(
                &terminal,
                show_caret,
                visible_rows,
                local_scroll_offset,
                visible_cols,
                horizontal_scroll_offset,
            )
        });
    let bridge =
        use_hook(move || NativeTerminalPaintBridge::new(initial_frame.clone(), ui_scale as f32));
    let paint_source = {
        let bridge = bridge.clone();
        use_wgpu(move || NativeTerminalPaintSource::new(bridge.clone()))
    };
    let next_frame = native_frame
        .as_ref()
        .map(|frame| {
            NativeTerminalFrame::from_native_or_snapshot(
                frame.as_ref(),
                &terminal,
                show_caret,
                visible_rows,
                local_scroll_offset,
                visible_cols,
                horizontal_scroll_offset,
            )
        })
        .unwrap_or_else(|| {
            NativeTerminalFrame::from_snapshot(
                &terminal,
                show_caret,
                visible_rows,
                local_scroll_offset,
                visible_cols,
                horizontal_scroll_offset,
            )
        });
    bridge.update_frame(next_frame, ui_scale as f32);

    use_native_surface_metric_sync(
        session_id,
        bridge.clone(),
        native_terminal_surface_cells,
        native_terminal_surface_sizes,
    );

    rsx! {
        canvas {
            class: "terminal-native-canvas",
            "src": paint_source,
        }
    }
}

#[component]
pub(crate) fn NativeTerminalBody(
    session_id: SessionId,
    terminal: Arc<TerminalSnapshot>,
    native_frame: Option<Arc<TerminalFrame>>,
    show_caret: bool,
    ui_scale: f64,
    visible_rows: u16,
    visible_cols: u16,
    local_scroll_offset: u16,
    horizontal_scroll_offset: u16,
    native_terminal_surface_cells: Signal<HashMap<SessionId, (u16, u16)>>,
    native_terminal_surface_sizes: Signal<HashMap<SessionId, (f64, f64)>>,
    input_value: String,
    onviewportmounted: EventHandler<Rc<MountedData>>,
    onclick: EventHandler<MouseEvent>,
    onfocus: EventHandler<FocusEvent>,
    onblur: EventHandler<FocusEvent>,
    onkeydown: EventHandler<KeyboardEvent>,
    oninput: EventHandler<FormEvent>,
    onpaste: EventHandler<ClipboardEvent>,
    onwheel: EventHandler<WheelEvent>,
    onmouseenter: EventHandler<MouseEvent>,
    onmouseleave: EventHandler<MouseEvent>,
) -> Element {
    let _ = &onblur;
    let mut layer_mount = use_signal(|| None::<Rc<MountedData>>);
    let mut input_mount = use_signal(|| None::<Rc<MountedData>>);

    {
        let layer_mount = layer_mount;
        let input_mount = input_mount;
        use_effect(move || {
            if !show_caret {
                return;
            }
            if let Some(input_mount) = input_mount.read().clone() {
                spawn(async move {
                    let _ = input_mount.set_focus(true).await;
                });
                return;
            }
            if let Some(layer_mount) = layer_mount.read().clone() {
                spawn(async move {
                    let _ = layer_mount.set_focus(true).await;
                });
            }
        });
    }

    rsx! {
        div {
            class: "terminal-native-layer",
            onmounted: move |event| {
                let mount = event.data();
                layer_mount.set(Some(mount.clone()));
                onviewportmounted.call(mount.clone());
            },
            onclick: move |event| {
                onclick.call(event);
                if let Some(input_mount) = input_mount.read().clone() {
                    spawn(async move {
                        let _ = input_mount.set_focus(true).await;
                    });
                } else if let Some(layer_mount) = layer_mount.read().clone() {
                    spawn(async move {
                        let _ = layer_mount.set_focus(true).await;
                    });
                }
            },
            onwheel: move |event| onwheel.call(event),
            onmouseenter: move |event| onmouseenter.call(event),
            onmouseleave: move |event| onmouseleave.call(event),
            NativeTerminalPaintHost {
                session_id: session_id,
                terminal: terminal.clone(),
                native_frame: native_frame.clone(),
                show_caret: show_caret,
                ui_scale: ui_scale,
                visible_rows: visible_rows,
                visible_cols: visible_cols,
                local_scroll_offset: local_scroll_offset,
                horizontal_scroll_offset: horizontal_scroll_offset,
                native_terminal_surface_cells: native_terminal_surface_cells,
                native_terminal_surface_sizes: native_terminal_surface_sizes,
            }
            input {
                r#type: "text",
                tabindex: "0",
                autofocus: "true",
                spellcheck: "false",
                value: "{input_value}",
                style: INPUT_SINK_STYLE,
                onmounted: move |event| {
                    let mount = event.data();
                    input_mount.set(Some(mount.clone()));
                    if show_caret {
                        spawn(async move {
                            let _ = mount.set_focus(true).await;
                        });
                    }
                },
                onclick: move |event| {
                    onclick.call(event);
                    if let Some(input_mount) = input_mount.read().clone() {
                        spawn(async move {
                            let _ = input_mount.set_focus(true).await;
                        });
                    }
                },
                onfocus: move |event| onfocus.call(event),
                onkeydown: move |event| onkeydown.call(event),
                oninput: move |event| oninput.call(event),
                onpaste: move |event| onpaste.call(event),
                onwheel: move |event| onwheel.call(event),
            }
        }
    }
}
