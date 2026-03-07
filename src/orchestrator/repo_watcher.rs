use crate::orchestrator::events::{OrchestratorEvent, RepoFsChanged, event_bus};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const WATCHER_POLL_MS: u64 = 1_000;

/// Owns an active-group repository monitor lifecycle.
pub struct RepoWatcherHandle {
    stop_tx: Sender<()>,
    join_handle: Option<JoinHandle<()>>,
}

impl RepoWatcherHandle {
    /// Requests shutdown and joins the watcher thread.
    pub fn stop(&mut self) {
        let _ = self.stop_tx.send(());
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for RepoWatcherHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Starts an active-group repository monitor that emits pub/sub events on change.
pub fn start_active_repo_watcher(group_path: &str) -> Option<RepoWatcherHandle> {
    let repo_root = crate::git::repo_root(group_path).ok()?;
    crate::git::repo_change_fingerprint_from_root(&repo_root).ok()?;
    let git_dir = crate::git::git_dir(group_path).ok();

    let watched_group_path = group_path.to_string();
    let watched_repo_root = repo_root.clone();
    let (stop_tx, stop_rx) = channel::<()>();
    let watch_paths = watch_paths(&repo_root, git_dir.as_deref());
    let join_handle = thread::Builder::new()
        .name("git-repo-monitor".to_string())
        .spawn(move || {
            if run_native_watcher(&watched_group_path, &watch_paths, &stop_rx).is_ok() {
                return;
            }

            run_fingerprint_poll_watcher(&watched_group_path, &watched_repo_root, &stop_rx);
        })
        .ok()?;

    Some(RepoWatcherHandle {
        stop_tx,
        join_handle: Some(join_handle),
    })
}

fn run_native_watcher(
    watched_group_path: &str,
    watch_paths: &[(PathBuf, RecursiveMode)],
    stop_rx: &std::sync::mpsc::Receiver<()>,
) -> notify::Result<()> {
    let group_path = watched_group_path.to_string();
    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if event.is_ok() {
                event_bus().publish(OrchestratorEvent::RepoFsChanged(RepoFsChanged {
                    group_path: group_path.clone(),
                }));
            }
        })?;

    for (path, mode) in watch_paths {
        watcher.watch(path, *mode)?;
    }

    loop {
        match stop_rx.recv() {
            Ok(_) => break,
            Err(_) => break,
        }
    }

    Ok(())
}

fn run_fingerprint_poll_watcher(
    watched_group_path: &str,
    watched_repo_root: &str,
    stop_rx: &std::sync::mpsc::Receiver<()>,
) {
    let mut last_fingerprint =
        crate::git::repo_change_fingerprint_from_root(watched_repo_root).ok();
    loop {
        match stop_rx.recv_timeout(Duration::from_millis(WATCHER_POLL_MS)) {
            Ok(_) => break,
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }

        let next_fingerprint =
            crate::git::repo_change_fingerprint_from_root(watched_repo_root).ok();
        if next_fingerprint.is_some() && next_fingerprint != last_fingerprint {
            last_fingerprint = next_fingerprint;
            event_bus().publish(OrchestratorEvent::RepoFsChanged(RepoFsChanged {
                group_path: watched_group_path.to_string(),
            }));
        }
    }
}

fn watch_paths(repo_root: &str, git_dir: Option<&str>) -> Vec<(PathBuf, RecursiveMode)> {
    let mut seen = HashSet::<PathBuf>::new();
    let mut paths = Vec::<(PathBuf, RecursiveMode)>::new();

    push_watch_path(&mut paths, &mut seen, Path::new(repo_root));
    if let Some(git_dir) = git_dir {
        push_watch_path(&mut paths, &mut seen, Path::new(git_dir));
    }

    paths
}

fn push_watch_path(
    paths: &mut Vec<(PathBuf, RecursiveMode)>,
    seen: &mut HashSet<PathBuf>,
    path: &Path,
) {
    let path = path.to_path_buf();
    if !seen.insert(path.clone()) {
        return;
    }

    let mode = if path.is_dir() {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    };
    paths.push((path, mode));
}

#[cfg(test)]
mod tests {
    use super::start_active_repo_watcher;
    use crate::orchestrator::events::{OrchestratorEvent, event_bus};
    use std::path::Path;
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    #[test]
    fn start_active_repo_watcher_returns_none_for_non_repo_path() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let path = std::env::temp_dir().join(format!("gestalt-repo-monitor-nonrepo-{nonce}"));
        std::fs::create_dir_all(&path).expect("temp dir should be created");

        let handle = start_active_repo_watcher(path.to_str().expect("path should be UTF-8"));
        assert!(handle.is_none());

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn start_active_repo_watcher_returns_handle_for_repo() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let path = std::env::temp_dir().join(format!("gestalt-repo-monitor-repo-{nonce}"));
        std::fs::create_dir_all(&path).expect("temp dir should be created");
        run_git(path.as_path(), &["init"]);
        run_git(
            path.as_path(),
            &["config", "user.email", "watcher-test@example.com"],
        );
        run_git(path.as_path(), &["config", "user.name", "Watcher Test"]);
        std::fs::write(path.join("README.md"), "test\n").expect("write should succeed");
        run_git(path.as_path(), &["add", "README.md"]);
        run_git(path.as_path(), &["commit", "-m", "chore: init"]);

        let mut handle = start_active_repo_watcher(path.to_str().expect("path should be UTF-8"))
            .expect("repo monitor should start");
        handle.stop();

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn repo_watcher_emits_fs_change_event() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let path = std::env::temp_dir().join(format!("gestalt-repo-monitor-event-{nonce}"));
        std::fs::create_dir_all(&path).expect("temp dir should be created");
        run_git(path.as_path(), &["init"]);
        run_git(
            path.as_path(),
            &["config", "user.email", "watcher-test@example.com"],
        );
        run_git(path.as_path(), &["config", "user.name", "Watcher Test"]);
        std::fs::write(path.join("README.md"), "test\n").expect("write should succeed");
        run_git(path.as_path(), &["add", "README.md"]);
        run_git(path.as_path(), &["commit", "-m", "chore: init"]);

        let group_path = path.to_str().expect("path should be UTF-8").to_string();
        let mut events = event_bus().subscribe();
        let mut handle = start_active_repo_watcher(&group_path).expect("watcher should start");

        std::fs::write(path.join("README.md"), "changed\n").expect("write should succeed");

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut saw_change = false;
        while Instant::now() < deadline {
            match events.try_recv() {
                Ok(OrchestratorEvent::RepoFsChanged(changed))
                    if changed.group_path == group_path =>
                {
                    saw_change = true;
                    break;
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {
                    thread::sleep(Duration::from_millis(50));
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                    saw_change = true;
                    break;
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
            }
        }

        handle.stop();
        let _ = std::fs::remove_dir_all(path);
        assert!(saw_change, "watcher should emit a repo fs change event");
    }

    fn run_git(cwd: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .expect("git command should run");

        if !output.status.success() {
            panic!(
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
