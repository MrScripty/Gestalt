use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Idle,
    Busy,
    Error,
}

impl SessionStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Busy => "Busy",
            Self::Error => "Error",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Idle => Self::Busy,
            Self::Busy => Self::Error,
            Self::Error => Self::Idle,
        }
    }

    pub fn css_var(self) -> &'static str {
        match self {
            Self::Idle => "--status-idle",
            Self::Busy => "--status-busy",
            Self::Error => "--status-error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionRole {
    Agent,
    Runner,
}

impl SessionRole {
    pub fn badge(self) -> &'static str {
        match self {
            Self::Agent => "AGENT",
            Self::Runner => "RUN",
        }
    }

    pub fn is_runner(self) -> bool {
        matches!(self, Self::Runner)
    }
}

pub type SessionId = u32;
pub type GroupId = u32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabGroup {
    pub id: GroupId,
    pub path: String,
    pub color: String,
}

impl TabGroup {
    pub fn label(&self) -> String {
        Path::new(&self.path)
            .file_name()
            .and_then(|name| name.to_str())
            .map_or_else(|| self.path.clone(), |name| name.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub title: String,
    pub group_id: GroupId,
    pub role: SessionRole,
    pub status: SessionStatus,
}

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

        let default_path = std::env::current_dir()
            .ok()
            .and_then(|path| path.to_str().map(|value| value.to_string()))
            .unwrap_or_else(|| ".".to_string());

        let (_, ids) = state.create_group_with_defaults(default_path);
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

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn into_restored(mut self) -> Self {
        self.repair_after_restore();
        self.revision = 0;
        self
    }

    pub fn repair_after_restore(&mut self) {
        self.groups.retain(|group| !group.path.trim().is_empty());

        if self.groups.is_empty() {
            let default_path = std::env::current_dir()
                .ok()
                .and_then(|path| path.to_str().map(|value| value.to_string()))
                .unwrap_or_else(|| ".".to_string());
            let (_, ids) = self.create_group_with_defaults(default_path);
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

    pub fn add_session(&mut self, group_id: GroupId) -> SessionId {
        let title = format!("Agent {:02}", self.next_session_id);
        self.add_session_with_title_and_role(group_id, title, SessionRole::Agent)
    }

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

    pub fn select_session(&mut self, session_id: SessionId) {
        if self.selected_session != Some(session_id) {
            self.selected_session = Some(session_id);
            self.mark_dirty();
        }
    }

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

    pub fn active_group_id(&self) -> Option<GroupId> {
        if let Some(selected) = self.selected_session
            && let Some(session) = self.sessions.iter().find(|session| session.id == selected)
        {
            return Some(session.group_id);
        }

        self.groups.first().map(|group| group.id)
    }

    pub fn sessions_in_group(&self, group_id: GroupId) -> Vec<Session> {
        self.sessions
            .iter()
            .filter(|session| session.group_id == group_id)
            .cloned()
            .collect()
    }

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

    pub fn group_path(&self, group_id: GroupId) -> Option<&str> {
        self.groups
            .iter()
            .find(|group| group.id == group_id)
            .map(|group| group.path.as_str())
    }

    pub fn session_count_by_status(&self, status: SessionStatus) -> usize {
        self.sessions
            .iter()
            .filter(|session| session.status == status)
            .count()
    }

    pub fn group_label(&self, group_id: GroupId) -> String {
        self.groups
            .iter()
            .find(|group| group.id == group_id)
            .map(TabGroup::label)
            .unwrap_or_else(|| "Unknown".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{AppState, SessionRole, SessionStatus};

    fn seeded_state() -> AppState {
        let mut state = AppState::default();
        let first_group = state.groups[0].id;
        let (second_group, _) = state.create_group_with_defaults("/tmp".to_string());

        state.add_session_with_title_and_role(
            first_group,
            "Session A".to_string(),
            SessionRole::Agent,
        );
        state.add_session_with_title_and_role(
            first_group,
            "Session B".to_string(),
            SessionRole::Agent,
        );
        state.add_session_with_title_and_role(
            second_group,
            "Session C".to_string(),
            SessionRole::Agent,
        );

        state
    }

    #[test]
    fn default_group_has_three_sessions() {
        let state = AppState::default();
        let group_id = state.groups[0].id;
        let sessions = state
            .sessions
            .iter()
            .filter(|session| session.group_id == group_id)
            .count();
        assert_eq!(sessions, 3);
    }

    #[test]
    fn move_session_before_reorders_and_adopts_target_group() {
        let mut state = seeded_state();
        let source = state.sessions[0].id;
        let target = state.sessions[3].id;
        let target_group = state.sessions[3].group_id;

        state.move_session_before(source, target);

        let source_idx = state
            .sessions
            .iter()
            .position(|session| session.id == source)
            .expect("source session to exist");
        let target_idx = state
            .sessions
            .iter()
            .position(|session| session.id == target)
            .expect("target session to exist");

        assert_eq!(source_idx + 1, target_idx);
        assert_eq!(state.sessions[source_idx].group_id, target_group);
    }

    #[test]
    fn move_session_to_group_end_places_session_after_group_tail() {
        let mut state = seeded_state();
        let source = state.sessions[1].id;
        let destination_group = state.sessions[3].group_id;

        state.move_session_to_group_end(source, destination_group);

        let moved_idx = state
            .sessions
            .iter()
            .position(|session| session.id == source)
            .expect("moved session to exist");
        assert_eq!(state.sessions[moved_idx].group_id, destination_group);

        let last_group_idx = state
            .sessions
            .iter()
            .rposition(|session| session.group_id == destination_group)
            .expect("destination group to contain at least one session");
        assert_eq!(moved_idx, last_group_idx);
    }

    #[test]
    fn session_status_cycle_changes_state() {
        let mut state = seeded_state();
        let id = state.sessions[0].id;

        state.set_session_status(id, SessionStatus::Idle);
        state.cycle_session_status(id);

        let status = state
            .sessions
            .iter()
            .find(|session| session.id == id)
            .expect("session to exist")
            .status;
        assert_eq!(status, SessionStatus::Busy);
    }

    #[test]
    fn test_into_restored_with_invalid_selection_selects_first_valid_session() {
        let state = AppState {
            selected_session: Some(u32::MAX),
            ..AppState::default()
        };

        let restored = state.into_restored();

        assert!(restored.selected_session.is_some());
        let selected = restored.selected_session.expect("selection exists");
        assert!(
            restored
                .sessions
                .iter()
                .any(|session| session.id == selected)
        );
    }

    #[test]
    fn test_revision_after_mutation_increments() {
        let mut state = AppState::default();
        let before = state.revision();
        let group_id = state.groups[0].id;
        state.add_session(group_id);
        assert!(state.revision() > before);
    }
}
