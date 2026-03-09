use crate::emily_bridge::EmilyBridge;
use crate::local_agent_context::PreparedLocalAgentCommand;
use crate::state::{GroupId, SessionId};
use emily::model::{
    CreateEpisodeRequest, EarlDecision, EarlEvaluationRecord, EpisodeRecord, EpisodeState,
    EpisodeTraceKind, TraceLinkRequest,
};
use serde_json::json;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAgentEpisodeRequest {
    pub group_id: GroupId,
    pub group_path: String,
    pub run_id: Option<String>,
    pub display_command: String,
    pub source_session_id: Option<SessionId>,
    pub context_object_ids: Vec<String>,
    pub context_item_count: usize,
    pub dispatch_ok_count: usize,
    pub dispatch_fail_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAgentEpisodeStatus {
    pub episode_id: String,
    pub state: EpisodeState,
    pub latest_earl: Option<EarlDecision>,
    pub gate: LocalAgentEpisodeGate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalAgentEpisodeGate {
    Proceed,
    Caution,
    Blocked,
}

impl LocalAgentEpisodeStatus {
    pub fn feedback_suffix(&self) -> String {
        match self.gate {
            LocalAgentEpisodeGate::Proceed => {
                format!(
                    " Emily episode {} recorded as {:?}.",
                    self.episode_id, self.state
                )
            }
            LocalAgentEpisodeGate::Caution => format!(
                " Emily episode {} is cautioned ({:?}).",
                self.episode_id, self.state
            ),
            LocalAgentEpisodeGate::Blocked => format!(
                " Emily episode {} is blocked ({:?}).",
                self.episode_id, self.state
            ),
        }
    }
}

pub async fn record_local_agent_episode(
    emily_bridge: Arc<EmilyBridge>,
    request: LocalAgentEpisodeRequest,
) -> Result<LocalAgentEpisodeStatus, String> {
    let started_at_unix_ms = current_unix_ms();
    let episode_id = request
        .run_id
        .as_deref()
        .map(|run_id| format!("local-agent:{run_id}"))
        .unwrap_or_else(|| format!("local-agent:{}", Uuid::new_v4()));
    let episode = emily_bridge
        .create_episode_async(CreateEpisodeRequest {
            episode_id: episode_id.clone(),
            stream_id: request.source_session_id.map(stream_id),
            source_kind: "gestalt-local-agent".to_string(),
            episode_kind: "local_agent_run".to_string(),
            started_at_unix_ms,
            intent: Some(request.display_command.clone()),
            metadata: json!({
                "group_id": request.group_id,
                "group_path": request.group_path,
                "run_id": request.run_id,
                "context_item_count": request.context_item_count,
                "dispatch_ok_count": request.dispatch_ok_count,
                "dispatch_fail_count": request.dispatch_fail_count,
            }),
        })
        .await?;

    for object_id in &request.context_object_ids {
        let _ = emily_bridge
            .link_text_to_episode_async(TraceLinkRequest {
                episode_id: episode_id.clone(),
                object_id: object_id.clone(),
                trace_kind: EpisodeTraceKind::Context,
                linked_at_unix_ms: started_at_unix_ms,
                metadata: json!({
                    "group_id": request.group_id,
                    "group_path": request.group_path,
                }),
            })
            .await?;
    }

    inspect_local_agent_episode(emily_bridge, &episode.id).await
}

pub async fn inspect_local_agent_episode(
    emily_bridge: Arc<EmilyBridge>,
    episode_id: &str,
) -> Result<LocalAgentEpisodeStatus, String> {
    let Some(episode) = emily_bridge.episode_async(episode_id.to_string()).await? else {
        return Err(format!("Emily episode {episode_id} was not found"));
    };
    let latest_earl = emily_bridge
        .latest_earl_evaluation_for_episode_async(episode_id.to_string())
        .await?;
    Ok(local_agent_episode_status_from_parts(
        &episode,
        latest_earl.as_ref(),
    ))
}

pub fn episode_request_from_prepared_command(
    group_id: GroupId,
    group_path: String,
    run_id: Option<String>,
    prepared: &PreparedLocalAgentCommand,
    dispatch_ok_count: usize,
    dispatch_fail_count: usize,
) -> LocalAgentEpisodeRequest {
    LocalAgentEpisodeRequest {
        group_id,
        group_path,
        run_id,
        display_command: prepared.display_command.clone(),
        source_session_id: prepared.source_session_id,
        context_object_ids: prepared.context_object_ids.clone(),
        context_item_count: prepared.context_item_count,
        dispatch_ok_count,
        dispatch_fail_count,
    }
}

pub fn local_agent_episode_status_from_parts(
    episode: &EpisodeRecord,
    latest_earl: Option<&EarlEvaluationRecord>,
) -> LocalAgentEpisodeStatus {
    let latest_earl_decision = latest_earl.map(|record| record.decision);
    let gate = match latest_earl_decision {
        Some(EarlDecision::Reflex) => LocalAgentEpisodeGate::Blocked,
        Some(EarlDecision::Caution) => LocalAgentEpisodeGate::Caution,
        Some(EarlDecision::Ok) | None => match episode.state {
            EpisodeState::Blocked | EpisodeState::Cancelled => LocalAgentEpisodeGate::Blocked,
            EpisodeState::Cautioned => LocalAgentEpisodeGate::Caution,
            EpisodeState::Open | EpisodeState::Completed => LocalAgentEpisodeGate::Proceed,
        },
    };
    LocalAgentEpisodeStatus {
        episode_id: episode.id.clone(),
        state: episode.state,
        latest_earl: latest_earl_decision,
        gate,
    }
}

fn current_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as i64)
}

