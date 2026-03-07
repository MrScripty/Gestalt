use crate::orchestrator::{
    GroupOrchestratorSnapshot, SessionRuntimeView, snapshot_group_from_runtime,
};
use crate::state::{AppState, GroupId, GroupLayout, Session, SessionId, SessionStatus};
use crate::terminal::{TerminalManager, TerminalSnapshot};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const STATUS_BUSY_ACTIVITY_WINDOW_MS: i64 = 900;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStatusCounts {
    pub idle: usize,
    pub busy: usize,
    pub error: usize,
}

#[derive(Clone)]
pub struct TerminalPaneProjection {
    pub session: Session,
    pub terminal: Arc<TerminalSnapshot>,
    pub cwd: String,
    pub is_runtime_ready: bool,
    pub is_selected: bool,
    pub is_focused: bool,
}

#[derive(Clone)]
pub struct WorkspaceProjection {
    pub group_id: GroupId,
    pub group_path: String,
    pub agents: Vec<TerminalPaneProjection>,
    pub runner: Option<TerminalPaneProjection>,
    pub layout: GroupLayout,
    pub status_counts: SessionStatusCounts,
    pub orchestrator: GroupOrchestratorSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionStatusUpdate {
    pub session_id: SessionId,
    pub status: SessionStatus,
}

pub fn active_workspace_projection(
    app_state: &AppState,
    terminal_manager: &TerminalManager,
    focused_session: Option<SessionId>,
) -> Option<WorkspaceProjection> {
    let group_id = app_state.active_group_id()?;
    let group_path = app_state.group_path(group_id).unwrap_or(".").to_string();
    let (agents, runner) = app_state.workspace_sessions_for_group(group_id);
    let pane_by_session = app_state
        .sessions_in_group(group_id)
        .into_iter()
        .map(|session| {
            let runtime_snapshot = terminal_manager.snapshot_shared(session.id);
            let is_runtime_ready = runtime_snapshot.is_some();
            let terminal =
                runtime_snapshot.unwrap_or_else(|| Arc::new(pending_terminal_snapshot()));
            let cwd = terminal_manager
                .session_cwd(session.id)
                .unwrap_or_else(|| group_path.clone());
            let pane = TerminalPaneProjection {
                is_selected: app_state.selected_session() == Some(session.id),
                is_focused: focused_session == Some(session.id),
                session,
                terminal,
                cwd,
                is_runtime_ready,
            };
            (pane.session.id, pane)
        })
        .collect::<HashMap<SessionId, TerminalPaneProjection>>();

    let runtime_by_session = pane_by_session
        .iter()
        .map(|(session_id, pane)| {
            (
                *session_id,
                SessionRuntimeView {
                    lines: &pane.terminal.lines,
                    cwd: pane.cwd.as_str(),
                    is_runtime_ready: pane.is_runtime_ready,
                },
            )
        })
        .collect::<HashMap<SessionId, SessionRuntimeView<'_>>>();

    Some(WorkspaceProjection {
        group_id,
        group_path: group_path.clone(),
        agents: agents
            .into_iter()
            .filter_map(|session| pane_by_session.get(&session.id).cloned())
            .collect(),
        runner: runner.and_then(|session| pane_by_session.get(&session.id).cloned()),
        layout: app_state.group_layout(group_id),
        status_counts: SessionStatusCounts {
            idle: app_state.session_count_by_status(SessionStatus::Idle),
            busy: app_state.session_count_by_status(SessionStatus::Busy),
            error: app_state.session_count_by_status(SessionStatus::Error),
        },
        orchestrator: snapshot_group_from_runtime(
            app_state,
            group_id,
            focused_session,
            &runtime_by_session,
        ),
    })
}

pub fn apply_session_activity(
    app_state: &AppState,
    terminal_manager: &TerminalManager,
    session_id: SessionId,
    idle_deadlines: &mut HashMap<SessionId, tokio::time::Instant>,
) -> Option<SessionStatusUpdate> {
    let now_ms = unix_now_ms();
    let last_activity = terminal_manager.session_last_activity_unix_ms(session_id);
    let current_status = app_state
        .sessions()
        .iter()
        .find(|session| session.id == session_id)
        .map(|session| session.status);
    let current_status = match current_status {
        Some(status) => status,
        None => {
            idle_deadlines.remove(&session_id);
            return None;
        }
    };

    if let Some(deadline) = idle_deadline_from_activity(last_activity, now_ms) {
        idle_deadlines.insert(session_id, deadline);
    } else {
        idle_deadlines.remove(&session_id);
    }

    let next_status = derive_session_status_from_activity(current_status, last_activity, now_ms);
    (next_status != current_status).then_some(SessionStatusUpdate {
        session_id,
        status: next_status,
    })
}

pub fn reconcile_session_statuses(
    app_state: &AppState,
    terminal_manager: &TerminalManager,
    idle_deadlines: &mut HashMap<SessionId, tokio::time::Instant>,
) -> Vec<SessionStatusUpdate> {
    let now_ms = unix_now_ms();
    let mut pending_updates = Vec::<SessionStatusUpdate>::new();
    let tracked_session_ids = app_state
        .sessions()
        .iter()
        .map(|session| {
            let last_activity = terminal_manager.session_last_activity_unix_ms(session.id);
            if let Some(deadline) = idle_deadline_from_activity(last_activity, now_ms) {
                idle_deadlines.insert(session.id, deadline);
            } else {
                idle_deadlines.remove(&session.id);
            }

            let next_status =
                derive_session_status_from_activity(session.status, last_activity, now_ms);
            if next_status != session.status {
                pending_updates.push(SessionStatusUpdate {
                    session_id: session.id,
                    status: next_status,
                });
            }
            session.id
        })
        .collect::<Vec<_>>();

    idle_deadlines.retain(|session_id, _| tracked_session_ids.contains(session_id));
    pending_updates
}

fn derive_session_status_from_activity(
    current_status: SessionStatus,
    last_activity_unix_ms: Option<i64>,
    now_unix_ms: i64,
) -> SessionStatus {
    let is_recent = is_recent_activity(last_activity_unix_ms, now_unix_ms);
    match current_status {
        SessionStatus::Error => {
            if is_recent {
                SessionStatus::Busy
            } else if last_activity_unix_ms.unwrap_or(0) > 0 {
                SessionStatus::Idle
            } else {
                SessionStatus::Error
            }
        }
        SessionStatus::Busy | SessionStatus::Idle => {
            if is_recent {
                SessionStatus::Busy
            } else {
                SessionStatus::Idle
            }
        }
    }
}

fn idle_deadline_from_activity(
    last_activity_unix_ms: Option<i64>,
    now_unix_ms: i64,
) -> Option<tokio::time::Instant> {
    if !is_recent_activity(last_activity_unix_ms, now_unix_ms) {
        return None;
    }

    let remaining_ms =
        STATUS_BUSY_ACTIVITY_WINDOW_MS.saturating_sub(now_unix_ms - last_activity_unix_ms?);
    Some(tokio::time::Instant::now() + Duration::from_millis(remaining_ms as u64))
}

fn is_recent_activity(last_activity_unix_ms: Option<i64>, now_unix_ms: i64) -> bool {
    let Some(last_activity_unix_ms) = last_activity_unix_ms else {
        return false;
    };
    if last_activity_unix_ms <= 0 {
        return false;
    }
    if now_unix_ms < last_activity_unix_ms {
        return true;
    }
    now_unix_ms - last_activity_unix_ms <= STATUS_BUSY_ACTIVITY_WINDOW_MS
}

fn unix_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn pending_terminal_snapshot() -> TerminalSnapshot {
    let rows = 42_u16;
    let cols = 140_u16;
    let mut lines = vec![String::new(); usize::from(rows)];
    lines[0] = "# Terminal pending startup".to_string();
    TerminalSnapshot {
        lines,
        rows,
        cols,
        cursor_row: 0,
        cursor_col: 0,
        hide_cursor: false,
        bracketed_paste: false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SessionStatusCounts, active_workspace_projection, derive_session_status_from_activity,
        reconcile_session_statuses,
    };
    use crate::state::{AppState, SessionStatus};
    use crate::terminal::TerminalManager;
    use std::collections::HashMap;

    #[test]
    fn active_workspace_projection_returns_pending_terminals_for_unstarted_sessions() {
        let state = AppState::default();
        let terminal_manager = TerminalManager::new();

        let projection = active_workspace_projection(&state, &terminal_manager, None)
            .expect("projection should exist for default workspace");

        assert_eq!(projection.group_path, ".");
        assert_eq!(
            projection.status_counts,
            SessionStatusCounts {
                idle: 3,
                busy: 0,
                error: 0,
            }
        );
        assert_eq!(projection.agents.len(), 2);
        assert!(projection.runner.is_some());
        assert!(
            projection.agents.iter().all(|pane| !pane.is_runtime_ready
                && pane.terminal.lines[0] == "# Terminal pending startup")
        );
    }

    #[test]
    fn derive_session_status_from_activity_drops_busy_session_to_idle_when_stale() {
        let now_ms = 10_000;
        let stale_activity = Some(now_ms - 5_000);

        assert_eq!(
            derive_session_status_from_activity(SessionStatus::Busy, stale_activity, now_ms),
            SessionStatus::Idle
        );
    }

    #[test]
    fn reconcile_session_statuses_retains_tracked_deadlines_for_live_sessions() {
        let state = AppState::default();
        let terminal_manager = TerminalManager::new();
        let mut idle_deadlines = HashMap::new();
        idle_deadlines.insert(999, tokio::time::Instant::now());

        let updates = reconcile_session_statuses(&state, &terminal_manager, &mut idle_deadlines);

        assert!(updates.is_empty());
        assert!(idle_deadlines.is_empty());
    }
}
