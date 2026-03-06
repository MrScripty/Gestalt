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
/// Opaque identifier for a persisted note entry.
pub type NoteId = u64;
/// Opaque identifier for a captured terminal snippet.
pub type SnippetId = u64;
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

fn default_note_title() -> String {
    "Note".to_string()
}

fn default_note_group_id() -> GroupId {
    0
}

/// Durable reference to a captured text range in a terminal log stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetLogRef {
    pub session_id: SessionId,
    pub stream_id: String,
    pub start_offset: u64,
    pub end_offset: u64,
    pub start_row: u32,
    pub end_row: u32,
}

/// Embedding pipeline status tracked for one snippet.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SnippetEmbeddingStatus {
    Pending,
    Processing,
    Ready,
    Failed,
}

impl SnippetEmbeddingStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Processing => "Processing",
            Self::Ready => "Ready",
            Self::Failed => "Failed",
        }
    }
}

/// One captured snippet anchored to terminal history metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub id: SnippetId,
    pub created_at_unix_ms: i64,
    pub source_cwd: String,
    pub text_snapshot_plain: String,
    pub log_ref: SnippetLogRef,
    pub embedding_status: SnippetEmbeddingStatus,
    pub embedding_object_id: Option<String>,
    pub embedding_profile_id: Option<String>,
    pub embedding_dimensions: Option<usize>,
    pub embedding_error: Option<String>,
}

/// Persisted markdown note surfaced in the Notes section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteDocument {
    pub id: NoteId,
    #[serde(default = "default_note_group_id")]
    pub group_id: GroupId,
    #[serde(default = "default_note_title")]
    pub title: String,
    #[serde(default)]
    pub markdown: String,
    #[serde(default)]
    pub updated_at_unix_ms: i64,
}

/// Input payload used to create a snippet.
#[derive(Debug, Clone)]
pub struct NewSnippet {
    pub source_session_id: SessionId,
    pub source_stream_id: String,
    pub source_cwd: String,
    pub text_snapshot_plain: String,
    pub start_offset: u64,
    pub end_offset: u64,
    pub start_row: u32,
    pub end_row: u32,
    pub created_at_unix_ms: i64,
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
    #[serde(default)]
    pub notes: Vec<NoteDocument>,
    #[serde(default)]
    pub snippets: Vec<Snippet>,
    #[serde(default = "default_ui_scale")]
    ui_scale: f64,
    pub selected_session: Option<SessionId>,
    #[serde(default)]
    pub selected_note_id: Option<NoteId>,
    next_session_id: SessionId,
    next_group_id: GroupId,
    #[serde(default = "default_next_note_id")]
    next_note_id: NoteId,
    #[serde(default = "default_next_snippet_id")]
    next_snippet_id: SnippetId,
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
            notes: Vec::new(),
            snippets: Vec::new(),
            ui_scale: UI_SCALE_DEFAULT,
            selected_session: None,
            selected_note_id: None,
            next_session_id: 1,
            next_group_id: 1,
            next_note_id: 1,
            next_snippet_id: 1,
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
        let fallback_group_id = self
            .selected_session
            .and_then(|session_id| self.sessions.iter().find(|session| session.id == session_id))
            .map(|session| session.group_id)
            .or_else(|| self.groups.first().map(|group| group.id))
            .unwrap_or(1);

