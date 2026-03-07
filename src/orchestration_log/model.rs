use crate::state::{GroupId, SessionId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandKind {
    BroadcastSendLine,
    BroadcastInterrupt,
    GitStageFiles,
    GitUnstageFiles,
    GitCreateCommit,
    GitUpdateCommitMessage,
    GitCreateTag,
    GitCheckoutTarget,
    GitCreateWorktree,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandPayload {
    BroadcastSendLine {
        group_id: GroupId,
        group_path: String,
        session_ids: Vec<SessionId>,
        line: String,
    },
    BroadcastInterrupt {
        group_id: GroupId,
        group_path: String,
        session_ids: Vec<SessionId>,
    },
    GitStageFiles {
        group_path: String,
        paths: Vec<String>,
    },
    GitUnstageFiles {
        group_path: String,
        paths: Vec<String>,
    },
    GitCreateCommit {
        group_path: String,
        title: String,
        has_message_body: bool,
    },
    GitUpdateCommitMessage {
        group_path: String,
        target_sha: String,
        title: String,
        has_message_body: bool,
    },
    GitCreateTag {
        group_path: String,
        tag_name: String,
        target_sha: String,
    },
    GitCheckoutTarget {
        group_path: String,
        target: String,
        target_kind: String,
    },
    GitCreateWorktree {
        group_path: String,
        new_path: String,
        target: String,
    },
}

impl CommandPayload {
    pub fn kind(&self) -> CommandKind {
        match self {
            Self::BroadcastSendLine { .. } => CommandKind::BroadcastSendLine,
            Self::BroadcastInterrupt { .. } => CommandKind::BroadcastInterrupt,
            Self::GitStageFiles { .. } => CommandKind::GitStageFiles,
            Self::GitUnstageFiles { .. } => CommandKind::GitUnstageFiles,
            Self::GitCreateCommit { .. } => CommandKind::GitCreateCommit,
            Self::GitUpdateCommitMessage { .. } => CommandKind::GitUpdateCommitMessage,
            Self::GitCreateTag { .. } => CommandKind::GitCreateTag,
            Self::GitCheckoutTarget { .. } => CommandKind::GitCheckoutTarget,
            Self::GitCreateWorktree { .. } => CommandKind::GitCreateWorktree,
        }
    }

    pub fn group_id(&self) -> Option<GroupId> {
        match self {
            Self::BroadcastSendLine { group_id, .. }
            | Self::BroadcastInterrupt { group_id, .. } => Some(*group_id),
            _ => None,
        }
    }

    pub fn group_path(&self) -> &str {
        match self {
            Self::BroadcastSendLine { group_path, .. }
            | Self::BroadcastInterrupt { group_path, .. }
            | Self::GitStageFiles { group_path, .. }
            | Self::GitUnstageFiles { group_path, .. }
            | Self::GitCreateCommit { group_path, .. }
            | Self::GitUpdateCommitMessage { group_path, .. }
            | Self::GitCreateTag { group_path, .. }
            | Self::GitCheckoutTarget { group_path, .. }
            | Self::GitCreateWorktree { group_path, .. } => group_path.as_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    BroadcastWriteSucceeded,
    BroadcastWriteFailed,
    GitPathSucceeded,
    GitPathFailed,
    GitOperationSucceeded,
    GitOperationFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventPayload {
    BroadcastWriteSucceeded {
        session_id: SessionId,
    },
    BroadcastWriteFailed {
        session_id: SessionId,
        error: String,
    },
    GitPathSucceeded {
        path: String,
    },
    GitPathFailed {
        path: String,
        error: String,
    },
    GitOperationSucceeded {
        summary: String,
    },
    GitOperationFailed {
        error: String,
    },
}

impl EventPayload {
    pub fn kind(&self) -> EventKind {
        match self {
            Self::BroadcastWriteSucceeded { .. } => EventKind::BroadcastWriteSucceeded,
            Self::BroadcastWriteFailed { .. } => EventKind::BroadcastWriteFailed,
            Self::GitPathSucceeded { .. } => EventKind::GitPathSucceeded,
            Self::GitPathFailed { .. } => EventKind::GitPathFailed,
            Self::GitOperationSucceeded { .. } => EventKind::GitOperationSucceeded,
            Self::GitOperationFailed { .. } => EventKind::GitOperationFailed,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Succeeded,
    PartiallySucceeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReceiptPayload {
    Broadcast {
        ok_count: usize,
        fail_count: usize,
    },
    Git {
        ok_count: usize,
        fail_count: usize,
        summary: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCommandRecord {
    pub command_id: String,
    pub timeline_id: String,
    pub requested_at_unix_ms: i64,
    pub recorded_at_unix_ms: i64,
    pub payload: CommandPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEventRecord {
    pub occurred_at_unix_ms: i64,
    pub recorded_at_unix_ms: i64,
    pub payload: EventPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewReceiptRecord {
    pub completed_at_unix_ms: i64,
    pub recorded_at_unix_ms: i64,
    pub status: ReceiptStatus,
    pub payload: ReceiptPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRecord {
    pub command_id: String,
    pub timeline_id: String,
    pub sequence_in_timeline: i64,
    pub kind: CommandKind,
    pub group_id: Option<GroupId>,
    pub group_path: String,
    pub requested_at_unix_ms: i64,
    pub recorded_at_unix_ms: i64,
    pub payload: CommandPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    pub event_id: String,
    pub command_id: String,
    pub timeline_id: String,
    pub sequence_in_timeline: i64,
    pub kind: EventKind,
    pub occurred_at_unix_ms: i64,
    pub recorded_at_unix_ms: i64,
    pub payload: EventPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptRecord {
    pub command_id: String,
    pub timeline_id: String,
    pub sequence_in_timeline: i64,
    pub status: ReceiptStatus,
    pub completed_at_unix_ms: i64,
    pub recorded_at_unix_ms: i64,
    pub payload: ReceiptPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineEntry {
    Command(CommandRecord),
    Event(EventRecord),
    Receipt(ReceiptRecord),
}

impl TimelineEntry {
    pub fn sequence_in_timeline(&self) -> i64 {
        match self {
            Self::Command(record) => record.sequence_in_timeline,
            Self::Event(record) => record.sequence_in_timeline,
            Self::Receipt(record) => record.sequence_in_timeline,
        }
    }

    pub fn timeline_id(&self) -> &str {
        match self {
            Self::Command(record) => record.timeline_id.as_str(),
            Self::Event(record) => record.timeline_id.as_str(),
            Self::Receipt(record) => record.timeline_id.as_str(),
        }
    }
}
