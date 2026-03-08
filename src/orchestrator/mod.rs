mod autosave;
pub mod events;
pub mod git;
pub mod repo_watcher;
mod runtime;
mod session;
mod startup;
mod workspace;

pub use autosave::{
    AutosaveController, AutosaveFeedback, AutosaveRequest, AutosaveResult, AutosaveSignature,
    AutosaveWorker,
};
pub use runtime::{
    GroupOrchestratorSnapshot, GroupTerminalState, LocalAgentRunDispatch, SessionRuntimeView,
    SessionWriteResult, TerminalRound, broadcast_line_to_group, group_session_ids, interrupt_group,
    interrupt_local_agent_group, interrupt_sessions, send_line_to_sessions,
    send_local_agent_command_to_group, snapshot_group, snapshot_group_from_runtime,
    start_local_agent_run,
};
pub use session::{
    GroupOpenResult, add_session_to_group, ensure_group_for_path, remove_group, remove_session,
};
pub use startup::{
    ACTIVE_GROUP_STARTUP_HISTORY_LINES, DEFERRED_SESSION_START_BATCH_SIZE,
    DEFERRED_SESSION_STARTUP_HISTORY_LINES, HistoryLoadState, HistoryLoadUpdate,
    STARTUP_BACKGROUND_TICK_MS, SessionStartupPriority, SessionStartupTarget, StartupCoordinator,
    StartupTickResult, has_deferred_startup_targets, startup_targets,
};
pub use workspace::{
    SessionStatusCounts, SessionStatusUpdate, TerminalPaneProjection, WorkspaceProjection,
    active_workspace_projection, apply_session_activity, reconcile_session_statuses,
};
