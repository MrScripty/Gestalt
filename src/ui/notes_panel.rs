use crate::state::{AppState, Snippet, SnippetId};
use dioxus::prelude::*;
use pulldown_cmark::{Options, Parser, html};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NotesSection {
    Notes,
    Snippets,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NotesViewMode {
    Edit,
    View,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NoteSegment {
    Markdown(String),
    SnippetRef(SnippetId),
}

#[component]
pub(crate) fn NotesPanel(app_state: Signal<AppState>) -> Element {
    let mut section = use_signal(|| NotesSection::Notes);
    let mut view_mode = use_signal(|| NotesViewMode::Edit);
    let mut snippet_picker_open = use_signal(|| false);
    let mut snippet_query = use_signal(String::new);
    let mut focused_snippet_id = use_signal(|| None::<SnippetId>);

    let notes = app_state.read().notes().to_vec();
    let snippets = app_state.read().snippets().to_vec();
    let selected_note_id = app_state
        .read()
        .selected_note_id()
        .or_else(|| notes.first().map(|note| note.id));
    let selected_note = selected_note_id
        .and_then(|id| notes.iter().find(|note| note.id == id))
        .cloned();
    let selected_markdown = selected_note
        .as_ref()
        .map(|note| note.markdown.clone())
        .unwrap_or_default();
    let snippet_query_value = snippet_query.read().trim().to_lowercase();

    let filtered_snippets = snippets
        .iter()
        .filter(|snippet| {
            snippet_query_value.is_empty()
                || snippet
                    .text_snapshot_plain
                    .to_lowercase()
                    .contains(&snippet_query_value)
                || snippet.source_cwd.to_lowercase().contains(&snippet_query_value)
        })
        .cloned()
        .collect::<Vec<_>>();

    rsx! {
        article { class: "notes-card",
            div { class: "notes-head",
                h3 { "Notes" }
                p { "Markdown notes, snippet references, and captured terminal snippets." }
            }

            div { class: "notes-top-tabs", role: "tablist", aria_label: "Notes sections",
                {
                    let is_active = *section.read() == NotesSection::Notes;
                    let class = if is_active {
                        "notes-tab active"
                    } else {
                        "notes-tab"
                    };
                    rsx! {
                        button {
                            class: "{class}",
                            r#type: "button",
                            onclick: move |_| section.set(NotesSection::Notes),
                            "Notes"
                        }
                    }
                }
                {
                    let is_active = *section.read() == NotesSection::Snippets;
                    let class = if is_active {
                        "notes-tab active"
                    } else {
                        "notes-tab"
                    };
                    rsx! {
                        button {
                            class: "{class}",
                            r#type: "button",
                            onclick: move |_| section.set(NotesSection::Snippets),
                            "Snippets"
                        }
                    }
                }
            }

            div { class: "notes-body",
                if *section.read() == NotesSection::Notes {
                    div { class: "notes-editor",
                        div { class: "notes-doc-tabs",
                            for note in notes.clone() {
                                {
                                    let note_id = note.id;
                                    let is_active = Some(note_id) == selected_note_id;
                                    let class = if is_active {
                                        "notes-doc-tab active"
                                    } else {
                                        "notes-doc-tab"
                                    };
                                    rsx! {
                                        button {
                                            key: "note-tab-{note_id}",
                                            class: "{class}",
                                            r#type: "button",
                                            onclick: move |_| app_state.write().select_note(note_id),
                                            "{note.title}"
                                        }
                                    }
                                }
                            }
                            button {
                                class: "notes-doc-add",
                                r#type: "button",
                                onclick: move |_| {
                                    let next_index = app_state.read().notes().len().saturating_add(1);
                                    let title = format!("Note {next_index}");
                                    app_state.write().create_note(title, unix_now_ms());
                                },
                                "New Note"
                            }
                        }

                        if let Some(note_id) = selected_note_id {
                            div { class: "notes-toolbar",
                                {
                                    let edit_class = if *view_mode.read() == NotesViewMode::Edit {
                                        "notes-mode active"
                                    } else {
                                        "notes-mode"
                                    };
                                    rsx! {
                                        button {
                                            class: "{edit_class}",
                                            r#type: "button",
                                            onclick: move |_| view_mode.set(NotesViewMode::Edit),
                                            "Edit"
                                        }
                                    }
                                }
                                {
                                    let view_class = if *view_mode.read() == NotesViewMode::View {
                                        "notes-mode active"
                                    } else {
                                        "notes-mode"
                                    };
                                    rsx! {
                                        button {
                                            class: "{view_class}",
                                            r#type: "button",
                                            onclick: move |_| view_mode.set(NotesViewMode::View),
                                            "View"
                                        }
                                    }
                                }
                                button {
                                    class: "notes-insert-ref",
                                    r#type: "button",
                                    onclick: move |_| {
                                        let is_open = *snippet_picker_open.read();
                                        snippet_picker_open.set(!is_open);
                                    },
                                    "Insert Snippet Ref"
                                }
                            }

                            if *snippet_picker_open.read() {
                                div { class: "notes-snippet-picker",
                                    input {
                                        class: "notes-filter-input",
                                        placeholder: "Search snippets",
                                        value: "{snippet_query.read()}",
                                        oninput: move |event| snippet_query.set(event.value()),
                                    }
                                    div { class: "notes-snippet-picker-list",
                                        if filtered_snippets.is_empty() {
                                            p { class: "notes-empty", "No snippets match this filter." }
                                        } else {
                                            for snippet in filtered_snippets.clone() {
                                                {
                                                    let snippet_id = snippet.id;
                                                    let title = snippet_preview(&snippet.text_snapshot_plain, 80);
                                                    rsx! {
                                                        button {
                                                            key: "snippet-pick-{snippet_id}",
                                                            class: "notes-snippet-pick-btn",
                                                            r#type: "button",
                                                            onclick: move |_| {
                                                                app_state.write().append_note_snippet_reference(
                                                                    note_id,
                                                                    snippet_id,
                                                                    unix_now_ms(),
                                                                );
                                                                snippet_picker_open.set(false);
                                                                snippet_query.set(String::new());
                                                            },
                                                            "{title}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if *view_mode.read() == NotesViewMode::Edit {
                                textarea {
                                    class: "notes-markdown-input",
                                    rows: "16",
                                    placeholder: "Write markdown notes here...",
                                    value: "{selected_markdown}",
                                    oninput: move |event| {
                                        app_state.write().update_note_markdown(
                                            note_id,
                                            event.value(),
                                            unix_now_ms(),
                                        );
                                    },
                                }
                            } else {
                                div { class: "notes-markdown-view",
                                    for segment in split_note_segments(&selected_markdown) {
                                        {
                                            match segment {
                                                NoteSegment::Markdown(markdown) => {
                                                    if markdown.trim().is_empty() {
                                                        rsx! { div {} }
                                                    } else {
                                                        let rendered = markdown_to_html(&markdown);
                                                        rsx! {
                                                            div {
                                                                class: "notes-markdown-segment",
                                                                dangerous_inner_html: "{rendered}",
                                                            }
                                                        }
                                                    }
                                                }
                                                NoteSegment::SnippetRef(snippet_id) => {
                                                    let label = snippet_label(&snippets, snippet_id);
                                                    rsx! {
                                                        button {
                                                            key: "note-ref-{snippet_id}",
                                                            class: "notes-snippet-ref",
                                                            r#type: "button",
                                                            onclick: move |_| {
                                                                let _ = app_state.write().promote_snippet(snippet_id);
                                                                focused_snippet_id.set(Some(snippet_id));
                                                                section.set(NotesSection::Snippets);
                                                            },
                                                            "{label}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            p { class: "notes-empty", "No notes yet." }
                        }
                    }
                } else {
                    div { class: "notes-snippets",
                        div { class: "notes-snippets-filter",
                            input {
                                class: "notes-filter-input",
                                placeholder: "Search snippets",
                                value: "{snippet_query.read()}",
                                oninput: move |event| snippet_query.set(event.value()),
                            }
                        }
                        if filtered_snippets.is_empty() {
                            if snippets.is_empty() {
                                p { class: "notes-empty", "No snippets yet. Highlight terminal text and press Insert, then S." }
                            } else {
                                p { class: "notes-empty", "No snippets match this filter." }
                            }
                        } else {
                            div { class: "notes-snippet-list",
                                for snippet in filtered_snippets {
                                    {
                                        let snippet_id = snippet.id;
                                        let is_focused = *focused_snippet_id.read() == Some(snippet_id);
                                        let class = if is_focused {
                                            "notes-snippet-item focused"
                                        } else {
                                            "notes-snippet-item"
                                        };
                                        let preview = snippet_preview(&snippet.text_snapshot_plain, 220);
                                        let row_range = format!(
                                            "rows {}-{}",
                                            snippet.log_ref.start_row,
                                            snippet.log_ref.end_row
                                        );
                                        rsx! {
                                            button {
                                                key: "snippet-item-{snippet_id}",
                                                class: "{class}",
                                                r#type: "button",
                                                onclick: move |_| {
                                                    let _ = app_state.write().promote_snippet(snippet_id);
                                                    focused_snippet_id.set(Some(snippet_id));
                                                },
                                                div { class: "notes-snippet-meta",
                                                    span { class: "snippet-id", "Snippet #{snippet_id}" }
                                                    span { class: "snippet-status", "{snippet.embedding_status.label()}" }
                                                }
                                                p { class: "snippet-preview", "{preview}" }
                                                p { class: "snippet-source", "{snippet.source_cwd} | {row_range}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn split_note_segments(markdown: &str) -> Vec<NoteSegment> {
    const PREFIX: &str = "[[snippet:";
    let mut segments = Vec::new();
    let mut remainder = markdown;
    while let Some(prefix_idx) = remainder.find(PREFIX) {
        let (head, candidate) = remainder.split_at(prefix_idx);
        if !head.is_empty() {
            segments.push(NoteSegment::Markdown(head.to_string()));
        }
        let candidate = &candidate[PREFIX.len()..];
        let Some(end_idx) = candidate.find("]]") else {
            segments.push(NoteSegment::Markdown(format!("{PREFIX}{candidate}")));
            return segments;
        };
        let raw_id = &candidate[..end_idx];
        if let Ok(snippet_id) = raw_id.trim().parse::<SnippetId>() {
            segments.push(NoteSegment::SnippetRef(snippet_id));
        } else {
            segments.push(NoteSegment::Markdown(format!("{PREFIX}{raw_id}]]")));
        }
        remainder = &candidate[end_idx + 2..];
    }
    if !remainder.is_empty() {
        segments.push(NoteSegment::Markdown(remainder.to_string()));
    }
    segments
}

fn markdown_to_html(markdown: &str) -> String {
    let mut html_output = String::new();
    let parser = Parser::new_ext(markdown, Options::all());
    html::push_html(&mut html_output, parser);
    html_output
}

fn snippet_preview(text: &str, max_chars: usize) -> String {
    let normalized = text.replace('\n', " ").trim().to_string();
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let mut preview = String::new();
    let mut chars = normalized.chars();
    for _ in 0..max_chars {
        let Some(ch) = chars.next() else {
            return normalized;
        };
        preview.push(ch);
    }
    if chars.next().is_some() {
        preview.push_str("...");
    }
    preview
}

fn snippet_label(snippets: &[Snippet], snippet_id: SnippetId) -> String {
    snippets
        .iter()
        .find(|snippet| snippet.id == snippet_id)
        .map(|snippet| format!("Snippet #{snippet_id}: {}", snippet_preview(&snippet.text_snapshot_plain, 40)))
        .unwrap_or_else(|| format!("Snippet #{snippet_id}"))
}

fn unix_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
