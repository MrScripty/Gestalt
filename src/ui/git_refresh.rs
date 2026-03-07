use crate::git::RepoContext;
use crate::orchestrator::events::{GitCommandExecuted, OrchestratorEvent, event_bus};
use crate::orchestrator::repo_watcher::RepoWatcherHandle;
use crate::state::AppState;
use dioxus::prelude::*;
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

const GIT_REFRESH_ACTIVE_INTERVAL_MS: u64 = 5_000;
const GIT_REFRESH_INACTIVE_INTERVAL_MS: u64 = 20_000;
const GIT_REFRESH_ACTIVE_JITTER_MS: u64 = 500;
const GIT_REFRESH_INACTIVE_JITTER_MS: u64 = 4_000;
const GIT_REFRESH_WATCHER_DEBOUNCE_MS: u64 = 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingRefresh {
    Scheduled,
    Immediate,
}

#[derive(Clone, Debug)]
struct GroupRefreshState {
    next_due_at: tokio::time::Instant,
}

/// Coordinates Git context refresh cadence across active and inactive groups.
pub(crate) fn use_git_refresh_coordinator(
    app_state: Signal<AppState>,
    mut git_context: Signal<Option<RepoContext>>,
    mut git_context_loading: Signal<bool>,
    git_refresh_nonce: Signal<u64>,
    refresh_notify: Arc<Notify>,
) {
    use_future(move || {
        let refresh_notify = refresh_notify.clone();
        async move {
            let mut events = event_bus().subscribe();
            let mut group_state: HashMap<String, GroupRefreshState> = HashMap::new();
            let mut pending: HashMap<String, PendingRefresh> = HashMap::new();
            let mut context_cache: HashMap<String, RepoContext> = HashMap::new();
            let mut watcher_debounce: HashMap<String, tokio::time::Instant> = HashMap::new();
            let mut active_path = None::<String>;
            let mut active_context_path = None::<String>;
            let mut active_watcher = None::<RepoWatcherHandle>;
            let mut nonce_seen = u64::MAX;

            loop {
                let (known_paths, active_group_path) = {
                    let state = app_state.read();
                    let known_paths = state
                        .groups()
                        .iter()
                        .map(|group| group.path.clone())
                        .collect::<Vec<_>>();
                    let active_group_path = state
                        .active_group_id()
                        .and_then(|group_id| state.group_path(group_id))
                        .map(ToString::to_string);
                    (known_paths, active_group_path)
                };
                let now = tokio::time::Instant::now();

                group_state.retain(|path, _| known_paths.contains(path));
                pending.retain(|path, _| known_paths.contains(path));
                context_cache.retain(|path, _| known_paths.contains(path));
                watcher_debounce.retain(|path, _| known_paths.contains(path));

                if active_path != active_group_path {
                    if let Some(mut watcher) = active_watcher.take() {
                        watcher.stop();
                    }
                    active_path = active_group_path.clone();
                    match active_path.as_ref() {
                        Some(path) => {
                            active_watcher =
                                crate::orchestrator::repo_watcher::start_active_repo_watcher(path);
                            if let Some(context) = context_cache.get(path).cloned() {
                                git_context_loading.set(false);
                                git_context.set(Some(context));
                                active_context_path = Some(path.clone());
                            } else {
                                git_context.set(None);
                                git_context_loading.set(true);
                                mark_pending(&mut pending, path, PendingRefresh::Immediate);
                            }
                        }
                        None => {
                            git_context.set(None);
                            git_context_loading.set(false);
                            active_context_path = None;
                        }
                    }
                }

                if drain_events(&mut events, &mut pending, &mut watcher_debounce).is_err() {
                    break;
                }

                let debounced_due = watcher_debounce
                    .iter()
                    .filter(|(_, due)| **due <= now)
                    .map(|(path, _)| path.clone())
                    .collect::<Vec<_>>();
                for path in debounced_due {
                    watcher_debounce.remove(&path);
                    mark_pending(&mut pending, &path, PendingRefresh::Immediate);
                }

                let refresh_nonce = *git_refresh_nonce.read();
                if nonce_seen != refresh_nonce {
                    nonce_seen = refresh_nonce;
                    if let Some(path) = active_path.as_ref() {
                        mark_pending(&mut pending, path, PendingRefresh::Immediate);
                    }
                }

                for path in &known_paths {
                    if !group_state.contains_key(path) {
                        let is_active = active_path.as_deref() == Some(path.as_str());
                        group_state.insert(
                            path.clone(),
                            GroupRefreshState {
                                next_due_at: now
                                    + duration_for_jittered_interval(path, is_active, now),
                            },
                        );
                    }
                }

                for path in &known_paths {
                    if let Some(state) = group_state.get(path)
                        && now >= state.next_due_at
                    {
                        mark_pending(&mut pending, path, PendingRefresh::Scheduled);
                    }
                }

                let Some(path_to_refresh) =
                    select_next_refresh_path(&pending, active_path.as_deref())
                else {
                    let next_deadline = wait_deadline(&group_state, &watcher_debounce);
                    if wait_for_refresh_wakeup(
                        &mut events,
                        &refresh_notify,
                        &mut pending,
                        &mut watcher_debounce,
                        next_deadline,
                    )
                    .await
                    .is_err()
                    {
                        break;
                    }
                    continue;
                };

                let is_active_refresh = active_path.as_deref() == Some(path_to_refresh.as_str());
                if is_active_refresh {
                    git_context_loading.set(true);
                }

                let context = load_repo_context_blocking(path_to_refresh.clone()).await;

                context_cache.insert(path_to_refresh.clone(), context.clone());

                if is_active_refresh {
                    git_context_loading.set(false);
                    git_context.set(Some(context));
                    active_context_path = Some(path_to_refresh.clone());
                } else if active_context_path.as_deref() == Some(path_to_refresh.as_str())
                    && let Some(cached) = context_cache.get(&path_to_refresh).cloned()
                {
                    git_context.set(Some(cached));
                }

                pending.remove(&path_to_refresh);
                if let Some(state) = group_state.get_mut(&path_to_refresh) {
                    let next_now = tokio::time::Instant::now();
                    state.next_due_at = next_now
                        + duration_for_jittered_interval(
                            &path_to_refresh,
                            is_active_group(&active_path, &path_to_refresh),
                            next_now,
                        );
                }

                continue;
            }
        }
    });
}

