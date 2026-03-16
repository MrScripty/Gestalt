use crate::terminal::TerminalSnapshot;
use crate::state::SessionId;
use crate::terminal_native::TerminalFrame;
use dioxus::prelude::*;
use dioxus_native::use_wgpu;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use super::frame::NativeTerminalFrame;
use super::paint::{NativeTerminalPaintBridge, NativeTerminalPaintSource};

const INPUT_SINK_STYLE: &str = "position: absolute; inset: 0; width: 100%; height: 100%; opacity: 0; background: transparent; color: transparent; caret-color: transparent; border: none; outline: none;";

#[component]
fn NativeTerminalPaintHost(
    session_id: SessionId,
    terminal: Arc<TerminalSnapshot>,
    native_frame: Option<Arc<TerminalFrame>>,
    show_caret: bool,
    ui_scale: f64,
    visible_rows: u16,
    local_scroll_offset: u16,
    native_terminal_surface_cells: Signal<HashMap<SessionId, (u16, u16)>>,
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
            )
        })
        .unwrap_or_else(|| {
            NativeTerminalFrame::from_snapshot(
                &terminal,
                show_caret,
                visible_rows,
                local_scroll_offset,
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
            )
        })
        .unwrap_or_else(|| {
            NativeTerminalFrame::from_snapshot(
                &terminal,
                show_caret,
                visible_rows,
                local_scroll_offset,
            )
        });
    bridge.update_frame(next_frame, ui_scale as f32);

    {
        let bridge = bridge.clone();
        let mut native_terminal_surface_cells = native_terminal_surface_cells;
        use_future(move || {
            let bridge = bridge.clone();
            async move {
                let mut last_surface_cells = None;
                loop {
                    tokio::time::sleep(Duration::from_millis(120)).await;
                    let next_surface_cells = bridge.surface_cells();
                    if next_surface_cells == last_surface_cells {
                        continue;
                    }
                    last_surface_cells = next_surface_cells;
                    let mut surface_cells = native_terminal_surface_cells.write();
                    if let Some(cells) = next_surface_cells {
                        surface_cells.insert(session_id, cells);
                    } else {
                        surface_cells.remove(&session_id);
                    }
                }
            }
        });
    }

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
    local_scroll_offset: u16,
    native_terminal_surface_cells: Signal<HashMap<SessionId, (u16, u16)>>,
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
    let mut input_mount = use_signal(|| None::<Rc<MountedData>>);

    {
        let input_mount = input_mount.read().clone();
        use_effect(move || {
            if !show_caret {
                return;
            }
            let Some(input_mount) = input_mount.clone() else {
                return;
            };
            spawn(async move {
                let _ = input_mount.set_focus(true).await;
            });
        });
    }

    {
        let mut native_terminal_surface_cells = native_terminal_surface_cells;
        use_drop(move || {
            native_terminal_surface_cells.write().remove(&session_id);
        });
    }

    rsx! {
        div {
            class: "terminal-native-layer",
            onclick: move |event| {
                onclick.call(event);
                if let Some(input_mount) = input_mount.read().clone() {
                    spawn(async move {
                        let _ = input_mount.set_focus(true).await;
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
                local_scroll_offset: local_scroll_offset,
                native_terminal_surface_cells: native_terminal_surface_cells,
            }
            input {
                r#type: "text",
                tabindex: "0",
                spellcheck: "false",
                value: "{input_value}",
                style: INPUT_SINK_STYLE,
                onmounted: move |event| {
                    let mount = event.data();
                    input_mount.set(Some(mount.clone()));
                    onviewportmounted.call(mount.clone());
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
