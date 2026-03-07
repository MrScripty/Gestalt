use crate::emily_bridge::EmilyBridge;
use crate::state::{AppState, GroupId, SessionId};
use crate::terminal::{PersistedTerminalState, TerminalManager};
use std::collections::{HashMap, HashSet};

pub const ACTIVE_GROUP_STARTUP_HISTORY_LINES: usize = 400;
pub const DEFERRED_SESSION_STARTUP_HISTORY_LINES: usize = 80;
pub const DEFERRED_SESSION_START_BATCH_SIZE: usize = 1;
pub const STARTUP_BACKGROUND_TICK_MS: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStartupPriority {
    ActiveGroupVisible,
    ActiveGroupDeferred,
    BackgroundDeferred,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStartupTarget {
    pub session_id: SessionId,
    pub group_id: GroupId,
    pub path: String,
    pub history_line_limit: usize,
    pub priority: SessionStartupPriority,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HistoryLoadState {
    pub before_sequence: Option<u64>,
    pub is_loading: bool,
    pub exhausted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryLoadUpdate {
    pub session_id: SessionId,
    pub state: HistoryLoadState,
}

#[derive(Debug, Default)]
pub struct StartupTickResult {
    pub history_updates: Vec<HistoryLoadUpdate>,
    pub failed_session_ids: Vec<SessionId>,
}

#[derive(Default)]
pub struct StartupCoordinator {
    started_session_ids: HashSet<SessionId>,
    pending_history_loads: HashMap<SessionId, usize>,
}

impl StartupCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_deferred_targets(&self, state: &AppState) -> bool {
        has_deferred_startup_targets(state, &self.started_session_ids)
    }

    pub fn prune_for_sessions(&mut self, state: &AppState) {
        let active_session_ids = state
            .sessions
            .iter()
            .map(|session| session.id)
            .collect::<HashSet<_>>();
        self.started_session_ids
            .retain(|session_id| active_session_ids.contains(session_id));
        self.pending_history_loads
            .retain(|session_id, _| active_session_ids.contains(session_id));
    }

    pub async fn load_pending_history(
        &mut self,
        state: &AppState,
        emily_bridge: &EmilyBridge,
        terminal_manager: &TerminalManager,
    ) -> Vec<HistoryLoadUpdate> {
        self.prune_for_sessions(state);
        self.load_pending_history_updates(emily_bridge, terminal_manager)
            .await
    }

    pub fn start_due_sessions(
        &mut self,
        state: &AppState,
        terminal_manager: &TerminalManager,
        restored: &mut HashMap<SessionId, PersistedTerminalState>,
    ) -> StartupTickResult {
        self.prune_for_sessions(state);
        let mut result = StartupTickResult::default();
        let mut deferred_starts = 0_usize;

        for target in startup_targets(state) {
            if self.started_session_ids.contains(&target.session_id) {
                continue;
            }
            let start_now = matches!(target.priority, SessionStartupPriority::ActiveGroupVisible)
                || deferred_starts < DEFERRED_SESSION_START_BATCH_SIZE;
            if !start_now {
                continue;
            }

            match self.start_session_target(terminal_manager, restored, &target) {
                Ok(update) => {
                    result.history_updates.push(update);
                    self.started_session_ids.insert(target.session_id);
                    if !matches!(target.priority, SessionStartupPriority::ActiveGroupVisible) {
                        deferred_starts += 1;
                    }
                }
                Err(()) => result.failed_session_ids.push(target.session_id),
            }
        }

        result
    }

    fn start_session_target(
        &mut self,
        terminal_manager: &TerminalManager,
        restored: &mut HashMap<SessionId, PersistedTerminalState>,
        target: &SessionStartupTarget,
    ) -> Result<HistoryLoadUpdate, ()> {
        if let Some(restored_terminal) = restored.remove(&target.session_id) {
            terminal_manager.seed_restored_terminal(restored_terminal);
        }

        self.pending_history_loads
            .insert(target.session_id, target.history_line_limit);
        terminal_manager
            .ensure_session(target.session_id, &target.path)
            .map_err(|_| ())?;

        Ok(HistoryLoadUpdate {
            session_id: target.session_id,
            state: HistoryLoadState {
                before_sequence: None,
                is_loading: true,
                exhausted: false,
            },
        })
    }

    async fn load_pending_history_updates(
        &mut self,
        emily_bridge: &EmilyBridge,
        terminal_manager: &TerminalManager,
    ) -> Vec<HistoryLoadUpdate> {
        let pending = self
            .pending_history_loads
            .iter()
            .map(|(session_id, limit)| (*session_id, *limit))
            .collect::<Vec<_>>();
        let mut updates = Vec::new();

        for (session_id, limit) in pending {
            let result = emily_bridge
                .page_history_before_async(session_id, None, limit)
                .await;
            let Ok(mut chunk) = result else {
                continue;
            };

            chunk.lines.reverse();
            let inserted = terminal_manager
                .prepend_history_lines(session_id, &chunk.lines)
                .unwrap_or(0);
            updates.push(HistoryLoadUpdate {
                session_id,
                state: HistoryLoadState {
                    before_sequence: chunk.next_before_sequence,
                    is_loading: false,
                    exhausted: chunk.next_before_sequence.is_none() || inserted == 0,
                },
            });
            self.pending_history_loads.remove(&session_id);
        }

        updates
    }
}

pub fn visible_active_group_session_ids(state: &AppState) -> Vec<SessionId> {
    state
        .active_group_id()
        .map(|group_id| state.workspace_session_ids_for_group(group_id))
        .unwrap_or_default()
}

pub fn startup_targets(state: &AppState) -> Vec<SessionStartupTarget> {
    let visible_active_sessions = visible_active_group_session_ids(state)
        .into_iter()
        .collect::<HashSet<_>>();
    let active_group_id = state.active_group_id();

    let mut immediate = Vec::new();
    let mut active_deferred = Vec::new();
    let mut background = Vec::new();

    for session in &state.sessions {
        let Some(path) = state.group_path(session.group_id) else {
            continue;
        };

        let target = if visible_active_sessions.contains(&session.id) {
            SessionStartupTarget {
                session_id: session.id,
                group_id: session.group_id,
                path: path.to_string(),
                history_line_limit: ACTIVE_GROUP_STARTUP_HISTORY_LINES,
                priority: SessionStartupPriority::ActiveGroupVisible,
            }
        } else if Some(session.group_id) == active_group_id {
            SessionStartupTarget {
                session_id: session.id,
                group_id: session.group_id,
                path: path.to_string(),
                history_line_limit: DEFERRED_SESSION_STARTUP_HISTORY_LINES,
                priority: SessionStartupPriority::ActiveGroupDeferred,
            }
        } else {
            SessionStartupTarget {
                session_id: session.id,
                group_id: session.group_id,
                path: path.to_string(),
                history_line_limit: DEFERRED_SESSION_STARTUP_HISTORY_LINES,
                priority: SessionStartupPriority::BackgroundDeferred,
            }
        };

        match target.priority {
            SessionStartupPriority::ActiveGroupVisible => immediate.push(target),
            SessionStartupPriority::ActiveGroupDeferred => active_deferred.push(target),
            SessionStartupPriority::BackgroundDeferred => background.push(target),
        }
    }

    immediate.extend(active_deferred);
    immediate.extend(background);
    immediate
}

pub fn has_deferred_startup_targets(
    state: &AppState,
    started_session_ids: &HashSet<SessionId>,
) -> bool {
    startup_targets(state).into_iter().any(|target| {
        !started_session_ids.contains(&target.session_id)
            && target.priority != SessionStartupPriority::ActiveGroupVisible
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ACTIVE_GROUP_STARTUP_HISTORY_LINES, DEFERRED_SESSION_STARTUP_HISTORY_LINES,
        SessionStartupPriority, StartupCoordinator, has_deferred_startup_targets, startup_targets,
        visible_active_group_session_ids,
    };
    use crate::state::AppState;
    use std::collections::HashSet;

    #[test]
    fn startup_targets_prioritize_visible_active_group_sessions() {
        let mut state = AppState::default();
        let (group_id, extra_ids) = state.create_group_with_defaults("/tmp/secondary".to_string());
        let hidden_id = state.add_session(group_id);
        state.select_session(extra_ids[0]);

        let targets = startup_targets(&state);
        let visible_ids = visible_active_group_session_ids(&state);
        assert_eq!(
            targets
                .iter()
                .take(visible_ids.len())
                .map(|target| target.session_id)
                .collect::<Vec<_>>(),
            visible_ids
        );
        assert_eq!(
            targets
                .iter()
                .find(|target| target.session_id == hidden_id)
                .map(|target| target.priority),
            Some(SessionStartupPriority::ActiveGroupDeferred)
        );
        assert_eq!(
            targets
                .iter()
                .find(|target| target.session_id == extra_ids[0])
                .map(|target| target.history_line_limit),
            Some(ACTIVE_GROUP_STARTUP_HISTORY_LINES)
        );
        assert_eq!(
            targets
                .iter()
                .find(|target| target.session_id == hidden_id)
                .map(|target| target.history_line_limit),
            Some(DEFERRED_SESSION_STARTUP_HISTORY_LINES)
        );
    }

    #[test]
    fn startup_targets_place_background_groups_after_active_group() {
        let mut state = AppState::default();
        let (group_id, extra_ids) = state.create_group_with_defaults("/tmp/secondary".to_string());
        state.select_session(state.sessions[0].id);

        let targets = startup_targets(&state);
        let background_indices = extra_ids
            .iter()
            .filter_map(|session_id| {
                targets
                    .iter()
                    .position(|target| target.session_id == *session_id)
            })
            .collect::<Vec<_>>();
        let active_indices = state
            .workspace_session_ids_for_group(state.active_group_id().expect("active group"))
            .into_iter()
            .filter_map(|session_id| {
                targets
                    .iter()
                    .position(|target| target.session_id == session_id)
            })
            .collect::<Vec<_>>();

        assert!(
            background_indices
                .iter()
                .all(|idx| { active_indices.iter().all(|active_idx| active_idx < idx) })
        );
        assert_eq!(
            targets
                .iter()
                .find(|target| target.group_id == group_id)
                .map(|target| target.priority),
            Some(SessionStartupPriority::BackgroundDeferred)
        );
    }

    #[test]
    fn deferred_target_detection_ignores_started_sessions() {
        let mut state = AppState::default();
        let (_group_id, extra_ids) = state.create_group_with_defaults("/tmp/secondary".to_string());
        let mut started = visible_active_group_session_ids(&state)
            .into_iter()
            .collect::<HashSet<_>>();

        assert!(has_deferred_startup_targets(&state, &started));

        started.extend(extra_ids);
        started.extend(state.sessions.iter().map(|session| session.id));
        assert!(!has_deferred_startup_targets(&state, &started));
    }

    #[test]
    fn coordinator_reports_existing_deferred_targets() {
        let mut state = AppState::default();
        let (_group_id, _extra_ids) =
            state.create_group_with_defaults("/tmp/secondary".to_string());
        let coordinator = StartupCoordinator::new();

        assert!(coordinator.has_deferred_targets(&state));
    }
}
