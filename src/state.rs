use crate::commands::{CommandId, CommandLibrary, InsertCommand};
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

/// Selects which center-stack agent pane should receive a swapped tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibleAgentSlot {
    Top,
    Bottom,
}

/// Opaque identifier for an individual terminal session.
pub type SessionId = u32;
/// Opaque identifier for a workspace path group.
pub type GroupId = u32;
pub const UI_SCALE_DEFAULT: f64 = 1.0;
pub const UI_SCALE_MIN: f64 = 0.7;
pub const UI_SCALE_MAX: f64 = 1.8;
pub const GROUP_RUNNER_WIDTH_DEFAULT_PX: i32 = 340;
pub const GROUP_SPLIT_RATIO_DEFAULT: f64 = 0.5;
const GROUP_RUNNER_WIDTH_MIN_PX: i32 = 260;
const GROUP_RUNNER_WIDTH_MAX_PX: i32 = 760;
const GROUP_SPLIT_MIN_RATIO: f64 = 0.28;
const GROUP_SPLIT_MAX_RATIO: f64 = 0.72;

pub fn clamp_ui_scale(scale: f64) -> f64 {
    if !scale.is_finite() {
        return UI_SCALE_DEFAULT;
    }

    scale.clamp(UI_SCALE_MIN, UI_SCALE_MAX)
}

fn clamp_group_runner_width_px(width: i32) -> i32 {
    width.clamp(GROUP_RUNNER_WIDTH_MIN_PX, GROUP_RUNNER_WIDTH_MAX_PX)
}

fn clamp_group_split_ratio(ratio: f64) -> f64 {
    if !ratio.is_finite() {
        return GROUP_SPLIT_RATIO_DEFAULT;
    }

    ratio.clamp(GROUP_SPLIT_MIN_RATIO, GROUP_SPLIT_MAX_RATIO)
}

fn default_group_runner_width_px() -> i32 {
    GROUP_RUNNER_WIDTH_DEFAULT_PX
}

fn default_group_split_ratio() -> f64 {
    GROUP_SPLIT_RATIO_DEFAULT
}

/// Persisted workspace layout controls scoped to one path group.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct GroupLayout {
    #[serde(default = "default_group_runner_width_px")]
    pub runner_width_px: i32,
    #[serde(default = "default_group_split_ratio")]
    pub agent_top_ratio: f64,
    #[serde(default = "default_group_split_ratio")]
    pub runner_top_ratio: f64,
}

impl Default for GroupLayout {
    fn default() -> Self {
        Self {
            runner_width_px: GROUP_RUNNER_WIDTH_DEFAULT_PX,
            agent_top_ratio: GROUP_SPLIT_RATIO_DEFAULT,
            runner_top_ratio: GROUP_SPLIT_RATIO_DEFAULT,
        }
    }
}

impl GroupLayout {
    fn normalized(self) -> Self {
        Self {
            runner_width_px: clamp_group_runner_width_px(self.runner_width_px),
            agent_top_ratio: clamp_group_split_ratio(self.agent_top_ratio),
            runner_top_ratio: clamp_group_split_ratio(self.runner_top_ratio),
        }
    }
}

