use super::SurrealEmilyStore;
use crate::error::EmilyError;
use crate::model::{RemoteEpisodeRecord, RoutingDecision, ValidationOutcome};

impl SurrealEmilyStore {
    fn normalize_sovereign_record_id(value: &str, table: &str) -> String {
        let prefix = format!("{table}:`");
        value
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix('`'))
            .map_or_else(|| value.to_string(), ToString::to_string)
    }

    fn normalize_routing_decision(mut decision: RoutingDecision) -> RoutingDecision {
        decision.decision_id =
            Self::normalize_sovereign_record_id(&decision.decision_id, "routing_decisions");
        decision.episode_id = Self::normalize_sovereign_record_id(&decision.episode_id, "episodes");
        decision
    }

    fn normalize_remote_episode(mut remote_episode: RemoteEpisodeRecord) -> RemoteEpisodeRecord {
        remote_episode.id =
            Self::normalize_sovereign_record_id(&remote_episode.id, "remote_episodes");
        remote_episode.episode_id =
            Self::normalize_sovereign_record_id(&remote_episode.episode_id, "episodes");
        if let Some(route_decision_id) = remote_episode.route_decision_id.as_ref() {
            remote_episode.route_decision_id = Some(Self::normalize_sovereign_record_id(
                route_decision_id,
                "routing_decisions",
            ));
        }
        remote_episode
    }

    fn normalize_validation_outcome(mut outcome: ValidationOutcome) -> ValidationOutcome {
        outcome.validation_id =
            Self::normalize_sovereign_record_id(&outcome.validation_id, "validation_outcomes");
        outcome.episode_id = Self::normalize_sovereign_record_id(&outcome.episode_id, "episodes");
        if let Some(remote_episode_id) = outcome.remote_episode_id.as_ref() {
            outcome.remote_episode_id = Some(Self::normalize_sovereign_record_id(
                remote_episode_id,
                "remote_episodes",
            ));
        }
        outcome
    }

    fn routing_decision_projection() -> &'static str {
        "type::string(id) AS decision_id, episode_id, kind, decided_at_unix_ms, rationale, targets, metadata"
    }

    fn remote_episode_projection() -> &'static str {
        "type::string(id) AS id, episode_id, route_decision_id, dispatch_kind, state, dispatched_at_unix_ms, completed_at_unix_ms, metadata"
    }

    fn validation_outcome_projection() -> &'static str {
        "type::string(id) AS validation_id, episode_id, remote_episode_id, decision, validated_at_unix_ms, findings, metadata"
    }

    pub(super) async fn upsert_routing_decision_internal(
        &self,
        decision: &RoutingDecision,
    ) -> Result<(), EmilyError> {
        let client = self.active_client().await?;
        client
            .query("UPSERT type::thing('routing_decisions', $id) CONTENT $decision")
            .bind(("id", decision.decision_id.clone()))
            .bind(("decision", decision.clone()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal routing decision upsert failed: {error}"))
            })?;
        Ok(())
    }

    pub(super) async fn get_routing_decision_internal(
        &self,
        decision_id: &str,
    ) -> Result<Option<RoutingDecision>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM type::thing('routing_decisions', $id)",
                Self::routing_decision_projection()
            ))
            .bind(("id", decision_id.to_string()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal select routing decision failed: {error}"))
            })?;
        let decisions: Vec<RoutingDecision> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (routing decision): {error}"
            ))
        })?;
        Ok(decisions
            .into_iter()
            .next()
            .map(Self::normalize_routing_decision))
    }

    pub(super) async fn list_routing_decisions_internal(
        &self,
        episode_id: &str,
    ) -> Result<Vec<RoutingDecision>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM routing_decisions WHERE episode_id = $episode_id",
                Self::routing_decision_projection()
            ))
            .bind(("episode_id", episode_id.to_string()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal select routing decisions failed: {error}"))
            })?;
        let mut decisions: Vec<RoutingDecision> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (routing decisions): {error}"
            ))
        })?;
        decisions = decisions
            .into_iter()
            .map(Self::normalize_routing_decision)
            .collect();
        decisions.sort_by(|left, right| left.decided_at_unix_ms.cmp(&right.decided_at_unix_ms));
        Ok(decisions)
    }

    pub(super) async fn upsert_remote_episode_internal(
        &self,
        remote_episode: &RemoteEpisodeRecord,
    ) -> Result<(), EmilyError> {
        let client = self.active_client().await?;
        client
            .query("UPSERT type::thing('remote_episodes', $id) CONTENT $remote_episode")
            .bind(("id", remote_episode.id.clone()))
            .bind(("remote_episode", remote_episode.clone()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal remote episode upsert failed: {error}"))
            })?;
        Ok(())
    }

    pub(super) async fn get_remote_episode_internal(
        &self,
        remote_episode_id: &str,
    ) -> Result<Option<RemoteEpisodeRecord>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM type::thing('remote_episodes', $id)",
                Self::remote_episode_projection()
            ))
            .bind(("id", remote_episode_id.to_string()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal select remote episode failed: {error}"))
            })?;
        let remote_episodes: Vec<RemoteEpisodeRecord> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (remote episode): {error}"
            ))
        })?;
        Ok(remote_episodes
            .into_iter()
            .next()
            .map(Self::normalize_remote_episode))
    }

    pub(super) async fn list_remote_episodes_internal(
        &self,
        episode_id: &str,
    ) -> Result<Vec<RemoteEpisodeRecord>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM remote_episodes WHERE episode_id = $episode_id",
                Self::remote_episode_projection()
            ))
            .bind(("episode_id", episode_id.to_string()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal select remote episodes failed: {error}"))
            })?;
        let mut remote_episodes: Vec<RemoteEpisodeRecord> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (remote episodes): {error}"
            ))
        })?;
        remote_episodes = remote_episodes
            .into_iter()
            .map(Self::normalize_remote_episode)
            .collect();
        remote_episodes
            .sort_by(|left, right| left.dispatched_at_unix_ms.cmp(&right.dispatched_at_unix_ms));
        Ok(remote_episodes)
    }

    pub(super) async fn upsert_validation_outcome_internal(
        &self,
        outcome: &ValidationOutcome,
    ) -> Result<(), EmilyError> {
        let client = self.active_client().await?;
        client
            .query("UPSERT type::thing('validation_outcomes', $id) CONTENT $outcome")
            .bind(("id", outcome.validation_id.clone()))
            .bind(("outcome", outcome.clone()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal validation outcome upsert failed: {error}"))
            })?;
        Ok(())
    }

    pub(super) async fn get_validation_outcome_internal(
        &self,
        validation_id: &str,
    ) -> Result<Option<ValidationOutcome>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM type::thing('validation_outcomes', $id)",
                Self::validation_outcome_projection()
            ))
            .bind(("id", validation_id.to_string()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal select validation outcome failed: {error}"))
            })?;
        let outcomes: Vec<ValidationOutcome> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (validation outcome): {error}"
            ))
        })?;
        Ok(outcomes
            .into_iter()
            .next()
            .map(Self::normalize_validation_outcome))
    }

    pub(super) async fn list_validation_outcomes_internal(
        &self,
        episode_id: &str,
    ) -> Result<Vec<ValidationOutcome>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM validation_outcomes WHERE episode_id = $episode_id",
                Self::validation_outcome_projection()
            ))
            .bind(("episode_id", episode_id.to_string()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!(
                    "surreal select validation outcomes failed: {error}"
                ))
            })?;
        let mut outcomes: Vec<ValidationOutcome> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (validation outcomes): {error}"
            ))
        })?;
        outcomes = outcomes
            .into_iter()
            .map(Self::normalize_validation_outcome)
            .collect();
        outcomes.sort_by(|left, right| left.validated_at_unix_ms.cmp(&right.validated_at_unix_ms));
        Ok(outcomes)
    }
}
