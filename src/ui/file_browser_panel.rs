use crate::git::GitError;
use crate::state::{AppState, GroupId, SessionId};
use crate::terminal::TerminalManager;
use dioxus::prelude::*;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

const FILE_BROWSER_REFRESH_MS: u64 = 2_000;
const FILE_BROWSER_LOOP_TICK_MS: u64 = 220;

#[derive(Clone, Debug, Default)]
struct FileBrowserListing {
    root_dir: String,
    current_dir: String,
    entries: Vec<FileBrowserEntry>,
    repo_root: Option<String>,
    git_warning: Option<String>,
}

#[derive(Clone, Debug)]
struct FileBrowserEntry {
    name: String,
    path: String,
    is_dir: bool,
    file_size_bytes: Option<u64>,
    modified: bool,
    ignored: bool,
}

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

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScanRequest {
    root_dir: String,
    current_dir: String,
}

#[derive(Clone, Debug, Default)]
struct GitPathMarks {
    repo_root: Option<PathBuf>,
    modified_paths: HashSet<String>,
    ignored_paths: HashSet<String>,
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
            let mut last_scan_request = None::<ScanRequest>;
            let mut last_scan_started = Instant::now();

            loop {
                let scan_request = ScanRequest {
                    root_dir: root_dir.read().clone(),
                    current_dir: current_dir.read().clone(),
                };
                let refresh_nonce_now = *refresh_nonce.read();
                let due =
                    last_scan_started.elapsed() >= Duration::from_millis(FILE_BROWSER_REFRESH_MS);
                let request_changed = last_scan_request.as_ref() != Some(&scan_request);
                let forced = refresh_nonce_now != last_seen_refresh_nonce;

                if request_changed || due || forced {
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
                    last_scan_request = Some(ScanRequest {
                        root_dir: root_dir.read().clone(),
                        current_dir: current_dir.read().clone(),
                    });
                }

                tokio::time::sleep(Duration::from_millis(FILE_BROWSER_LOOP_TICK_MS)).await;
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

fn scan_directory(request: ScanRequest) -> Result<FileBrowserListing, String> {
    let root = canonical_dir(Path::new(&request.root_dir))?;
    let mut current =
        canonical_dir(Path::new(&request.current_dir)).unwrap_or_else(|_| root.clone());
    if !current.starts_with(&root) {
        current = root.clone();
    }

    let mut git_warning = None::<String>;
    let marks = match load_git_marks(&root) {
        Ok(marks) => marks,
        Err(error) => {
            git_warning = Some(error);
            GitPathMarks::default()
        }
    };

    let mut entries = fs::read_dir(&current)
        .map_err(|error| format!("Failed to read directory '{}': {error}", current.display()))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.is_empty() {
                return None;
            }

            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).ok()?;
            let is_dir = metadata.is_dir();
            let file_size_bytes = if metadata.is_file() || metadata.file_type().is_symlink() {
                Some(metadata.len())
            } else {
                None
            };
            let (modified, ignored) = marks.marker_for_path(&path, is_dir);

            Some(FileBrowserEntry {
                name,
                path: path.to_string_lossy().into_owned(),
                is_dir,
                file_size_bytes,
                modified,
                ignored,
            })
        })
        .collect::<Vec<_>>();

    entries.sort_by(file_browser_entry_ordering);

    Ok(FileBrowserListing {
        root_dir: root.to_string_lossy().into_owned(),
        current_dir: current.to_string_lossy().into_owned(),
        entries,
        repo_root: marks
            .repo_root
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        git_warning,
    })
}

impl GitPathMarks {
    fn marker_for_path(&self, absolute_path: &Path, is_dir: bool) -> (bool, bool) {
        let Some(repo_root) = self.repo_root.as_ref() else {
            return (false, false);
        };

        let Some(relative_path) = repo_relative_path(repo_root, absolute_path) else {
            return (false, false);
        };

        let modified = path_has_marker(&self.modified_paths, &relative_path, is_dir);
        let ignored = path_has_marker(&self.ignored_paths, &relative_path, is_dir);
        (modified, ignored)
    }
}

