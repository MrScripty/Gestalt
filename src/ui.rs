mod autosave;
mod tab_rail;
mod terminal_input;
mod terminal_view;
mod workspace;

use crate::persistence;
use crate::state::{SessionId, SessionStatus};
use crate::terminal::{PersistedTerminalState, TerminalManager};
use crate::ui::autosave::{AutosaveRequest, AutosaveSignature, AutosaveWorker};
use crate::ui::tab_rail::TabRail;
use crate::ui::terminal_input::measure_terminal_viewport;
use crate::ui::workspace::WorkspaceMain;
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

const STYLE: &str = concat!(
    include_str!("style/base.css"),
    include_str!("style/workspace.css")
);
const TERMINAL_REFRESH_POLL_MS: u64 = 33;
const TERMINAL_RESIZE_POLL_MS: u64 = 180;
const AUTOSAVE_POLL_MS: u64 = 1_200;
const AUTOSAVE_QUEUE_CAPACITY: usize = 1;

/// Root desktop UI component.
#[component]
pub fn App() -> Element {
    let initial_workspace = use_signal(|| persistence::load_workspace().ok().flatten());
    let mut app_state = {
        let loaded = initial_workspace.read().clone();
        use_signal(move || {
            loaded
                .as_ref()
                .map(|workspace| workspace.app_state.clone().into_restored())
                .unwrap_or_default()
        })
    };
    let mut restored_terminals = {
        let loaded = initial_workspace.read().clone();
        use_signal(move || {
            loaded
                .as_ref()
                .map(|workspace| {
                    workspace
                        .terminals
                        .iter()
                        .cloned()
                        .map(|terminal| (terminal.session_id, terminal))
                        .collect::<HashMap<SessionId, PersistedTerminalState>>()
                })
                .unwrap_or_default()
        })
    };
    let dragging_tab = use_signal(|| None::<SessionId>);
    let terminal_manager = use_signal(|| Arc::new(TerminalManager::new()));
    let autosave_worker = use_signal(|| Arc::new(AutosaveWorker::spawn(AUTOSAVE_QUEUE_CAPACITY)));
    let new_group_path = use_signal(String::new);
    let persistence_feedback = use_signal(String::new);
    let refresh_tick = use_signal(|| 0_u64);
    let focused_terminal = use_signal(|| None::<SessionId>);
    let round_anchor = use_signal(|| None::<(SessionId, u16)>);
    let local_agent_command = use_signal(String::new);
    let local_agent_feedback = use_signal(String::new);
    let renaming_tab = use_signal(|| None::<SessionId>);
    let rename_draft = use_signal(String::new);

    {
        let mut refresh_tick = refresh_tick;
        let app_state = app_state;
        let terminal_manager = terminal_manager.read().clone();
        use_future(move || {
            let terminal_manager = terminal_manager.clone();
            async move {
                let mut last_revisions = Vec::<(SessionId, u64)>::new();
                loop {
                    tokio::time::sleep(Duration::from_millis(TERMINAL_REFRESH_POLL_MS)).await;

                    let snapshot = app_state.read().clone();
                    let Some(group_id) = snapshot.active_group_id() else {
                        if !last_revisions.is_empty() {
                            last_revisions.clear();
                            let next = *refresh_tick.read() + 1;
                            refresh_tick.set(next);
                        }
                        continue;
                    };

                    let mut revisions = snapshot
                        .sessions_in_group(group_id)
                        .into_iter()
                        .map(|session| {
                            (
                                session.id,
                                terminal_manager
                                    .session_snapshot_revision(session.id)
                                    .unwrap_or(0),
                            )
                        })
                        .collect::<Vec<_>>();
                    revisions.sort_unstable_by_key(|(session_id, _)| *session_id);

                    if revisions != last_revisions {
                        last_revisions = revisions;
                        let next = *refresh_tick.read() + 1;
                        refresh_tick.set(next);
                    }
                }
            }
        });
    }

    {
        let app_state = app_state;
        let terminal_manager = terminal_manager.read().clone();
        use_future(move || {
            let terminal_manager = terminal_manager.clone();
            async move {
                let mut last_sizes: HashMap<SessionId, (u16, u16)> = HashMap::new();

                loop {
                    tokio::time::sleep(Duration::from_millis(TERMINAL_RESIZE_POLL_MS)).await;

                    let snapshot = app_state.read().clone();
                    let Some(group_id) = snapshot.active_group_id() else {
                        last_sizes.clear();
                        continue;
                    };

                    let (agents, runner) = snapshot.workspace_sessions_for_group(group_id);
                    let mut active_session_ids: Vec<SessionId> =
                        agents.into_iter().map(|session| session.id).collect();
                    if let Some(runner) = runner {
                        active_session_ids.push(runner.id);
                    }

                    let active_session_set: HashSet<SessionId> =
                        active_session_ids.iter().copied().collect();
                    last_sizes.retain(|session_id, _| active_session_set.contains(session_id));

                    for session_id in active_session_ids {
                        let body_id = format!("terminal-body-{session_id}");
                        let Some((rows, cols)) = measure_terminal_viewport(body_id).await else {
                            continue;
                        };

                        if last_sizes.get(&session_id).copied() == Some((rows, cols)) {
                            continue;
                        }

                        if terminal_manager
                            .resize_session(session_id, rows, cols)
                            .is_ok()
                        {
                            last_sizes.insert(session_id, (rows, cols));
                        }
                    }
                }
            }
        });
    }

    {
        let app_state = app_state;
        let terminal_manager = terminal_manager.read().clone();
        let autosave_worker = autosave_worker.read().clone();
        let mut persistence_feedback = persistence_feedback;
        let loaded = initial_workspace.read().clone();
        use_future(move || {
            let terminal_manager = terminal_manager.clone();
            let autosave_worker = autosave_worker.clone();
            let initial_fingerprint = loaded
                .as_ref()
                .and_then(|workspace| workspace.stable_fingerprint().ok());
            async move {
                let mut last_saved_fingerprint = initial_fingerprint;
                let mut last_saved_signature = None::<AutosaveSignature>;
                let mut inflight_signature = None::<AutosaveSignature>;
                let mut deferred_request = None::<AutosaveRequest>;

                loop {
                    tokio::time::sleep(Duration::from_millis(AUTOSAVE_POLL_MS)).await;

                    for result in autosave_worker.drain_results() {
                        if result.error.is_none() {
                            last_saved_fingerprint = Some(result.fingerprint);
                            last_saved_signature = Some(result.signature.clone());
                            persistence_feedback.set(String::new());
                        } else if let Some(error) = result.error {
                            persistence_feedback.set(error);
                        }

                        if inflight_signature.as_ref() == Some(&result.signature) {
                            inflight_signature = None;
                        }
                    }

                    if inflight_signature.is_none()
                        && let Some(request) = deferred_request.take()
                    {
                        match autosave_worker.try_enqueue(request.clone()) {
                            Ok(()) => {
                                inflight_signature = Some(request.signature);
                            }
                            Err(error) => {
                                deferred_request = Some(request);
                                persistence_feedback.set(error);
                            }
                        }
                    }

                    let state = app_state.read().clone();
                    let mut terminal_revisions = state
                        .sessions
                        .iter()
                        .map(|session| {
                            (
                                session.id,
                                terminal_manager
                                    .session_snapshot_revision(session.id)
                                    .unwrap_or(0),
                            )
                        })
                        .collect::<Vec<_>>();
                    terminal_revisions.sort_unstable_by_key(|(session_id, _)| *session_id);
                    let save_signature = (state.revision(), terminal_revisions);

                    if last_saved_signature.as_ref() == Some(&save_signature) {
                        continue;
                    }
                    if inflight_signature.as_ref() == Some(&save_signature) {
                        continue;
                    }
                    if deferred_request.as_ref().map(|request| &request.signature)
                        == Some(&save_signature)
                    {
                        continue;
                    }

                    let workspace =
                        persistence::build_workspace_snapshot(&state, &terminal_manager);

                    let Ok(fingerprint) = workspace.stable_fingerprint() else {
                        persistence_feedback
                            .set("Autosave paused: failed to fingerprint workspace.".to_string());
                        continue;
                    };

                    if last_saved_fingerprint == Some(fingerprint) {
                        last_saved_signature = Some(save_signature);
                        continue;
                    }

                    let request = AutosaveRequest {
                        workspace,
                        fingerprint,
                        signature: save_signature.clone(),
                    };

                    if inflight_signature.is_none() {
                        match autosave_worker.try_enqueue(request.clone()) {
                            Ok(()) => {
                                inflight_signature = Some(save_signature);
                            }
                            Err(error) => {
                                deferred_request = Some(request);
                                persistence_feedback.set(error);
                            }
                        }
                    } else {
                        deferred_request = Some(request);
                    }
                }
            }
        });
    }

    use_drop({
        let app_state = app_state;
        let terminal_manager = terminal_manager.read().clone();
        let autosave_worker = autosave_worker.read().clone();
        move || {
            autosave_worker.shutdown();
            let state = app_state.read().clone();
            let workspace = persistence::build_workspace_snapshot(&state, &terminal_manager);
            let _ = persistence::save_workspace(&workspace);
        }
    });

    let snapshot = app_state.read().clone();

    let failed_starts = {
        let mut failures = Vec::new();
        let runtime = terminal_manager.read().clone();
        let mut restored = restored_terminals.write();
        for session in &snapshot.sessions {
            if let Some(restored_terminal) = restored.remove(&session.id) {
                runtime.seed_restored_terminal(restored_terminal);
            }
            if let Some(path) = snapshot.group_path(session.group_id)
                && runtime.ensure_session(session.id, path).is_err()
            {
                failures.push(session.id);
            }
        }
        failures
    };

    if !failed_starts.is_empty() {
        let mut state = app_state.write();
        for session_id in failed_starts {
            state.set_session_status(session_id, SessionStatus::Error);
        }
    }

    rsx! {
        style { "{STYLE}" }

        div { class: "shell",
            TabRail {
                app_state: app_state,
                terminal_manager: terminal_manager,
                dragging_tab: dragging_tab,
                new_group_path: new_group_path,
                renaming_tab: renaming_tab,
                rename_draft: rename_draft,
            }

            WorkspaceMain {
                app_state: app_state,
                terminal_manager: terminal_manager,
                focused_terminal: focused_terminal,
                round_anchor: round_anchor,
                local_agent_command: local_agent_command,
                local_agent_feedback: local_agent_feedback,
                persistence_feedback: persistence_feedback,
                refresh_tick: refresh_tick,
            }
        }
    }
}
