use crate::emily_bridge::EmilyBridge;
use crate::local_agent_episode::{LocalAgentEpisodeGate, local_agent_episode_status_from_parts};
use crate::orchestration_log::{CommandPayload, OrchestrationLogStore, RecentActivityRecord};
use emily::model::{
    EarlDecision, EpisodeState, RemoteEpisodeState, RoutingDecisionKind, ValidationDecision,
};
use std::sync::Arc;

/// One recent orchestration record optionally enriched with Emily state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentActivitySnapshot {
    pub activity: RecentActivityRecord,
    pub emily: Option<RecentActivityEmilySummary>,
}

/// Read-only Emily summary derived from the run-linked episode and sovereign records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentActivityEmilySummary {
    pub episode_id: String,
    pub gate: LocalAgentEpisodeGate,
    pub episode_state: EpisodeState,
    pub latest_earl: Option<EarlDecision>,
    pub latest_route_kind: Option<RoutingDecisionKind>,
    pub latest_validation_decision: Option<ValidationDecision>,
    pub latest_remote_state: Option<RemoteEpisodeState>,
}

impl RecentActivityEmilySummary {
    pub fn gate_label(&self) -> &'static str {
        match self.gate {
            LocalAgentEpisodeGate::Proceed => "EMILY OK",
            LocalAgentEpisodeGate::Caution => "EMILY CAUTION",
            LocalAgentEpisodeGate::Blocked => "EMILY BLOCKED",
        }
    }

    pub fn detail_line(&self) -> String {
        let mut parts = vec![
            format!("episode {}", self.episode_id),
            format!("state {:?}", self.episode_state),
        ];
        if let Some(decision) = self.latest_earl {
            parts.push(format!("earl {:?}", decision));
        }
        if let Some(kind) = self.latest_route_kind {
            parts.push(format!("route {:?}", kind));
        }
        if let Some(decision) = self.latest_validation_decision {
            parts.push(format!("validation {:?}", decision));
        }
        if let Some(state) = self.latest_remote_state {
            parts.push(format!("remote {:?}", state));
        }
        format!("Emily {}", parts.join(" | "))
    }
}

/// Load recent orchestration activity and enrich any run-linked local-agent entries with Emily state.
pub async fn load_recent_activity_snapshot(
    emily_bridge: Arc<EmilyBridge>,
    group_path: String,
    limit: usize,
) -> Result<Vec<RecentActivitySnapshot>, String> {
    let records = tokio::task::spawn_blocking(move || {
        OrchestrationLogStore::default().load_recent_activity_for_group_path(&group_path, limit)
    })
    .await
    .map_err(|error| format!("Failed loading recent activity: {error}"))?
    .map_err(|error| error.to_string())?;

    let mut snapshots = Vec::with_capacity(records.len());
    for record in records {
        let emily = load_emily_summary(emily_bridge.clone(), &record).await?;
        snapshots.push(RecentActivitySnapshot {
            activity: record,
            emily,
        });
    }
    Ok(snapshots)
}

async fn load_emily_summary(
    emily_bridge: Arc<EmilyBridge>,
    record: &RecentActivityRecord,
) -> Result<Option<RecentActivityEmilySummary>, String> {
    let Some(episode_id) = episode_id_for_activity(record) else {
        return Ok(None);
    };
    let Some(episode) = emily_bridge.episode_async(episode_id.clone()).await? else {
        return Ok(None);
    };
    let latest_earl = emily_bridge
        .latest_earl_evaluation_for_episode_async(episode_id.clone())
        .await?;
    let episode_status = local_agent_episode_status_from_parts(&episode, latest_earl.as_ref());
    let latest_route = latest_record_by(
        emily_bridge
            .routing_decisions_for_episode_async(episode_id.clone())
            .await?,
        |record| record.decided_at_unix_ms,
    );
    let latest_validation = latest_record_by(
        emily_bridge
            .validation_outcomes_for_episode_async(episode_id.clone())
            .await?,
        |record| record.validated_at_unix_ms,
    );
    let latest_remote = latest_record_by(
        emily_bridge
            .remote_episodes_for_episode_async(episode_id.clone())
            .await?,
        |record| record.dispatched_at_unix_ms,
    );

    Ok(Some(RecentActivityEmilySummary {
        episode_id,
        gate: episode_status.gate,
        episode_state: episode_status.state,
        latest_earl: episode_status.latest_earl,
        latest_route_kind: latest_route.map(|record| record.kind),
        latest_validation_decision: latest_validation.map(|record| record.decision),
        latest_remote_state: latest_remote.map(|record| record.state),
    }))
}

fn episode_id_for_activity(record: &RecentActivityRecord) -> Option<String> {
    match &record.command.payload {
        CommandPayload::LocalAgentSendLine {
            run_id: Some(run_id),
            ..
        } => Some(format!("local-agent:{run_id}")),
        _ => None,
    }
}

fn latest_record_by<T, F>(records: Vec<T>, key: F) -> Option<T>
where
    F: Fn(&T) -> i64,
{
    records.into_iter().max_by_key(key)
}

#[cfg(test)]
mod tests {
    use super::{RecentActivityEmilySummary, latest_record_by};
    use crate::local_agent_episode::LocalAgentEpisodeGate;
    use emily::model::{
        EarlDecision, EpisodeState, RemoteEpisodeState, RoutingDecisionKind, ValidationDecision,
    };

    #[test]
    fn detail_line_formats_available_emily_fields() {
        let summary = RecentActivityEmilySummary {
            episode_id: "local-agent:run-1".to_string(),
            gate: LocalAgentEpisodeGate::Caution,
            episode_state: EpisodeState::Cautioned,
            latest_earl: Some(EarlDecision::Caution),
            latest_route_kind: Some(RoutingDecisionKind::SingleRemote),
            latest_validation_decision: Some(ValidationDecision::NeedsReview),
            latest_remote_state: Some(RemoteEpisodeState::Dispatched),
        };

        let detail = summary.detail_line();
        assert!(detail.contains("episode local-agent:run-1"));
        assert!(detail.contains("state Cautioned"));
        assert!(detail.contains("earl Caution"));
        assert!(detail.contains("route SingleRemote"));
        assert!(detail.contains("validation NeedsReview"));
        assert!(detail.contains("remote Dispatched"));
    }

    #[test]
    fn latest_record_helper_prefers_highest_timestamp() {
        let chosen = latest_record_by(vec![(1_i64, "a"), (5_i64, "b"), (3_i64, "c")], |entry| {
            entry.0
        });
        assert_eq!(chosen, Some((5, "b")));
    }
}
