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
