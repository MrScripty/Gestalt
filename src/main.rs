mod state;
mod terminal;
mod ui;

fn main() {
    dioxus::launch(ui::App);
}
