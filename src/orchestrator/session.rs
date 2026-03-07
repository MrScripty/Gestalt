use crate::state::{AppState, GroupId, SessionId, SessionStatus};
use crate::terminal::TerminalManager;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupOpenResult {
    pub group_id: GroupId,
    pub session_ids: Vec<SessionId>,
    pub was_created: bool,
}

pub fn ensure_group_for_path(
    app_state: &mut AppState,
    terminal_manager: &TerminalManager,
    group_path: String,
) -> GroupOpenResult {
    if let Some(group_id) = app_state
        .groups()
        .iter()
        .find(|group| group.path == group_path)
        .map(|group| group.id)
    {
        let session_ids = app_state.session_ids_in_group(group_id);
        if let Some(first_session_id) = session_ids.first().copied() {
            app_state.select_session(first_session_id);
        }
        return GroupOpenResult {
            group_id,
            session_ids,
            was_created: false,
        };
    }

    let (group_id, session_ids) = app_state.create_group_with_defaults(group_path.clone());
    if let Some(first_session_id) = session_ids.first().copied() {
        app_state.select_session(first_session_id);
    }
    mark_failed_session_starts(
        app_state,
        terminal_manager,
        &session_ids,
        group_path.as_str(),
    );

    GroupOpenResult {
        group_id,
        session_ids,
        was_created: true,
    }
}

pub fn add_session_to_group(
    app_state: &mut AppState,
    terminal_manager: &TerminalManager,
    group_id: GroupId,
) -> SessionId {
    let session_id = app_state.add_session(group_id);
    app_state.select_session(session_id);
    if let Some(path) = app_state.group_path(group_id).map(ToString::to_string) {
        mark_failed_session_starts(app_state, terminal_manager, &[session_id], path.as_str());
    } else {
        app_state.set_session_status(session_id, SessionStatus::Error);
    }
    session_id
}

pub fn remove_group(
    app_state: &mut AppState,
    terminal_manager: &TerminalManager,
    group_id: GroupId,
) -> Vec<SessionId> {
    let removed_session_ids = app_state.remove_group(group_id);
    terminate_sessions(terminal_manager, &removed_session_ids);
    removed_session_ids
}

pub fn remove_session(
    app_state: &mut AppState,
    terminal_manager: &TerminalManager,
    session_id: SessionId,
) -> bool {
    if !app_state.remove_session(session_id) {
        return false;
    }
    let _ = terminal_manager.terminate_session(session_id);
    true
}

fn mark_failed_session_starts(
    app_state: &mut AppState,
    terminal_manager: &TerminalManager,
    session_ids: &[SessionId],
    path: &str,
) {
    let failed_session_ids = session_ids
        .iter()
        .copied()
        .filter(|session_id| terminal_manager.ensure_session(*session_id, path).is_err())
        .collect::<Vec<_>>();

    for session_id in failed_session_ids {
        app_state.set_session_status(session_id, SessionStatus::Error);
    }
}

fn terminate_sessions(terminal_manager: &TerminalManager, session_ids: &[SessionId]) {
    for session_id in session_ids {
        let _ = terminal_manager.terminate_session(*session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::{ensure_group_for_path, remove_group};
    use crate::state::AppState;
    use crate::terminal::TerminalManager;

    #[test]
    fn ensure_group_for_path_selects_existing_group_without_duplication() {
        let mut state = AppState::default();
        let terminal_manager = TerminalManager::new();
        let existing_group_path = state.groups()[0].path.clone();
        let original_group_count = state.groups().len();

        let result = ensure_group_for_path(&mut state, &terminal_manager, existing_group_path);

        assert!(!result.was_created);
        assert_eq!(state.groups().len(), original_group_count);
        assert_eq!(state.active_group_id(), Some(result.group_id));
        assert_eq!(result.session_ids.len(), 3);
    }

    #[test]
    fn remove_group_terminates_group_state_even_without_live_runtime() {
        let mut state = AppState::default();
        let terminal_manager = TerminalManager::new();
        let (group_id, _) = state.create_group_with_defaults("/tmp/remove-me".to_string());

        let removed = remove_group(&mut state, &terminal_manager, group_id);

        assert!(!removed.is_empty());
        assert!(state.groups().iter().all(|group| group.id != group_id));
        assert!(
            state
                .sessions()
                .iter()
                .all(|session| session.group_id != group_id)
        );
    }
}
