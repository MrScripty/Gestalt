use dioxus::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaletteRow {
    pub name: String,
    pub description: String,
    pub prompt_preview: String,
}

#[component]
pub(crate) fn InsertCommandPalette(
    query: String,
    highlighted_index: usize,
    rows: Vec<PaletteRow>,
) -> Element {
    let query_label = if query.is_empty() {
        "Type to filter commands".to_string()
    } else {
        format!("Query: {query}")
    };

    rsx! {
        div { class: "insert-command-palette",
            div { class: "insert-command-palette-head",
                h4 { "Insert Command" }
                p { "{query_label}" }
            }

            if rows.is_empty() {
                p { class: "insert-command-empty", "No commands match. Open the Commands panel to create one." }
            } else {
                div { class: "insert-command-list",
                    for (index, row) in rows.into_iter().enumerate() {
                        {
                            let item_class = if index == highlighted_index {
                                "insert-command-item selected"
                            } else {
                                "insert-command-item"
                            };
                            rsx! {
                                div { class: "{item_class}",
                                    p { class: "insert-command-name", "{row.name}" }
                                    if !row.description.trim().is_empty() {
                                        p { class: "insert-command-description", "{row.description}" }
                                    }
                                    p { class: "insert-command-prompt", "{row.prompt_preview}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