async fn wait_for_refresh_wakeup(
    events: &mut tokio::sync::broadcast::Receiver<OrchestratorEvent>,
    refresh_notify: &Notify,
    pending: &mut HashMap<String, PendingRefresh>,
    watcher_debounce: &mut HashMap<String, tokio::time::Instant>,
    next_deadline: Option<tokio::time::Instant>,
) -> Result<(), ()> {
    match next_deadline {
        Some(deadline) => {
            tokio::select! {
                event = events.recv() => {
                    match event {
                        Ok(event) => handle_event(event, pending, watcher_debounce),
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return Err(()),
                    }
                }
                _ = refresh_notify.notified() => {}
                _ = tokio::time::sleep_until(deadline) => {}
            }
        }
        None => {
            tokio::select! {
                event = events.recv() => {
                    match event {
                        Ok(event) => handle_event(event, pending, watcher_debounce),
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return Err(()),
                    }
                }
                _ = refresh_notify.notified() => {}
            }
        }
    }
    Ok(())
}

fn wait_deadline(
    group_state: &HashMap<String, GroupRefreshState>,
    watcher_debounce: &HashMap<String, tokio::time::Instant>,
) -> Option<tokio::time::Instant> {
    group_state
        .values()
        .map(|state| state.next_due_at)
        .chain(watcher_debounce.values().copied())
        .min()
}

fn drain_events(
    events: &mut tokio::sync::broadcast::Receiver<OrchestratorEvent>,
    pending: &mut HashMap<String, PendingRefresh>,
    watcher_debounce: &mut HashMap<String, tokio::time::Instant>,
) -> Result<(), ()> {
    loop {
        match events.try_recv() {
            Ok(event) => handle_event(event, pending, watcher_debounce),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => return Ok(()),
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::TryRecvError::Closed) => return Err(()),
        }
    }
}

fn handle_event(
    event: OrchestratorEvent,
    pending: &mut HashMap<String, PendingRefresh>,
    watcher_debounce: &mut HashMap<String, tokio::time::Instant>,
) {
    match event {
        OrchestratorEvent::GitCommandExecuted(GitCommandExecuted { group_path, .. }) => {
            mark_pending(pending, &group_path, PendingRefresh::Immediate);
        }
        OrchestratorEvent::RepoFsChanged(changed) => {
            watcher_debounce.insert(
                changed.group_path,
                tokio::time::Instant::now()
                    + Duration::from_millis(GIT_REFRESH_WATCHER_DEBOUNCE_MS),
            );
        }
    }
}

