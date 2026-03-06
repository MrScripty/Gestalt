use crate::state::{AppState, Snippet, SnippetId};
use dioxus::prelude::*;
use pulldown_cmark::{Options, Parser, html};
use std::time::{SystemTime, UNIX_EPOCH};

const SNIPPET_DELETE_HOLD_MS: u64 = 1_000;

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
    let mut view_mode = use_signal(|| NotesViewMode::Edit);
    let mut snippet_query = use_signal(String::new);
    let mut focused_snippet_id = use_signal(|| None::<SnippetId>);
    let mut deleting_snippet_hold = use_signal(|| None::<SnippetId>);
    let mut deleting_snippet_hold_nonce = use_signal(|| 0_u64);

    let active_group_id = app_state.read().active_group_id();
    let notes = active_group_id
        .map(|group_id| {
            app_state
                .read()
                .notes_for_group(group_id)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let snippets = app_state.read().snippets().to_vec();
    let selected_note_id = active_group_id
        .and_then(|group_id| app_state.read().selected_note_id_for_group(group_id));
    let selected_note = selected_note_id
        .and_then(|note_id| app_state.read().note_by_id(note_id).cloned());
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
        .take(8)
        .cloned()
        .collect::<Vec<_>>();

    rsx! {
        article { class: "notes-card",
            div { class: "notes-control-row",
                select {
                    class: "notes-note-select",
                    value: "{selected_note_id.map(|id| id.to_string()).unwrap_or_default()}",
                    disabled: notes.is_empty(),
                    onchange: move |event| {
                        if let Ok(note_id) = event.value().parse::<u64>() {
                            app_state.write().select_note(note_id);
                        }
                    },
                    if notes.is_empty() {
                        option { value: "", "-- No Notes For This Path --" }
                    } else {
                        for note in notes.clone() {
                            option {
                                key: "note-select-{note.id}",
                                value: "{note.id}",
                                "{note.title}"
                            }
                        }
                    }
                }

                button {
                    class: "notes-icon-btn",
                    r#type: "button",
                    title: "Create note",
                    aria_label: "Create note",
                    onclick: move |_| {
                        if let Some(group_id) = active_group_id {
                            let next_index = app_state
                                .read()
                                .notes_for_group(group_id)
                                .len()
                                .saturating_add(1);
                            let title = format!("Note {next_index}");
                            app_state
                                .write()
                                .create_note_for_group(group_id, title, unix_now_ms());
                            view_mode.set(NotesViewMode::Edit);
                        }
                    },
                    {plus_icon()}
                }

                {
                    let is_view_mode = *view_mode.read() == NotesViewMode::View;
                    let mut view_mode = view_mode;
                    rsx! {
                        button {
                            class: if is_view_mode {
                                "notes-icon-btn active"
                            } else {
                                "notes-icon-btn"
                            },
                            r#type: "button",
                            title: if is_view_mode {
                                "Switch to edit mode"
                            } else {
                                "Switch to view mode"
                            },
                            aria_label: if is_view_mode {
                                "Switch to edit mode"
                            } else {
                                "Switch to view mode"
                            },
                            disabled: selected_note_id.is_none(),
                            onclick: move |_| {
                                let next_mode = if *view_mode.read() == NotesViewMode::View {
                                    NotesViewMode::Edit
                                } else {
                                    NotesViewMode::View
                                };
                                view_mode.set(next_mode);
                            },
                            {eye_icon(is_view_mode)}
                        }
                    }
                }
            }

            div { class: "notes-content",
                if let Some(note_id) = selected_note_id {
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
                                            let class = if snippet_exists(&snippets, snippet_id) {
                                                "notes-snippet-ref"
                                            } else {
                                                "notes-snippet-ref missing"
                                            };
                                            rsx! {
                                                button {
                                                    key: "note-ref-{snippet_id}",
                                                    class: "{class}",
                                                    r#type: "button",
                                                    onclick: move |_| {
                                                        let _ = app_state.write().promote_snippet(snippet_id);
                                                        focused_snippet_id.set(Some(snippet_id));
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
                    p { class: "notes-empty", "No notes for this path yet. Use + to add one." }
                }
            }

            div { class: "notes-bottom-search",
                p {
                    class: "notes-search-hint",
                    if *view_mode.read() == NotesViewMode::Edit {
                        "Snippet Search: click to insert reference. Hold trash for 1 second to delete."
                    } else {
                        "Snippet Search: click to focus and pin. Hold trash for 1 second to delete."
                    }
                }
                input {
                    class: "notes-filter-input",
                    placeholder: "Search snippets",
                    value: "{snippet_query.read()}",
                    oninput: move |event| snippet_query.set(event.value()),
                }
                div { class: "notes-snippet-search-list",
                    if filtered_snippets.is_empty() {
                        if snippets.is_empty() {
                            p { class: "notes-empty", "No snippets yet." }
                        } else {
                            p { class: "notes-empty", "No snippets match this filter." }
                        }
                    } else {
                        for snippet in filtered_snippets {
                            {
                                let snippet_id = snippet.id;
                                let is_focused = *focused_snippet_id.read() == Some(snippet_id);
                                let item_class = if is_focused {
                                    "notes-snippet-item focused"
                                } else {
                                    "notes-snippet-item"
                                };
                                let preview = snippet_preview(&snippet.text_snapshot_plain, 160);
                                let is_holding_delete = *deleting_snippet_hold.read() == Some(snippet_id);
                                let delete_class = if is_holding_delete {
                                    "notes-snippet-delete holding"
                                } else {
                                    "notes-snippet-delete"
                                };
                                rsx! {
                                    div {
                                        key: "snippet-search-{snippet_id}",
                                        class: "notes-snippet-row",
                                        button {
                                            class: "{item_class}",
                                            r#type: "button",
                                            onclick: move |_| {
                                                let _ = app_state.write().promote_snippet(snippet_id);
                                                focused_snippet_id.set(Some(snippet_id));
                                                if *view_mode.read() == NotesViewMode::Edit
                                                    && let Some(note_id) = selected_note_id
                                                {
                                                    app_state.write().append_note_snippet_reference(
                                                        note_id,
                                                        snippet_id,
                                                        unix_now_ms(),
                                                    );
                                                }
                                            },
                                            "{preview}"
                                        }

                                        button {
                                            class: "{delete_class}",
                                            r#type: "button",
                                            title: "Hold to delete snippet",
                                            aria_label: "Hold to delete snippet",
                                            onpointerdown: move |event| {
                                                event.prevent_default();
                                                event.stop_propagation();
                                                let next_nonce = deleting_snippet_hold_nonce.read().saturating_add(1);
                                                deleting_snippet_hold_nonce.set(next_nonce);
                                                deleting_snippet_hold.set(Some(snippet_id));
                                                spawn(async move {
                                                    tokio::time::sleep(std::time::Duration::from_millis(
                                                        SNIPPET_DELETE_HOLD_MS,
                                                    ))
                                                    .await;
                                                    if *deleting_snippet_hold.read() == Some(snippet_id)
                                                        && *deleting_snippet_hold_nonce.read() == next_nonce
                                                    {
                                                        let _ = app_state.write().delete_snippet(snippet_id);
                                                        if *focused_snippet_id.read() == Some(snippet_id) {
                                                            focused_snippet_id.set(None);
                                                        }
                                                        deleting_snippet_hold.set(None);
                                                    }
                                                });
                                            },
                                            onpointerup: move |event| {
                                                event.prevent_default();
                                                event.stop_propagation();
                                                deleting_snippet_hold.set(None);
                                                let next_nonce = deleting_snippet_hold_nonce.read().saturating_add(1);
                                                deleting_snippet_hold_nonce.set(next_nonce);
                                            },
                                            onpointerleave: move |event| {
                                                event.prevent_default();
                                                event.stop_propagation();
                                                deleting_snippet_hold.set(None);
                                                let next_nonce = deleting_snippet_hold_nonce.read().saturating_add(1);
                                                deleting_snippet_hold_nonce.set(next_nonce);
                                            },
                                            onpointercancel: move |event| {
                                                event.prevent_default();
                                                event.stop_propagation();
                                                deleting_snippet_hold.set(None);
                                                let next_nonce = deleting_snippet_hold_nonce.read().saturating_add(1);
                                                deleting_snippet_hold_nonce.set(next_nonce);
                                            },
                                            {trash_icon()}
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

fn plus_icon() -> Element {
    rsx! {
        svg {
            class: "notes-icon-svg",
            view_box: "0 0 24 24",
            width: "16",
            height: "16",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M12 5v14" }
            path { d: "M5 12h14" }
        }
    }
}

fn eye_icon(active: bool) -> Element {
    let active_class = if active {
        "notes-icon-svg active"
    } else {
        "notes-icon-svg"
    };
    rsx! {
        svg {
            class: "{active_class}",
            view_box: "0 0 24 24",
            width: "16",
            height: "16",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M1 12s4-7 11-7 11 7 11 7-4 7-11 7S1 12 1 12z" }
            circle { cx: "12", cy: "12", r: "3" }
        }
    }
}

fn trash_icon() -> Element {
    rsx! {
        svg {
            class: "notes-icon-svg",
            view_box: "0 0 24 24",
            width: "14",
            height: "14",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M3 6h18" }
            path { d: "M8 6V4h8v2" }
            path { d: "M19 6l-1 14H6L5 6" }
            path { d: "M10 11v6" }
            path { d: "M14 11v6" }
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
        .unwrap_or_else(|| format!("Missing snippet #{snippet_id}"))
}

fn snippet_exists(snippets: &[Snippet], snippet_id: SnippetId) -> bool {
    snippets.iter().any(|snippet| snippet.id == snippet_id)
}

fn unix_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
