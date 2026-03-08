use super::EmilyRuntime;
use crate::error::EmilyError;
use crate::model::{
    AppendAuditRecordRequest, AppendSovereignAuditRecordRequest, AuditRecord, EpisodeState,
    RemoteEpisodeRecord, RemoteEpisodeRequest, RemoteEpisodeState, RoutingDecision,
    RoutingDecisionKind, RoutingTarget, SovereignAuditMetadata, UpdateRemoteEpisodeStateRequest,
    ValidationDecision, ValidationFinding, ValidationOutcome,
};
use crate::store::EmilyStore;
use serde_json::json;

impl<S: EmilyStore + 'static> EmilyRuntime<S> {
    fn sovereign_audit_id(kind: crate::model::AuditRecordKind, record_id: &str) -> String {
        let prefix = match kind {
            crate::model::AuditRecordKind::RoutingDecided => "routing",
            crate::model::AuditRecordKind::RemoteEpisodeRecorded => "remote",
            crate::model::AuditRecordKind::ValidationRecorded => "validation",
            crate::model::AuditRecordKind::BoundaryEvent => "boundary",
            _ => "other",
        };
        format!("audit:sovereign:{prefix}:{record_id}")
    }

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

    fn route_kind_allows_remote_dispatch(kind: RoutingDecisionKind) -> bool {
        matches!(
            kind,
            RoutingDecisionKind::SingleRemote | RoutingDecisionKind::MultiRemote
        )
    }

    fn apply_routing_decision_to_episode(
        state: EpisodeState,
        kind: RoutingDecisionKind,
    ) -> EpisodeState {
        match kind {
            RoutingDecisionKind::Rejected if !matches!(state, EpisodeState::Cancelled) => {
                EpisodeState::Blocked
            }
            RoutingDecisionKind::LocalOnly
            | RoutingDecisionKind::SingleRemote
            | RoutingDecisionKind::MultiRemote
            | RoutingDecisionKind::Rejected => state,
        }
    }

    fn apply_validation_to_episode(
        state: EpisodeState,
        decision: ValidationDecision,
    ) -> EpisodeState {
        match decision {
            ValidationDecision::Accepted => state,
            ValidationDecision::AcceptedWithCaution | ValidationDecision::NeedsReview => {
                if matches!(state, EpisodeState::Open) {
                    EpisodeState::Cautioned
                } else {
                    state
                }
            }
            ValidationDecision::Rejected => {
                if matches!(state, EpisodeState::Cancelled) {
                    state
                } else {
                    EpisodeState::Blocked
                }
            }
        }
    }

    fn validation_target_remote_state(decision: ValidationDecision) -> Option<RemoteEpisodeState> {
        match decision {
            ValidationDecision::Accepted | ValidationDecision::AcceptedWithCaution => {
                Some(RemoteEpisodeState::Succeeded)
            }
            ValidationDecision::NeedsReview => None,
            ValidationDecision::Rejected => Some(RemoteEpisodeState::Rejected),
        }
    }

    fn reconcile_remote_episode_from_validation(
        mut remote_episode: RemoteEpisodeRecord,
        decision: ValidationDecision,
        validated_at_unix_ms: i64,
    ) -> Result<Option<RemoteEpisodeRecord>, EmilyError> {
        let Some(target_state) = Self::validation_target_remote_state(decision) else {
            return Ok(None);
        };
        let target_completed_at = Some(
            remote_episode
                .completed_at_unix_ms
                .map_or(validated_at_unix_ms, |existing| {
                    existing.max(validated_at_unix_ms)
                }),
        );

        match remote_episode.state {
            RemoteEpisodeState::Planned | RemoteEpisodeState::Dispatched => {
                remote_episode.state = target_state;
                remote_episode.completed_at_unix_ms = target_completed_at;
                Ok(Some(remote_episode))
            }
            state if state == target_state => {
                if remote_episode.completed_at_unix_ms != target_completed_at {
                    remote_episode.completed_at_unix_ms = target_completed_at;
                    Ok(Some(remote_episode))
                } else {
                    Ok(None)
                }
            }
            RemoteEpisodeState::Succeeded
            | RemoteEpisodeState::Failed
            | RemoteEpisodeState::Cancelled
            | RemoteEpisodeState::Rejected => Err(EmilyError::InvalidRequest(format!(
                "remote episode '{}' is already in terminal state '{:?}'",
                remote_episode.id, remote_episode.state
            ))),
        }
    }

    fn update_remote_episode_terminal_state(
        mut remote_episode: RemoteEpisodeRecord,
        next_state: RemoteEpisodeState,
        transitioned_at_unix_ms: i64,
    ) -> Result<Option<RemoteEpisodeRecord>, EmilyError> {
        match next_state {
            RemoteEpisodeState::Succeeded
            | RemoteEpisodeState::Failed
            | RemoteEpisodeState::Cancelled
            | RemoteEpisodeState::Rejected => {}
            RemoteEpisodeState::Planned | RemoteEpisodeState::Dispatched => {
                return Err(EmilyError::InvalidRequest(format!(
                    "explicit remote state updates cannot transition '{}' to '{next_state:?}'",
                    remote_episode.id
                )));
            }
        }

        let target_completed_at = Some(
            remote_episode
                .completed_at_unix_ms
                .map_or(transitioned_at_unix_ms, |existing| {
                    existing.max(transitioned_at_unix_ms)
                }),
        );

        match remote_episode.state {
            RemoteEpisodeState::Planned | RemoteEpisodeState::Dispatched => {
                remote_episode.state = next_state;
                remote_episode.completed_at_unix_ms = target_completed_at;
                Ok(Some(remote_episode))
            }
            state if state == next_state => {
                if remote_episode.completed_at_unix_ms != target_completed_at {
                    remote_episode.completed_at_unix_ms = target_completed_at;
                    Ok(Some(remote_episode))
                } else {
                    Ok(None)
                }
            }
            RemoteEpisodeState::Succeeded
            | RemoteEpisodeState::Failed
            | RemoteEpisodeState::Cancelled
            | RemoteEpisodeState::Rejected => Err(EmilyError::InvalidRequest(format!(
                "remote episode '{}' is already in terminal state '{:?}'",
                remote_episode.id, remote_episode.state
            ))),
        }
    }

    fn apply_explicit_remote_state_to_episode(
        state: EpisodeState,
        next_state: RemoteEpisodeState,
    ) -> EpisodeState {
        match next_state {
            RemoteEpisodeState::Succeeded => state,
            RemoteEpisodeState::Failed => {
                if matches!(state, EpisodeState::Open) {
                    EpisodeState::Cautioned
                } else {
                    state
                }
            }
            RemoteEpisodeState::Rejected => {
                if matches!(state, EpisodeState::Cancelled) {
                    state
                } else {
                    EpisodeState::Blocked
                }
            }
            RemoteEpisodeState::Cancelled
            | RemoteEpisodeState::Planned
            | RemoteEpisodeState::Dispatched => state,
        }
    }

    async fn reconcile_routing_episode_projection(
        &self,
        decision: &RoutingDecision,
    ) -> Result<(), EmilyError> {
        let Some(mut episode) = self.store.get_episode(&decision.episode_id).await? else {
            return Err(EmilyError::InvalidRequest(format!(
                "episode '{}' does not exist",
                decision.episode_id
            )));
        };
        let next_state = Self::apply_routing_decision_to_episode(episode.state, decision.kind);
        if next_state != episode.state || decision.decided_at_unix_ms > episode.updated_at_unix_ms {
            episode.state = next_state;
            episode.updated_at_unix_ms =
                episode.updated_at_unix_ms.max(decision.decided_at_unix_ms);
            self.store.upsert_episode(&episode).await?;
        }
        Ok(())
    }

    async fn reconcile_validation_episode_projection(
        &self,
        outcome: &ValidationOutcome,
    ) -> Result<(), EmilyError> {
        let Some(mut episode) = self.store.get_episode(&outcome.episode_id).await? else {
            return Err(EmilyError::InvalidRequest(format!(
                "episode '{}' does not exist",
                outcome.episode_id
            )));
        };
        let next_state = Self::apply_validation_to_episode(episode.state, outcome.decision);
        if next_state != episode.state || outcome.validated_at_unix_ms > episode.updated_at_unix_ms
        {
            episode.state = next_state;
            episode.updated_at_unix_ms =
                episode.updated_at_unix_ms.max(outcome.validated_at_unix_ms);
            self.store.upsert_episode(&episode).await?;
        }
        Ok(())
    }

    async fn reconcile_validation_remote_projection(
        &self,
        outcome: &ValidationOutcome,
    ) -> Result<(), EmilyError> {
        let Some(remote_episode_id) = outcome.remote_episode_id.as_deref() else {
            return Ok(());
        };
        let Some(remote_episode) = self.store.get_remote_episode(remote_episode_id).await? else {
            return Err(EmilyError::InvalidRequest(format!(
                "remote episode '{}' does not exist",
                remote_episode_id
            )));
        };
        let Some(updated_remote_episode) = Self::reconcile_remote_episode_from_validation(
            remote_episode,
            outcome.decision,
            outcome.validated_at_unix_ms,
        )?
        else {
            return Ok(());
        };
        self.store
            .upsert_remote_episode(&updated_remote_episode)
            .await
    }

    async fn reconcile_explicit_remote_state_episode_projection(
        &self,
        episode_id: &str,
        next_state: RemoteEpisodeState,
        transitioned_at_unix_ms: i64,
    ) -> Result<(), EmilyError> {
        let Some(mut episode) = self.store.get_episode(episode_id).await? else {
            return Err(EmilyError::InvalidRequest(format!(
                "episode '{}' does not exist",
                episode_id
            )));
        };
        let next_episode_state =
            Self::apply_explicit_remote_state_to_episode(episode.state, next_state);
        if next_episode_state != episode.state
            || transitioned_at_unix_ms > episode.updated_at_unix_ms
        {
            episode.state = next_episode_state;
            episode.updated_at_unix_ms = episode.updated_at_unix_ms.max(transitioned_at_unix_ms);
            self.store.upsert_episode(&episode).await?;
        }
        Ok(())
    }

    fn sovereign_metadata_value(metadata: SovereignAuditMetadata) -> serde_json::Value {
        json!({ "sovereign": metadata })
    }

    async fn append_generated_sovereign_audit(
        &self,
        episode_id: &str,
        kind: crate::model::AuditRecordKind,
        ts_unix_ms: i64,
        summary: String,
        metadata: SovereignAuditMetadata,
        record_id: &str,
    ) -> Result<AuditRecord, EmilyError> {
        self.append_sovereign_audit_record_internal(AppendSovereignAuditRecordRequest {
            audit_id: Self::sovereign_audit_id(kind, record_id),
            episode_id: episode_id.to_string(),
            kind,
            ts_unix_ms,
            summary,
            metadata,
        })
        .await
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

        let persisted = if let Some(existing) = self
            .store
            .get_routing_decision(&decision.decision_id)
            .await?
        {
            if existing == decision {
                existing
            } else {
                return Err(Self::conflict_error(
                    "routing decision",
                    &decision.decision_id,
                ));
            }
        } else {
            self.store.upsert_routing_decision(&decision).await?;
            decision
        };

        self.reconcile_routing_episode_projection(&persisted)
            .await?;

        self.append_generated_sovereign_audit(
            &persisted.episode_id,
            crate::model::AuditRecordKind::RoutingDecided,
            persisted.decided_at_unix_ms,
            format!("routing decision '{}' recorded", persisted.decision_id),
            SovereignAuditMetadata {
                remote_episode_id: None,
                route_decision_id: Some(persisted.decision_id.clone()),
                validation_id: None,
                boundary_profile: None,
                metadata: json!({"kind": persisted.kind, "targets": persisted.targets.len()}),
            },
            &persisted.decision_id,
        )
        .await?;

        Ok(persisted)
    }

    pub(super) async fn create_remote_episode_internal(
        &self,
        request: RemoteEpisodeRequest,
    ) -> Result<RemoteEpisodeRecord, EmilyError> {
        Self::validate_required_text("remote_episode_id", &request.remote_episode_id)?;
        Self::validate_required_text("episode_id", &request.episode_id)?;
        Self::validate_required_text("dispatch_kind", &request.dispatch_kind)?;

        let Some(episode) = self.store.get_episode(&request.episode_id).await? else {
            return Err(EmilyError::InvalidRequest(format!(
                "episode '{}' does not exist",
                request.episode_id
            )));
        };
        if !matches!(episode.state, EpisodeState::Open | EpisodeState::Cautioned) {
            return Err(EmilyError::InvalidRequest(format!(
                "episode '{}' does not allow remote dispatch while in state '{:?}'",
                request.episode_id, episode.state
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
            if !Self::route_kind_allows_remote_dispatch(route_decision.kind) {
                return Err(EmilyError::InvalidRequest(format!(
                    "routing decision '{}' does not allow remote dispatch",
                    route_decision_id
                )));
            }
        }

        let persisted = if let Some(existing) = self
            .store
            .get_remote_episode(&request.remote_episode_id)
            .await?
        {
            if Self::remote_episode_matches_request(&existing, &request) {
                existing
            } else {
                return Err(Self::conflict_error(
                    "remote episode",
                    &request.remote_episode_id,
                ));
            }
        } else {
            let record = Self::build_remote_episode_record(request);
            self.store.upsert_remote_episode(&record).await?;
            record
        };

        self.append_generated_sovereign_audit(
            &persisted.episode_id,
            crate::model::AuditRecordKind::RemoteEpisodeRecorded,
            persisted.dispatched_at_unix_ms,
            format!("remote episode '{}' recorded", persisted.id),
            SovereignAuditMetadata {
                remote_episode_id: Some(persisted.id.clone()),
                route_decision_id: persisted.route_decision_id.clone(),
                validation_id: None,
                boundary_profile: None,
                metadata: json!({"dispatch_kind": persisted.dispatch_kind}),
            },
            &persisted.id,
        )
        .await?;

        Ok(persisted)
    }

    pub(super) async fn update_remote_episode_state_internal(
        &self,
        request: UpdateRemoteEpisodeStateRequest,
    ) -> Result<RemoteEpisodeRecord, EmilyError> {
        Self::validate_required_text("remote_episode_id", &request.remote_episode_id)?;
        Self::validate_optional_text("summary", request.summary.as_deref())?;

        let Some(remote_episode) = self
            .store
            .get_remote_episode(&request.remote_episode_id)
            .await?
        else {
            return Err(EmilyError::InvalidRequest(format!(
                "remote episode '{}' does not exist",
                request.remote_episode_id
            )));
        };

        let persisted = match Self::update_remote_episode_terminal_state(
            remote_episode,
            request.next_state,
            request.transitioned_at_unix_ms,
        )? {
            Some(updated_remote_episode) => {
                self.store
                    .upsert_remote_episode(&updated_remote_episode)
                    .await?;
                updated_remote_episode
            }
            None => self
                .store
                .get_remote_episode(&request.remote_episode_id)
                .await?
                .ok_or_else(|| {
                    EmilyError::Store(format!(
                        "remote episode '{}' disappeared during state update",
                        request.remote_episode_id
                    ))
                })?,
        };

        self.reconcile_explicit_remote_state_episode_projection(
            &persisted.episode_id,
            persisted.state,
            request.transitioned_at_unix_ms,
        )
        .await?;

        self.append_generated_sovereign_audit(
            &persisted.episode_id,
            crate::model::AuditRecordKind::BoundaryEvent,
            request.transitioned_at_unix_ms,
            request.summary.unwrap_or_else(|| {
                format!(
                    "remote episode '{}' transitioned to '{:?}'",
                    persisted.id, persisted.state
                )
            }),
            SovereignAuditMetadata {
                remote_episode_id: Some(persisted.id.clone()),
                route_decision_id: persisted.route_decision_id.clone(),
                validation_id: None,
                boundary_profile: None,
                metadata: json!({
                    "remote_state": persisted.state,
                    "origin": "explicit_remote_state",
                    "request": request.metadata,
                }),
            },
            &format!(
                "remote-state:{}:{:?}:{}",
                persisted.id, persisted.state, request.transitioned_at_unix_ms
            ),
        )
        .await?;

        Ok(persisted)
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

        let persisted = if let Some(existing) = self
            .store
            .get_validation_outcome(&outcome.validation_id)
            .await?
        {
            if existing == outcome {
                existing
            } else {
                return Err(Self::conflict_error(
                    "validation outcome",
                    &outcome.validation_id,
                ));
            }
        } else {
            self.store.upsert_validation_outcome(&outcome).await?;
            outcome
        };

        self.reconcile_validation_remote_projection(&persisted)
            .await?;
        self.reconcile_validation_episode_projection(&persisted)
            .await?;

        self
            .append_generated_sovereign_audit(
                &persisted.episode_id,
                crate::model::AuditRecordKind::ValidationRecorded,
                persisted.validated_at_unix_ms,
                format!("validation outcome '{}' recorded", persisted.validation_id),
                SovereignAuditMetadata {
                    remote_episode_id: persisted.remote_episode_id.clone(),
                    route_decision_id: None,
                    validation_id: Some(persisted.validation_id.clone()),
                    boundary_profile: None,
                    metadata: json!({"decision": persisted.decision, "findings": persisted.findings.len()}),
                },
                &persisted.validation_id,
            )
            .await?;

        Ok(persisted)
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
