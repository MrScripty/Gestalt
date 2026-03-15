use crate::terminal::TerminalSnapshot;
use dioxus::prelude::*;
use dioxus_native::use_wgpu;
use std::sync::Arc;

use super::frame::NativeTerminalFrame;
use super::paint::{NativeTerminalPaintBridge, NativeTerminalPaintSource};

#[component]
pub(crate) fn NativeTerminalBody(terminal: Arc<TerminalSnapshot>, show_caret: bool) -> Element {
    let initial_frame = NativeTerminalFrame::from_snapshot(&terminal, show_caret);
    let bridge = use_hook(move || NativeTerminalPaintBridge::new(initial_frame.clone()));
    let paint_source = {
        let bridge = bridge.clone();
        use_wgpu(move || NativeTerminalPaintSource::new(bridge.clone()))
    };

    {
        let bridge = bridge.clone();
        let terminal = terminal.clone();
        use_effect(move || {
            bridge.update_frame(NativeTerminalFrame::from_snapshot(&terminal, show_caret));
        });
    }

    rsx! {
        canvas {
            class: "terminal-native-canvas",
            "src": paint_source,
        }
    }
}
