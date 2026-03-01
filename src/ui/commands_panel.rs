use crate::commands::{
    CommandId, InsertCommand, parse_tags_csv, validate_command_name, validate_command_prompt,
};
use crate::state::AppState;
use dioxus::prelude::*;

#[component]
pub(crate) fn CommandsPanel(app_state: Signal<AppState>) -> Element {
    let mut filter_query = use_signal(String::new);
    let mut selected_command_id = use_signal(|| None::<CommandId>);
    let mut editor_name = use_signal(String::new);
    let mut editor_prompt = use_signal(String::new);
    let mut editor_description = use_signal(String::new);
    let mut editor_tags_csv = use_signal(String::new);
    let mut editor_feedback = use_signal(String::new);

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
    }

    if selected_command_id.read().is_none()
        && let Some(first) = all_commands.first()
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
    let editor_name_value = editor_name.read().clone();
    let editor_prompt_value = editor_prompt.read().clone();
    let editor_description_value = editor_description.read().clone();
    let editor_tags_value = editor_tags_csv.read().clone();
    let feedback_value = editor_feedback.read().clone();
    let filter_query_value = filter_query.read().clone();

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
                        selected_command_id.set(None);
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
                                            selected_command_id.set(Some(command.id));
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
                        rows: "5",
                        value: "{editor_prompt_value}",
                        placeholder: "Prompt text inserted into terminal",
                        oninput: move |event| editor_prompt.set(event.value()),
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

                    div { class: "commands-editor-actions",
                        button {
                            class: "commands-action-btn primary",
                            r#type: "button",
                            onclick: move |_| {
                                let name = editor_name.read().trim().to_string();
                                if let Err(error) = validate_command_name(&name) {
                                    editor_feedback.set(error);
                                    return;
                                }

                                let prompt = editor_prompt.read().to_string();
                                if let Err(error) = validate_command_prompt(&prompt) {
                                    editor_feedback.set(error);
                                    return;
                                }

                                let description = editor_description.read().trim().to_string();
                                let tags = parse_tags_csv(editor_tags_csv.read().as_str());
                                let selected = *selected_command_id.read();

                                if let Some(command_id) = selected {
                                    let updated = app_state.write().update_insert_command(
                                        command_id,
                                        name,
                                        prompt,
                                        description,
                                        tags,
                                    );
                                    if updated {
                                        editor_feedback.set("Command updated.".to_string());
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
                                    selected_command_id.set(Some(command_id));
                                    editor_feedback.set("Command created.".to_string());
                                }
                            },
                            "Save"
                        }

                        button {
                            class: "commands-action-btn danger",
                            r#type: "button",
                            disabled: selected_id.is_none(),
                            onclick: move |_| {
                                let Some(command_id) = *selected_command_id.read() else {
                                    return;
                                };

                                let removed = app_state.write().delete_insert_command(command_id);
                                if !removed {
                                    editor_feedback.set("Command not found.".to_string());
                                    return;
                                }

                                selected_command_id.set(None);
                                let next = app_state.read().commands().first().cloned();
                                if let Some(next_command) = next {
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
                                }
                                editor_feedback.set("Command deleted.".to_string());
                            },
                            "Delete"
                        }
                    }

                    if !feedback_value.is_empty() {
                        p { class: "commands-feedback", "{feedback_value}" }
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
