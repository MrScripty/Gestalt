use dioxus::prelude::*;
use dioxus_native::Config;

use super::constants::{DEFAULT_SPIKE_PANE_COUNT, WINDOW_HEIGHT_PX, WINDOW_TITLE, WINDOW_WIDTH_PX};
use super::controller::NativeTerminalController;
use super::demo::TerminalNativeDemo;
use super::gpu_renderer::NativeTerminalGpuShared;

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
        let pane_count = spike_pane_count();
        (0..pane_count)
            .map(|_| NativeTerminalController::spawn_for_current_dir())
            .collect::<Vec<_>>()
    });
    let shared_gpu = use_hook(NativeTerminalGpuShared::default);

    rsx! {
        TerminalNativeDemo {
            panes: controllers.clone(),
            shared_gpu: shared_gpu.clone(),
        }
    }
}

fn spike_pane_count() -> usize {
    std::env::var("GESTALT_NATIVE_SPIKE_PANES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_SPIKE_PANE_COUNT)
}
