use crate::emily_bridge::EmilyBridge;
use crate::orchestrator::GroupOrchestratorSnapshot;
use crate::state::SessionId;
use emily::model::{ContextItem, ContextPacket, TextObjectKind};
use std::sync::Arc;

const LOCAL_AGENT_CONTEXT_TOP_K: usize = 3;
const LOCAL_AGENT_CONTEXT_ITEM_LIMIT: usize = 3;
const LOCAL_AGENT_CONTEXT_TEXT_LIMIT: usize = 220;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedContextFragment {
    pub object_id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedLocalAgentCommand {
    pub display_command: String,
    pub dispatched_command: String,
    pub context_status: LocalAgentContextStatus,
    pub source_session_id: Option<SessionId>,
    pub context_object_ids: Vec<String>,
    pub context_fragments: Vec<PreparedContextFragment>,
    pub context_item_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalAgentContextStatus {
    Attached {
        session_id: SessionId,
        item_count: usize,
    },
    NoCandidateSession,
    NoContext {
        session_id: SessionId,
    },
    Unavailable {
        session_id: SessionId,
        error: String,
    },
}

impl LocalAgentContextStatus {
    pub fn feedback_suffix(&self) -> Option<String> {
        match self {
            Self::Attached {
                session_id,
                item_count,
            } => Some(format!(
                " Emily context attached from session {session_id} ({item_count} items)."
            )),
            Self::NoCandidateSession => Some(
                " Emily context unavailable because this group has no candidate session."
                    .to_string(),
            ),
            Self::NoContext { session_id } => Some(format!(
                " Emily returned no matching context for session {session_id}."
            )),
            Self::Unavailable { session_id, error } => Some(format!(
                " Emily context fallback for session {session_id}: {error}."
            )),
        }
    }
}

pub async fn prepare_local_agent_command(
    emily_bridge: Arc<EmilyBridge>,
    group_orchestrator: GroupOrchestratorSnapshot,
    command: String,
) -> PreparedLocalAgentCommand {
    let Some(session_id) = select_context_session(&group_orchestrator) else {
        return PreparedLocalAgentCommand {
            display_command: command.clone(),
            dispatched_command: command,
            context_status: LocalAgentContextStatus::NoCandidateSession,
            source_session_id: None,
            context_object_ids: Vec::new(),
            context_fragments: Vec::new(),
            context_item_count: 0,
        };
    };

    let context_result = emily_bridge
        .query_context_async(session_id, command.clone(), LOCAL_AGENT_CONTEXT_TOP_K)
        .await;

    match context_result {
        Ok(packet) if !packet.items.is_empty() => PreparedLocalAgentCommand {
            display_command: command.clone(),
            dispatched_command: format_command_with_context(session_id, &command, &packet),
            context_status: LocalAgentContextStatus::Attached {
                session_id,
                item_count: packet.items.len(),
            },
            source_session_id: Some(session_id),
            context_object_ids: packet
                .items
                .iter()
                .map(|item| item.object.id.clone())
                .collect(),
            context_fragments: packet
                .items
                .iter()
                .map(|item| PreparedContextFragment {
                    object_id: item.object.id.clone(),
                    text: item.object.text.clone(),
                })
                .collect(),
            context_item_count: packet.items.len(),
        },
        Ok(_) => PreparedLocalAgentCommand {
            display_command: command.clone(),
            dispatched_command: command,
            context_status: LocalAgentContextStatus::NoContext { session_id },
            source_session_id: Some(session_id),
            context_object_ids: Vec::new(),
            context_fragments: Vec::new(),
            context_item_count: 0,
        },
        Err(error) => PreparedLocalAgentCommand {
            display_command: command.clone(),
            dispatched_command: command,
            context_status: LocalAgentContextStatus::Unavailable { session_id, error },
            source_session_id: Some(session_id),
            context_object_ids: Vec::new(),
            context_fragments: Vec::new(),
            context_item_count: 0,
        },
    }
}

fn select_context_session(group_orchestrator: &GroupOrchestratorSnapshot) -> Option<SessionId> {
    group_orchestrator
        .terminals
        .iter()
        .find(|terminal| terminal.is_focused)
        .or_else(|| {
            group_orchestrator
                .terminals
                .iter()
                .find(|terminal| terminal.is_selected)
        })
        .or_else(|| {
            group_orchestrator
                .terminals
                .iter()
                .find(|terminal| terminal.is_runtime_ready)
        })
        .or_else(|| group_orchestrator.terminals.first())
        .map(|terminal| terminal.session_id)
}

fn format_command_with_context(
    session_id: SessionId,
    command: &str,
    packet: &ContextPacket,
) -> String {
    let context_lines = packet
        .items
        .iter()
        .take(LOCAL_AGENT_CONTEXT_ITEM_LIMIT)
        .map(format_context_item)
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "{command}\n\nEmily context from session {session_id}:\n{context_lines}\nUse the Emily context above only when it helps the current task."
    )
}

fn format_context_item(item: &ContextItem) -> String {
    let text = truncate_context_text(&item.object.text);
    format!(
        "- [{} #{}] {}",
        object_kind_label(item.object.object_kind),
        item.object.sequence,
        text
    )
}

fn object_kind_label(kind: TextObjectKind) -> &'static str {
    match kind {
        TextObjectKind::UserInput => "input",
        TextObjectKind::SystemOutput => "output",
        TextObjectKind::Summary => "summary",
        TextObjectKind::Note => "note",
        TextObjectKind::Other => "other",
    }
}

fn truncate_context_text(text: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= LOCAL_AGENT_CONTEXT_TEXT_LIMIT {
        return normalized;
    }

    let mut truncated = normalized
        .chars()
        .take(LOCAL_AGENT_CONTEXT_TEXT_LIMIT)
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use super::{
        LocalAgentContextStatus, PreparedContextFragment, PreparedLocalAgentCommand,
        object_kind_label, truncate_context_text,
    };
    use emily::model::TextObjectKind;

    #[test]
    fn attached_feedback_mentions_source_session() {
        let status = LocalAgentContextStatus::Attached {
            session_id: 7,
            item_count: 2,
        };
        assert_eq!(
            status.feedback_suffix().as_deref(),
            Some(" Emily context attached from session 7 (2 items).")
        );
    }

    #[test]
    fn long_context_text_is_truncated_for_prompt() {
        let long_text = "a".repeat(400);
        let truncated = truncate_context_text(&long_text);
        assert!(truncated.len() < long_text.len());
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn object_kind_labels_are_stable() {
        assert_eq!(object_kind_label(TextObjectKind::UserInput), "input");
        assert_eq!(object_kind_label(TextObjectKind::SystemOutput), "output");
        assert_eq!(object_kind_label(TextObjectKind::Summary), "summary");
        assert_eq!(object_kind_label(TextObjectKind::Note), "note");
        assert_eq!(object_kind_label(TextObjectKind::Other), "other");
    }

    #[test]
    fn prepared_command_keeps_display_command_separate() {
        let prepared = PreparedLocalAgentCommand {
            display_command: "cargo test".to_string(),
            dispatched_command: "cargo test\n\nEmily context".to_string(),
            context_status: LocalAgentContextStatus::NoCandidateSession,
            source_session_id: None,
            context_object_ids: Vec::new(),
            context_fragments: vec![PreparedContextFragment {
                object_id: "ctx-1".to_string(),
                text: "repository clean".to_string(),
            }],
            context_item_count: 0,
        };
        assert_eq!(prepared.display_command, "cargo test");
        assert!(prepared.dispatched_command.starts_with("cargo test"));
    }
}