fn is_active_group(active_path: &Option<String>, path: &str) -> bool {
    active_path.as_deref() == Some(path)
}

fn mark_pending(
    pending: &mut HashMap<String, PendingRefresh>,
    group_path: &str,
    next: PendingRefresh,
) {
    let key = group_path.to_string();
    match pending.get(&key).copied() {
        Some(PendingRefresh::Immediate) => {}
        Some(PendingRefresh::Scheduled) if next == PendingRefresh::Immediate => {
            pending.insert(key, PendingRefresh::Immediate);
        }
        Some(_) => {}
        None => {
            pending.insert(key, next);
        }
    }
}

fn select_next_refresh_path(
    pending: &HashMap<String, PendingRefresh>,
    active_path: Option<&str>,
) -> Option<String> {
    if let Some(active_path) = active_path
        && let Some(PendingRefresh::Immediate) = pending.get(active_path)
    {
        return Some(active_path.to_string());
    }

    if let Some(path) = pending
        .iter()
        .find_map(|(path, reason)| (*reason == PendingRefresh::Immediate).then(|| path.clone()))
    {
        return Some(path);
    }

    if let Some(active_path) = active_path
        && let Some(PendingRefresh::Scheduled) = pending.get(active_path)
    {
        return Some(active_path.to_string());
    }

    pending
        .iter()
        .find_map(|(path, reason)| (*reason == PendingRefresh::Scheduled).then(|| path.clone()))
}

fn duration_for_jittered_interval(
    group_path: &str,
    active: bool,
    now: tokio::time::Instant,
) -> Duration {
    let (base_ms, jitter_ms) = if active {
        (GIT_REFRESH_ACTIVE_INTERVAL_MS, GIT_REFRESH_ACTIVE_JITTER_MS)
    } else {
        (
            GIT_REFRESH_INACTIVE_INTERVAL_MS,
            GIT_REFRESH_INACTIVE_JITTER_MS,
        )
    };

    let jitter_offset = jitter_offset_ms(group_path, now, jitter_ms);
    let interval_ms = (base_ms as i64 + jitter_offset).max(1) as u64;
    Duration::from_millis(interval_ms)
}

fn jitter_offset_ms(group_path: &str, now: tokio::time::Instant, jitter_ms: u64) -> i64 {
    if jitter_ms == 0 {
        return 0;
    }

    let mut hasher = DefaultHasher::new();
    group_path.hash(&mut hasher);
    now.hash(&mut hasher);
    let value = hasher.finish();
    let span = jitter_ms.saturating_mul(2).saturating_add(1);
    (value % span) as i64 - jitter_ms as i64
}

async fn load_repo_context_blocking(group_path: String) -> RepoContext {
    let path_for_task = group_path.clone();
    match tokio::task::spawn_blocking(move || {
        crate::orchestrator::git::load_repo_context(&path_for_task)
    })
    .await
    {
        Ok(Ok(context)) => context,
        Ok(Err(_)) | Err(_) => RepoContext::NotRepo {
            inspected_path: group_path,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PendingRefresh, duration_for_jittered_interval, jitter_offset_ms, mark_pending,
        select_next_refresh_path,
    };
    use std::collections::HashMap;
    use std::time::Duration;

    #[test]
    fn immediate_pending_overrides_scheduled() {
        let mut pending = HashMap::new();
        mark_pending(&mut pending, "/tmp/repo", PendingRefresh::Scheduled);
        mark_pending(&mut pending, "/tmp/repo", PendingRefresh::Immediate);
        assert_eq!(
            pending.get("/tmp/repo").copied(),
            Some(PendingRefresh::Immediate)
        );
    }

    #[test]
    fn refresh_selection_prioritizes_active_immediate() {
        let mut pending = HashMap::new();
        pending.insert("/tmp/a".to_string(), PendingRefresh::Immediate);
        pending.insert("/tmp/b".to_string(), PendingRefresh::Immediate);
        let selected = select_next_refresh_path(&pending, Some("/tmp/b"));
        assert_eq!(selected.as_deref(), Some("/tmp/b"));
    }

    #[test]
    fn jittered_duration_respects_interval_floor() {
        let duration =
            duration_for_jittered_interval("/tmp/repo", true, tokio::time::Instant::now());
        assert!(duration >= Duration::from_millis(1));
    }

    #[test]
    fn jitter_offset_respects_bounds() {
        let offset = jitter_offset_ms("/tmp/repo", tokio::time::Instant::now(), 4_000);
        assert!((-4_000..=4_000).contains(&offset));
    }
}
