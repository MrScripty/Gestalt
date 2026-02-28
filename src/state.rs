use serde::{Deserialize, Serialize};
use std::path::Path;

/// Runtime status displayed for each terminal tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Idle,
    Busy,
    Error,
}

impl SessionStatus {
    /// Returns a user-facing status label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Busy => "Busy",
            Self::Error => "Error",
        }
    }

    /// Cycles through the status enum in a fixed order.
    pub fn next(self) -> Self {
        match self {
            Self::Idle => Self::Busy,
            Self::Busy => Self::Error,
            Self::Error => Self::Idle,
        }
    }

    /// Returns the CSS variable backing this status color.
    pub fn css_var(self) -> &'static str {
        match self {
            Self::Idle => "--status-idle",
            Self::Busy => "--status-busy",
            Self::Error => "--status-error",
        }
    }
}

/// Role controls layout behavior for each session tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionRole {
    Agent,
    Runner,
}

impl SessionRole {
    /// Returns a compact badge shown in tab and orchestrator views.
    pub fn badge(self) -> &'static str {
        match self {
            Self::Agent => "AGENT",
            Self::Runner => "RUN",
        }
    }

    /// True when this session is the dedicated runner pane.
    pub fn is_runner(self) -> bool {
        matches!(self, Self::Runner)
    }
}

/// Opaque identifier for an individual terminal session.
pub type SessionId = u32;
/// Opaque identifier for a workspace path group.
pub type GroupId = u32;

/// Path-scoped tab group metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabGroup {
    pub id: GroupId,
    pub path: String,
    pub color: String,
}

impl TabGroup {
    /// Human-friendly label derived from the final path segment.
    pub fn label(&self) -> String {
        Path::new(&self.path)
            .file_name()
            .and_then(|name| name.to_str())
            .map_or_else(|| self.path.clone(), |name| name.to_string())
    }
}

/// Terminal tab state tracked in the workspace model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub title: String,
    pub group_id: GroupId,
    pub role: SessionRole,
    pub status: SessionStatus,
}

/// Top-level workspace state for groups, sessions, and selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub sessions: Vec<Session>,
    pub groups: Vec<TabGroup>,
    pub selected_session: Option<SessionId>,
    next_session_id: SessionId,
    next_group_id: GroupId,
    #[serde(skip, default)]
    revision: u64,
}

impl Default for AppState {
    fn default() -> Self {
        let mut state = Self::empty();
        let (_, ids) = state.create_group_with_defaults(".".to_string());
        if let Some(first) = ids.first().copied() {
            state.select_session(first);
        }

        state
    }
}

impl AppState {
    fn empty() -> Self {
        Self {
            sessions: Vec::new(),
            groups: Vec::new(),
            selected_session: None,
            next_session_id: 1,
            next_group_id: 1,
            revision: 0,
        }
    }

    fn mark_dirty(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }

    /// Returns the monotonic mutation counter for autosave signaling.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Repairs persisted invariants and resets mutation tracking.
    pub fn into_restored(mut self) -> Self {
        self.repair_after_restore();
        self.revision = 0;
        self
    }

    /// Repairs invalid relationships after loading persisted state.
    pub fn repair_after_restore(&mut self) {
        self.groups.retain(|group| !group.path.trim().is_empty());

        if self.groups.is_empty() {
            let (_, ids) = self.create_group_with_defaults(".".to_string());
            self.selected_session = ids.first().copied();
            return;
        }

        let valid_group_ids: std::collections::HashSet<GroupId> =
            self.groups.iter().map(|group| group.id).collect();
        self.sessions
            .retain(|session| valid_group_ids.contains(&session.group_id));

        if self.sessions.is_empty() {
            let fallback_group = self.groups[0].id;
            let ids = [
                self.add_session_with_title_and_role(
                    fallback_group,
                    "Agent A".to_string(),
                    SessionRole::Agent,
                ),
                self.add_session_with_title_and_role(
                    fallback_group,
                    "Agent B".to_string(),
                    SessionRole::Agent,
                ),
                self.add_session_with_title_and_role(
                    fallback_group,
                    "Run / Compile".to_string(),
                    SessionRole::Runner,
                ),
            ];
            self.selected_session = ids.first().copied();
        }

        if let Some(selected) = self.selected_session {
            let selection_is_valid = self.sessions.iter().any(|session| session.id == selected);
            if !selection_is_valid {
                self.selected_session = self.sessions.first().map(|session| session.id);
            }
        } else {
            self.selected_session = self.sessions.first().map(|session| session.id);
        }

        let max_group = self.groups.iter().map(|group| group.id).max().unwrap_or(0);
        let max_session = self
            .sessions
            .iter()
            .map(|session| session.id)
            .max()
            .unwrap_or(0);
        self.next_group_id = self.next_group_id.max(max_group.saturating_add(1));
        self.next_session_id = self.next_session_id.max(max_session.saturating_add(1));
    }

    /// Creates a group and seeds default Agent/Runner sessions.
    pub fn create_group_with_defaults(&mut self, path: String) -> (GroupId, Vec<SessionId>) {
        let group_id = self.add_group_with_path(path);
        let ids = vec![
            self.add_session_with_title_and_role(
                group_id,
                "Agent A".to_string(),
                SessionRole::Agent,
            ),
            self.add_session_with_title_and_role(
                group_id,
                "Agent B".to_string(),
                SessionRole::Agent,
            ),
            self.add_session_with_title_and_role(
                group_id,
                "Run / Compile".to_string(),
                SessionRole::Runner,
            ),
        ];

        self.mark_dirty();
        (group_id, ids)
    }

