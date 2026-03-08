use super::EmilyRuntime;
use crate::error::EmilyError;
use crate::model::{
    AppendAuditRecordRequest, AppendSovereignAuditRecordRequest, AuditRecord, RemoteEpisodeRecord,
    RemoteEpisodeRequest, RemoteEpisodeState, RoutingDecision, RoutingDecisionKind, RoutingTarget,
    SovereignAuditMetadata, ValidationFinding, ValidationOutcome,
};
use crate::store::EmilyStore;
use serde_json::json;

impl<S: EmilyStore + 'static> EmilyRuntime<S> {
    fn is_sovereign_audit_kind(kind: crate::model::AuditRecordKind) -> bool {
        matches!(
            kind,
            crate::model::AuditRecordKind::RoutingDecided
                | crate::model::AuditRecordKind::RemoteEpisodeRecorded
                | crate::model::AuditRecordKind::ValidationRecorded
                | crate::model::AuditRecordKind::BoundaryEvent
        )
    }

    fn routing_targets_are_valid(
        kind: RoutingDecisionKind,
        targets: &[RoutingTarget],
    ) -> Result<(), EmilyError> {
        match kind {
            RoutingDecisionKind::LocalOnly | RoutingDecisionKind::Rejected
                if !targets.is_empty() =>
            {
                return Err(EmilyError::InvalidRequest(
                    "local-only and rejected routing decisions cannot include remote targets"
                        .to_string(),
                ));
            }
            RoutingDecisionKind::SingleRemote if targets.len() != 1 => {
                return Err(EmilyError::InvalidRequest(
                    "single-remote routing decisions must include exactly one target".to_string(),
                ));
            }
            RoutingDecisionKind::MultiRemote if targets.len() < 2 => {
                return Err(EmilyError::InvalidRequest(
                    "multi-remote routing decisions must include at least two targets".to_string(),
                ));
            }
            _ => {}
        }

        for target in targets {
            Self::validate_required_text("provider_id", &target.provider_id)?;
            Self::validate_optional_text("model_id", target.model_id.as_deref())?;
            Self::validate_optional_text("profile_id", target.profile_id.as_deref())?;
            for tag in &target.capability_tags {
                Self::validate_required_text("capability_tag", tag)?;
            }
        }
        Ok(())
    }

    fn remote_episode_matches_request(
        record: &RemoteEpisodeRecord,
        request: &RemoteEpisodeRequest,
    ) -> bool {
        record.id == request.remote_episode_id
            && record.episode_id == request.episode_id
            && record.route_decision_id == request.route_decision_id
            && record.dispatch_kind == request.dispatch_kind
            && record.state == RemoteEpisodeState::Dispatched
            && record.dispatched_at_unix_ms == request.dispatched_at_unix_ms
            && record.completed_at_unix_ms.is_none()
            && record.metadata == request.metadata
    }

    fn build_remote_episode_record(request: RemoteEpisodeRequest) -> RemoteEpisodeRecord {
        RemoteEpisodeRecord {
            id: request.remote_episode_id,
            episode_id: request.episode_id,
            route_decision_id: request.route_decision_id,
            dispatch_kind: request.dispatch_kind,
            state: RemoteEpisodeState::Dispatched,
            dispatched_at_unix_ms: request.dispatched_at_unix_ms,
            completed_at_unix_ms: None,
            metadata: request.metadata,
        }
    }

    fn validation_findings_are_valid(findings: &[ValidationFinding]) -> Result<(), EmilyError> {
        for finding in findings {
            Self::validate_required_text("finding.code", &finding.code)?;
            Self::validate_required_text("finding.message", &finding.message)?;
        }
        Ok(())
    }

    fn sovereign_metadata_value(metadata: SovereignAuditMetadata) -> serde_json::Value {
        json!({ "sovereign": metadata })
    }

    pub(super) async fn record_routing_decision_internal(
        &self,
        decision: RoutingDecision,
    ) -> Result<RoutingDecision, EmilyError> {
        Self::validate_required_text("decision_id", &decision.decision_id)?;
        Self::validate_required_text("episode_id", &decision.episode_id)?;
        Self::validate_optional_text("rationale", decision.rationale.as_deref())?;
        Self::routing_targets_are_valid(decision.kind, &decision.targets)?;

        if self
            .store
            .get_episode(&decision.episode_id)
            .await?
            .is_none()
        {
            return Err(EmilyError::InvalidRequest(format!(
                "episode '{}' does not exist",
                decision.episode_id
            )));
        }

        if let Some(existing) = self
            .store
            .get_routing_decision(&decision.decision_id)
            .await?
        {
            if existing == decision {
                return Ok(existing);
            }
            return Err(Self::conflict_error(
                "routing decision",
                &decision.decision_id,
            ));
        }

        self.store.upsert_routing_decision(&decision).await?;
        Ok(decision)
    }

    pub(super) async fn create_remote_episode_internal(
        &self,
        request: RemoteEpisodeRequest,
    ) -> Result<RemoteEpisodeRecord, EmilyError> {
        Self::validate_required_text("remote_episode_id", &request.remote_episode_id)?;
        Self::validate_required_text("episode_id", &request.episode_id)?;
        Self::validate_required_text("dispatch_kind", &request.dispatch_kind)?;

        if self.store.get_episode(&request.episode_id).await?.is_none() {
            return Err(EmilyError::InvalidRequest(format!(
                "episode '{}' does not exist",
                request.episode_id
            )));
        }

        if let Some(route_decision_id) = request.route_decision_id.as_deref() {
            let Some(route_decision) = self.store.get_routing_decision(route_decision_id).await?
            else {
                return Err(EmilyError::InvalidRequest(format!(
                    "routing decision '{}' does not exist",
                    route_decision_id
                )));
            };
            if route_decision.episode_id != request.episode_id {
                return Err(EmilyError::InvalidRequest(format!(
                    "routing decision '{}' belongs to episode '{}', expected '{}'",
                    route_decision_id, route_decision.episode_id, request.episode_id
                )));
            }
        }

        if let Some(existing) = self
            .store
            .get_remote_episode(&request.remote_episode_id)
            .await?
        {
            if Self::remote_episode_matches_request(&existing, &request) {
                return Ok(existing);
            }
            return Err(Self::conflict_error(
                "remote episode",
                &request.remote_episode_id,
            ));
        }

        let record = Self::build_remote_episode_record(request);
        self.store.upsert_remote_episode(&record).await?;
        Ok(record)
    }

    pub(super) async fn record_validation_outcome_internal(
        &self,
        outcome: ValidationOutcome,
    ) -> Result<ValidationOutcome, EmilyError> {
        Self::validate_required_text("validation_id", &outcome.validation_id)?;
        Self::validate_required_text("episode_id", &outcome.episode_id)?;
        Self::validation_findings_are_valid(&outcome.findings)?;

        if self.store.get_episode(&outcome.episode_id).await?.is_none() {
            return Err(EmilyError::InvalidRequest(format!(
                "episode '{}' does not exist",
                outcome.episode_id
            )));
        }

        if let Some(remote_episode_id) = outcome.remote_episode_id.as_deref() {
            let Some(remote_episode) = self.store.get_remote_episode(remote_episode_id).await?
            else {
                return Err(EmilyError::InvalidRequest(format!(
                    "remote episode '{}' does not exist",
                    remote_episode_id
                )));
            };
            if remote_episode.episode_id != outcome.episode_id {
                return Err(EmilyError::InvalidRequest(format!(
                    "remote episode '{}' belongs to episode '{}', expected '{}'",
                    remote_episode_id, remote_episode.episode_id, outcome.episode_id
                )));
            }
        }

        if let Some(existing) = self
            .store
            .get_validation_outcome(&outcome.validation_id)
            .await?
        {
            if existing == outcome {
                return Ok(existing);
            }
            return Err(Self::conflict_error(
                "validation outcome",
                &outcome.validation_id,
            ));
        }

        self.store.upsert_validation_outcome(&outcome).await?;
        Ok(outcome)
    }

    pub(super) async fn append_sovereign_audit_record_internal(
        &self,
        request: AppendSovereignAuditRecordRequest,
    ) -> Result<AuditRecord, EmilyError> {
        if let Some(route_decision_id) = request.metadata.route_decision_id.as_deref() {
            let Some(decision) = self.store.get_routing_decision(route_decision_id).await? else {
                return Err(EmilyError::InvalidRequest(format!(
                    "routing decision '{}' does not exist",
                    route_decision_id
                )));
            };
            if decision.episode_id != request.episode_id {
                return Err(EmilyError::InvalidRequest(format!(
                    "routing decision '{}' belongs to episode '{}', expected '{}'",
                    route_decision_id, decision.episode_id, request.episode_id
                )));
            }
        }

        if let Some(remote_episode_id) = request.metadata.remote_episode_id.as_deref() {
            let Some(remote_episode) = self.store.get_remote_episode(remote_episode_id).await?
            else {
                return Err(EmilyError::InvalidRequest(format!(
                    "remote episode '{}' does not exist",
                    remote_episode_id
                )));
            };
            if remote_episode.episode_id != request.episode_id {
                return Err(EmilyError::InvalidRequest(format!(
                    "remote episode '{}' belongs to episode '{}', expected '{}'",
                    remote_episode_id, remote_episode.episode_id, request.episode_id
                )));
            }
        }

        if let Some(validation_id) = request.metadata.validation_id.as_deref() {
            let Some(outcome) = self.store.get_validation_outcome(validation_id).await? else {
                return Err(EmilyError::InvalidRequest(format!(
                    "validation outcome '{}' does not exist",
                    validation_id
                )));
            };
            if outcome.episode_id != request.episode_id {
                return Err(EmilyError::InvalidRequest(format!(
                    "validation outcome '{}' belongs to episode '{}', expected '{}'",
                    validation_id, outcome.episode_id, request.episode_id
                )));
            }
        }

        self.append_audit_record_internal(AppendAuditRecordRequest {
            audit_id: request.audit_id,
            episode_id: request.episode_id,
            kind: request.kind,
            ts_unix_ms: request.ts_unix_ms,
            summary: request.summary,
            metadata: Self::sovereign_metadata_value(request.metadata),
        })
        .await
    }

    pub(super) async fn routing_decision_internal(
        &self,
        decision_id: &str,
    ) -> Result<Option<RoutingDecision>, EmilyError> {
        Self::validate_required_text("decision_id", decision_id)?;
        self.store.get_routing_decision(decision_id).await
    }

    pub(super) async fn routing_decisions_for_episode_internal(
        &self,
        episode_id: &str,
    ) -> Result<Vec<RoutingDecision>, EmilyError> {
        Self::validate_required_text("episode_id", episode_id)?;
        self.store.list_routing_decisions(episode_id).await
    }

    pub(super) async fn remote_episode_internal(
        &self,
        remote_episode_id: &str,
    ) -> Result<Option<RemoteEpisodeRecord>, EmilyError> {
        Self::validate_required_text("remote_episode_id", remote_episode_id)?;
        self.store.get_remote_episode(remote_episode_id).await
    }

    pub(super) async fn remote_episodes_for_episode_internal(
        &self,
        episode_id: &str,
    ) -> Result<Vec<RemoteEpisodeRecord>, EmilyError> {
        Self::validate_required_text("episode_id", episode_id)?;
        self.store.list_remote_episodes(episode_id).await
    }

    pub(super) async fn validation_outcome_internal(
        &self,
        validation_id: &str,
    ) -> Result<Option<ValidationOutcome>, EmilyError> {
        Self::validate_required_text("validation_id", validation_id)?;
        self.store.get_validation_outcome(validation_id).await
    }

    pub(super) async fn validation_outcomes_for_episode_internal(
        &self,
        episode_id: &str,
    ) -> Result<Vec<ValidationOutcome>, EmilyError> {
        Self::validate_required_text("episode_id", episode_id)?;
        self.store.list_validation_outcomes(episode_id).await
    }

    pub(super) async fn sovereign_audit_records_for_episode_internal(
        &self,
        episode_id: &str,
    ) -> Result<Vec<AuditRecord>, EmilyError> {
        Self::validate_required_text("episode_id", episode_id)?;
        let records = self.store.list_audit_records(episode_id).await?;
        Ok(records
            .into_iter()
            .filter(|record| Self::is_sovereign_audit_kind(record.kind))
            .collect())
    }
}