/// Path-scoped tab group metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabGroup {
    pub id: GroupId,
    pub path: String,
    pub color: String,
    #[serde(default)]
    pub layout: GroupLayout,
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
    #[serde(default)]
    pub command_library: CommandLibrary,
    #[serde(default = "default_ui_scale")]
    ui_scale: f64,
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
            command_library: CommandLibrary::default(),
            ui_scale: UI_SCALE_DEFAULT,
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
        self.command_library.repair_after_restore();
        self.ui_scale = clamp_ui_scale(self.ui_scale);
        self.groups.retain(|group| !group.path.trim().is_empty());
        for group in &mut self.groups {
            group.layout = group.layout.normalized();
        }

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
            layout: GroupLayout::default(),
        });
        self.mark_dirty();

        id
    }

    /// Removes a group and every session assigned to it.
    pub fn remove_group(&mut self, group_id: GroupId) -> Vec<SessionId> {
        let group_exists = self.groups.iter().any(|group| group.id == group_id);
        if !group_exists {
            return Vec::new();
        }

        self.groups.retain(|group| group.id != group_id);

        let removed_session_ids = self
            .sessions
            .iter()
            .filter(|session| session.group_id == group_id)
            .map(|session| session.id)
            .collect::<Vec<_>>();

        if !removed_session_ids.is_empty() {
            let removed_ids: std::collections::HashSet<SessionId> =
                removed_session_ids.iter().copied().collect();
            self.sessions
                .retain(|session| !removed_ids.contains(&session.id));

            if self
                .selected_session
                .is_some_and(|selected| removed_ids.contains(&selected))
            {
                self.selected_session = self.sessions.first().map(|session| session.id);
            }
        }

        self.mark_dirty();
        removed_session_ids
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

    /// Removes a session by identifier.
    pub fn remove_session(&mut self, session_id: SessionId) -> bool {
        let Some(remove_idx) = self
            .sessions
            .iter()
            .position(|session| session.id == session_id)
        else {
            return false;
        };

        let removed_group_id = self.sessions[remove_idx].group_id;
        self.sessions.remove(remove_idx);

        if self.selected_session == Some(session_id) {
            self.selected_session = self
                .sessions
                .iter()
                .find(|session| session.group_id == removed_group_id)
                .map(|session| session.id)
                .or_else(|| self.sessions.first().map(|session| session.id));
        }

        self.mark_dirty();
        true
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

    /// Swaps a session with one of the currently visible center-stack agent panes.
    pub fn swap_session_with_visible_agent_slot(
        &mut self,
        source_id: SessionId,
        slot: VisibleAgentSlot,
    ) {
        let Some(source_idx) = self
            .sessions
            .iter()
            .position(|session| session.id == source_id)
        else {
            return;
        };

        let source_group_id = self.sessions[source_idx].group_id;
        let Some(target_id) = self.visible_agent_slot_session_id(source_group_id, slot) else {
            return;
        };
        let Some(target_idx) = self
            .sessions
            .iter()
            .position(|session| session.id == target_id)
        else {
            return;
        };

        let mut changed = false;
        if source_idx != target_idx {
            self.sessions.swap(source_idx, target_idx);
            changed = true;
        }

        if self.selected_session != Some(source_id) {
            self.selected_session = Some(source_id);
            changed = true;
        }

        if changed {
            self.mark_dirty();
        }
    }

    fn visible_agent_slot_session_id(
        &self,
        group_id: GroupId,
        slot: VisibleAgentSlot,
    ) -> Option<SessionId> {
        let mut runner_seen = false;
        let mut visible_agents = Vec::with_capacity(2);

        for session in self
            .sessions
            .iter()
            .filter(|session| session.group_id == group_id)
        {
            if session.role.is_runner() && !runner_seen {
                runner_seen = true;
                continue;
            }

            visible_agents.push(session.id);
            if visible_agents.len() == 2 {
                break;
            }
        }

        match slot {
            VisibleAgentSlot::Top => visible_agents.first().copied(),
            VisibleAgentSlot::Bottom => visible_agents
                .get(1)
                .copied()
                .or_else(|| visible_agents.first().copied()),
        }
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

    /// Returns session identifiers in insertion order for a group.
    pub fn session_ids_in_group(&self, group_id: GroupId) -> Vec<SessionId> {
        self.sessions
            .iter()
            .filter(|session| session.group_id == group_id)
            .map(|session| session.id)
            .collect()
    }

    /// Returns center-stack agent sessions and optional runner for UI layout.
    pub fn workspace_sessions_for_group(
        &self,
        group_id: GroupId,
    ) -> (Vec<Session>, Option<Session>) {
        let mut agents = Vec::with_capacity(2);
        let mut runner = None;

        for session in self
            .sessions
            .iter()
            .filter(|session| session.group_id == group_id)
        {
            if session.role.is_runner() && runner.is_none() {
                runner = Some(session.clone());
                continue;
            }

            if agents.len() < 2 {
                agents.push(session.clone());
            }
        }

        if runner.is_none() {
            runner = agents.pop();
        }

        (agents, runner)
    }

    /// Returns center-stack agent identifiers plus optional runner identifier.
    pub fn workspace_session_ids_for_group(&self, group_id: GroupId) -> Vec<SessionId> {
        let mut session_ids = Vec::with_capacity(3);
        let mut runner_id = None;

        for session in self
            .sessions
            .iter()
            .filter(|session| session.group_id == group_id)
        {
            if session.role.is_runner() && runner_id.is_none() {
                runner_id = Some(session.id);
                continue;
            }

            if session_ids.len() < 2 {
                session_ids.push(session.id);
            }
        }

        if let Some(runner_id) = runner_id {
            session_ids.push(runner_id);
        }

        session_ids
    }

    /// Returns the configured path for a group identifier.
    pub fn group_path(&self, group_id: GroupId) -> Option<&str> {
        self.groups
            .iter()
            .find(|group| group.id == group_id)
            .map(|group| group.path.as_str())
    }

    /// Returns layout controls for a group identifier.
    pub fn group_layout(&self, group_id: GroupId) -> GroupLayout {
        self.groups
            .iter()
            .find(|group| group.id == group_id)
            .map(|group| group.layout.normalized())
            .unwrap_or_default()
    }

    /// Updates the run sidebar width for a group.
    pub fn set_group_runner_width_px(&mut self, group_id: GroupId, width: i32) {
        let next = clamp_group_runner_width_px(width);
        self.update_group_layout(group_id, |layout| layout.runner_width_px = next);
    }

    /// Updates the agent stack split ratio for a group.
    pub fn set_group_agent_top_ratio(&mut self, group_id: GroupId, ratio: f64) {
        let next = clamp_group_split_ratio(ratio);
        self.update_group_layout(group_id, |layout| layout.agent_top_ratio = next);
    }

    /// Updates the run/local-agent split ratio for a group.
    pub fn set_group_runner_top_ratio(&mut self, group_id: GroupId, ratio: f64) {
        let next = clamp_group_split_ratio(ratio);
        self.update_group_layout(group_id, |layout| layout.runner_top_ratio = next);
    }

    fn update_group_layout(
        &mut self,
        group_id: GroupId,
        update: impl FnOnce(&mut GroupLayout),
    ) -> bool {
        let Some(group) = self.groups.iter_mut().find(|group| group.id == group_id) else {
            return false;
        };
        let before = group.layout.normalized();
        let mut next = before;
        update(&mut next);
        next = next.normalized();
        if before == next {
            return false;
        }

        group.layout = next;
        self.mark_dirty();
        true
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

    /// Returns all insert commands in insertion order.
    pub fn commands(&self) -> &[InsertCommand] {
        &self.command_library.commands
    }

    /// Returns the persisted GUI font scale.
    pub fn ui_scale(&self) -> f64 {
        clamp_ui_scale(self.ui_scale)
    }

    /// Sets the GUI font scale and marks the state dirty when changed.
    pub fn set_ui_scale(&mut self, scale: f64) {
        let normalized = clamp_ui_scale(scale);
        if (self.ui_scale - normalized).abs() < f64::EPSILON {
            return;
        }

        self.ui_scale = normalized;
        self.mark_dirty();
    }

    /// Returns a command by identifier.
    pub fn command_by_id(&self, command_id: CommandId) -> Option<&InsertCommand> {
        self.command_library.command(command_id)
    }

    /// Creates an insert command and returns its identifier.
    pub fn create_insert_command(
        &mut self,
        name: String,
        prompt: String,
        description: String,
        tags: Vec<String>,
    ) -> CommandId {
        let id = self.command_library.create(name, prompt, description, tags);
        self.mark_dirty();
        id
    }

    /// Updates an existing insert command. Returns true on mutation.
    pub fn update_insert_command(
        &mut self,
        command_id: CommandId,
        name: String,
        prompt: String,
        description: String,
        tags: Vec<String>,
    ) -> bool {
        let updated = self
            .command_library
            .update(command_id, name, prompt, description, tags);
        if updated {
            self.mark_dirty();
        }
        updated
    }

    /// Deletes an existing insert command. Returns true when removed.
    pub fn delete_insert_command(&mut self, command_id: CommandId) -> bool {
        let removed = self.command_library.delete(command_id);
        if removed {
            self.mark_dirty();
        }
        removed
    }
}

fn default_ui_scale() -> f64 {
    UI_SCALE_DEFAULT
}