fn load_git_marks(root: &Path) -> Result<GitPathMarks, String> {
    let root_text = root.to_string_lossy().into_owned();
    let repo_root_text = match crate::git::repo_root(&root_text) {
        Ok(path) => path,
        Err(GitError::NotRepo { .. }) => return Ok(GitPathMarks::default()),
        Err(error) => return Err(error.to_string()),
    };

    let output = Command::new("git")
        .current_dir(&repo_root_text)
        .args([
            "-c",
            "core.quotepath=false",
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignored=matching",
        ])
        .output()
        .map_err(|error| format!("Failed running git status: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!(
            "git status failed for '{}': {}",
            repo_root_text,
            if stderr.is_empty() {
                "unknown error".to_string()
            } else {
                stderr
            }
        ));
    }

    let status_output = String::from_utf8_lossy(&output.stdout).to_string();
    let (modified_paths, ignored_paths) = parse_status_with_ignored(&status_output);

    Ok(GitPathMarks {
        repo_root: Some(PathBuf::from(repo_root_text)),
        modified_paths,
        ignored_paths,
    })
}

fn parse_status_with_ignored(output: &str) -> (HashSet<String>, HashSet<String>) {
    let mut modified_paths = HashSet::new();
    let mut ignored_paths = HashSet::new();

    for line in output.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() || trimmed.starts_with("##") {
            continue;
        }

        if let Some(path) = trimmed.strip_prefix("!! ") {
            let normalized = normalize_status_path(path);
            if !normalized.is_empty() {
                ignored_paths.insert(normalized);
            }
            continue;
        }

        if let Some(path) = trimmed.strip_prefix("?? ") {
            let normalized = normalize_status_path(path);
            if !normalized.is_empty() {
                modified_paths.insert(normalized);
            }
            continue;
        }

        if trimmed.len() < 4 {
            continue;
        }

        let status = &trimmed[..2];
        if status == "  " {
            continue;
        }

        let raw_path = trimmed[3..].trim();
        if raw_path.is_empty() {
            continue;
        }

        let normalized = normalize_status_path(raw_path);
        if !normalized.is_empty() {
            modified_paths.insert(normalized);
        }
    }

    (modified_paths, ignored_paths)
}

fn normalize_status_path(raw_path: &str) -> String {
    let renamed = raw_path
        .rsplit_once(" -> ")
        .map(|(_, destination)| destination)
        .unwrap_or(raw_path);

    renamed
        .trim()
        .trim_matches('"')
        .trim_end_matches('/')
        .replace('\\', "/")
}

fn path_has_marker(marked_paths: &HashSet<String>, relative_path: &str, is_dir: bool) -> bool {
    if marked_paths.is_empty() {
        return false;
    }
    if relative_path.is_empty() {
        return !marked_paths.is_empty();
    }

    if marked_paths.contains(relative_path) {
        return true;
    }

    let mut cursor = relative_path;
    while let Some((parent, _)) = cursor.rsplit_once('/') {
        if marked_paths.contains(parent) {
            return true;
        }
        cursor = parent;
    }

    if is_dir {
        let prefix = format!("{relative_path}/");
        return marked_paths.iter().any(|path| path.starts_with(&prefix));
    }

    false
}

fn repo_relative_path(repo_root: &Path, absolute_path: &Path) -> Option<String> {
    let relative = absolute_path.strip_prefix(repo_root).ok()?;
    Some(relative.to_string_lossy().replace('\\', "/"))
}

