use dioxus::prelude::*;
use dioxus_native::Config;

use super::constants::{WINDOW_HEIGHT_PX, WINDOW_TITLE, WINDOW_WIDTH_PX};
use super::controller::NativeTerminalController;
use super::demo::TerminalNativeDemo;

pub fn launch_terminal_native_spike() {
    let attributes = dioxus_native::WindowAttributes::default()
        .with_title(WINDOW_TITLE)
        .with_inner_size(dioxus_native::LogicalSize::new(
            WINDOW_WIDTH_PX,
            WINDOW_HEIGHT_PX,
        ));

    dioxus_native::launch_cfg(
        TerminalNativeSpikeApp,
        Vec::new(),
        vec![
            Box::new(Config::new().with_window_attributes(attributes.clone())),
            Box::new(attributes),
        ],
    );
}

#[component]
fn TerminalNativeSpikeApp() -> Element {
    let controllers = use_hook(|| {
        [
            NativeTerminalController::spawn_for_current_dir(),
            NativeTerminalController::spawn_for_current_dir(),
        ]
    });

    rsx! {
        TerminalNativeDemo {
            left: controllers[0].clone(),
            right: controllers[1].clone(),
        }
    }
}
