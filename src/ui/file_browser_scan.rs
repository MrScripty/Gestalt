use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub(crate) struct FileBrowserListing {
    pub(crate) root_dir: String,
    pub(crate) current_dir: String,
    pub(crate) entries: Vec<FileBrowserEntry>,
    pub(crate) repo_root: Option<String>,
    pub(crate) git_warning: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct FileBrowserEntry {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) is_dir: bool,
    pub(crate) file_size_bytes: Option<u64>,
    pub(crate) modified: bool,
    pub(crate) ignored: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScanRequest {
    pub(crate) root_dir: String,
    pub(crate) current_dir: String,
}

#[derive(Clone, Debug, Default)]
struct GitPathMarks {
    repo_root: Option<PathBuf>,
    modified_paths: HashSet<String>,
    ignored_paths: HashSet<String>,
}

pub(crate) fn scan_directory(request: ScanRequest) -> Result<FileBrowserListing, String> {
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

pub(crate) fn canonical_dir(path: &Path) -> Result<PathBuf, String> {
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

pub(crate) fn parent_within_root(current_dir: &str, root_dir: &str) -> Option<String> {
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

pub(crate) fn can_navigate_up(root_dir: &str, current_dir: &str) -> bool {
    parent_within_root(current_dir, root_dir).is_some()
}

pub(crate) fn compute_recursive_dir_stats(path: PathBuf) -> Result<(u64, u64), String> {
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
    let marks = crate::orchestrator::git::load_repo_path_marks(&root_text)
        .map_err(|error| error.to_string())?;

    Ok(GitPathMarks {
        repo_root: marks.repo_root.map(PathBuf::from),
        modified_paths: marks.modified_paths,
        ignored_paths: marks.ignored_paths,
    })
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

#[cfg(test)]
mod tests {
    use super::{
        FileBrowserEntry, can_navigate_up, compute_recursive_dir_stats,
        file_browser_entry_ordering, path_has_marker,
    };
    use std::collections::HashSet;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

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
}
