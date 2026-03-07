use crate::state::{AppState, GroupId, SessionId};
use crate::terminal::TerminalManager;
use crate::ui::file_browser_scan::{
    FileBrowserListing, ScanRequest, can_navigate_up, canonical_dir, compute_recursive_dir_stats,
    parent_within_root, scan_directory,
};
use dioxus::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const FILE_BROWSER_REFRESH_MS: u64 = 2_000;
const FILE_BROWSER_IDLE_POLL_MS: u64 = 1_000;

#[derive(Clone, Debug)]
struct SelectedEntryStats {
    relative_path: String,
    is_dir: bool,
    file_size_bytes: Option<u64>,
    recursive_size_bytes: Option<u64>,
    recursive_file_count: Option<u64>,
    modified: bool,
    ignored: bool,
}

#[component]
pub(crate) fn FileBrowserPanel(
    app_state: Signal<AppState>,
    terminal_manager: Signal<Arc<TerminalManager>>,
    group_id: GroupId,
    active_group_path: String,
) -> Element {
    let root_dir_value = normalize_group_root(&active_group_path);
    let mut root_dir = use_signal(|| root_dir_value.clone());
    let mut current_dir = use_signal(|| root_dir_value.clone());
    let mut selected_path = use_signal(|| None::<String>);
    let mut selected_stats = use_signal(|| None::<SelectedEntryStats>);
    let mut selected_stats_loading = use_signal(|| false);
    let listing = use_signal(FileBrowserListing::default);
    let loading = use_signal(|| true);
    let mut panel_feedback = use_signal(String::new);
    let mut refresh_nonce = use_signal(|| 0_u64);

    if *root_dir.read() != root_dir_value {
        root_dir.set(root_dir_value.clone());
        current_dir.set(root_dir_value);
        selected_path.set(None);
        selected_stats.set(None);
        selected_stats_loading.set(false);
        panel_feedback.set(String::new());
        let next = refresh_nonce.read().saturating_add(1);
        refresh_nonce.set(next);
    }

    {
        let mut listing = listing;
        let mut loading = loading;
        let mut panel_feedback = panel_feedback;
        use_future(move || async move {
            let mut last_seen_refresh_nonce = u64::MAX;
            let mut last_scan_started = Instant::now();

            loop {
                let scan_request = ScanRequest {
                    root_dir: root_dir.read().clone(),
                    current_dir: current_dir.read().clone(),
                };
                let refresh_nonce_now = *refresh_nonce.read();
                let due =
                    last_scan_started.elapsed() >= Duration::from_millis(FILE_BROWSER_REFRESH_MS);
                let forced = refresh_nonce_now != last_seen_refresh_nonce;

                if due || forced {
                    last_seen_refresh_nonce = refresh_nonce_now;
                    last_scan_started = Instant::now();
                    loading.set(true);

                    let scan_result =
                        tokio::task::spawn_blocking(move || scan_directory(scan_request)).await;
                    match scan_result {
                        Ok(Ok(snapshot)) => {
                            if let Some(warning) = snapshot.git_warning.clone() {
                                panel_feedback.set(warning);
                            } else {
                                panel_feedback.set(String::new());
                            }
                            listing.set(snapshot);
                        }
                        Ok(Err(error)) => {
                            panel_feedback.set(error);
                            listing.set(FileBrowserListing::default());
                        }
                        Err(error) => {
                            panel_feedback.set(format!("Background scan failed: {error}"));
                            listing.set(FileBrowserListing::default());
                        }
                    }

                    loading.set(false);
                }

                tokio::time::sleep(Duration::from_millis(FILE_BROWSER_IDLE_POLL_MS)).await;
            }
        });
    }

    let listing_snapshot = listing.read().clone();
    let current_dir_value = if listing_snapshot.current_dir.is_empty() {
        current_dir.read().clone()
    } else {
        listing_snapshot.current_dir.clone()
    };
    let root_dir_snapshot = if listing_snapshot.root_dir.is_empty() {
        root_dir.read().clone()
    } else {
        listing_snapshot.root_dir.clone()
    };
    let entries = listing_snapshot.entries.clone();
    let selected_path_value = selected_path.read().clone();
    let selected_stats_value = selected_stats.read().clone();
    let selected_stats_loading_value = *selected_stats_loading.read();
    let panel_feedback_value = panel_feedback.read().clone();
    let breadcrumb_items = build_breadcrumb_items(&root_dir_snapshot, &current_dir_value);
    let can_go_up = can_navigate_up(&root_dir_snapshot, &current_dir_value);
    let selected_is_missing = selected_path_value
        .as_ref()
        .is_some_and(|path| !entries.iter().any(|entry| &entry.path == path));

    if selected_is_missing {
        selected_path.set(None);
        selected_stats.set(None);
        selected_stats_loading.set(false);
    }

    rsx! {
        article { class: "file-browser-card",
            div { class: "file-browser-head",
                h3 { "Files" }
                p { "Root: {root_dir_snapshot}" }
            }

            div { class: "file-browser-toolbar",
                button {
                    class: "file-browser-action-btn",
                    disabled: !can_go_up,
                    onclick: move |_| {
                        if let Some(parent) = parent_within_root(&current_dir_value, &root_dir_snapshot) {
                            current_dir.set(parent);
                            selected_path.set(None);
                            selected_stats.set(None);
                            selected_stats_loading.set(false);
                            let next = refresh_nonce.read().saturating_add(1);
                            refresh_nonce.set(next);
                        }
                    },
                    "Up"
                }
                button {
                    class: "file-browser-action-btn",
                    onclick: move |_| {
                        let next = refresh_nonce.read().saturating_add(1);
                        refresh_nonce.set(next);
                    },
                    "Refresh"
                }
                if let Some(repo_root) = listing_snapshot.repo_root.clone() {
                    p { class: "file-browser-meta", "Git root: {repo_root}" }
                } else {
                    p { class: "file-browser-meta", "Git root: (none)" }
                }
            }

            div { class: "file-browser-breadcrumbs",
                for crumb in breadcrumb_items {
                    {
                        let crumb_path = crumb.path.clone();
                        rsx! {
                            button {
                                class: "file-browser-crumb",
                                r#type: "button",
                                onclick: move |_| {
                                    current_dir.set(crumb_path.clone());
                                    selected_path.set(None);
                                    selected_stats.set(None);
                                    selected_stats_loading.set(false);
                                    let next = refresh_nonce.read().saturating_add(1);
                                    refresh_nonce.set(next);
                                },
                                "{crumb.label}"
                            }
                        }
                    }
                }
            }

            div { class: "file-browser-body",
                if entries.is_empty() {
                    p { class: "file-browser-empty", "No files or folders found in this directory." }
                } else {
                    div { class: "file-browser-list",
                        for entry in entries {
                            {
                                let is_selected = selected_path_value.as_ref() == Some(&entry.path);
                                let mut row_classes = vec!["file-browser-row"];
                                if is_selected {
                                    row_classes.push("selected");
                                }
                                if entry.modified {
                                    row_classes.push("modified");
                                }
                                if entry.ignored {
                                    row_classes.push("ignored");
                                }
                                let row_class = row_classes.join(" ");
                                let entry_for_click = entry.clone();
                                let entry_for_double_click = entry.clone();
                                let entry_for_keydown = entry.clone();
                                let selected_path_for_stats = selected_path;
                                let root_for_click = root_dir_snapshot.clone();
                                let root_for_keydown = root_dir_snapshot.clone();
                                let current_for_keydown = current_dir_value.clone();
                                let mut selected_stats = selected_stats;
                                let mut selected_stats_loading = selected_stats_loading;
                                let mut panel_feedback = panel_feedback;

                                rsx! {
                                    button {
                                        class: "{row_class}",
                                        r#type: "button",
                                        onclick: move |_| {
                                            selected_path.set(Some(entry_for_click.path.clone()));
                                            selected_stats.set(Some(SelectedEntryStats {
                                                relative_path: path_relative_to_root(
                                                    &entry_for_click.path,
                                                    &root_for_click,
                                                ),
                                                is_dir: entry_for_click.is_dir,
                                                file_size_bytes: entry_for_click.file_size_bytes,
                                                recursive_size_bytes: None,
                                                recursive_file_count: None,
                                                modified: entry_for_click.modified,
                                                ignored: entry_for_click.ignored,
                                            }));

                                            if entry_for_click.is_dir {
                                                selected_stats_loading.set(true);
                                                let path_for_stats = entry_for_click.path.clone();
                                                let path_for_stats_check = path_for_stats.clone();
                                                let root_for_stats = root_for_click.clone();
                                                let selected_path_signal = selected_path_for_stats;
                                                spawn(async move {
                                                    let stats_result = tokio::task::spawn_blocking(move || {
                                                        compute_recursive_dir_stats(PathBuf::from(path_for_stats))
                                                    })
                                                    .await;

                                                    if selected_path_signal.read().as_ref() != Some(&path_for_stats_check) {
                                                        return;
                                                    }

                                                    match stats_result {
                                                        Ok(Ok((size, count))) => {
                                                            selected_stats.set(Some(SelectedEntryStats {
                                                                relative_path: path_relative_to_root(
                                                                    &path_for_stats_check,
                                                                    &root_for_stats,
                                                                ),
                                                                is_dir: true,
                                                                file_size_bytes: None,
                                                                recursive_size_bytes: Some(size),
                                                                recursive_file_count: Some(count),
                                                                modified: entry_for_click.modified,
                                                                ignored: entry_for_click.ignored,
                                                            }));
                                                            selected_stats_loading.set(false);
                                                        }
                                                        Ok(Err(error)) => {
                                                            panel_feedback.set(error);
                                                            selected_stats_loading.set(false);
                                                        }
                                                        Err(error) => {
                                                            panel_feedback.set(format!(
                                                                "Directory stats failed: {error}"
                                                            ));
                                                            selected_stats_loading.set(false);
                                                        }
                                                    }
                                                });
                                            } else {
                                                selected_stats_loading.set(false);
                                            }
                                        },
                                        ondoubleclick: move |_| {
                                            if entry_for_double_click.is_dir {
                                                current_dir.set(entry_for_double_click.path.clone());
                                                selected_path.set(None);
                                                selected_stats.set(None);
                                                selected_stats_loading.set(false);
                                                let next = refresh_nonce.read().saturating_add(1);
                                                refresh_nonce.set(next);
                                                return;
                                            }

                                            match open_file_in_group_terminal(
                                                app_state,
                                                terminal_manager,
                                                group_id,
                                                &entry_for_double_click.path,
                                            ) {
                                                Ok(message) => panel_feedback.set(message),
                                                Err(error) => panel_feedback.set(error),
                                            }
                                        },
                                        onkeydown: move |event| {
                                            match event.key() {
                                                Key::Enter => {
                                                    event.prevent_default();
                                                    if entry_for_keydown.is_dir {
                                                        current_dir.set(entry_for_keydown.path.clone());
                                                        selected_path.set(None);
                                                        selected_stats.set(None);
                                                        selected_stats_loading.set(false);
                                                        let next = refresh_nonce.read().saturating_add(1);
                                                        refresh_nonce.set(next);
                                                        return;
                                                    }

                                                    match open_file_in_group_terminal(
                                                        app_state,
                                                        terminal_manager,
                                                        group_id,
                                                        &entry_for_keydown.path,
                                                    ) {
                                                        Ok(message) => panel_feedback.set(message),
                                                        Err(error) => panel_feedback.set(error),
                                                    }
                                                }
                                                Key::Backspace => {
                                                    if let Some(parent) = parent_within_root(
                                                        &current_for_keydown,
                                                        &root_for_keydown,
                                                    ) {
                                                        event.prevent_default();
                                                        current_dir.set(parent);
                                                        selected_path.set(None);
                                                        selected_stats.set(None);
                                                        selected_stats_loading.set(false);
                                                        let next = refresh_nonce.read().saturating_add(1);
                                                        refresh_nonce.set(next);
                                                    }
                                                }
                                                _ => {}
                                            }
                                        },
                                        div { class: "file-browser-row-main",
                                            p { class: "file-browser-row-name",
                                                if entry.is_dir {
                                                    "{entry.name}/"
                                                } else {
                                                    "{entry.name}"
                                                }
                                            }
                                            p { class: "file-browser-row-sub",
                                                if entry.is_dir {
                                                    "directory"
                                                } else {
                                                    "file"
                                                }
                                                if let Some(file_size) = entry.file_size_bytes {
                                                    " | "
                                                    "{format_bytes(file_size)}"
                                                }
                                            }
                                        }
                                        div { class: "file-browser-row-badges",
                                            if entry.modified {
                                                span { class: "file-browser-badge modified", "MOD" }
                                            }
                                            if entry.ignored {
                                                span { class: "file-browser-badge ignored", "IGN" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div { class: "file-browser-stats",
                if let Some(stats) = selected_stats_value {
                    p { class: "file-browser-stats-head", "Selected: {stats.relative_path}" }
                    p { class: "file-browser-stats-line",
                        "Type: "
                        if stats.is_dir { "directory" } else { "file" }
                    }
                    p { class: "file-browser-stats-line",
                        "Modified: "
                        if stats.modified { "yes" } else { "no" }
                        " | Ignored: "
                        if stats.ignored { "yes" } else { "no" }
                    }
                    if stats.is_dir {
                        if selected_stats_loading_value {
                            p { class: "file-browser-stats-line", "Recursive stats: calculating..." }
                        } else {
                            p { class: "file-browser-stats-line",
                                "Recursive size: "
                                {stats
                                    .recursive_size_bytes
                                    .map(format_bytes)
                                    .unwrap_or_else(|| "n/a".to_string())}
                            }
                            p { class: "file-browser-stats-line",
                                "Recursive file count: "
                                {stats
                                    .recursive_file_count
                                    .map(|count| count.to_string())
                                    .unwrap_or_else(|| "n/a".to_string())}
                            }
                        }
                    } else {
                        p { class: "file-browser-stats-line",
                            "File size: "
                            {stats
                                .file_size_bytes
                                .map(format_bytes)
                                .unwrap_or_else(|| "n/a".to_string())}
                        }
                    }
                } else {
                    p { class: "file-browser-stats-head", "Select a file or folder to view stats." }
                }
            }

            p { class: "file-browser-tip", "Double click: open file in terminal editor or enter directory." }

            if !panel_feedback_value.trim().is_empty() {
                p { class: "file-browser-feedback", "{panel_feedback_value}" }
            }
        }
    }
}

fn open_file_in_group_terminal(
    app_state: Signal<AppState>,
    terminal_manager: Signal<Arc<TerminalManager>>,
    group_id: GroupId,
    file_path: &str,
) -> Result<String, String> {
    let target_session_id = select_group_terminal(app_state, group_id)
        .ok_or_else(|| "No active terminal found in this path group.".to_string())?;

    let editor = std::env::var("VISUAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("EDITOR")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| "nvim".to_string());

    let command = format!("{editor} {}", shell_quote(file_path));
    terminal_manager
        .read()
        .send_line(target_session_id, &command)
        .map_err(|error| format!("Failed to send open-file command: {error}"))?;

    Ok(format!(
        "Opened '{}' in terminal {}.",
        file_path, target_session_id
    ))
}

fn select_group_terminal(app_state: Signal<AppState>, group_id: GroupId) -> Option<SessionId> {
    let snapshot = app_state.read().clone();

    if let Some(selected_id) = snapshot.selected_session()
        && snapshot
            .sessions()
            .iter()
            .any(|session| session.id == selected_id && session.group_id == group_id)
    {
        return Some(selected_id);
    }

    snapshot
        .sessions()
        .iter()
        .find(|session| session.group_id == group_id)
        .map(|session| session.id)
}

fn shell_quote(input: &str) -> String {
    let escaped = input.replace('\'', "'\\''");
    format!("'{escaped}'")
}

fn normalize_group_root(path: &str) -> String {
    canonical_dir(Path::new(path)).map_or_else(
        |_| PathBuf::from(path).to_string_lossy().into_owned(),
        |canonical| canonical.to_string_lossy().into_owned(),
    )
}

#[derive(Clone)]
struct BreadcrumbItem {
    label: String,
    path: String,
}

fn build_breadcrumb_items(root_dir: &str, current_dir: &str) -> Vec<BreadcrumbItem> {
    let root = PathBuf::from(root_dir);
    let current = PathBuf::from(current_dir);

    let mut items = vec![BreadcrumbItem {
        label: root
            .file_name()
            .and_then(|name| name.to_str())
            .map_or_else(|| root.to_string_lossy().into_owned(), ToString::to_string),
        path: root_dir.to_string(),
    }];

    if let Ok(relative) = current.strip_prefix(&root) {
        let mut composed = root.clone();
        for component in relative.components() {
            composed.push(component.as_os_str());
            items.push(BreadcrumbItem {
                label: component.as_os_str().to_string_lossy().into_owned(),
                path: composed.to_string_lossy().into_owned(),
            });
        }
    }

    items
}

fn path_relative_to_root(path: &str, root_dir: &str) -> String {
    let root = PathBuf::from(root_dir);
    let absolute = PathBuf::from(path);
    absolute
        .strip_prefix(root)
        .map(|relative| relative.to_string_lossy().into_owned())
        .unwrap_or_else(|_| absolute.to_string_lossy().into_owned())
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;

    let value = bytes as f64;
    if value >= GIB {
        return format!("{:.2} GiB", value / GIB);
    }
    if value >= MIB {
        return format!("{:.2} MiB", value / MIB);
    }
    if value >= KIB {
        return format!("{:.2} KiB", value / KIB);
    }

    format!("{bytes} B")
}

#[cfg(test)]
mod tests {
    use super::format_bytes;

    #[test]
    fn format_bytes_uses_human_readable_units() {
        assert_eq!(format_bytes(12), "12 B");
        assert_eq!(format_bytes(1024), "1.00 KiB");
    }
}
