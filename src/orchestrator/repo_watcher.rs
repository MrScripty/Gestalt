use crate::orchestrator::events::{OrchestratorEvent, RepoFsChanged, event_bus};
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

    let watched_group_path = group_path.to_string();
    let watched_repo_root = repo_root;
    let (stop_tx, stop_rx) = channel::<()>();
    let join_handle = thread::Builder::new()
        .name("git-repo-monitor".to_string())
        .spawn(move || {
            let mut last_fingerprint =
                crate::git::repo_change_fingerprint_from_root(&watched_repo_root).ok();
            loop {
                match stop_rx.recv_timeout(Duration::from_millis(WATCHER_POLL_MS)) {
                    Ok(_) => break,
                    Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {}
                }

                let next_fingerprint =
                    crate::git::repo_change_fingerprint_from_root(&watched_repo_root).ok();
                if next_fingerprint.is_some() && next_fingerprint != last_fingerprint {
                    last_fingerprint = next_fingerprint;
                    event_bus().publish(OrchestratorEvent::RepoFsChanged(RepoFsChanged {
                        group_path: watched_group_path.clone(),
                    }));
                }
            }
        })
        .ok()?;

    Some(RepoWatcherHandle {
        stop_tx,
        join_handle: Some(join_handle),
    })
}

#[cfg(test)]
mod tests {
    use super::start_active_repo_watcher;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

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
