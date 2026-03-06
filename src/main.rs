fn main() {
    dioxus::LaunchBuilder::desktop()
        .with_cfg(dioxus::desktop::Config::new().with_on_window(|window, _| {
            window.set_title("Gestalt");
            window.set_always_on_top(false);
        }))
        .launch(gestalt::ui::App);
}