fn canonical_dir(path: &Path) -> Result<PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("Path '{}' is not accessible: {error}", path.display()))?;
    if !canonical.is_dir() {
        return Err(format!(
            "Path '{}' is not a directory.",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn parent_within_root(current_dir: &str, root_dir: &str) -> Option<String> {
    let current = PathBuf::from(current_dir);
    let root = PathBuf::from(root_dir);
    if current == root {
        return None;
    }

    let parent = current.parent()?.to_path_buf();
    if !parent.starts_with(&root) {
        return None;
    }

    Some(parent.to_string_lossy().into_owned())
}

fn can_navigate_up(root_dir: &str, current_dir: &str) -> bool {
    parent_within_root(current_dir, root_dir).is_some()
}

fn compute_recursive_dir_stats(path: PathBuf) -> Result<(u64, u64), String> {
    let mut total_size = 0_u64;
    let mut file_count = 0_u64;
    let mut stack = vec![path];

    while let Some(current) = stack.pop() {
        let entries = fs::read_dir(&current)
            .map_err(|error| format!("Failed to read '{}': {error}", current.display()))?;

        for entry in entries.filter_map(|entry| entry.ok()) {
            let child_path = entry.path();
            let metadata = match fs::symlink_metadata(&child_path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };

            if metadata.is_dir() {
                stack.push(child_path);
                continue;
            }

            if metadata.is_file() || metadata.file_type().is_symlink() {
                file_count = file_count.saturating_add(1);
                total_size = total_size.saturating_add(metadata.len());
            }
        }
    }

    Ok((total_size, file_count))
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

    if let Some(selected_id) = snapshot.selected_session
        && snapshot
            .sessions
            .iter()
            .any(|session| session.id == selected_id && session.group_id == group_id)
    {
        return Some(selected_id);
    }

    snapshot
        .sessions
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

fn file_browser_entry_ordering(left: &FileBrowserEntry, right: &FileBrowserEntry) -> Ordering {
    match (left.is_dir, right.is_dir) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => left
            .name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.name.cmp(&right.name)),
    }
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
    use super::{
        FileBrowserEntry, can_navigate_up, compute_recursive_dir_stats,
        file_browser_entry_ordering, format_bytes, normalize_status_path,
        parse_status_with_ignored, path_has_marker,
    };
    use std::collections::HashSet;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parse_status_with_ignored_tracks_modified_and_ignored_paths() {
        let input = concat!(
            "## main\n",
            " M src/ui.rs\n",
            "R  old/path.rs -> new/path.rs\n",
            "?? docs/new.md\n",
            "!! target/\n"
        );

        let (modified, ignored) = parse_status_with_ignored(input);

        assert!(modified.contains("src/ui.rs"));
        assert!(modified.contains("new/path.rs"));
        assert!(modified.contains("docs/new.md"));
        assert!(ignored.contains("target"));
    }

    #[test]
    fn marker_lookup_handles_ancestors_and_descendants() {
        let mut marked = HashSet::new();
        marked.insert("target".to_string());
        marked.insert("src/lib.rs".to_string());

        assert!(path_has_marker(&marked, "target", true));
        assert!(path_has_marker(&marked, "target/debug", true));
        assert!(path_has_marker(&marked, "target/debug/log.txt", false));
        assert!(path_has_marker(&marked, "src", true));
        assert!(path_has_marker(&marked, "src/lib.rs", false));
        assert!(!path_has_marker(&marked, "README.md", false));
    }

    #[test]
    fn normalize_status_path_extracts_rename_destination() {
        assert_eq!(
            normalize_status_path("old/path.rs -> src/new/path.rs"),
            "src/new/path.rs"
        );
        assert_eq!(normalize_status_path("target/"), "target");
    }

    #[test]
    fn entry_sorting_groups_directories_first() {
        let mut entries = [
            FileBrowserEntry {
                name: "zeta.txt".to_string(),
                path: "zeta.txt".to_string(),
                is_dir: false,
                file_size_bytes: Some(1),
                modified: false,
                ignored: false,
            },
            FileBrowserEntry {
                name: "src".to_string(),
                path: "src".to_string(),
                is_dir: true,
                file_size_bytes: None,
                modified: false,
                ignored: false,
            },
            FileBrowserEntry {
                name: "alpha.txt".to_string(),
                path: "alpha.txt".to_string(),
                is_dir: false,
                file_size_bytes: Some(1),
                modified: false,
                ignored: false,
            },
        ];

        entries.sort_by(file_browser_entry_ordering);

        assert_eq!(entries[0].name, "src");
        assert_eq!(entries[1].name, "alpha.txt");
        assert_eq!(entries[2].name, "zeta.txt");
    }

    #[test]
    fn recursive_stats_count_files_and_sizes() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let root = std::env::temp_dir().join(format!("gestalt-file-browser-stats-{nonce}"));
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("nested dir should be created");

        let file_a = root.join("a.txt");
        let file_b = nested.join("b.txt");
        fs::write(&file_a, "1234").expect("file a write should succeed");
        fs::write(&file_b, "123456").expect("file b write should succeed");

        let (size, count) =
            compute_recursive_dir_stats(Path::new(&root).to_path_buf()).expect("stats should load");
        assert_eq!(count, 2);
        assert_eq!(size, 10);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn navigate_up_only_within_root() {
        let root = "/tmp/gestalt-root";
        assert!(!can_navigate_up(root, root));
        assert!(can_navigate_up(root, "/tmp/gestalt-root/src"));
    }

    #[test]
    fn format_bytes_uses_human_readable_units() {
        assert_eq!(format_bytes(12), "12 B");
        assert_eq!(format_bytes(1024), "1.00 KiB");
    }
}
