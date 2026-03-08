use super::{EcglRuntimeState, EmilyRuntime};
use crate::error::EmilyError;
use crate::model::{
    EpisodeRecord, EpisodeState, IntegritySnapshot, MemoryState, OutcomeRecord, OutcomeStatus,
    TextEdgeType, TextObject, TextObjectKind,
};
use crate::store::EmilyStore;

pub(super) const WEIGHT_CONFIDENCE: f32 = 0.35;
pub(super) const WEIGHT_OUTCOME: f32 = 0.35;
pub(super) const WEIGHT_NOVELTY: f32 = 0.20;
pub(super) const WEIGHT_STABILITY: f32 = 0.10;
pub(super) const TAU_INITIAL: f32 = 0.65;
const TAU_MIN: f32 = 0.35;
const TAU_MAX: f32 = 0.90;
const TAU_ADAPTATION_RATE: f32 = 0.10;
const CI_TARGET: f32 = 0.88;
const SIGMOID_STEEPNESS: f32 = 6.0;
const EPSILON_MAX: f32 = 1.0;
const DEFERRED_TAU_RATIO: f32 = 0.60;

impl<S: EmilyStore + 'static> EmilyRuntime<S> {
    fn confidence_factor_for_object(object: &TextObject) -> f32 {
        let epsilon = object.epsilon.unwrap_or(0.0).clamp(0.0, EPSILON_MAX);
        (1.0 - (epsilon / EPSILON_MAX)).clamp(0.0, 1.0)
    }

    fn outcome_factor_for_status(status: OutcomeStatus) -> f32 {
        match status {
            OutcomeStatus::Succeeded => 1.0,
            OutcomeStatus::Partial => 0.65,
            OutcomeStatus::Unknown => 0.50,
            OutcomeStatus::Failed | OutcomeStatus::Cancelled => 0.0,
        }
    }

    fn stability_factor_for_episode(state: EpisodeState) -> f32 {
        match state {
            EpisodeState::Open => 0.50,
            EpisodeState::Cautioned => 0.35,
            EpisodeState::Blocked => 0.0,
            EpisodeState::Completed => 1.0,
            EpisodeState::Cancelled => 0.0,
        }
    }

    fn novelty_base_for_kind(kind: TextObjectKind) -> f32 {
        match kind {
            TextObjectKind::Summary => 0.75,
            TextObjectKind::Note => 0.70,
            TextObjectKind::UserInput | TextObjectKind::SystemOutput => 0.45,
            TextObjectKind::Other => 0.55,
        }
    }

    fn novelty_factor_for_object(object: &TextObject, semantic_neighbor_count: usize) -> f32 {
        let base = Self::novelty_base_for_kind(object.object_kind);
        let novelty = match semantic_neighbor_count {
            0 => (base + 0.20).min(1.0),
            1 => (base + 0.10).min(1.0),
            2 => base,
            _ => (base * 0.60).max(0.15),
        };
        novelty.clamp(0.0, 1.0)
    }

    fn learning_weight(confidence: f32, outcome: f32, novelty: f32, stability: f32) -> f32 {
        ((confidence * WEIGHT_CONFIDENCE)
            + (outcome * WEIGHT_OUTCOME)
            + (novelty * WEIGHT_NOVELTY)
            + (stability * WEIGHT_STABILITY))
            .clamp(0.0, 1.0)
    }

    fn gate_score(learning_weight: f32, tau: f32) -> f32 {
        1.0 / (1.0 + (-SIGMOID_STEEPNESS * (learning_weight - tau)).exp())
    }

    fn quarantine_score(novelty: f32, learning_weight: f32) -> f32 {
        (novelty * (1.0 - learning_weight)).clamp(0.0, 1.0)
    }

    fn memory_state_for_object(
        episode: &EpisodeRecord,
        outcome: &OutcomeRecord,
        learning_weight: f32,
        gate_score: f32,
        tau: f32,
    ) -> MemoryState {
        if matches!(episode.state, EpisodeState::Blocked) {
            return MemoryState::Quarantined;
        }
        if matches!(
            outcome.status,
            OutcomeStatus::Failed | OutcomeStatus::Cancelled
        ) {
            return MemoryState::Quarantined;
        }
        if learning_weight >= tau && gate_score >= 0.5 {
            return MemoryState::Integrated;
        }
        if learning_weight >= tau * DEFERRED_TAU_RATIO {
            return MemoryState::Deferred;
        }
        MemoryState::Quarantined
    }

    fn ci_contribution(object: &TextObject) -> f32 {
        match object.memory_state {
            MemoryState::Integrated => object.learning_weight,
            MemoryState::Deferred => object.learning_weight * 0.50,
            MemoryState::Pending => object.learning_weight * 0.25,
            MemoryState::Quarantined => object.learning_weight * 0.10,
        }
    }

    fn next_tau(current_tau: f32, ci_value: f32) -> f32 {
        (current_tau + ((CI_TARGET - ci_value) * TAU_ADAPTATION_RATE)).clamp(TAU_MIN, TAU_MAX)
    }

    fn count_objects_by_state(objects: &[TextObject], state: MemoryState) -> u64 {
        objects
            .iter()
            .filter(|object| object.memory_state == state)
            .count() as u64
    }

    async fn evaluate_object_with_ecgl(
        &self,
        mut object: TextObject,
        episode: &EpisodeRecord,
        outcome: &OutcomeRecord,
        tau: f32,
    ) -> Result<TextObject, EmilyError> {
        let edges = self.store.list_text_edges(&[object.id.clone()], 1).await?;
        let semantic_neighbor_count = edges
            .into_iter()
            .filter(|edge| edge.edge_type == TextEdgeType::SemanticSimilar)
            .count();

        let confidence = Self::confidence_factor_for_object(&object);
        let outcome_factor = Self::outcome_factor_for_status(outcome.status);
        let novelty_factor = Self::novelty_factor_for_object(&object, semantic_neighbor_count);
        let stability_factor = Self::stability_factor_for_episode(episode.state);
        let learning_weight =
            Self::learning_weight(confidence, outcome_factor, novelty_factor, stability_factor);
        let gate_score = Self::gate_score(learning_weight, tau);
        let memory_state =
            Self::memory_state_for_object(episode, outcome, learning_weight, gate_score, tau);

        object.confidence = confidence;
        object.outcome_factor = outcome_factor;
        object.novelty_factor = novelty_factor;
        object.stability_factor = stability_factor;
        object.learning_weight = learning_weight;
        object.gate_score = Some(gate_score);
        object.memory_state = memory_state;
        object.integrated = matches!(memory_state, MemoryState::Integrated);
        object.quarantine_score = match memory_state {
            MemoryState::Quarantined => object
                .quarantine_score
                .max(Self::quarantine_score(novelty_factor, learning_weight)),
            MemoryState::Integrated | MemoryState::Pending | MemoryState::Deferred => {
                object.quarantine_score
            }
        };

        Ok(object)
    }

    async fn recompute_integrity_snapshot(
        &self,
        ts_unix_ms: i64,
    ) -> Result<IntegritySnapshot, EmilyError> {
        let current_tau = self.ecgl.read().await.tau;
        let objects = self.store.list_text_objects(None).await?;
        let ci_value = if objects.is_empty() {
            1.0
        } else {
            let total = objects
                .iter()
                .map(Self::ci_contribution)
                .fold(0.0_f32, |acc, value| acc + value);
            (total / objects.len() as f32).clamp(0.0, 1.0)
        };
        let next_tau = Self::next_tau(current_tau, ci_value);
        let snapshot = IntegritySnapshot {
            id: format!("integrity:{ts_unix_ms}"),
            ts_unix_ms,
            ci_value,
            tau: next_tau,
            integrated_count: Self::count_objects_by_state(&objects, MemoryState::Integrated),
            quarantined_count: Self::count_objects_by_state(&objects, MemoryState::Quarantined),
            pending_count: Self::count_objects_by_state(&objects, MemoryState::Pending),
            deferred_count: Self::count_objects_by_state(&objects, MemoryState::Deferred),
        };
        self.store.upsert_integrity_snapshot(&snapshot).await?;
        let mut ecgl = self.ecgl.write().await;
        *ecgl = EcglRuntimeState {
            tau: snapshot.tau,
            last_snapshot: Some(snapshot.clone()),
        };
        Ok(snapshot)
    }

    pub(super) async fn apply_ecgl_after_outcome(
        &self,
        episode: &EpisodeRecord,
        outcome: &OutcomeRecord,
    ) -> Result<Option<IntegritySnapshot>, EmilyError> {
        let tau = self.ecgl.read().await.tau;
        let links = self
            .store
            .list_episode_trace_links(&episode.id)
            .await?
            .into_iter()
            .collect::<Vec<_>>();
        if links.is_empty() {
            return Ok(None);
        }

        for link in links {
            let Some(object) = self.store.get_text_object(&link.object_id).await? else {
                continue;
            };
            let updated = self
                .evaluate_object_with_ecgl(object, episode, outcome, tau)
                .await?;
            self.store.upsert_text_object(&updated).await?;
        }

        let snapshot = self
            .recompute_integrity_snapshot(outcome.recorded_at_unix_ms)
            .await?;
        Ok(Some(snapshot))
    }
}
