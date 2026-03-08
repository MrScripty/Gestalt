mod commands;
mod knowledge;
mod panel_dock;
mod workspace;

pub use commands::CommandState;
pub use knowledge::KnowledgeState;
pub use panel_dock::{AuxiliaryPanelHost, AuxiliaryPanelKind, AuxiliaryPanelLayout};
pub use workspace::WorkspaceState;

use crate::commands::{CommandId, InsertCommand};
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

pub(crate) fn clamp_group_runner_width_px(width: i32) -> i32 {
    width.clamp(GROUP_RUNNER_WIDTH_MIN_PX, GROUP_RUNNER_WIDTH_MAX_PX)
}

pub(crate) fn clamp_group_split_ratio(ratio: f64) -> f64 {
    if !ratio.is_finite() {
        return GROUP_SPLIT_RATIO_DEFAULT;
    }

    ratio.clamp(GROUP_SPLIT_MIN_RATIO, GROUP_SPLIT_MAX_RATIO)
}

pub(crate) fn default_group_runner_width_px() -> i32 {
    GROUP_RUNNER_WIDTH_DEFAULT_PX
}

pub(crate) fn default_group_split_ratio() -> f64 {
    GROUP_SPLIT_RATIO_DEFAULT
}

pub(crate) fn default_note_title() -> String {
    "Note".to_string()
}

pub(crate) fn default_note_group_id() -> GroupId {
    0
}

pub(crate) fn default_ui_scale() -> f64 {
    UI_SCALE_DEFAULT
}

pub(crate) fn default_next_note_id() -> NoteId {
    1
}

pub(crate) fn default_next_snippet_id() -> SnippetId {
    1
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
    pub(crate) fn normalized(self) -> Self {
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

/// Aggregate durable app state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    #[serde(flatten, default)]
    pub(crate) workspace: WorkspaceState,
    #[serde(flatten, default)]
    pub(crate) knowledge: KnowledgeState,
    #[serde(flatten, default)]
    pub(crate) commands: CommandState,
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
            workspace: WorkspaceState::default(),
            knowledge: KnowledgeState::default(),
            commands: CommandState::default(),
            revision: 0,
        }
    }

    pub(crate) fn mark_dirty(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }

    /// Returns the monotonic mutation counter for autosave signaling.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns all sessions in insertion order.
    pub fn sessions(&self) -> &[Session] {
        self.workspace.sessions()
    }

    /// Returns all groups in insertion order.
    pub fn groups(&self) -> &[TabGroup] {
        self.workspace.groups()
    }

    /// Returns the active selected session identifier.
    pub fn selected_session(&self) -> Option<SessionId> {
        self.workspace.selected_session()
    }

    /// Returns the durable workspace domain.
    pub fn workspace_state(&self) -> &WorkspaceState {
        &self.workspace
    }

    /// Returns the durable knowledge domain.
    pub fn knowledge_state(&self) -> &KnowledgeState {
        &self.knowledge
    }

    /// Returns the durable command domain.
    pub fn command_state(&self) -> &CommandState {
        &self.commands
    }

    /// Repairs persisted invariants and resets mutation tracking.
    pub fn into_restored(mut self) -> Self {
        self.repair_after_restore();
        self.revision = 0;
        self
    }

    /// Repairs invalid relationships after loading persisted state.
    pub fn repair_after_restore(&mut self) {
        self.commands.repair_after_restore();
        self.workspace.repair_after_restore();
        let valid_group_ids = self.workspace.valid_group_ids();
        let fallback_group_id = self.workspace.active_group_id().unwrap_or(1);
        self.knowledge
            .repair_after_restore(&valid_group_ids, fallback_group_id);
    }
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
mod tests;
