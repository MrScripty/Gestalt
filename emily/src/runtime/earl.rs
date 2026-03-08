use super::EmilyRuntime;
use crate::error::EmilyError;
use crate::model::{
    AuditRecord, AuditRecordKind, EarlDecision, EarlEvaluationRecord, EarlEvaluationRequest,
    EarlHostAction, EarlSignalVector, EpisodeState,
};
use crate::store::EmilyStore;

const UNCERTAINTY_WEIGHT: f32 = 0.18;
const CONFLICT_WEIGHT: f32 = 0.20;
const CONTINUITY_DRIFT_WEIGHT: f32 = 0.22;
const CONSTRAINT_PRESSURE_WEIGHT: f32 = 0.16;
const TOOL_INSTABILITY_WEIGHT: f32 = 0.14;
const NOVELTY_SPIKE_WEIGHT: f32 = 0.10;

const CAUTION_THRESHOLD: f32 = 0.42;
const REFLEX_THRESHOLD: f32 = 0.72;
const CONTINUITY_REFLEX_THRESHOLD: f32 = 0.90;
const CONFLICT_REFLEX_THRESHOLD: f32 = 0.90;
const CONSTRAINT_REFLEX_THRESHOLD: f32 = 0.92;

impl<S: EmilyStore + 'static> EmilyRuntime<S> {
    fn validate_signal(field_name: &str, value: f32) -> Result<(), EmilyError> {
        if !(0.0..=1.0).contains(&value) {
            return Err(EmilyError::InvalidRequest(format!(
                "{field_name} must be between 0 and 1"
            )));
        }
        Ok(())
    }

    fn validate_earl_request(request: &EarlEvaluationRequest) -> Result<(), EmilyError> {
        Self::validate_required_text("evaluation_id", &request.evaluation_id)?;
        Self::validate_required_text("episode_id", &request.episode_id)?;
        Self::validate_signal("uncertainty", request.signals.uncertainty)?;
        Self::validate_signal("conflict", request.signals.conflict)?;
        Self::validate_signal("continuity_drift", request.signals.continuity_drift)?;
        Self::validate_signal("constraint_pressure", request.signals.constraint_pressure)?;
        Self::validate_signal("tool_instability", request.signals.tool_instability)?;
        Self::validate_signal("novelty_spike", request.signals.novelty_spike)?;
        Ok(())
    }

    fn compute_earl_risk(signals: &EarlSignalVector) -> f32 {
        (signals.uncertainty * UNCERTAINTY_WEIGHT)
            + (signals.conflict * CONFLICT_WEIGHT)
            + (signals.continuity_drift * CONTINUITY_DRIFT_WEIGHT)
            + (signals.constraint_pressure * CONSTRAINT_PRESSURE_WEIGHT)
            + (signals.tool_instability * TOOL_INSTABILITY_WEIGHT)
            + (signals.novelty_spike * NOVELTY_SPIKE_WEIGHT)
    }

    fn decide_earl_gate(signals: &EarlSignalVector, risk_score: f32) -> EarlDecision {
        if signals.continuity_drift >= CONTINUITY_REFLEX_THRESHOLD
            || signals.conflict >= CONFLICT_REFLEX_THRESHOLD
            || signals.constraint_pressure >= CONSTRAINT_REFLEX_THRESHOLD
            || risk_score >= REFLEX_THRESHOLD
        {
            return EarlDecision::Reflex;
        }
        if risk_score >= CAUTION_THRESHOLD {
            return EarlDecision::Caution;
        }
        EarlDecision::Ok
    }

    fn host_action_for_decision(decision: EarlDecision) -> EarlHostAction {
        match decision {
            EarlDecision::Ok => EarlHostAction::Proceed,
            EarlDecision::Caution => EarlHostAction::Clarify,
            EarlDecision::Reflex => EarlHostAction::Abort,
        }
    }

    fn retryable_for_decision(decision: EarlDecision) -> bool {
        matches!(decision, EarlDecision::Caution)
    }

    fn rationale_for_decision(signals: &EarlSignalVector, decision: EarlDecision) -> String {
        let mut ranked = [
            ("uncertainty", signals.uncertainty),
            ("conflict", signals.conflict),
            ("continuity_drift", signals.continuity_drift),
            ("constraint_pressure", signals.constraint_pressure),
            ("tool_instability", signals.tool_instability),
            ("novelty_spike", signals.novelty_spike),
        ];
        ranked.sort_by(|left, right| right.1.total_cmp(&left.1));
        let dominant = format!("{}={:.2}", ranked[0].0, ranked[0].1);
        let secondary = format!("{}={:.2}", ranked[1].0, ranked[1].1);
        match decision {
            EarlDecision::Ok => {
                format!("EARL OK: low aggregate risk with dominant signals {dominant}, {secondary}")
            }
            EarlDecision::Caution => format!(
                "EARL CAUTION: clarification recommended due to elevated {dominant} and {secondary}"
            ),
            EarlDecision::Reflex => {
                format!("EARL REFLEX: abort path due to dominant {dominant} and {secondary}")
            }
        }
    }

    fn build_earl_evaluation(request: EarlEvaluationRequest) -> EarlEvaluationRecord {
        let risk_score = Self::compute_earl_risk(&request.signals);
        let decision = Self::decide_earl_gate(&request.signals, risk_score);
        EarlEvaluationRecord {
            id: request.evaluation_id,
            episode_id: request.episode_id,
            evaluated_at_unix_ms: request.evaluated_at_unix_ms,
            signals: request.signals.clone(),
            risk_score,
            decision,
            host_action: Self::host_action_for_decision(decision),
            retryable: Self::retryable_for_decision(decision),
            rationale: Self::rationale_for_decision(&request.signals, decision),
            metadata: request.metadata,
        }
    }

    fn earl_matches_request(
        evaluation: &EarlEvaluationRecord,
        request: &EarlEvaluationRequest,
    ) -> bool {
        evaluation.id == request.evaluation_id
            && evaluation.episode_id == request.episode_id
            && evaluation.evaluated_at_unix_ms == request.evaluated_at_unix_ms
            && evaluation.signals == request.signals
            && evaluation.metadata == request.metadata
    }

    fn apply_earl_decision_to_episode(
        mut state: EpisodeState,
        decision: EarlDecision,
    ) -> EpisodeState {
        state = match decision {
            EarlDecision::Ok => {
                if matches!(state, EpisodeState::Cautioned) {
                    EpisodeState::Open
                } else {
                    state
                }
            }
            EarlDecision::Caution => {
                if matches!(state, EpisodeState::Open | EpisodeState::Cautioned) {
                    EpisodeState::Cautioned
                } else {
                    state
                }
            }
            EarlDecision::Reflex => EpisodeState::Blocked,
        };
        state
    }

    async fn reconcile_earl_episode_projection(
        &self,
        evaluation: &EarlEvaluationRecord,
    ) -> Result<(), EmilyError> {
        let Some(mut episode) = self.store.get_episode(&evaluation.episode_id).await? else {
            return Err(EmilyError::InvalidRequest(format!(
                "episode '{}' does not exist",
                evaluation.episode_id
            )));
        };
        let next_state = Self::apply_earl_decision_to_episode(episode.state, evaluation.decision);
        if next_state != episode.state
            || evaluation.evaluated_at_unix_ms > episode.updated_at_unix_ms
        {
            episode.state = next_state;
            episode.updated_at_unix_ms = episode
                .updated_at_unix_ms
                .max(evaluation.evaluated_at_unix_ms);
            self.store.upsert_episode(&episode).await?;
        }
        Ok(())
    }

    async fn reconcile_earl_text_projection(
        &self,
        evaluation: &EarlEvaluationRecord,
    ) -> Result<(), EmilyError> {
        if matches!(evaluation.decision, EarlDecision::Ok) {
            return Ok(());
        }

        let links = self
            .store
            .list_episode_trace_links(&evaluation.episode_id)
            .await?;
        for link in links {
            let Some(mut object) = self.store.get_text_object(&link.object_id).await? else {
                continue;
            };
            object.gate_score = Some(object.gate_score.map_or(evaluation.risk_score, |score| {
                score.max(evaluation.risk_score)
            }));
            if matches!(evaluation.decision, EarlDecision::Reflex) {
                object.quarantine_score = object.quarantine_score.max(evaluation.risk_score);
                object.integrated = false;
            }
            self.store.upsert_text_object(&object).await?;
        }
        Ok(())
    }

    async fn reconcile_earl_audit(
        &self,
        evaluation: &EarlEvaluationRecord,
    ) -> Result<(), EmilyError> {
        let audit = AuditRecord {
            id: format!("audit:earl:{}", evaluation.id),
            episode_id: evaluation.episode_id.clone(),
            kind: AuditRecordKind::EarlEvaluated,
            ts_unix_ms: evaluation.evaluated_at_unix_ms,
            summary: evaluation.rationale.clone(),
            metadata: evaluation.metadata.clone(),
        };

        match self.store.get_audit_record(&audit.id).await? {
            Some(existing) if existing != audit => Err(EmilyError::InvalidRequest(format!(
                "audit record '{}' already exists with different content",
                audit.id
            ))),
            Some(_) => Ok(()),
            None => self.store.upsert_audit_record(&audit).await,
        }
    }

    pub(super) async fn evaluate_episode_risk_internal(
        &self,
        request: EarlEvaluationRequest,
    ) -> Result<EarlEvaluationRecord, EmilyError> {
        Self::validate_earl_request(&request)?;

        let Some(episode) = self.store.get_episode(&request.episode_id).await? else {
            return Err(EmilyError::InvalidRequest(format!(
                "episode '{}' does not exist",
                request.episode_id
            )));
        };
        if matches!(
            episode.state,
            EpisodeState::Completed | EpisodeState::Cancelled
        ) {
            return Err(EmilyError::InvalidRequest(format!(
                "episode '{}' is closed",
                request.episode_id
            )));
        }
        if matches!(episode.state, EpisodeState::Blocked)
            && self
                .store
                .get_earl_evaluation(&request.evaluation_id)
                .await?
                .is_none()
        {
            return Err(EmilyError::EpisodeGated(format!(
                "episode '{}' is already blocked by EARL",
                request.episode_id
            )));
        }

        let evaluation = Self::build_earl_evaluation(request.clone());
        match self.store.get_earl_evaluation(&evaluation.id).await? {
            Some(existing) if !Self::earl_matches_request(&existing, &request) => {
                return Err(EmilyError::InvalidRequest(format!(
                    "EARL evaluation '{}' already exists with different content",
                    evaluation.id
                )));
            }
            Some(_) => {}
            None => {
                self.store.upsert_earl_evaluation(&evaluation).await?;
            }
        }

        self.reconcile_earl_episode_projection(&evaluation).await?;
        self.reconcile_earl_text_projection(&evaluation).await?;
        self.reconcile_earl_audit(&evaluation).await?;

        Ok(evaluation)
    }
}
