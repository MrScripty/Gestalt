use crate::state::SessionId;
use crate::ui::insert_command_mode::InsertModeState;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TerminalHistoryState {
    pub before_sequence: Option<u64>,
    pub is_loading: bool,
    pub exhausted: bool,
}

/// Shared transient UI state owned at the root shell level.
#[derive(Clone)]
pub(crate) struct UiState {
    pub focused_terminal: Option<SessionId>,
    pub round_anchor: Option<(SessionId, u16)>,
    pub local_agent_command: String,
    pub local_agent_feedback: String,
    pub persistence_feedback: String,
    pub sidebar_open: bool,
    pub insert_mode_state: Option<InsertModeState>,
    pub terminal_history_by_session: HashMap<SessionId, TerminalHistoryState>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            focused_terminal: None,
            round_anchor: None,
            local_agent_command: String::new(),
            local_agent_feedback: String::new(),
            persistence_feedback: String::new(),
            sidebar_open: true,
            insert_mode_state: None,
            terminal_history_by_session: HashMap::new(),
        }
    }
}
