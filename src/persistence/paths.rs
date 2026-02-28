use std::path::PathBuf;

const APP_DIR_NAME: &str = "gestalt";
const WORKSPACE_FILE_NAME: &str = "workspace.v1.json";

/// Returns the absolute workspace file path for the current platform.
pub fn workspace_path() -> PathBuf {
    if let Some(path) = std::env::var_os("GESTALT_WORKSPACE_PATH") {
        return PathBuf::from(path);
    }

    workspace_dir().join(WORKSPACE_FILE_NAME)
}

/// Returns the platform-specific root directory used for workspace state.
pub fn workspace_dir() -> PathBuf {
    platform_state_home().join(APP_DIR_NAME)
}

#[cfg(target_os = "linux")]
fn platform_state_home() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(path);
    }

    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local")
        .join("state")
}

#[cfg(target_os = "windows")]
fn platform_state_home() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

#[cfg(target_os = "macos")]
fn platform_state_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library")
        .join("Application Support")
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn platform_state_home() -> PathBuf {
    std::env::temp_dir()
}
