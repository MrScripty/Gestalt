use crate::persistence::error::PersistenceError;
use crate::persistence::paths::workspace_path;
use crate::persistence::schema::PersistedWorkspaceV1;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Loads workspace state from primary path, then backup as fallback.
pub fn load_workspace() -> Result<Option<PersistedWorkspaceV1>, PersistenceError> {
    let primary = workspace_path();
    let backup = backup_path(&primary);

    if let Some(workspace) = load_from_path(&primary)? {
        return Ok(Some(workspace));
    }

    load_from_path(&backup)
}

/// Saves workspace state atomically and writes a rolling backup.
pub fn save_workspace(workspace: &PersistedWorkspaceV1) -> Result<(), PersistenceError> {
    let path = workspace_path();
    let Some(parent_dir) = path.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent_dir).map_err(|source| PersistenceError::CreateDirectory {
        path: parent_dir.to_path_buf(),
        source,
    })?;

    let payload = workspace
        .clone()
        .without_terminal_history()
        .with_saved_at_unix(unix_timestamp_seconds());
    let serialized = serde_json::to_vec_pretty(&payload)?;

    let temp_path = temp_path(&path);
    let mut file = File::create(&temp_path).map_err(|source| PersistenceError::WriteTempFile {
        path: temp_path.clone(),
        source,
    })?;
    file.write_all(&serialized)
        .map_err(|source| PersistenceError::WriteTempFile {
            path: temp_path.clone(),
            source,
        })?;
    file.sync_all()
        .map_err(|source| PersistenceError::FlushTempFile {
            path: temp_path.clone(),
            source,
        })?;
    drop(file);

    if path.exists() {
        let _ = std::fs::copy(&path, backup_path(&path));
    }

    std::fs::rename(&temp_path, &path).map_err(|source| PersistenceError::AtomicRename {
        from: temp_path,
        to: path,
        source,
    })?;

    Ok(())
}

fn load_from_path(path: &Path) -> Result<Option<PersistedWorkspaceV1>, PersistenceError> {
    if !path.exists() {
        return Ok(None);
    }

    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(source) => {
            return Err(PersistenceError::ReadFile {
                path: path.to_path_buf(),
                source,
            });
        }
    };

    match serde_json::from_str::<PersistedWorkspaceV1>(&contents) {
        Ok(workspace) => Ok(Some(workspace)),
        Err(_) => {
            quarantine_corrupt(path);
            Ok(None)
        }
    }
}

fn backup_path(path: &Path) -> PathBuf {
    append_to_filename(path, ".bak")
}

fn temp_path(path: &Path) -> PathBuf {
    append_to_filename(path, ".tmp")
}

fn append_to_filename(path: &Path, suffix: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("workspace.json");
    path.with_file_name(format!("{file_name}{suffix}"))
}

fn quarantine_corrupt(path: &Path) {
    let quarantined = append_to_filename(path, ".corrupt");
    let _ = std::fs::rename(path, quarantined);
}

fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}