    /// Adds a group for the provided path and returns its identifier.
    pub fn add_group_with_path(&mut self, path: String) -> GroupId {
        const PALETTE: [&str; 8] = [
            "#f4a261", "#2a9d8f", "#457b9d", "#e76f51", "#8ab17d", "#e9c46a", "#264653", "#219ebc",
        ];

        let id = self.next_group_id;
        self.next_group_id += 1;

        let normalized = if path.trim().is_empty() {
            ".".to_string()
        } else {
            path.trim().to_string()
        };

        let color = PALETTE[(id as usize - 1) % PALETTE.len()].to_string();
        self.groups.push(TabGroup {
            id,
            path: normalized,
            color,
        });
        self.mark_dirty();

        id
    }

    /// Adds a new agent session with an auto-generated title.
    pub fn add_session(&mut self, group_id: GroupId) -> SessionId {
        let title = format!("Agent {:02}", self.next_session_id);
        self.add_session_with_title_and_role(group_id, title, SessionRole::Agent)
    }

    /// Adds a session with explicit title and role.
    pub fn add_session_with_title_and_role(
        &mut self,
        group_id: GroupId,
        title: String,
        role: SessionRole,
    ) -> SessionId {
        let id = self.next_session_id;
        self.next_session_id += 1;

        self.sessions.push(Session {
            id,
            title,
            group_id,
            role,
            status: SessionStatus::Idle,
        });

        if self.selected_session.is_none() {
            self.selected_session = Some(id);
        }
        self.mark_dirty();

        id
    }

    /// Renames a session when the provided title is non-empty.
    pub fn rename_session(&mut self, session_id: SessionId, title: String) {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return;
        }

        if let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.title = trimmed.to_string();
            self.mark_dirty();
        }
    }

    /// Marks a session as selected.
    pub fn select_session(&mut self, session_id: SessionId) {
        if self.selected_session != Some(session_id) {
            self.selected_session = Some(session_id);
            self.mark_dirty();
        }
    }

    /// Cycles the selected session status forward.
    pub fn cycle_session_status(&mut self, session_id: SessionId) {
        if let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.status = session.status.next();
            self.mark_dirty();
        }
    }

    /// Sets a session status to an explicit value.
    pub fn set_session_status(&mut self, session_id: SessionId, status: SessionStatus) {
        if let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
            && session.status != status
        {
            session.status = status;
            self.mark_dirty();
        }
    }

    /// Moves one session before another and aligns group membership.
    pub fn move_session_before(&mut self, source_id: SessionId, target_id: SessionId) {
        if source_id == target_id {
            return;
        }

        let Some(source_idx) = self
            .sessions
            .iter()
            .position(|session| session.id == source_id)
        else {
            return;
        };
        let mut source = self.sessions.remove(source_idx);

        let Some(target_idx) = self
            .sessions
            .iter()
            .position(|session| session.id == target_id)
        else {
            self.sessions.push(source);
            self.mark_dirty();
            return;
        };

        source.group_id = self.sessions[target_idx].group_id;
        self.sessions.insert(target_idx, source);
        self.mark_dirty();
    }

    /// Moves a session to the end of a target group.
    pub fn move_session_to_group_end(&mut self, source_id: SessionId, group_id: GroupId) {
        let Some(source_idx) = self
            .sessions
            .iter()
            .position(|session| session.id == source_id)
        else {
            return;
        };

        let mut source = self.sessions.remove(source_idx);
        source.group_id = group_id;

        let insert_idx = self
            .sessions
            .iter()
            .rposition(|session| session.group_id == group_id)
            .map_or(self.sessions.len(), |idx| idx + 1);

        self.sessions.insert(insert_idx, source);
        self.mark_dirty();
    }

    /// Returns the active group based on selected session fallback.
    pub fn active_group_id(&self) -> Option<GroupId> {
        if let Some(selected) = self.selected_session
            && let Some(session) = self.sessions.iter().find(|session| session.id == selected)
        {
            return Some(session.group_id);
        }

        self.groups.first().map(|group| group.id)
    }

    /// Returns all sessions currently belonging to a group.
    pub fn sessions_in_group(&self, group_id: GroupId) -> Vec<Session> {
        self.sessions
            .iter()
            .filter(|session| session.group_id == group_id)
            .cloned()
            .collect()
    }

    /// Returns center-stack agent sessions and optional runner for UI layout.
    pub fn workspace_sessions_for_group(
        &self,
        group_id: GroupId,
    ) -> (Vec<Session>, Option<Session>) {
        let group_sessions = self.sessions_in_group(group_id);
        let mut agents = Vec::new();
        let mut runner = None;

        for session in group_sessions {
            if session.role.is_runner() && runner.is_none() {
                runner = Some(session);
            } else {
                agents.push(session);
            }
        }

        agents.truncate(2);

        if runner.is_none() {
            runner = agents.pop();
        }

        (agents, runner)
    }

    /// Returns the configured path for a group identifier.
    pub fn group_path(&self, group_id: GroupId) -> Option<&str> {
        self.groups
            .iter()
            .find(|group| group.id == group_id)
            .map(|group| group.path.as_str())
    }

    /// Counts sessions in a given status.
    pub fn session_count_by_status(&self, status: SessionStatus) -> usize {
        self.sessions
            .iter()
            .filter(|session| session.status == status)
            .count()
    }

    /// Returns a display label for the given group identifier.
    pub fn group_label(&self, group_id: GroupId) -> String {
        self.groups
            .iter()
            .find(|group| group.id == group_id)
            .map(TabGroup::label)
            .unwrap_or_else(|| "Unknown".to_string())
    }
}
