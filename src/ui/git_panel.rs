use crate::git::{CheckoutTarget, CommitDetails, CommitDraft, RepoContext, TagDetails, TagInfo};
use crate::state::AppState;
use crate::terminal::TerminalManager;
use crate::ui::git_helpers::bump_refresh_nonce;
use dioxus::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

#[path = "git_commit_graph.rs"]
mod git_commit_graph;

use git_commit_graph::{GRAPH_NODE_RADIUS_PX, GRAPH_ROW_HEIGHT_PX, build_commit_graph_layout};

#[component]
pub(crate) fn GitPanel(
    app_state: Signal<AppState>,
    terminal_manager: Signal<Arc<TerminalManager>>,
    active_group_path: String,
    repo_context: Signal<Option<RepoContext>>,
    repo_loading: Signal<bool>,
    git_refresh_nonce: Signal<u64>,
) -> Element {
    let _ = (&app_state, &terminal_manager);
    let _ = repo_loading;
    let context = repo_context.read().clone();
    let mut op_feedback = use_signal(String::new);
    let op_feedback_value = op_feedback.read().clone();
    let mut selected_commit = use_signal(String::new);
    let mut selected_commit_details = use_signal(|| None::<CommitDetails>);
    let selected_commit_details_value = selected_commit_details.read().clone();
    let mut selected_commit_details_error = use_signal(String::new);
    let selected_commit_details_error_value = selected_commit_details_error.read().clone();
    let mut editing_commit = use_signal(|| None::<String>);
    let editing_commit_value = editing_commit.read().clone();
    let mut edit_title = use_signal(String::new);
    let mut edit_message = use_signal(String::new);
    let mut commit_title = use_signal(String::new);
    let mut commit_message = use_signal(String::new);
    let mut tag_panel_commit = use_signal(|| None::<String>);
    let tag_panel_commit_value = tag_panel_commit.read().clone();
    let mut tag_original_name = use_signal(|| None::<String>);
    let tag_original_name_value = tag_original_name.read().clone();
    let mut tag_name = use_signal(String::new);
    let tag_name_value = tag_name.read().clone();
    let mut tag_message = use_signal(String::new);
    let tag_message_value = tag_message.read().clone();
    let mut tag_error = use_signal(String::new);
    let tag_error_value = tag_error.read().clone();
    let op_inflight = use_signal(|| false);
    let op_inflight_value = *op_inflight.read();

    if selected_commit.read().is_empty()
        && let Some(RepoContext::Available(snapshot)) = context.as_ref()
        && let Some(head) = snapshot
            .head
            .clone()
            .or_else(|| snapshot.commits.first().map(|commit| commit.sha.clone()))
    {
        selected_commit.set(head);
    }

    rsx! {
        article { class: "git-panel-card",
            div { class: "git-panel-head",
                h3 { "Git" }
            }

            if let Some(context) = context {
                {
                    match context {
                        RepoContext::NotRepo { inspected_path } => rsx! {
                            div { class: "git-empty",
                                h4 { "No Repository" }
                                p { "No Git repository found for this path group." }
                                p { class: "git-meta", "Inspected path: {inspected_path}" }
                            }
                        },
                        RepoContext::Available(snapshot) => {
                            let selected_commit_value = {
                                let selected = selected_commit.read().trim().to_string();
                                if selected.is_empty() {
                                    snapshot
                                        .head
                                        .clone()
                                        .or_else(|| snapshot.commits.first().map(|commit| commit.sha.clone()))
                                        .unwrap_or_default()
                                } else {
                                    selected
                                }
                            };
                            let commit_title_value = commit_title.read().clone();
                            let commit_message_value = commit_message.read().clone();
                            let edit_title_value = edit_title.read().clone();
                            let edit_message_value = edit_message.read().clone();
                            let graph_layout = build_commit_graph_layout(&snapshot.commits);
                            let graph_overlay_width = graph_layout.gutter_width_px;
                            let graph_overlay_height = graph_layout.overlay_height_px;
                            let graph_node_x_by_sha = graph_layout
                                .nodes
                                .iter()
                                .map(|node| (node.sha.clone(), node.x))
                                .collect::<HashMap<_, _>>();
                            let tags_by_target_sha = tags_by_target_sha(&snapshot.tags);
                            let selected_commit_tags = tags_by_target_sha
                                .get(&selected_commit_value)
                                .cloned()
                                .unwrap_or_default();
                            let tag_panel_open =
                                tag_panel_commit_value.as_deref() == Some(selected_commit_value.as_str());
                            let branch_checkout_group_path = active_group_path.clone();
                            let commit_checkout_group_path = active_group_path.clone();
                            let commit_group_path = active_group_path.clone();
                            let selected_commit_for_checkout = selected_commit_value.clone();

                            rsx! {
                                section { class: "git-section",
                                    h4 { "Branches" }
                                    div { class: "git-branch-list",
                                        for branch in snapshot.branches.clone() {
                                            {
                                                let branch_name = branch.name.clone();
                                                let branch_name_for_checkout = branch_name.clone();
                                                let branch_row_class = if branch.is_current {
                                                    "git-branch-row current"
                                                } else {
                                                    "git-branch-row"
                                                };
                                                let remote_badge = if branch.is_remote { "remote" } else { "local" };
                                                let checkout_group_path = branch_checkout_group_path.clone();
                                                rsx! {
                                                    div { class: "{branch_row_class}",
                                                        span { class: "git-branch-name", "{branch_name}" }
                                                        span { class: "git-branch-kind", "{remote_badge}" }
                                                        button {
                                                            class: "git-action-btn",
                                                            disabled: op_inflight_value,
                                                            onclick: move |_| {
                                                                if !start_operation(op_inflight, op_feedback) {
                                                                    return;
                                                                }
                                                                let result = crate::orchestrator::git::checkout_target(
                                                                    &checkout_group_path,
                                                                    CheckoutTarget::Branch(branch_name_for_checkout.clone()),
                                                                );
                                                                match result {
                                                                    Ok(_) => {
                                                                        op_feedback.set(format!("Checked out branch '{branch_name_for_checkout}'."));
                                                                        bump_refresh_nonce(git_refresh_nonce);
                                                                    }
                                                                    Err(error) => op_feedback.set(error.to_string()),
                                                                }
                                                                finish_operation(op_inflight);
                                                            },
                                                            "Checkout"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                section { class: "git-section",
                                    h4 { "Commit Tree (Newest First)" }
                                    div {
                                        class: "git-commit-viewport",
                                        style: "--git-graph-gutter: {graph_overlay_width}px; --git-graph-row-height: {GRAPH_ROW_HEIGHT_PX}px; --git-graph-lanes: {graph_layout.lane_count};",
                                        svg {
                                            class: "git-commit-overlay",
                                            width: "{graph_overlay_width}",
                                            height: "{graph_overlay_height}",
                                            view_box: "0 0 {graph_overlay_width} {graph_overlay_height}",
                                            preserve_aspect_ratio: "none",
                                            circle {
                                                cx: "{graph_layout.summary_x}",
                                                cy: "{graph_layout.summary_y}",
                                                r: "{GRAPH_NODE_RADIUS_PX}",
                                                class: "git-graph-summary-node",
                                            }
                                            for segment in graph_layout.segments.clone() {
                                                {
                                                    let stroke = if segment.is_merge {
                                                        lane_color(segment.to_lane)
                                                    } else {
                                                        lane_color(segment.from_lane)
                                                    };
                                                    let class = if segment.is_merge {
                                                        "git-graph-segment git-graph-segment-merge"
                                                    } else {
                                                        "git-graph-segment"
                                                    };
                                                    rsx! {
                                                        line {
                                                            class: "{class}",
                                                            x1: "{segment.x1}",
                                                            y1: "{segment.y1}",
                                                            x2: "{segment.x2}",
                                                            y2: "{segment.y2}",
                                                            stroke: "{stroke}",
                                                        }
                                                    }
                                                }
                                            }
                                            for node in graph_layout.nodes.clone() {
                                                {
                                                    let node_fill = if selected_commit_value == node.sha {
                                                        "#f4d35e"
                                                    } else if node.is_unpushed {
                                                        "#6ff39a"
                                                    } else {
                                                        lane_color(node.lane)
                                                    };
                                                    rsx! {
                                                        circle {
                                                            class: "git-graph-node",
                                                            cx: "{node.x}",
                                                            cy: "{node.y}",
                                                            r: "{GRAPH_NODE_RADIUS_PX}",
                                                            fill: "{node_fill}",
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        div { class: "git-commit-list",
                                            div {
                                                class: "git-summary-row",
                                                span { class: "git-graph-spacer", aria_hidden: "true" }
                                                div { class: "git-summary-content",
                                                    p { class: "git-summary-label", "Unstaged files: {snapshot.unstaged_count}" }
                                                    p { class: "git-summary-meta", "Select a commit node to inspect it. Double-click an unpushed row to edit its message." }
                                                }
                                            }
                                            for commit in snapshot.commits.clone() {
                                                {
                                                    let commit_sha = commit.sha.clone();
                                                    let commit_sha_for_select = commit_sha.clone();
                                                    let commit_sha_for_tag = commit_sha.clone();
                                                    let commit_for_click = commit.clone();
                                                    let commit_for_edit = commit.clone();
                                                    let commit_tags = tags_by_target_sha
                                                        .get(&commit.sha)
                                                        .cloned()
                                                        .unwrap_or_default();
                                                    let has_tags = !commit_tags.is_empty();
                                                    let tag_button_class = if has_tags {
                                                        "git-node-tag-btn visible"
                                                    } else {
                                                        "git-node-tag-btn"
                                                    };
                                                    let tag_button_title = if has_tags {
                                                        format!("Manage tags for {}", commit.subject)
                                                    } else {
                                                        format!("Add a tag to {}", commit.subject)
                                                    };
                                                    let tag_button_left = graph_node_x_by_sha
                                                        .get(&commit.sha)
                                                        .copied()
                                                        .unwrap_or(GRAPH_NODE_RADIUS_PX * 3.0)
                                                        + GRAPH_NODE_RADIUS_PX
                                                        + 14.0;
                                                    let details_group_path = active_group_path.clone();
                                                    let edit_group_path = active_group_path.clone();
                                                    let tag_group_path = active_group_path.clone();
                                                    let selected_class = if selected_commit_value == commit.sha {
                                                        "git-commit-row selected"
                                                    } else {
                                                        "git-commit-row"
                                                    };
                                                    let decoration_text = if commit.decorations.is_empty() {
                                                        String::new()
                                                    } else {
                                                        format!(" ({})", commit.decorations.join(", "))
                                                    };
                                                    rsx! {
                                                        div { class: "git-commit-row-shell",
                                                            button {
                                                                class: "{selected_class}",
                                                                r#type: "button",
                                                                onclick: move |_| {
                                                                selected_commit.set(commit_sha_for_select.clone());
                                                                editing_commit.set(None);
                                                                if tag_panel_commit.read().as_deref()
                                                                    != Some(commit_sha_for_select.as_str())
                                                                {
                                                                    tag_panel_commit.set(None);
                                                                    clear_tag_editor(
                                                                        tag_original_name,
                                                                        tag_name,
                                                                        tag_message,
                                                                        tag_error,
                                                                    );
                                                                }
                                                                load_commit_details_for_selection(
                                                                    &details_group_path,
                                                                    &commit_for_click.sha,
                                                                    selected_commit_details,
                                                                    selected_commit_details_error,
                                                                );
                                                                },
                                                                ondoubleclick: move |_| {
                                                                if !commit_for_edit.is_unpushed {
                                                                    op_feedback.set("Only unpushed commits can be edited.".to_string());
                                                                    return;
                                                                }
                                                                selected_commit.set(commit_for_edit.sha.clone());
                                                                tag_panel_commit.set(None);
                                                                clear_tag_editor(
                                                                    tag_original_name,
                                                                    tag_name,
                                                                    tag_message,
                                                                    tag_error,
                                                                );
                                                                selected_commit_details_error.set(String::new());
                                                                match crate::orchestrator::git::load_commit_details(&edit_group_path, &commit_for_edit.sha) {
                                                                    Ok(details) => {
                                                                        if details.is_unpushed {
                                                                            edit_title.set(details.title.clone());
                                                                            edit_message.set(details.message.clone());
                                                                            editing_commit.set(Some(details.sha.clone()));
                                                                            selected_commit_details.set(Some(details));
                                                                        } else {
                                                                            editing_commit.set(None);
                                                                            selected_commit_details.set(Some(details));
                                                                            op_feedback.set("This commit is already pushed and cannot be edited.".to_string());
                                                                        }
                                                                    }
                                                                    Err(error) => {
                                                                        editing_commit.set(None);
                                                                        selected_commit_details.set(None);
                                                                        selected_commit_details_error.set(error.to_string());
                                                                    }
                                                                }
                                                                },
                                                                span { class: "git-graph-spacer", aria_hidden: "true" }
                                                                div {
                                                                    class: "git-commit-main",
                                                                    div {
                                                                        class: "git-commit-line",
                                                                        span { class: "git-subject", "{commit.subject}" }
                                                                        if commit.is_unpushed {
                                                                            span { class: "git-commit-badge", "Unpushed" }
                                                                        }
                                                                    }
                                                                    p { class: "git-meta git-commit-meta", "{commit.short_sha} • {commit.author} • {commit.authored_at}{decoration_text}" }
                                                                }
                                                            }
                                                            button {
                                                                class: "{tag_button_class}",
                                                                r#type: "button",
                                                                style: "--git-tag-left: {tag_button_left}px;",
                                                                title: "{tag_button_title}",
                                                                aria_label: "{tag_button_title}",
                                                                onclick: move |event| {
                                                                    event.prevent_default();
                                                                    event.stop_propagation();
                                                                    selected_commit.set(commit_sha_for_tag.clone());
                                                                    editing_commit.set(None);
                                                                    load_commit_details_for_selection(
                                                                        &tag_group_path,
                                                                        &commit_sha_for_tag,
                                                                        selected_commit_details,
                                                                        selected_commit_details_error,
                                                                    );
                                                                    tag_panel_commit.set(Some(commit_sha_for_tag.clone()));
                                                                    if let Some(tag) = commit_tags.first() {
                                                                        load_tag_editor(
                                                                            &tag_group_path,
                                                                            &tag.name,
                                                                            tag_original_name,
                                                                            tag_name,
                                                                            tag_message,
                                                                            tag_error,
                                                                        );
                                                                    } else {
                                                                        clear_tag_editor(
                                                                            tag_original_name,
                                                                            tag_name,
                                                                            tag_message,
                                                                            tag_error,
                                                                        );
                                                                    }
                                                                },
                                                                svg {
                                                                    class: "git-node-tag-icon",
                                                                    view_box: "0 0 24 24",
                                                                    "aria-hidden": "true",
                                                                    path {
                                                                        d: "M11.5 3H5a2 2 0 0 0-2 2v6.5a2 2 0 0 0 .59 1.41l8.5 8.5a2 2 0 0 0 2.82 0l5.59-5.59a2 2 0 0 0 0-2.82l-8.5-8.5A2 2 0 0 0 11.5 3Z",
                                                                    }
                                                                    circle {
                                                                        cx: "7.75",
                                                                        cy: "7.75",
                                                                        r: "1.75",
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if let Some(details) = selected_commit_details_value.clone() {
                                        div { class: "git-commit-details",
                                            h5 { "Selected Commit" }
                                            p { class: "git-meta", "Short ID: {details.short_sha}" }
                                            if editing_commit_value.as_deref() == Some(details.sha.as_str()) {
                                                input {
                                                    class: "git-input",
                                                    disabled: op_inflight_value,
                                                    value: "{edit_title_value}",
                                                    oninput: move |event| edit_title.set(event.value()),
                                                }
                                                textarea {
                                                    class: "git-textarea",
                                                    disabled: op_inflight_value,
                                                    rows: "5",
                                                    value: "{edit_message_value}",
                                                    oninput: move |event| edit_message.set(event.value()),
                                                }
                                                {
                                                    let save_group_path = active_group_path.clone();
                                                    let save_sha = details.sha.clone();
                                                    rsx! {
                                                div { class: "git-change-actions",
                                                    button {
                                                        class: "git-action-btn git-action-primary",
                                                        disabled: op_inflight_value,
                                                        onclick: move |_| {
                                                            if !start_operation(op_inflight, op_feedback) {
                                                                return;
                                                            }
                                                            let draft = CommitDraft {
                                                                title: edit_title.read().clone(),
                                                                message: edit_message.read().clone(),
                                                            };
                                                            match crate::orchestrator::git::update_commit_message(
                                                                &save_group_path,
                                                                &save_sha,
                                                                draft,
                                                            ) {
                                                                Ok(_) => {
                                                                    op_feedback.set("Commit message updated.".to_string());
                                                                    editing_commit.set(None);
                                                                    selected_commit.set(String::new());
                                                                    selected_commit_details.set(None);
                                                                    selected_commit_details_error.set(String::new());
                                                                    edit_title.set(String::new());
                                                                    edit_message.set(String::new());
                                                                    bump_refresh_nonce(git_refresh_nonce);
                                                                }
                                                                Err(error) => op_feedback.set(error.to_string()),
                                                            }
                                                            finish_operation(op_inflight);
                                                        },
                                                        "Save Edit"
                                                    }
                                                    button {
                                                        class: "git-action-btn",
                                                        disabled: op_inflight_value,
                                                        onclick: move |_| {
                                                            editing_commit.set(None);
                                                            edit_title.set(String::new());
                                                            edit_message.set(String::new());
                                                        },
                                                        "Cancel"
                                                    }
                                                }
                                                    }
                                                }
                                            } else {
                                                p { class: "git-commit-details-title", "{details.title}" }
                                                pre { class: "git-commit-details-message", "{details.message}" }
                                                if details.is_unpushed {
                                                    p { class: "git-meta", "Double-click this node to edit title and message." }
                                                } else if !snapshot.has_upstream {
                                                    p { class: "git-meta", "No upstream configured, so unpushed status is unavailable." }
                                                } else {
                                                    p { class: "git-meta", "This commit has already been pushed and is read-only." }
                                                }
                                                {
                                                    let manage_tags_group_path = active_group_path.clone();
                                                    let manage_tags_sha = details.sha.clone();
                                                    let manage_tags_open = tag_panel_open;
                                                    let manage_tags_text = if manage_tags_open {
                                                        "Hide Tags"
                                                    } else if selected_commit_tags.is_empty() {
                                                        "Add Tag"
                                                    } else {
                                                        "Manage Tags"
                                                    };
                                                    let selected_commit_tags_for_toggle = selected_commit_tags.clone();
                                                    rsx! {
                                                        div { class: "git-detail-actions",
                                                            button {
                                                                class: "git-action-btn",
                                                                disabled: op_inflight_value,
                                                                onclick: move |_| {
                                                                    if manage_tags_open {
                                                                        tag_panel_commit.set(None);
                                                                        clear_tag_editor(
                                                                            tag_original_name,
                                                                            tag_name,
                                                                            tag_message,
                                                                            tag_error,
                                                                        );
                                                                        return;
                                                                    }

                                                                    tag_panel_commit.set(Some(manage_tags_sha.clone()));
                                                                    if let Some(tag) = selected_commit_tags_for_toggle.first() {
                                                                        load_tag_editor(
                                                                            &manage_tags_group_path,
                                                                            &tag.name,
                                                                            tag_original_name,
                                                                            tag_name,
                                                                            tag_message,
                                                                            tag_error,
                                                                        );
                                                                    } else {
                                                                        clear_tag_editor(
                                                                            tag_original_name,
                                                                            tag_name,
                                                                            tag_message,
                                                                            tag_error,
                                                                        );
                                                                    }
                                                                },
                                                                "{manage_tags_text}"
                                                            }
                                                        }
                                                    }
                                                }
                                                if tag_panel_open {
                                                    div { class: "git-tag-section",
                                                        h6 { "Tags" }
                                                        p { class: "git-meta", "Click a tag chip to edit it, or leave the form blank to create a new tag on this commit." }
                                                        if selected_commit_tags.is_empty() {
                                                            p { class: "git-meta", "No tags on this commit yet." }
                                                        } else {
                                                            div { class: "git-tag-list",
                                                                for tag in selected_commit_tags.clone() {
                                                                    {
                                                                        let chip_class = if tag_original_name_value.as_deref()
                                                                            == Some(tag.name.as_str())
                                                                        {
                                                                            "git-tag-chip selected"
                                                                        } else {
                                                                            "git-tag-chip"
                                                                        };
                                                                        let chip_group_path = active_group_path.clone();
                                                                        let chip_tag_name = tag.name.clone();
                                                                        rsx! {
                                                                            button {
                                                                                class: "{chip_class}",
                                                                                r#type: "button",
                                                                                disabled: op_inflight_value,
                                                                                onclick: move |_| {
                                                                                    load_tag_editor(
                                                                                        &chip_group_path,
                                                                                        &chip_tag_name,
                                                                                        tag_original_name,
                                                                                        tag_name,
                                                                                        tag_message,
                                                                                        tag_error,
                                                                                    );
                                                                                },
                                                                                "{tag.name}"
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        if !tag_error_value.is_empty() {
                                                            p { class: "git-meta", "{tag_error_value}" }
                                                        }
                                                        input {
                                                            class: "git-input",
                                                            disabled: op_inflight_value,
                                                            placeholder: "Tag name (e.g. v1.2.0)",
                                                            value: "{tag_name_value}",
                                                            oninput: move |event| tag_name.set(event.value()),
                                                        }
                                                        textarea {
                                                            class: "git-textarea",
                                                            disabled: op_inflight_value,
                                                            rows: "2",
                                                            placeholder: "Tag annotation message",
                                                            value: "{tag_message_value}",
                                                            oninput: move |event| tag_message.set(event.value()),
                                                        }
                                                        div { class: "git-change-actions",
                                                            {
                                                                let tag_group_path = active_group_path.clone();
                                                                let tag_target_sha = details.sha.clone();
                                                                let existing_tag_name = tag_original_name_value.clone();
                                                                let action_label = if existing_tag_name.is_some() {
                                                                    "Save Tag"
                                                                } else {
                                                                    "Create Tag"
                                                                };
                                                                rsx! {
                                                                    button {
                                                                        class: "git-action-btn git-action-primary",
                                                                        disabled: op_inflight_value,
                                                                        onclick: move |_| {
                                                                            if !start_operation(op_inflight, op_feedback) {
                                                                                return;
                                                                            }

                                                                            let next_name = tag_name.read().trim().to_string();
                                                                            let next_message = tag_message.read().trim().to_string();
                                                                            let existing_tag = tag_original_name.read().clone();
                                                                            let result = match existing_tag.clone() {
                                                                                Some(current_name) => crate::orchestrator::git::update_tag(
                                                                                    &tag_group_path,
                                                                                    &current_name,
                                                                                    &next_name,
                                                                                    &next_message,
                                                                                    &tag_target_sha,
                                                                                ),
                                                                                None => crate::orchestrator::git::create_tag(
                                                                                    &tag_group_path,
                                                                                    &next_name,
                                                                                    &next_message,
                                                                                    &tag_target_sha,
                                                                                ),
                                                                            };

                                                                            match result {
                                                                                Ok(_) => {
                                                                                    tag_original_name.set(Some(next_name.clone()));
                                                                                    tag_error.set(String::new());
                                                                                    tag_panel_commit.set(Some(tag_target_sha.clone()));
                                                                                    let feedback = if existing_tag.is_some() {
                                                                                        format!("Tag '{}' updated.", next_name)
                                                                                    } else {
                                                                                        format!("Tag '{}' created.", next_name)
                                                                                    };
                                                                                    op_feedback.set(feedback);
                                                                                    bump_refresh_nonce(git_refresh_nonce);
                                                                                }
                                                                                Err(error) => op_feedback.set(error.to_string()),
                                                                            }
                                                                            finish_operation(op_inflight);
                                                                        },
                                                                        "{action_label}"
                                                                    }
                                                                }
                                                            }
                                                            if tag_original_name_value.is_some() {
                                                                {
                                                                    let delete_group_path = active_group_path.clone();
                                                                    let delete_target_sha = details.sha.clone();
                                                                    rsx! {
                                                                        button {
                                                                            class: "git-action-btn",
                                                                            disabled: op_inflight_value,
                                                                            onclick: move |_| {
                                                                                let Some(existing_tag_name) = tag_original_name.read().clone() else {
                                                                                    op_feedback.set("Select a tag to remove.".to_string());
                                                                                    return;
                                                                                };
                                                                                if !start_operation(op_inflight, op_feedback) {
                                                                                    return;
                                                                                }
                                                                                match crate::orchestrator::git::delete_tag(
                                                                                    &delete_group_path,
                                                                                    &existing_tag_name,
                                                                                ) {
                                                                                    Ok(_) => {
                                                                                        clear_tag_editor(
                                                                                            tag_original_name,
                                                                                            tag_name,
                                                                                            tag_message,
                                                                                            tag_error,
                                                                                        );
                                                                                        tag_panel_commit.set(Some(delete_target_sha.clone()));
                                                                                        op_feedback.set(format!(
                                                                                            "Tag '{}' removed.",
                                                                                            existing_tag_name
                                                                                        ));
                                                                                        bump_refresh_nonce(git_refresh_nonce);
                                                                                    }
                                                                                    Err(error) => op_feedback.set(error.to_string()),
                                                                                }
                                                                                finish_operation(op_inflight);
                                                                            },
                                                                            "Delete Tag"
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            button {
                                                                class: "git-action-btn",
                                                                disabled: op_inflight_value,
                                                                onclick: move |_| {
                                                                    clear_tag_editor(
                                                                        tag_original_name,
                                                                        tag_name,
                                                                        tag_message,
                                                                        tag_error,
                                                                    );
                                                                    tag_panel_commit.set(Some(details.sha.clone()));
                                                                },
                                                                if tag_original_name_value.is_some() { "New Tag" } else { "Clear" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    } else if !selected_commit_details_error_value.is_empty() {
                                        p { class: "git-meta", "{selected_commit_details_error_value}" }
                                    }
                                    button {
                                        class: "git-action-btn",
                                        disabled: op_inflight_value,
                                        onclick: move |_| {
                                            if selected_commit_for_checkout.trim().is_empty() {
                                                op_feedback.set("Select a commit first.".to_string());
                                                return;
                                            }
                                            if !start_operation(op_inflight, op_feedback) {
                                                return;
                                            }
                                            let result = crate::orchestrator::git::checkout_target(&commit_checkout_group_path, CheckoutTarget::Commit(selected_commit_for_checkout.clone()));
                                            match result {
                                                Ok(_) => {
                                                    op_feedback.set(format!(
                                                        "Checked out commit {}.",
                                                        selected_commit_for_checkout
                                                    ));
                                                    bump_refresh_nonce(git_refresh_nonce);
                                                }
                                                Err(error) => op_feedback.set(error.to_string()),
                                            }
                                            finish_operation(op_inflight);
                                        },
                                        "Checkout Selected Commit"
                                    }
                                }

                                section { class: "git-section",
                                    h4 { "Changed Files" }
                                    if snapshot.changes.is_empty() {
                                        p { class: "git-meta", "Working tree clean." }
                                    } else {
                                        div { class: "git-change-list",
                                            for change in snapshot.changes.clone() {
                                                {
                                                    let change_path = change.path.clone();
                                                    let path_for_stage = change.path.clone();
                                                    let path_for_unstage = change.path.clone();
                                                    let stageable = change.is_unstaged || change.is_untracked;
                                                    let unstageable = change.is_staged;
                                                    let stage_group_path = active_group_path.clone();
                                                    let unstage_group_path = active_group_path.clone();
                                                    rsx! {
                                                        div { class: "git-change-row",
                                                            span { class: "git-change-code", "{change.code}" }
                                                            span { class: "git-change-path", "{change_path}" }
                                                            div { class: "git-change-actions",
                                                                if stageable {
                                                                    button {
                                                                        class: "git-action-btn",
                                                                        disabled: op_inflight_value,
                                                                        onclick: move |_| {
                                                                            if !start_operation(op_inflight, op_feedback) {
                                                                                return;
                                                                            }
                                                                            let results = crate::orchestrator::git::stage_files(
                                                                                &stage_group_path,
                                                                                std::slice::from_ref(&path_for_stage),
                                                                            );
                                                                            if let Some(error) = results
                                                                                .iter()
                                                                                .find_map(|result| result.error.clone())
                                                                            {
                                                                                op_feedback.set(error.to_string());
                                                                            } else {
                                                                                op_feedback
                                                                                    .set(format!("Staged {}.", path_for_stage));
                                                                                bump_refresh_nonce(git_refresh_nonce);
                                                                            }
                                                                            finish_operation(op_inflight);
                                                                        },
                                                                        "Stage"
                                                                    }
                                                                }
                                                                if unstageable {
                                                                    button {
                                                                        class: "git-action-btn",
                                                                        disabled: op_inflight_value,
                                                                        onclick: move |_| {
                                                                            if !start_operation(op_inflight, op_feedback) {
                                                                                return;
                                                                            }
                                                                            let results = crate::orchestrator::git::unstage_files(
                                                                                &unstage_group_path,
                                                                                std::slice::from_ref(&path_for_unstage),
                                                                            );
                                                                            if let Some(error) = results
                                                                                .iter()
                                                                                .find_map(|result| result.error.clone())
                                                                            {
                                                                                op_feedback.set(error.to_string());
                                                                            } else {
                                                                                op_feedback
                                                                                    .set(format!("Unstaged {}.", path_for_unstage));
                                                                                bump_refresh_nonce(git_refresh_nonce);
                                                                            }
                                                                            finish_operation(op_inflight);
                                                                        },
                                                                        "Unstage"
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

                                section { class: "git-section",
                                    h4 { "Commit" }
                                    input {
                                        class: "git-input",
                                        disabled: op_inflight_value,
                                        placeholder: "Commit title",
                                        value: "{commit_title_value}",
                                        oninput: move |event| commit_title.set(event.value()),
                                    }
                                    textarea {
                                        class: "git-textarea",
                                        disabled: op_inflight_value,
                                        rows: "3",
                                        placeholder: "Commit message body",
                                        value: "{commit_message_value}",
                                        oninput: move |event| commit_message.set(event.value()),
                                    }
                                    button {
                                        class: "git-action-btn git-action-primary",
                                        disabled: op_inflight_value,
                                        onclick: move |_| {
                                            if !start_operation(op_inflight, op_feedback) {
                                                return;
                                            }
                                            let draft = CommitDraft {
                                                title: commit_title.read().clone(),
                                                message: commit_message.read().clone(),
                                            };
                                            match crate::orchestrator::git::create_commit(&commit_group_path, draft) {
                                                Ok(_) => {
                                                    op_feedback.set("Commit created.".to_string());
                                                    commit_title.set(String::new());
                                                    commit_message.set(String::new());
                                                    bump_refresh_nonce(git_refresh_nonce);
                                                }
                                                Err(error) => op_feedback.set(error.to_string()),
                                            }
                                            finish_operation(op_inflight);
                                        },
                                        "Create Commit"
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                p { class: "git-empty", "No active path-group selected." }
            }

            if !op_feedback_value.is_empty() {
                p { class: "git-feedback", "{op_feedback_value}" }
            }
        }
    }
}

fn start_operation(mut op_inflight: Signal<bool>, mut op_feedback: Signal<String>) -> bool {
    if *op_inflight.read() {
        op_feedback.set("Another Git operation is already running.".to_string());
        return false;
    }

    op_inflight.set(true);
    true
}

fn finish_operation(mut op_inflight: Signal<bool>) {
    op_inflight.set(false);
}

fn lane_color(lane: usize) -> &'static str {
    const LANE_COLORS: [&str; 8] = [
        "#71c6ff", "#7af8b2", "#f5d167", "#ff9bb9", "#b7a8ff", "#8ee7d6", "#ffb77a", "#c8ff7a",
    ];
    LANE_COLORS[lane % LANE_COLORS.len()]
}

fn load_commit_details_for_selection(
    group_path: &str,
    sha: &str,
    mut selected_commit_details: Signal<Option<CommitDetails>>,
    mut selected_commit_details_error: Signal<String>,
) {
    selected_commit_details_error.set(String::new());
    match crate::orchestrator::git::load_commit_details(group_path, sha) {
        Ok(details) => selected_commit_details.set(Some(details)),
        Err(error) => {
            selected_commit_details.set(None);
            selected_commit_details_error.set(error.to_string());
        }
    }
}

fn load_tag_editor(
    group_path: &str,
    tag_name_value: &str,
    tag_original_name: Signal<Option<String>>,
    tag_name: Signal<String>,
    tag_message: Signal<String>,
    mut tag_error: Signal<String>,
) {
    tag_error.set(String::new());
    match crate::orchestrator::git::load_tag_details(group_path, tag_name_value) {
        Ok(details) => apply_tag_details(details, tag_original_name, tag_name, tag_message),
        Err(error) => {
            clear_tag_editor(tag_original_name, tag_name, tag_message, tag_error);
            tag_error.set(error.to_string());
        }
    }
}

fn apply_tag_details(
    details: TagDetails,
    mut tag_original_name: Signal<Option<String>>,
    mut tag_name: Signal<String>,
    mut tag_message: Signal<String>,
) {
    let _ = (&details.target_sha, details.annotated);
    tag_original_name.set(Some(details.name.clone()));
    tag_name.set(details.name);
    tag_message.set(details.message);
}

fn clear_tag_editor(
    mut tag_original_name: Signal<Option<String>>,
    mut tag_name: Signal<String>,
    mut tag_message: Signal<String>,
    mut tag_error: Signal<String>,
) {
    tag_original_name.set(None);
    tag_name.set(String::new());
    tag_message.set(String::new());
    tag_error.set(String::new());
}

fn tags_by_target_sha(tags: &[TagInfo]) -> HashMap<String, Vec<TagInfo>> {
    let mut grouped = HashMap::<String, Vec<TagInfo>>::new();
    for tag in tags {
        grouped
            .entry(tag.target_sha.clone())
            .or_default()
            .push(tag.clone());
    }
    grouped
}
