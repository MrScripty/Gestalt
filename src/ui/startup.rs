use crate::state::{AppState, GroupId, SessionId};
use std::collections::HashSet;

pub(crate) const ACTIVE_GROUP_STARTUP_HISTORY_LINES: usize = 400;
pub(crate) const DEFERRED_SESSION_STARTUP_HISTORY_LINES: usize = 80;
pub(crate) const DEFERRED_SESSION_START_BATCH_SIZE: usize = 1;
pub(crate) const STARTUP_BACKGROUND_TICK_MS: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionStartupPriority {
    ActiveGroupVisible,
    ActiveGroupDeferred,
    BackgroundDeferred,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionStartupTarget {
    pub session_id: SessionId,
    pub group_id: GroupId,
    pub path: String,
    pub history_line_limit: usize,
    pub priority: SessionStartupPriority,
}

pub(crate) fn visible_active_group_session_ids(state: &AppState) -> Vec<SessionId> {
    state
        .active_group_id()
        .map(|group_id| state.workspace_session_ids_for_group(group_id))
        .unwrap_or_default()
}

pub(crate) fn startup_targets(state: &AppState) -> Vec<SessionStartupTarget> {
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

pub(crate) fn has_deferred_startup_targets(
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
        SessionStartupPriority, has_deferred_startup_targets, startup_targets,
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
}
