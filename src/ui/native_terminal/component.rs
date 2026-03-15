use crate::terminal::TerminalSnapshot;
use crate::terminal_native::TerminalFrame;
use dioxus::prelude::*;
use dioxus_native::use_wgpu;
use std::rc::Rc;
use std::sync::Arc;

use super::frame::NativeTerminalFrame;
use super::paint::{NativeTerminalPaintBridge, NativeTerminalPaintSource};

const INPUT_SINK_STYLE: &str = "position: absolute; inset: 0; width: 100%; height: 100%; opacity: 0; background: transparent; color: transparent; caret-color: transparent; border: none; outline: none;";

#[component]
fn NativeTerminalPaintHost(
    terminal: Arc<TerminalSnapshot>,
    native_frame: Option<Arc<TerminalFrame>>,
    show_caret: bool,
) -> Element {
    let initial_frame = native_frame
        .as_ref()
        .map(|frame| {
            NativeTerminalFrame::from_native_or_snapshot(frame.as_ref(), &terminal, show_caret)
        })
        .unwrap_or_else(|| NativeTerminalFrame::from_snapshot(&terminal, show_caret));
    let bridge = use_hook(move || NativeTerminalPaintBridge::new(initial_frame.clone()));
    let paint_source = {
        let bridge = bridge.clone();
        use_wgpu(move || NativeTerminalPaintSource::new(bridge.clone()))
    };
    let next_frame = native_frame
        .as_ref()
        .map(|frame| NativeTerminalFrame::from_native_or_snapshot(frame.as_ref(), &terminal, show_caret))
        .unwrap_or_else(|| NativeTerminalFrame::from_snapshot(&terminal, show_caret));
    bridge.update_frame(next_frame);

    rsx! {
        canvas {
            class: "terminal-native-canvas",
            "src": paint_source,
        }
    }
}

#[component]
pub(crate) fn NativeTerminalBody(
    terminal: Arc<TerminalSnapshot>,
    native_frame: Option<Arc<TerminalFrame>>,
    show_caret: bool,
    input_value: String,
    onclick: EventHandler<MouseEvent>,
    onfocus: EventHandler<FocusEvent>,
    onblur: EventHandler<FocusEvent>,
    onkeydown: EventHandler<KeyboardEvent>,
    oninput: EventHandler<FormEvent>,
    onpaste: EventHandler<ClipboardEvent>,
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
            NativeTerminalPaintHost {
                terminal: terminal.clone(),
                native_frame: native_frame.clone(),
                show_caret: show_caret,
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
            }
        }
    }
}
