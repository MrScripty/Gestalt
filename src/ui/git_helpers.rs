use crate::path_validation;
use crate::state::{AppState, SessionStatus};
use crate::terminal::TerminalManager;
use dioxus::prelude::*;
use std::sync::Arc;

pub(crate) fn bump_refresh_nonce(mut git_refresh_nonce: Signal<u64>) {
    let next = git_refresh_nonce.read().saturating_add(1);
    git_refresh_nonce.set(next);
}

pub(crate) fn toggle_bool_signal(mut signal: Signal<bool>) {
    let next = {
        let current = *signal.read();
        !current
    };
    signal.set(next);
}

pub(crate) fn create_group_for_worktree(
    mut app_state: Signal<AppState>,
    terminal_manager: Signal<Arc<TerminalManager>>,
    worktree_path: &str,
) -> Result<(), String> {
    let canonical_path = path_validation::validate_group_path(worktree_path)?;
    let existing_group_id = app_state
        .read()
        .groups
        .iter()
        .find(|group| group.path == canonical_path)
        .map(|group| group.id);

    if let Some(group_id) = existing_group_id {
        let first_session_id = {
            let state = app_state.read();
            state
                .sessions
                .iter()
                .find(|session| session.group_id == group_id)
                .map(|session| session.id)
        };
        if let Some(first_session_id) = first_session_id {
            app_state.write().select_session(first_session_id);
        }
        return Ok(());
    }

    let default_sessions = {
        let mut state = app_state.write();
        let (_group_id, ids) = state.create_group_with_defaults(canonical_path.clone());
        if let Some(first) = ids.first().copied() {
            state.select_session(first);
        }
        ids
    };

    let runtime = terminal_manager.read().clone();
    let failed = default_sessions
        .iter()
        .filter_map(|session_id| {
            runtime
                .ensure_session(*session_id, &canonical_path)
                .err()
                .map(|_| *session_id)
        })
        .collect::<Vec<_>>();

    if !failed.is_empty() {
        let mut state = app_state.write();
        for session_id in failed {
            state.set_session_status(session_id, SessionStatus::Error);
        }
    }

    Ok(())
}
