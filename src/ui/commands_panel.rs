use crate::commands::{
    CommandId, InsertCommand, parse_tags_csv, validate_command_name, validate_command_prompt,
};
use crate::persistence;
use crate::state::AppState;
use crate::terminal::TerminalManager;
use dioxus::prelude::*;
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
struct PromptSegment {
    text: String,
    is_path: bool,
}

#[component]
pub(crate) fn CommandsPanel(
    app_state: Signal<AppState>,
    terminal_manager: Signal<Arc<TerminalManager>>,
) -> Element {
    let mut filter_query = use_signal(String::new);
    let mut selected_command_id = use_signal(|| None::<CommandId>);
    let mut is_new_draft = use_signal(|| false);
    let mut editor_name = use_signal(String::new);
    let mut editor_prompt = use_signal(String::new);
    let mut editor_description = use_signal(String::new);
    let mut editor_tags_csv = use_signal(String::new);
    let mut editor_feedback = use_signal(String::new);
    let mut pending_delete_id = use_signal(|| None::<CommandId>);

    let all_commands = app_state.read().commands().to_vec();
    let filter = filter_query.read().trim().to_lowercase();
    let filtered_commands = if filter.is_empty() {
        all_commands.clone()
    } else {
        all_commands
            .iter()
            .filter(|command| matches_filter(command, &filter))
            .cloned()
            .collect::<Vec<_>>()
    };

    let selected_snapshot = *selected_command_id.read();
    if let Some(selected_id) = selected_snapshot
        && app_state.read().command_by_id(selected_id).is_none()
    {
        selected_command_id.set(None);
        pending_delete_id.set(None);
    }

    if should_auto_select_first(
        *selected_command_id.read(),
        *is_new_draft.read(),
        !all_commands.is_empty(),
    ) && let Some(first) = all_commands.first()
    {
        selected_command_id.set(Some(first.id));
        load_editor_from_command(
            first,
            editor_name,
            editor_prompt,
            editor_description,
            editor_tags_csv,
        );
    }

    let selected_id = *selected_command_id.read();
    let is_editing_mode = selected_id.is_some();
    let editor_mode_label = if is_editing_mode {
        "Editing existing command"
    } else {
        "Creating new command"
    };
    let primary_button_label = if is_editing_mode {
        "Save Changes"
    } else {
        "Create Command"
    };
    let editor_name_value = editor_name.read().clone();
    let editor_prompt_value = editor_prompt.read().clone();
    let editor_description_value = editor_description.read().clone();
    let editor_tags_value = editor_tags_csv.read().clone();
    let feedback_value = editor_feedback.read().clone();
    let filter_query_value = filter_query.read().clone();
    let selected_delete_target = *pending_delete_id.read();
    let delete_needs_confirm = selected_id.is_some() && selected_delete_target == selected_id;
    let delete_button_label = if delete_needs_confirm {
        "Confirm Delete"
    } else {
        "Delete"
    };
    let prompt_segments = prompt_segments_with_paths(&editor_prompt_value);

    rsx! {
        article { class: "commands-panel-card",
            div { class: "commands-panel-head",
                h3 { "Commands" }
                p { "Create reusable prompt snippets for Insert command mode." }
            }

            div { class: "commands-panel-toolbar",
                input {
                    class: "commands-input",
                    value: "{filter_query_value}",
                    placeholder: "Filter commands",
                    oninput: move |event| filter_query.set(event.value()),
                }
                button {
                    class: "commands-action-btn",
                    r#type: "button",
                    onclick: move |_| {
                        is_new_draft.set(true);
                        selected_command_id.set(None);
                        pending_delete_id.set(None);
                        editor_name.set(String::new());
                        editor_prompt.set(String::new());
                        editor_description.set(String::new());
                        editor_tags_csv.set(String::new());
                        editor_feedback.set("Draft reset. Fill fields and click Save.".to_string());
                    },
                    "New Draft"
                }
            }

            div { class: "commands-panel-content",
                div { class: "commands-editor",
                    label { class: "commands-label", "Name" }
                    input {
                        class: "commands-input",
                        value: "{editor_name_value}",
                        placeholder: "Command name",
                        oninput: move |event| editor_name.set(event.value()),
                    }

                    label { class: "commands-label", "Prompt" }
                    textarea {
                        class: "commands-textarea",
                        rows: "6",
                        value: "{editor_prompt_value}",
                        placeholder: "Prompt text inserted into terminal",
                        oninput: move |event| editor_prompt.set(event.value()),
                    }
                    div { class: "commands-prompt-preview-wrap",
                        p { class: "commands-prompt-preview-label", "Prompt Path Highlight" }
                        pre { class: "commands-prompt-preview",
                            if prompt_segments.is_empty() {
                                span { class: "commands-prompt-preview-empty", "Type a prompt to preview detected paths." }
                            } else {
                                for segment in prompt_segments {
                                    if segment.is_path {
                                        span { class: "commands-path-token", "{segment.text}" }
                                    } else {
                                        span { "{segment.text}" }
                                    }
                                }
                            }
                        }
                    }

                    label { class: "commands-label", "Description" }
                    textarea {
                        class: "commands-textarea",
                        rows: "3",
                        value: "{editor_description_value}",
                        placeholder: "Optional context shown in autocomplete",
                        oninput: move |event| editor_description.set(event.value()),
                    }

                    label { class: "commands-label", "Tags (comma-separated)" }
                    input {
                        class: "commands-input",
                        value: "{editor_tags_value}",
                        placeholder: "build, test, deploy",
                        oninput: move |event| editor_tags_csv.set(event.value()),
                    }

                    p { class: "commands-editor-mode", "{editor_mode_label}" }

                    div { class: "commands-editor-actions",
                        button {
                            class: "commands-action-btn primary",
                            r#type: "button",
                            onclick: move |_| {
                                let name = editor_name.read().trim().to_string();
                                if let Err(error) = validate_command_name(&name) {
                                    editor_feedback.set(error.to_string());
                                    return;
                                }

                                let prompt = editor_prompt.read().to_string();
                                if let Err(error) = validate_command_prompt(&prompt) {
                                    editor_feedback.set(error.to_string());
                                    return;
                                }

                                let description = editor_description.read().trim().to_string();
                                let tags = parse_tags_csv(editor_tags_csv.read().as_str());
                                let selected = *selected_command_id.read();

                                if let Some(command_id) = selected {
                                    is_new_draft.set(false);
                                    let updated = app_state.write().update_insert_command(
                                        command_id,
                                        name,
                                        prompt,
                                        description,
                                        tags,
                                    );
                                    if updated {
                                        pending_delete_id.set(None);
                                        if let Err(error) =
                                            persist_workspace_snapshot(app_state, terminal_manager)
                                        {
                                            editor_feedback.set(format!(
                                                "Command updated, but save failed: {error}"
                                            ));
                                        } else {
                                            editor_feedback.set("Command updated.".to_string());
                                        }
                                    } else {
                                        editor_feedback.set("No changes to save.".to_string());
                                    }
                                } else {
                                    let command_id = app_state.write().create_insert_command(
                                        name,
                                        prompt,
                                        description,
                                        tags,
                                    );
                                    is_new_draft.set(false);
                                    selected_command_id.set(Some(command_id));
                                    pending_delete_id.set(None);
                                    if let Err(error) =
                                        persist_workspace_snapshot(app_state, terminal_manager)
                                    {
                                        editor_feedback.set(format!(
                                            "Command created, but save failed: {error}"
                                        ));
                                    } else {
                                        editor_feedback.set("Command created.".to_string());
                                    }
                                }
                            },
                            "{primary_button_label}"
                        }

                        button {
                            class: "commands-action-btn danger",
                            r#type: "button",
                            disabled: selected_id.is_none(),
                            onclick: move |_| {
                                let Some(command_id) = *selected_command_id.read() else {
                                    return;
                                };
                                if *pending_delete_id.read() != Some(command_id) {
                                    pending_delete_id.set(Some(command_id));
                                    editor_feedback.set("Click delete again to confirm.".to_string());
                                    return;
                                }

                                let removed = app_state.write().delete_insert_command(command_id);
                                if !removed {
                                    pending_delete_id.set(None);
                                    editor_feedback.set("Command not found.".to_string());
                                    return;
                                }

                                pending_delete_id.set(None);
                                selected_command_id.set(None);
                                let next = app_state.read().commands().first().cloned();
                                if let Some(next_command) = next {
                                    is_new_draft.set(false);
                                    selected_command_id.set(Some(next_command.id));
                                    load_editor_from_command(
                                        &next_command,
                                        editor_name,
                                        editor_prompt,
                                        editor_description,
                                        editor_tags_csv,
                                    );
                                } else {
                                    editor_name.set(String::new());
                                    editor_prompt.set(String::new());
                                    editor_description.set(String::new());
                                    editor_tags_csv.set(String::new());
                                    is_new_draft.set(true);
                                }
                                if let Err(error) =
                                    persist_workspace_snapshot(app_state, terminal_manager)
                                {
                                    editor_feedback
                                        .set(format!("Command deleted, but save failed: {error}"));
                                } else {
                                    editor_feedback.set("Command deleted.".to_string());
                                }
                            },
                            "{delete_button_label}"
                        }
                    }

                    if !feedback_value.is_empty() {
                        p { class: "commands-feedback", "{feedback_value}" }
                    }
                }

                div { class: "commands-list-section",
                    p { class: "commands-list-title", "Saved Commands" }
                    div { class: "commands-list",
                        if filtered_commands.is_empty() {
                            p { class: "commands-empty", "No commands yet. Create one from the editor." }
                        } else {
                            for command in filtered_commands {
                                {
                                    let row_class = if selected_id == Some(command.id) {
                                        "commands-row selected"
                                    } else {
                                        "commands-row"
                                    };
                                    let prompt = command.prompt.clone();
                                    let description = command.description.clone();
                                    let tags = command.tags.join(", ");
                                    let command_name = command.name.clone();
                                    rsx! {
                                        button {
                                            class: "{row_class}",
                                            r#type: "button",
                                            onclick: move |_| {
                                                is_new_draft.set(false);
                                                selected_command_id.set(Some(command.id));
                                                pending_delete_id.set(None);
                                                editor_name.set(command_name.clone());
                                                editor_prompt.set(prompt.clone());
                                                editor_description.set(description.clone());
                                                editor_tags_csv.set(tags.clone());
                                                editor_feedback.set(String::new());
                                            },
                                            p { class: "commands-row-name", "{command.name}" }
                                            p { class: "commands-row-preview", "{prompt_preview(&command.prompt)}" }
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

fn load_editor_from_command(
    command: &InsertCommand,
    mut name: Signal<String>,
    mut prompt: Signal<String>,
    mut description: Signal<String>,
    mut tags_csv: Signal<String>,
) {
    name.set(command.name.clone());
    prompt.set(command.prompt.clone());
    description.set(command.description.clone());
    tags_csv.set(command.tags.join(", "));
}

fn persist_workspace_snapshot(
    app_state: Signal<AppState>,
    terminal_manager: Signal<Arc<TerminalManager>>,
) -> Result<(), String> {
    let state = app_state.read().clone();
    let runtime = terminal_manager.read().clone();
    let workspace = persistence::build_workspace_snapshot(&state, runtime.as_ref());
    persistence::save_workspace(&workspace).map_err(|error| error.to_string())
}

fn matches_filter(command: &InsertCommand, filter: &str) -> bool {
    let name = command.name.to_lowercase();
    let prompt = command.prompt.to_lowercase();
    let description = command.description.to_lowercase();
    let tags = command.tags.join(" ").to_lowercase();

    name.contains(filter)
        || prompt.contains(filter)
        || description.contains(filter)
        || tags.contains(filter)
}

fn prompt_preview(prompt: &str) -> String {
    let normalized = prompt.trim().replace('\n', " ");
    let mut chars = normalized.chars();
    let mut preview = String::new();
    for _ in 0..78 {
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

fn should_auto_select_first(
    selected_id: Option<CommandId>,
    is_new_draft: bool,
    has_commands: bool,
) -> bool {
    selected_id.is_none() && !is_new_draft && has_commands
}

fn prompt_segments_with_paths(prompt: &str) -> Vec<PromptSegment> {
    let mut segments = Vec::new();
    let mut index = 0usize;
    let len = prompt.len();

    while index < len {
        let Some(ch) = prompt[index..].chars().next() else {
            break;
        };
        if ch == '"' {
            let start = index;
            index += ch.len_utf8();
            let mut escaped = false;
            while index < len {
                let Some(next) = prompt[index..].chars().next() else {
                    break;
                };
                index += next.len_utf8();
                if next == '"' && !escaped {
                    break;
                }
                if next == '\\' {
                    escaped = !escaped;
                } else {
                    escaped = false;
                }
            }

            let token = &prompt[start..index];
            let inner = token
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .unwrap_or_else(|| token.trim_start_matches('"'));
            push_prompt_segment(&mut segments, token, looks_like_path(inner, true));
            continue;
        }

        if ch.is_whitespace() {
            let start = index;
            index += ch.len_utf8();
            while index < len {
                let Some(next) = prompt[index..].chars().next() else {
                    break;
                };
                if !next.is_whitespace() {
                    break;
                }
                index += next.len_utf8();
            }
            push_prompt_segment(&mut segments, &prompt[start..index], false);
            continue;
        }

        let start = index;
        index += ch.len_utf8();
        while index < len {
            let Some(next) = prompt[index..].chars().next() else {
                break;
            };
            if next.is_whitespace() || next == '"' {
                break;
            }
            index += next.len_utf8();
        }

        let token = &prompt[start..index];
        let (prefix, core, suffix) = split_wrapping_punctuation(token);
        if core.is_empty() || !looks_like_path(core, false) {
            push_prompt_segment(&mut segments, token, false);
            continue;
        }

        push_prompt_segment(&mut segments, prefix, false);
        push_prompt_segment(&mut segments, core, true);
        push_prompt_segment(&mut segments, suffix, false);
    }

    segments
}

fn push_prompt_segment(segments: &mut Vec<PromptSegment>, text: &str, is_path: bool) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = segments.last_mut()
        && last.is_path == is_path
    {
        last.text.push_str(text);
        return;
    }

    segments.push(PromptSegment {
        text: text.to_string(),
        is_path,
    });
}

fn split_wrapping_punctuation(token: &str) -> (&str, &str, &str) {
    let mut start = 0usize;
    let mut end = token.len();

    while start < end {
        let Some(ch) = token[start..].chars().next() else {
            break;
        };
        if !is_leading_wrapper(ch) {
            break;
        }
        start += ch.len_utf8();
    }

    while start < end {
        let Some(ch) = token[..end].chars().next_back() else {
            break;
        };
        if !is_trailing_wrapper(ch) {
            break;
        }
        end -= ch.len_utf8();
    }

    (&token[..start], &token[start..end], &token[end..])
}

fn is_leading_wrapper(ch: char) -> bool {
    matches!(ch, '(' | '[' | '{')
}

fn is_trailing_wrapper(ch: char) -> bool {
    matches!(ch, ')' | ']' | '}' | ',' | ';' | ':' | '!' | '?')
}

fn looks_like_path(token: &str, quoted: bool) -> bool {
    let candidate = token.trim();
    if candidate.is_empty() || candidate == "." || candidate == ".." || candidate.starts_with('-') {
        return false;
    }

    if candidate.starts_with("http://") || candidate.starts_with("https://") {
        return false;
    }

    if candidate.contains('/') || candidate.contains('\\') {
        return candidate.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(ch, '/' | '\\' | '.' | '_' | '-' | '~' | ':' | ' ')
        });
    }

    if candidate.starts_with("~/") || candidate.starts_with("./") || candidate.starts_with("../") {
        return true;
    }

    if let Some((base, extension)) = candidate.rsplit_once('.') {
        if !base.is_empty()
            && !extension.is_empty()
            && extension.len() <= 10
            && extension.chars().all(|ch| ch.is_ascii_alphanumeric())
            && base.chars().all(|ch| {
                ch.is_ascii_alphanumeric()
                    || matches!(ch, '_' | '-' | '.' | '~')
                    || (quoted && ch == ' ')
            })
        {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::{prompt_segments_with_paths, should_auto_select_first};

    #[test]
    fn auto_selects_when_no_selection_and_not_in_draft_mode() {
        assert!(should_auto_select_first(None, false, true));
    }

    #[test]
    fn does_not_auto_select_while_new_draft_is_active() {
        assert!(!should_auto_select_first(None, true, true));
    }

    #[test]
    fn does_not_auto_select_when_a_command_is_already_selected() {
        assert!(!should_auto_select_first(Some(42), false, true));
    }

    #[test]
    fn highlights_unquoted_unix_paths() {
        let segments = prompt_segments_with_paths("cat src/ui/commands_panel.rs");
        assert!(
            segments
                .iter()
                .any(|segment| { segment.is_path && segment.text == "src/ui/commands_panel.rs" })
        );
    }

    #[test]
    fn highlights_quoted_paths_with_spaces() {
        let segments = prompt_segments_with_paths("open \"Linux Software/Gestalt/src/main.rs\"");
        assert!(segments.iter().any(|segment| {
            segment.is_path && segment.text == "\"Linux Software/Gestalt/src/main.rs\""
        }));
    }

    #[test]
    fn keeps_non_paths_plain() {
        let segments = prompt_segments_with_paths("cargo run --release");
        assert!(!segments.iter().any(|segment| segment.is_path));
    }
}
