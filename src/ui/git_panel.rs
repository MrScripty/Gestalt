use crate::git::{CheckoutTarget, CommitDetails, CommitDraft, RepoContext};
use crate::state::AppState;
use crate::terminal::TerminalManager;
use crate::ui::git_helpers::{bump_refresh_nonce, create_group_for_worktree, toggle_bool_signal};
use dioxus::prelude::*;
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
    let mut tag_name = use_signal(String::new);
    let mut tag_message = use_signal(String::new);
    let mut worktree_path = use_signal(String::new);
    let mut worktree_target = use_signal(String::new);
    let auto_workspace = use_signal(|| true);
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
                p { "Path context: {active_group_path}" }
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
                            let worktree_target_value = {
                                let value = worktree_target.read().trim().to_string();
                                if value.is_empty() {
                                    snapshot
                                        .current_branch
                                        .clone()
                                        .or_else(|| snapshot.head.clone())
                                        .unwrap_or_default()
                                } else {
                                    value
                                }
                            };
                            let commit_title_value = commit_title.read().clone();
                            let commit_message_value = commit_message.read().clone();
                            let edit_title_value = edit_title.read().clone();
                            let edit_message_value = edit_message.read().clone();
                            let tag_name_value = tag_name.read().clone();
                            let tag_message_value = tag_message.read().clone();
                            let worktree_path_value = worktree_path.read().clone();
                            let worktree_target_input_value = worktree_target.read().clone();
                            let auto_workspace_checked = *auto_workspace.read();
                            let graph_layout = build_commit_graph_layout(&snapshot.commits);
                            let graph_overlay_width = graph_layout.gutter_width_px;
                            let graph_overlay_height = graph_layout.overlay_height_px;
                            let recent_tags = snapshot
                                .tags
                                .iter()
                                .take(8)
                                .map(|tag| tag.name.clone())
                                .collect::<Vec<_>>()
                                .join(", ");
                            let branch_checkout_group_path = active_group_path.clone();
                            let commit_checkout_group_path = active_group_path.clone();
                            let commit_group_path = active_group_path.clone();
                            let tag_group_path = active_group_path.clone();
                            let worktree_group_path = active_group_path.clone();
                            let selected_commit_for_checkout = selected_commit_value.clone();
                            let selected_commit_for_tag = selected_commit_value.clone();
                            let worktree_target_for_create = worktree_target_value.clone();

                            rsx! {
                                div { class: "git-repo-meta",
                                    p { class: "git-meta", "Repo root: {snapshot.root}" }
                                    p { class: "git-meta",
                                        "HEAD: "
                                        {snapshot.head.clone().unwrap_or_else(|| "(none)".to_string())}
                                    }
                                    p { class: "git-meta",
                                        "Branch: "
                                        {snapshot.current_branch.clone().unwrap_or_else(|| "(detached)".to_string())}
                                    }
                                }

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
                                                    let commit_for_click = commit.clone();
                                                    let commit_for_edit = commit.clone();
                                                    let details_group_path = active_group_path.clone();
                                                    let edit_group_path = active_group_path.clone();
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
                                                        button {
                                                            class: "{selected_class}",
                                                            r#type: "button",
                                                            onclick: move |_| {
                                                                selected_commit.set(commit_sha.clone());
                                                                editing_commit.set(None);
                                                                selected_commit_details_error.set(String::new());
                                                                match crate::orchestrator::git::load_commit_details(&details_group_path, &commit_for_click.sha) {
                                                                    Ok(details) => selected_commit_details.set(Some(details)),
                                                                    Err(error) => {
                                                                        selected_commit_details.set(None);
                                                                        selected_commit_details_error.set(error.to_string());
                                                                    }
                                                                }
                                                            },
                                                            ondoubleclick: move |_| {
                                                                if !commit_for_edit.is_unpushed {
                                                                    op_feedback.set("Only unpushed commits can be edited.".to_string());
                                                                    return;
                                                                }
                                                                selected_commit.set(commit_for_edit.sha.clone());
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

                                section { class: "git-section",
                                    h4 { "Tag Release" }
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
                                    button {
                                        class: "git-action-btn",
                                        disabled: op_inflight_value,
                                        onclick: move |_| {
                                            if selected_commit_for_tag.trim().is_empty() {
                                                op_feedback.set("Select a commit to tag.".to_string());
                                                return;
                                            }
                                            if !start_operation(op_inflight, op_feedback) {
                                                return;
                                            }
                                            let tag_name_value = tag_name.read().trim().to_string();
                                            let tag_message_value = tag_message.read().trim().to_string();

                                            match crate::orchestrator::git::create_tag(
                                                &tag_group_path,
                                                &tag_name_value,
                                                &tag_message_value,
                                                &selected_commit_for_tag,
                                            ) {
                                                Ok(_) => {
                                                    op_feedback.set(format!(
                                                        "Tag '{}' created.",
                                                        tag_name_value
                                                    ));
                                                    tag_name.set(String::new());
                                                    tag_message.set(String::new());
                                                    bump_refresh_nonce(git_refresh_nonce);
                                                }
                                                Err(error) => op_feedback.set(error.to_string()),
                                            }
                                            finish_operation(op_inflight);
                                        },
                                        "Create Tag"
                                    }
                                    if !snapshot.tags.is_empty() {
                                        p { class: "git-meta", "Recent tags: {recent_tags}" }
                                    }
                                }

                                section { class: "git-section",
                                    h4 { "Create Workspace (Worktree)" }
                                    input {
                                        class: "git-input",
                                        disabled: op_inflight_value,
                                        placeholder: "/abs/path/to/new-worktree",
                                        value: "{worktree_path_value}",
                                        oninput: move |event| worktree_path.set(event.value()),
                                    }
                                    input {
                                        class: "git-input",
                                        disabled: op_inflight_value,
                                        placeholder: "Target branch or commit (defaults to current branch)",
                                        value: "{worktree_target_input_value}",
                                        oninput: move |event| worktree_target.set(event.value()),
                                    }
                                    label {
                                        class: "git-checkbox-row",
                                        input {
                                            r#type: "checkbox",
                                            checked: auto_workspace_checked,
                                            disabled: op_inflight_value,
                                            onchange: move |_| toggle_bool_signal(auto_workspace),
                                        }
                                        "Create Gestalt path-group after worktree creation"
                                    }
                                    button {
                                        class: "git-action-btn git-action-primary",
                                        disabled: op_inflight_value,
                                        onclick: move |_| {
                                            let path_value = worktree_path.read().trim().to_string();
                                            if path_value.is_empty() {
                                                op_feedback.set("Worktree path is required.".to_string());
                                                return;
                                            }
                                            if !start_operation(op_inflight, op_feedback) {
                                                return;
                                            }

                                            let result = crate::orchestrator::git::create_worktree(
                                                &worktree_group_path,
                                                &path_value,
                                                &worktree_target_for_create,
                                            );

                                            match result {
                                                Ok(_) => {
                                                    if *auto_workspace.read() {
                                                        match create_group_for_worktree(
                                                            app_state,
                                                            terminal_manager,
                                                            &path_value,
                                                        ) {
                                                            Ok(_) => {
                                                                op_feedback.set(format!(
                                                                    "Worktree created and workspace group added at {}.",
                                                                    path_value
                                                                ));
                                                            }
                                                            Err(error) => op_feedback.set(error),
                                                        }
                                                    } else {
                                                        op_feedback
                                                            .set(format!("Worktree created at {}.", path_value));
                                                    }

                                                    worktree_path.set(String::new());
                                                    worktree_target.set(String::new());
                                                    bump_refresh_nonce(git_refresh_nonce);
                                                }
                                                Err(error) => op_feedback.set(error.to_string()),
                                            }
                                            finish_operation(op_inflight);
                                        },
                                        "Create Worktree"
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
