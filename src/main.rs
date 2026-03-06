fn main() {
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
