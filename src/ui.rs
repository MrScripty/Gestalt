use dioxus::prelude::*;

const STYLE: &str = include_str!("style.css");

#[component]
pub fn App() -> Element {
    rsx! {
        style { "{STYLE}" }
        main { class: "app-shell",
            h1 { "Gestalt" }
            p { "Rust + Dioxus terminal workspace scaffold." }
        }
    }
}