fn stream_id(session_id: SessionId) -> String {
    format!("terminal:{session_id}")
}

#[cfg(test)]
mod tests {
    use super::{LocalAgentEpisodeGate, current_unix_ms, local_agent_episode_status_from_parts};
    use emily::model::{
        EarlDecision, EarlEvaluationRecord, EarlHostAction, EarlSignalVector, EpisodeRecord,
        EpisodeState,
    };
    use serde_json::json;

    #[test]
    fn host_gate_prefers_reflex_earl_over_episode_state() {
        let episode = episode_with_state(EpisodeState::Open);
        let latest_earl = EarlEvaluationRecord {
            id: "earl-1".to_string(),
            episode_id: "ep-1".to_string(),
            evaluated_at_unix_ms: current_unix_ms(),
            signals: EarlSignalVector {
                uncertainty: 0.9,
                conflict: 0.9,
                continuity_drift: 0.9,
                constraint_pressure: 0.9,
                tool_instability: 0.9,
                novelty_spike: 0.9,
            },
            risk_score: 0.95,
            decision: EarlDecision::Reflex,
            host_action: EarlHostAction::Abort,
            retryable: false,
            rationale: "seed reflex".to_string(),
            metadata: json!({}),
        };
        let status = local_agent_episode_status_from_parts(&episode, Some(&latest_earl));
        assert_eq!(status.gate, LocalAgentEpisodeGate::Blocked);
    }

    #[test]
    fn host_gate_cautions_cautioned_episode_without_earl() {
        let episode = episode_with_state(EpisodeState::Cautioned);
        let status = local_agent_episode_status_from_parts(&episode, None);
        assert_eq!(status.gate, LocalAgentEpisodeGate::Caution);
    }

    fn episode_with_state(state: EpisodeState) -> EpisodeRecord {
        EpisodeRecord {
            id: "ep-1".to_string(),
            stream_id: Some("terminal:1".to_string()),
            source_kind: "gestalt-local-agent".to_string(),
            episode_kind: "local_agent_run".to_string(),
            state,
            started_at_unix_ms: 1,
            closed_at_unix_ms: None,
            intent: Some("cargo check".to_string()),
            metadata: json!({}),
            last_outcome_id: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
        }
    }
}
