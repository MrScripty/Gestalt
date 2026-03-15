use crate::terminal::TerminalSnapshot;
use crate::terminal_native::TerminalFrame;
use dioxus::prelude::*;
use dioxus_native::use_wgpu;
use std::sync::Arc;

use super::frame::NativeTerminalFrame;
use super::paint::{NativeTerminalPaintBridge, NativeTerminalPaintSource};

#[component]
pub(crate) fn NativeTerminalBody(
    terminal: Arc<TerminalSnapshot>,
    native_frame: Option<Arc<TerminalFrame>>,
    show_caret: bool,
) -> Element {
    let initial_frame = native_frame
        .as_ref()
        .map(|frame| NativeTerminalFrame::from_native_frame(frame.as_ref(), show_caret))
        .unwrap_or_else(|| NativeTerminalFrame::from_snapshot(&terminal, show_caret));
    let bridge = use_hook(move || NativeTerminalPaintBridge::new(initial_frame.clone()));
    let paint_source = {
        let bridge = bridge.clone();
        use_wgpu(move || NativeTerminalPaintSource::new(bridge.clone()))
    };

    {
        let bridge = bridge.clone();
        let terminal = terminal.clone();
        let native_frame = native_frame.clone();
        use_effect(move || {
            let next_frame = native_frame
                .as_ref()
                .map(|frame| NativeTerminalFrame::from_native_frame(frame.as_ref(), show_caret))
                .unwrap_or_else(|| NativeTerminalFrame::from_snapshot(&terminal, show_caret));
            bridge.update_frame(next_frame);
        });
    }

    rsx! {
        canvas {
            class: "terminal-native-canvas",
            "src": paint_source,
        }
    }
}
