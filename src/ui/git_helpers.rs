use crate::orchestrator;
use crate::path_validation;
use crate::state::AppState;
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
    let runtime = terminal_manager.read().clone();
    let mut state = app_state.write();
    let _ = orchestrator::ensure_group_for_path(&mut state, &runtime, canonical_path);

    Ok(())
}
