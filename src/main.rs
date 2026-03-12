fn main() {
    launch_app();
}

#[cfg(feature = "native-renderer")]
fn launch_app() {
    use dioxus_native::{Config, Features, Limits, LogicalSize, WindowAttributes};

    let attributes = WindowAttributes::default()
        .with_title("Gestalt")
        .with_inner_size(LogicalSize::new(1600.0, 980.0));
    let limits = Limits {
        max_push_constant_size: 16,
        ..Limits::default()
    };

    dioxus_native::launch_cfg(
        gestalt::ui::App,
        Vec::new(),
        vec![
            Box::new(Features::PUSH_CONSTANTS),
            Box::new(limits),
            Box::new(Config::new().with_window_attributes(attributes.clone())),
            Box::new(attributes),
        ],
    );
}

#[cfg(not(feature = "native-renderer"))]
fn launch_app() {
    let icon = dioxus::desktop::tao::window::Icon::from_rgba(
        include_bytes!("../assets/Gestalt_small.rgba").to_vec(),
        128,
        128,
    )
    .expect("Gestalt icon bytes should decode as 128x128 RGBA");

    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            dioxus::desktop::Config::new()
                .with_icon(icon)
                .with_on_window(|window, _| {
                    window.set_title("Gestalt");
                    window.set_always_on_top(false);
                }),
        )
        .launch(gestalt::ui::App);
}
