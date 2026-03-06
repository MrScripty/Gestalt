use crate::path_validation;
use crate::resource_monitor::{RESOURCE_POLL_MS, ResourceSnapshot, sample_resource_snapshot};
use crate::state::{AppState, SessionId, SessionStatus, VisibleAgentSlot};
use crate::terminal::TerminalManager;
use dioxus::prelude::*;
use std::sync::Arc;
use std::time::Duration;

#[component]
pub(crate) fn TabRail(
    app_state: Signal<AppState>,
    terminal_manager: Signal<Arc<TerminalManager>>,
    mut dragging_tab: Signal<Option<SessionId>>,
    mut new_group_path: Signal<String>,
    mut renaming_tab: Signal<Option<SessionId>>,
    mut rename_draft: Signal<String>,
) -> Element {
    let snapshot = app_state.read().clone();
    let mut resource_snapshot = use_signal(ResourceSnapshot::default);
    {
        let terminal_manager = terminal_manager.read().clone();
        use_future(move || {
            let terminal_manager = terminal_manager.clone();
            async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(RESOURCE_POLL_MS)).await;
                    let session_roots = {
                        let state = app_state.read();
                        state
                            .sessions
                            .iter()
                            .filter_map(|session| {
                                terminal_manager
                                    .session_process_id(session.id)
                                    .map(|pid| (session.id, pid))
                            })
                            .collect::<Vec<_>>()
                    };
                    resource_snapshot.set(sample_resource_snapshot(&session_roots));
                }
            }
        });
    }
    let resource_snapshot_value = resource_snapshot.read().clone();
    let active_group_id = snapshot.active_group_id();
    let renaming_tab_id = *renaming_tab.read();
    let rename_draft_value = rename_draft.read().clone();
    let new_group_path_value = new_group_path.read().clone();
    let mut new_group_feedback = use_signal(String::new);
    let new_group_feedback_value = new_group_feedback.read().clone();

    rsx! {
        aside { class: "tab-rail",
            div { class: "group-list",
                for group in snapshot.groups.clone() {
                    {
                        let group_id = group.id;
                        let group_color = group.color.clone();
                        let group_path = group.path.clone();
                        let group_label = group.label();
                        let group_class = if active_group_id == Some(group_id) {
                            "group active"
                        } else {
                            "group"
                        };
                        let terminal_manager_for_add = terminal_manager.read().clone();
                        let terminal_manager_for_drop = terminal_manager.read().clone();
                        let terminal_manager_for_remove = terminal_manager.read().clone();

                        let sessions_in_group: Vec<_> = snapshot
                            .sessions
                            .iter()
                            .filter(|session| session.group_id == group_id)
                            .cloned()
                            .collect();

                        rsx! {
                            section {
                                class: "{group_class}",
                                key: "group-{group_id}",

                                div {
                                    class: "group-header",
                                    style: "border-left-color: {group_color};",
                                    div {
                                        h3 { "{group_label}" }
                                    }
                                    div { class: "group-header-actions",
                                        button {
                                            class: "pill-btn",
                                            onclick: move |_| {
                                                let (session_id, path) = {
                                                    let mut state = app_state.write();
                                                    let id = state.add_session(group_id);
                                                    state.select_session(id);
                                                    let path = state.group_path(group_id).unwrap_or(".").to_string();
                                                    (id, path)
                                                };

                                                let start_error = terminal_manager_for_add
                                                    .ensure_session(session_id, &path)
                                                    .err();

                                                if start_error.is_some() {
                                                    app_state.write().set_session_status(session_id, SessionStatus::Error);
                                                }
                                            },
                                            "+ tab"
                                        }
                                        button {
                                            class: "pill-btn danger-btn",
                                            onclick: move |_| {
                                                let removed_session_ids = app_state.write().remove_group(group_id);
                                                if removed_session_ids.is_empty() {
                                                    return;
                                                }

                                                let removed_renaming_session = renaming_tab
                                                    .read()
                                                    .is_some_and(|session_id| removed_session_ids.contains(&session_id));
                                                for session_id in &removed_session_ids {
                                                    let _ = terminal_manager_for_remove.terminate_session(*session_id);
                                                }

                                                if removed_renaming_session {
                                                    renaming_tab.set(None);
                                                    rename_draft.set(String::new());
                                                }
                                                dragging_tab.set(None);
                                            },
                                            "remove"
                                        }
                                    }
                                }

                                ul { class: "tab-list",
                                    for session in sessions_in_group {
                                        {
                                            let session_id = session.id;
                                            let selected = snapshot.selected_session == Some(session_id);
                                            let is_runner = session.role.is_runner();
                                            let load_style = match resource_snapshot_value
                                                .session_loads
                                                .get(&session_id)
                                                .map(|load| load.level.css_class())
                                            {
                                                Some("load-hot") => {
                                                    "box-shadow: inset 0 0 0 1px #ff8a8a, 0 0 0 1px #ff8a8a55;"
                                                }
                                                Some("load-warm") => {
                                                    "box-shadow: inset 0 0 0 1px #f6d373, 0 0 0 1px #f6d37355;"
                                                }
                                                _ => "",
                                            };
                                            let base_tab_class = if selected {
                                                if is_runner { "tab active role-run" } else { "tab active role-agent" }
                                            } else if is_runner {
                                                "tab role-run"
                                            } else {
                                                "tab role-agent"
                                            };
                                            let status_class = match session.status {
                                                SessionStatus::Busy => "status-busy",
                                                SessionStatus::Idle => "status-idle",
                                                SessionStatus::Error => "status-error",
                                            };
                                            let tab_class = format!("{base_tab_class} {status_class}");
                                            let status_style = format!("background: var({});", session.status.css_var());
                                            let target_path = snapshot.group_path(session.group_id).unwrap_or(".").to_string();
                                            let terminal_manager_for_reorder = terminal_manager.read().clone();
                                            let terminal_manager_for_close = terminal_manager.read().clone();
                                            let is_renaming = renaming_tab_id == Some(session_id);
                                            let title_for_start = session.title.clone();
                                            let close_aria_label = format!("Close terminal {}", session.title);

                                            rsx! {
                                                li {
                                                    class: "{tab_class}",
                                                    key: "session-{session_id}",
                                                    style: "{load_style}",
                                                    draggable: if is_renaming { "false" } else { "true" },
                                                    ondragstart: move |event| {
                                                        if *renaming_tab.read() == Some(session_id) {
                                                            event.prevent_default();
                                                            return;
                                                        }
                                                        dragging_tab.set(Some(session_id));
                                                    },
                                                    ondragend: move |_| {
                                                        dragging_tab.set(None);
                                                    },
                                                    ondragover: move |event| {
                                                        event.prevent_default();
                                                    },
                                                    ondrop: move |event| {
                                                        event.prevent_default();
                                                        if let Some(source_id) = *dragging_tab.read() {
                                                            app_state.write().move_session_before(source_id, session_id);
                                                            let _ = terminal_manager_for_reorder.set_cwd(source_id, &target_path);
                                                        }
                                                        dragging_tab.set(None);
                                                    },
                                                    onclick: move |_| {
                                                        if is_runner {
                                                            app_state.write().select_session(session_id);
                                                            return;
                                                        }
                                                        app_state.write().swap_session_with_visible_agent_slot(
                                                            session_id,
                                                            VisibleAgentSlot::Top,
                                                        );
                                                    },
                                                    oncontextmenu: move |event| {
                                                        event.prevent_default();
                                                        if is_runner {
                                                            app_state.write().select_session(session_id);
                                                            return;
                                                        }
                                                        app_state.write().swap_session_with_visible_agent_slot(
                                                            session_id,
                                                            VisibleAgentSlot::Bottom,
                                                        );
                                                    },
                                                    ondoubleclick: move |_| {
                                                        renaming_tab.set(Some(session_id));
                                                        rename_draft.set(title_for_start.clone());
                                                    },

                                                    span { class: "status-dot", style: "{status_style}" }

                                                    if is_renaming {
                                                        input {
                                                            class: "tab-rename-input",
                                                            value: "{rename_draft_value}",
                                                            oninput: move |event| rename_draft.set(event.value()),
                                                            onkeydown: move |event| {
                                                                match event.key() {
                                                                    Key::Enter => {
                                                                        event.prevent_default();
                                                                        let title = rename_draft.read().trim().to_string();
                                                                        if !title.is_empty() {
                                                                            app_state.write().rename_session(session_id, title);
                                                                        }
                                                                        renaming_tab.set(None);
                                                                    }
                                                                    Key::Escape => {
                                                                        event.prevent_default();
                                                                        renaming_tab.set(None);
                                                                    }
                                                                    _ => {}
                                                                }
                                                            },
                                                            onblur: move |_| {
                                                                let was_editing = *renaming_tab.read() == Some(session_id);
                                                                if was_editing {
                                                                    let title = rename_draft.read().trim().to_string();
                                                                    if !title.is_empty() {
                                                                        app_state.write().rename_session(session_id, title);
                                                                    }
                                                                    renaming_tab.set(None);
                                                                }
                                                            },
                                                            oncontextmenu: move |event| {
                                                                event.stop_propagation();
                                                            },
                                                        }
                                                    } else {
                                                        div { class: "tab-main",
                                                            span { class: "title", "{session.title}" }
                                                            span { class: "role-pill", "{session.role.badge()}" }
                                                        }
                                                    }

                                                    div { class: "tab-actions",
                                                        button {
                                                            class: "close-btn",
                                                            r#type: "button",
                                                            aria_label: "{close_aria_label}",
                                                            onmousedown: move |event| {
                                                                event.stop_propagation();
                                                            },
                                                            onclick: move |event| {
                                                                event.stop_propagation();
                                                                let removed = app_state.write().remove_session(session_id);
                                                                if !removed {
                                                                    return;
                                                                }

                                                                let _ = terminal_manager_for_close.terminate_session(session_id);
                                                                if *dragging_tab.read() == Some(session_id) {
                                                                    dragging_tab.set(None);
                                                                }
                                                                if *renaming_tab.read() == Some(session_id) {
                                                                    renaming_tab.set(None);
                                                                    rename_draft.set(String::new());
                                                                }
                                                            },
                                                            oncontextmenu: move |event| {
                                                                event.stop_propagation();
                                                            },
                                                            "X"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                div {
                                    class: "group-drop-target",
                                    ondragover: move |event| {
                                        event.prevent_default();
                                    },
                                    ondrop: move |event| {
                                        event.prevent_default();
                                        if let Some(source_id) = *dragging_tab.read() {
                                            app_state.write().move_session_to_group_end(source_id, group_id);
                                            let _ = terminal_manager_for_drop.set_cwd(source_id, &group_path);
                                        }
                                        dragging_tab.set(None);
                                    },
                                    "Drop tab here to move into this path"
                                }
                            }
                        }
                    }
                }
            }

            div { class: "rail-footer",
                p { class: "meta-label", "New Path Group" }
                input {
                    class: "path-input",
                    placeholder: "/abs/path/to/project",
                    value: "{new_group_path_value}",
                    oninput: move |event| {
                        new_group_path.set(event.value());
                        new_group_feedback.set(String::new());
                    },
                }
                label {
                    class: "path-picker-label",
                    "Browse folder"
                    input {
                        class: "path-picker-input",
                        r#type: "file",
                        directory: true,
                        onchange: move |event| {
                            let Some(selection) = event.files().into_iter().next() else {
                                return;
                            };

                            let selected_path = path_validation::derive_directory_from_selection(selection.path());
                            let display_path = selected_path.to_string_lossy().into_owned();
                            new_group_path.set(display_path);
                            new_group_feedback.set(String::new());
                        },
                    }
                }
                button {
                    class: "primary-btn",
                    onclick: move |_| {
                        let raw_path = new_group_path.read().trim().to_string();
                        if raw_path.is_empty() {
                            new_group_feedback.set("Path is required.".to_string());
                            return;
                        }

                        let path = match path_validation::validate_group_path(&raw_path) {
                            Ok(path) => path,
                            Err(error) => {
                                new_group_feedback.set(error);
                                return;
                            }
                        };

                        let default_sessions = {
                            let mut state = app_state.write();
                            let (_group_id, ids) = state.create_group_with_defaults(path.clone());
                            if let Some(first) = ids.first().copied() {
                                state.select_session(first);
                            }
                            ids
                        };

                        new_group_path.set(String::new());
                        new_group_feedback.set(String::new());

                        let runtime = terminal_manager.read().clone();
                        let failed = default_sessions
                            .iter()
                            .filter_map(|session_id| {
                                runtime
                                    .ensure_session(*session_id, &path)
                                    .err()
                                    .map(|_| *session_id)
                            })
                            .collect::<Vec<_>>();

                        if !failed.is_empty() {
                            let mut state = app_state.write();
                            for session_id in failed {
                                state.set_session_status(session_id, SessionStatus::Error);
                            }
                        }
                    },
                    "Create Path Group"
                }
                if !new_group_feedback_value.is_empty() {
                    p { class: "path-feedback", "{new_group_feedback_value}" }
                }
            }
        }
    }
}
