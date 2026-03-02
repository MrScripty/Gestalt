use std::path::{Path, PathBuf};

/// Validates and normalizes a workspace group path selected by the user.
pub(crate) fn validate_group_path(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Path is required.".to_string());
    }

    let candidate = Path::new(trimmed);
    let absolute = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("Could not resolve current directory: {error}"))?
            .join(candidate)
    };

    let canonical = absolute
        .canonicalize()
        .map_err(|_| format!("Path does not exist: {trimmed}"))?;

    if !canonical.is_dir() {
        return Err("Path must be a directory.".to_string());
    }

    Ok(canonical.to_string_lossy().into_owned())
}

/// Validates and normalizes a new worktree path before creation.
pub(crate) fn validate_new_worktree_path(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Worktree path is required.".to_string());
    }

    let candidate = Path::new(trimmed);
    let absolute = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("Could not resolve current directory: {error}"))?
            .join(candidate)
    };

    let parent = absolute
        .parent()
        .ok_or_else(|| "Worktree path must include a parent directory.".to_string())?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|_| format!("Worktree parent does not exist: {}", parent.display()))?;
    if !canonical_parent.is_dir() {
        return Err("Worktree parent must be a directory.".to_string());
    }

    let leaf = absolute
        .file_name()
        .ok_or_else(|| "Worktree path must include a directory name.".to_string())?;

    let normalized = canonical_parent.join(leaf);
    Ok(normalized.to_string_lossy().into_owned())
}

/// Derives a directory path from a selected file-system entry.
pub(crate) fn derive_directory_from_selection(path: PathBuf) -> PathBuf {
    if path.is_dir() {
        return path;
    }

    path.parent().map_or(path.clone(), Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_group_path_rejects_empty_input() {
        let result = validate_group_path("   ");
        assert!(result.is_err());
    }

    #[test]
    fn validate_new_worktree_path_rejects_empty_input() {
        let result = validate_new_worktree_path("   ");
        assert!(result.is_err());
    }

    #[test]
    fn validate_new_worktree_path_resolves_relative_path() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let root = std::env::temp_dir().join(format!("gestalt-path-validation-worktree-{nonce}"));
        std::fs::create_dir_all(&root).expect("temp root should be created");

        let original = std::env::current_dir().expect("cwd should be available");
        std::env::set_current_dir(&root).expect("cwd switch should succeed");

        let result = validate_new_worktree_path("new-worktree");
        std::env::set_current_dir(original).expect("cwd restore should succeed");

        let _ = std::fs::remove_dir_all(&root);

        let normalized = result.expect("relative path should validate");
        assert!(normalized.ends_with("new-worktree"));
    }

    #[test]
    fn validate_new_worktree_path_rejects_missing_parent() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let missing_parent = std::env::temp_dir().join(format!("missing-parent-{nonce}"));
        let target = missing_parent.join("worktree");

        let result = validate_new_worktree_path(target.to_string_lossy().as_ref());
        assert!(result.is_err());
    }

    #[test]
    fn derive_directory_from_selection_uses_parent_for_files() {
        let file_path = std::env::temp_dir()
            .join("gestalt-path-validation")
            .join("child.txt");
        let derived = derive_directory_from_selection(file_path);
        assert_eq!(
            derived,
            std::env::temp_dir().join("gestalt-path-validation")
        );
    }
}