        let max_group = self.groups.iter().map(|group| group.id).max().unwrap_or(0);
        let max_session = self
            .sessions
            .iter()
            .map(|session| session.id)
            .max()
            .unwrap_or(0);
        for note in &mut self.notes {
            if note.group_id == 0 || !valid_group_ids.contains(&note.group_id) {
                note.group_id = fallback_group_id;
            }
            if note.title.trim().is_empty() {
                note.title = default_note_title();
            }
        }
        self.notes.retain(|note| !note.title.trim().is_empty());
        if self
            .selected_note_id
            .is_some_and(|selected| !self.notes.iter().any(|note| note.id == selected))
        {
            self.selected_note_id = None;
        }
        for snippet in &mut self.snippets {
            if snippet.log_ref.end_offset < snippet.log_ref.start_offset {
                std::mem::swap(&mut snippet.log_ref.start_offset, &mut snippet.log_ref.end_offset);
            }
            if snippet.log_ref.end_row < snippet.log_ref.start_row {
                std::mem::swap(&mut snippet.log_ref.start_row, &mut snippet.log_ref.end_row);
            }
        }
        self.snippets
            .retain(|snippet| !snippet.text_snapshot_plain.trim().is_empty());
        let max_note = self.notes.iter().map(|note| note.id).max().unwrap_or(0);
        let max_snippet = self.snippets.iter().map(|snippet| snippet.id).max().unwrap_or(0);
        self.next_group_id = self.next_group_id.max(max_group.saturating_add(1));
        self.next_session_id = self.next_session_id.max(max_session.saturating_add(1));
        self.next_note_id = self.next_note_id.max(max_note.saturating_add(1));
        self.next_snippet_id = self.next_snippet_id.max(max_snippet.saturating_add(1));
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
        self.notes.retain(|note| note.group_id != group_id);

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
        if self
            .selected_note_id
            .is_some_and(|selected| !self.notes.iter().any(|note| note.id == selected))
        {
            self.selected_note_id = None;
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

    /// Returns all notes in insertion order.
    pub fn notes(&self) -> &[NoteDocument] {
        &self.notes
    }

    /// Returns notes for one path group in insertion order.
    pub fn notes_for_group(&self, group_id: GroupId) -> Vec<&NoteDocument> {
        self.notes
            .iter()
            .filter(|note| note.group_id == group_id)
            .collect()
    }

    /// Returns a note by identifier.
    pub fn note_by_id(&self, note_id: NoteId) -> Option<&NoteDocument> {
        self.notes.iter().find(|note| note.id == note_id)
    }

    /// Returns selected note for one group, falling back to first note in group.
    pub fn selected_note_id_for_group(&self, group_id: GroupId) -> Option<NoteId> {
        if let Some(selected) = self.selected_note_id
            && self
                .notes
                .iter()
                .any(|note| note.id == selected && note.group_id == group_id)
        {
            return Some(selected);
        }
        self.notes
            .iter()
            .find(|note| note.group_id == group_id)
            .map(|note| note.id)
    }

    /// Returns all snippets in display order (newest and promoted first).
    pub fn snippets(&self) -> &[Snippet] {
        &self.snippets
    }

    /// Returns a snippet by identifier.
    pub fn snippet_by_id(&self, snippet_id: SnippetId) -> Option<&Snippet> {
        self.snippets.iter().find(|snippet| snippet.id == snippet_id)
    }

    /// Returns snippets originating from one terminal session.
    pub fn snippets_for_session(&self, session_id: SessionId) -> Vec<&Snippet> {
        self.snippets
            .iter()
            .filter(|snippet| snippet.log_ref.session_id == session_id)
            .collect()
    }

    /// Returns the selected note identifier.
    pub fn selected_note_id(&self) -> Option<NoteId> {
        self.selected_note_id
    }

    /// Selects the active note.
    pub fn select_note(&mut self, note_id: NoteId) {
        if self.selected_note_id == Some(note_id) {
            return;
        }
        if !self.notes.iter().any(|note| note.id == note_id) {
            return;
        }
        self.selected_note_id = Some(note_id);
        self.mark_dirty();
    }

    /// Creates an empty note and returns its identifier.
    pub fn create_note_for_group(
        &mut self,
        group_id: GroupId,
        title: String,
        updated_at_unix_ms: i64,
    ) -> NoteId {
        let trimmed = title.trim();
        let note_id = self.next_note_id;
        self.next_note_id = self.next_note_id.saturating_add(1);
        self.notes.push(NoteDocument {
            id: note_id,
            group_id,
            title: if trimmed.is_empty() {
                default_note_title()
            } else {
                trimmed.to_string()
            },
            markdown: String::new(),
            updated_at_unix_ms,
        });
        self.selected_note_id = Some(note_id);
        self.mark_dirty();
        note_id
    }

    /// Creates an empty note in the active path group.
    pub fn create_note(&mut self, title: String, updated_at_unix_ms: i64) -> Option<NoteId> {
        let group_id = self.active_group_id()?;
        Some(self.create_note_for_group(group_id, title, updated_at_unix_ms))
    }

    /// Updates note markdown and touched timestamp.
    pub fn update_note_markdown(
        &mut self,
        note_id: NoteId,
        markdown: String,
        updated_at_unix_ms: i64,
    ) -> bool {
        let Some(note) = self.notes.iter_mut().find(|note| note.id == note_id) else {
            return false;
        };
        if note.markdown == markdown && note.updated_at_unix_ms == updated_at_unix_ms {
            return false;
        }
        note.markdown = markdown;
        note.updated_at_unix_ms = updated_at_unix_ms;
        self.mark_dirty();
        true
    }

    /// Appends a markdown snippet reference token to a note.
    pub fn append_note_snippet_reference(
        &mut self,
        note_id: NoteId,
        snippet_id: SnippetId,
        updated_at_unix_ms: i64,
    ) -> bool {
        let Some(note) = self.notes.iter_mut().find(|note| note.id == note_id) else {
            return false;
        };
        let token = snippet_reference_token(snippet_id);
        if !note.markdown.is_empty() && !note.markdown.ends_with('\n') {
            note.markdown.push('\n');
        }
        note.markdown.push_str(&token);
        note.markdown.push('\n');
        note.updated_at_unix_ms = updated_at_unix_ms;
        self.mark_dirty();
        true
    }

    /// Creates a snippet and returns its identifier.
    pub fn create_snippet(&mut self, new_snippet: NewSnippet) -> SnippetId {
        let snippet_id = self.next_snippet_id;
        self.next_snippet_id = self.next_snippet_id.saturating_add(1);
        let mut log_ref = SnippetLogRef {
            session_id: new_snippet.source_session_id,
            stream_id: new_snippet.source_stream_id,
            start_offset: new_snippet.start_offset.min(new_snippet.end_offset),
            end_offset: new_snippet.start_offset.max(new_snippet.end_offset),
            start_row: new_snippet.start_row.min(new_snippet.end_row),
            end_row: new_snippet.start_row.max(new_snippet.end_row),
        };
        if log_ref.end_offset < log_ref.start_offset {
            std::mem::swap(&mut log_ref.start_offset, &mut log_ref.end_offset);
        }
        if log_ref.end_row < log_ref.start_row {
            std::mem::swap(&mut log_ref.start_row, &mut log_ref.end_row);
        }
        self.snippets.insert(
            0,
            Snippet {
                id: snippet_id,
                created_at_unix_ms: new_snippet.created_at_unix_ms,
                source_cwd: new_snippet.source_cwd,
                text_snapshot_plain: new_snippet.text_snapshot_plain,
                log_ref,
                embedding_status: SnippetEmbeddingStatus::Pending,
                embedding_object_id: None,
                embedding_profile_id: None,
                embedding_dimensions: None,
                embedding_error: None,
            },
        );
        self.mark_dirty();
        snippet_id
    }

    /// Promotes one snippet to the top of the snippets list.
    pub fn promote_snippet(&mut self, snippet_id: SnippetId) -> bool {
        let Some(index) = self.snippets.iter().position(|snippet| snippet.id == snippet_id) else {
            return false;
        };
        if index == 0 {
            return false;
        }
        let snippet = self.snippets.remove(index);
        self.snippets.insert(0, snippet);
        self.mark_dirty();
        true
    }

    /// Marks snippet embedding as processing.
    pub fn set_snippet_embedding_processing(&mut self, snippet_id: SnippetId) -> bool {
        let Some(snippet) = self.snippets.iter_mut().find(|snippet| snippet.id == snippet_id)
        else {
            return false;
        };
        if snippet.embedding_status == SnippetEmbeddingStatus::Processing {
            return false;
        }
        snippet.embedding_status = SnippetEmbeddingStatus::Processing;
        snippet.embedding_error = None;
        self.mark_dirty();
        true
    }

    /// Marks snippet embedding as ready.
    pub fn set_snippet_embedding_ready(
        &mut self,
        snippet_id: SnippetId,
        embedding_object_id: String,
        embedding_profile_id: Option<String>,
        embedding_dimensions: Option<usize>,
    ) -> bool {
        let Some(snippet) = self.snippets.iter_mut().find(|snippet| snippet.id == snippet_id)
        else {
            return false;
        };
        snippet.embedding_status = SnippetEmbeddingStatus::Ready;
        snippet.embedding_object_id = Some(embedding_object_id);
        snippet.embedding_profile_id = embedding_profile_id;
        snippet.embedding_dimensions = embedding_dimensions;
        snippet.embedding_error = None;
        self.mark_dirty();
        true
    }

    /// Marks snippet embedding as failed.
    pub fn set_snippet_embedding_failed(
        &mut self,
        snippet_id: SnippetId,
        error: String,
    ) -> bool {
        let Some(snippet) = self.snippets.iter_mut().find(|snippet| snippet.id == snippet_id)
        else {
            return false;
        };
        snippet.embedding_status = SnippetEmbeddingStatus::Failed;
        snippet.embedding_error = Some(error);
        self.mark_dirty();
        true
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

fn default_next_note_id() -> NoteId {
    1
}

fn default_next_snippet_id() -> SnippetId {
    1
}

/// Returns the markdown token used to reference a snippet inside notes.
pub fn snippet_reference_token(snippet_id: SnippetId) -> String {
    format!("[[snippet:{snippet_id}]]")
}

/// Parses snippet identifiers referenced by markdown snippet tokens.
pub fn parse_snippet_reference_tokens(markdown: &str) -> Vec<SnippetId> {
    const PREFIX: &str = "[[snippet:";
    let mut refs = Vec::new();
    let mut remainder = markdown;
    while let Some(prefix_idx) = remainder.find(PREFIX) {
        let candidate = &remainder[prefix_idx + PREFIX.len()..];
        let Some(end_idx) = candidate.find("]]") else {
            break;
        };
        let raw_id = &candidate[..end_idx];
        if let Ok(snippet_id) = raw_id.trim().parse::<SnippetId>() {
            refs.push(snippet_id);
        }
        remainder = &candidate[end_idx + 2..];
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::{
        AppState, NewSnippet, NoteDocument, SnippetEmbeddingStatus, parse_snippet_reference_tokens,
        snippet_reference_token,
    };

    fn sample_snippet(created_at_unix_ms: i64, text: &str) -> NewSnippet {
        NewSnippet {
            source_session_id: 7,
            source_stream_id: "terminal:7".to_string(),
            source_cwd: "/tmp".to_string(),
            text_snapshot_plain: text.to_string(),
            start_offset: 10,
            end_offset: 40,
            start_row: 3,
            end_row: 5,
            created_at_unix_ms,
        }
    }

    #[test]
    fn snippet_reference_tokens_round_trip() {
        let token = snippet_reference_token(42);
        assert_eq!(token, "[[snippet:42]]");
        assert_eq!(
            parse_snippet_reference_tokens("a [[snippet:2]] b [[snippet:44]]"),
            vec![2, 44]
        );
    }

    #[test]
    fn create_snippet_prepends_to_collection() {
        let mut state = AppState::default();
        let first_id = state.create_snippet(sample_snippet(100, "first"));
        let second_id = state.create_snippet(sample_snippet(200, "second"));
        assert_eq!(state.snippets().first().map(|snippet| snippet.id), Some(second_id));
        assert_eq!(state.snippets().get(1).map(|snippet| snippet.id), Some(first_id));
    }

    #[test]
    fn snippet_embedding_status_transitions() {
        let mut state = AppState::default();
        let snippet_id = state.create_snippet(sample_snippet(100, "hello"));
        assert_eq!(
            state
                .snippet_by_id(snippet_id)
                .map(|snippet| snippet.embedding_status),
            Some(SnippetEmbeddingStatus::Pending)
        );
        assert!(state.set_snippet_embedding_processing(snippet_id));
        assert!(state.set_snippet_embedding_ready(
            snippet_id,
            "obj-1".to_string(),
            Some("qwen3-0.6b".to_string()),
            Some(1024),
        ));
        assert_eq!(
            state
                .snippet_by_id(snippet_id)
                .and_then(|snippet| snippet.embedding_object_id.clone()),
            Some("obj-1".to_string())
        );
    }

    #[test]
    fn default_state_starts_without_notes() {
        let state = AppState::default();
        assert!(state.notes().is_empty());
        let active_group = state.active_group_id().expect("active group");
        assert_eq!(state.selected_note_id_for_group(active_group), None);
    }

    #[test]
    fn create_note_for_group_scopes_selection() {
        let mut state = AppState::default();
        let active_group = state.active_group_id().expect("active group");
        let group_two = state.add_group_with_path("/tmp/other".to_string());

        let note_a = state.create_note_for_group(active_group, "Build".to_string(), 100);
        let note_b = state.create_note_for_group(group_two, "Ideas".to_string(), 200);

        assert_eq!(state.selected_note_id(), Some(note_b));
        assert_eq!(state.selected_note_id_for_group(active_group), Some(note_a));
        assert_eq!(state.selected_note_id_for_group(group_two), Some(note_b));
        assert_eq!(state.notes_for_group(active_group).len(), 1);
        assert_eq!(state.notes_for_group(group_two).len(), 1);
    }

    #[test]
    fn restore_assigns_legacy_group_zero_notes_to_active_group() {
        let mut state = AppState::default();
        let active_group = state.active_group_id().expect("active group");
        state.notes.push(NoteDocument {
            id: 999,
            group_id: 0,
            title: "Legacy".to_string(),
            markdown: "old".to_string(),
            updated_at_unix_ms: 1,
        });
        state.selected_note_id = Some(999);
        state.next_note_id = 1_000;

        let restored = state.into_restored();
        let note = restored.note_by_id(999).expect("note exists");
        assert_eq!(note.group_id, active_group);
        assert_eq!(restored.selected_note_id(), Some(999));
    }
}
