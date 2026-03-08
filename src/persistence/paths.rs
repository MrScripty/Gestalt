use std::path::PathBuf;

const APP_DIR_NAME: &str = "gestalt";
const WORKSPACE_FILE_NAME: &str = "workspace.v1.json";

/// Returns the absolute workspace file path for the current platform.
pub fn workspace_path() -> PathBuf {
    if let Some(path) = std::env::var_os("GESTALT_WORKSPACE_PATH") {
        return PathBuf::from(path);
    }

    default_workspace_path()
}

/// Returns the default host workspace file path, ignoring `GESTALT_WORKSPACE_PATH`.
pub fn default_workspace_path() -> PathBuf {
    default_workspace_dir().join(WORKSPACE_FILE_NAME)
}

/// Returns the platform-specific root directory used for workspace state.
pub fn workspace_dir() -> PathBuf {
    platform_state_home().join(APP_DIR_NAME)
}

/// Returns the platform default root directory used for host workspace state.
pub fn default_workspace_dir() -> PathBuf {
    platform_default_state_home().join(APP_DIR_NAME)
}

#[cfg(target_os = "linux")]
fn platform_state_home() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(path);
    }

    platform_default_state_home()
}

#[cfg(target_os = "linux")]
fn platform_default_state_home() -> PathBuf {
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

#[cfg(target_os = "windows")]
fn platform_default_state_home() -> PathBuf {
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

#[cfg(target_os = "macos")]
fn platform_default_state_home() -> PathBuf {
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

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn platform_default_state_home() -> PathBuf {
    std::env::temp_dir()
}
