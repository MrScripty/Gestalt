pub mod events;
pub mod git;
pub mod repo_watcher;
mod runtime;
mod workspace;

pub use runtime::{
    broadcast_line_to_group, group_session_ids, interrupt_group, interrupt_local_agent_group,
    interrupt_sessions, send_line_to_sessions, send_local_agent_command_to_group, snapshot_group,
    snapshot_group_from_runtime, GroupOrchestratorSnapshot, GroupTerminalState, SessionRuntimeView,
    SessionWriteResult, TerminalRound,
};
pub use workspace::{
    active_workspace_projection, apply_session_activity, reconcile_session_statuses,
    SessionStatusCounts, SessionStatusUpdate, TerminalPaneProjection, WorkspaceProjection,
};
