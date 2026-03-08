use crate::error::EmilyError;
use crate::model::{
    AuditRecord, ContextPacket, ContextQuery, DatabaseLocator, EarlEvaluationRecord, EpisodeRecord,
    EpisodeTraceLink, HistoryPage, HistoryPageRequest, IntegritySnapshot, OutcomeRecord,
    RemoteEpisodeRecord, RoutingDecision, TextEdge, TextObject, TextVector, ValidationOutcome,
    VectorizationConfig,
};
use crate::store::EmilyStore;
use async_trait::async_trait;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use tokio::sync::RwLock;

mod earl;
mod ecgl;
mod episodes;
mod sovereign;
#[cfg(test)]
mod tests;
mod text;

#[derive(Debug, Default)]
struct StoreState {
    active_locator: Option<DatabaseLocator>,
    active_client: Option<Surreal<Db>>,
}

/// Embedded SurrealDB-backed store implementation.
#[derive(Debug, Default)]
pub struct SurrealEmilyStore {
    state: RwLock<StoreState>,
}

impl SurrealEmilyStore {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(StoreState::default()),
        }
    }

    async fn active_client(&self) -> Result<Surreal<Db>, EmilyError> {
        let state = self.state.read().await;
        state
            .active_client
            .clone()
            .ok_or(EmilyError::DatabaseNotOpen)
    }
}

#[async_trait]
impl EmilyStore for SurrealEmilyStore {
    async fn open(&self, locator: &DatabaseLocator) -> Result<(), EmilyError> {
        self.open_internal(locator).await
    }

    async fn close(&self) -> Result<(), EmilyError> {
        let mut state = self.state.write().await;
        state.active_locator = None;
        state.active_client = None;
        Ok(())
    }

    async fn insert_text_object(&self, object: &TextObject) -> Result<(), EmilyError> {
        self.insert_text_object_internal(object).await
    }

    async fn upsert_text_object(&self, object: &TextObject) -> Result<(), EmilyError> {
        self.upsert_text_object_internal(object).await
    }

    async fn get_text_object(&self, object_id: &str) -> Result<Option<TextObject>, EmilyError> {
        self.get_text_object_internal(object_id).await
    }

    async fn upsert_text_edge(&self, edge: &TextEdge) -> Result<(), EmilyError> {
        self.upsert_text_edge_internal(edge).await
    }

    async fn upsert_text_vector(&self, vector: &TextVector) -> Result<(), EmilyError> {
        self.upsert_text_vector_internal(vector).await
    }

    async fn get_text_vector(&self, object_id: &str) -> Result<Option<TextVector>, EmilyError> {
        self.get_text_vector_internal(object_id).await
    }

    async fn list_text_vectors(
        &self,
        stream_id: Option<&str>,
    ) -> Result<Vec<TextVector>, EmilyError> {
        self.list_text_vectors_internal(stream_id).await
    }

    async fn list_text_objects(
        &self,
        stream_id: Option<&str>,
    ) -> Result<Vec<TextObject>, EmilyError> {
        self.list_text_objects_internal(stream_id).await
    }

    async fn list_text_edges(
        &self,
        object_ids: &[String],
        max_depth: u8,
    ) -> Result<Vec<TextEdge>, EmilyError> {
        self.list_text_edges_internal(object_ids, max_depth).await
    }

    async fn get_vectorization_config(&self) -> Result<Option<VectorizationConfig>, EmilyError> {
        self.get_vectorization_config_internal().await
    }

    async fn upsert_vectorization_config(
        &self,
        config: &VectorizationConfig,
    ) -> Result<(), EmilyError> {
        self.upsert_vectorization_config_internal(config).await
    }

    async fn upsert_episode(&self, episode: &EpisodeRecord) -> Result<(), EmilyError> {
        self.upsert_episode_internal(episode).await
    }

    async fn get_episode(&self, episode_id: &str) -> Result<Option<EpisodeRecord>, EmilyError> {
        self.get_episode_internal(episode_id).await
    }

    async fn upsert_episode_trace_link(&self, link: &EpisodeTraceLink) -> Result<(), EmilyError> {
        self.upsert_episode_trace_link_internal(link).await
    }

    async fn get_episode_trace_link(
        &self,
        link_id: &str,
    ) -> Result<Option<EpisodeTraceLink>, EmilyError> {
        self.get_episode_trace_link_internal(link_id).await
    }

    async fn list_episode_trace_links(
        &self,
        episode_id: &str,
    ) -> Result<Vec<EpisodeTraceLink>, EmilyError> {
        self.list_episode_trace_links_internal(episode_id).await
    }

    async fn upsert_outcome(&self, outcome: &OutcomeRecord) -> Result<(), EmilyError> {
        self.upsert_outcome_internal(outcome).await
    }

    async fn get_outcome(&self, outcome_id: &str) -> Result<Option<OutcomeRecord>, EmilyError> {
        self.get_outcome_internal(outcome_id).await
    }

    async fn list_outcomes(&self, episode_id: &str) -> Result<Vec<OutcomeRecord>, EmilyError> {
        self.list_outcomes_internal(episode_id).await
    }

    async fn upsert_earl_evaluation(
        &self,
        evaluation: &EarlEvaluationRecord,
    ) -> Result<(), EmilyError> {
        self.upsert_earl_evaluation_internal(evaluation).await
    }

    async fn get_earl_evaluation(
        &self,
        evaluation_id: &str,
    ) -> Result<Option<EarlEvaluationRecord>, EmilyError> {
        self.get_earl_evaluation_internal(evaluation_id).await
    }

    async fn list_earl_evaluations(
        &self,
        episode_id: &str,
    ) -> Result<Vec<EarlEvaluationRecord>, EmilyError> {
        self.list_earl_evaluations_internal(episode_id).await
    }

    async fn upsert_audit_record(&self, audit: &AuditRecord) -> Result<(), EmilyError> {
        self.upsert_audit_record_internal(audit).await
    }

    async fn get_audit_record(&self, audit_id: &str) -> Result<Option<AuditRecord>, EmilyError> {
        self.get_audit_record_internal(audit_id).await
    }

    async fn list_audit_records(&self, episode_id: &str) -> Result<Vec<AuditRecord>, EmilyError> {
        self.list_audit_records_internal(episode_id).await
    }

    async fn upsert_routing_decision(&self, decision: &RoutingDecision) -> Result<(), EmilyError> {
        self.upsert_routing_decision_internal(decision).await
    }

    async fn get_routing_decision(
        &self,
        decision_id: &str,
    ) -> Result<Option<RoutingDecision>, EmilyError> {
        self.get_routing_decision_internal(decision_id).await
    }

    async fn list_routing_decisions(
        &self,
        episode_id: &str,
    ) -> Result<Vec<RoutingDecision>, EmilyError> {
        self.list_routing_decisions_internal(episode_id).await
    }

    async fn upsert_remote_episode(
        &self,
        remote_episode: &RemoteEpisodeRecord,
    ) -> Result<(), EmilyError> {
        self.upsert_remote_episode_internal(remote_episode).await
    }

    async fn get_remote_episode(
        &self,
        remote_episode_id: &str,
    ) -> Result<Option<RemoteEpisodeRecord>, EmilyError> {
        self.get_remote_episode_internal(remote_episode_id).await
    }

    async fn list_remote_episodes(
        &self,
        episode_id: &str,
    ) -> Result<Vec<RemoteEpisodeRecord>, EmilyError> {
        self.list_remote_episodes_internal(episode_id).await
    }

    async fn upsert_validation_outcome(
        &self,
        outcome: &ValidationOutcome,
    ) -> Result<(), EmilyError> {
        self.upsert_validation_outcome_internal(outcome).await
    }

    async fn get_validation_outcome(
        &self,
        validation_id: &str,
    ) -> Result<Option<ValidationOutcome>, EmilyError> {
        self.get_validation_outcome_internal(validation_id).await
    }

    async fn list_validation_outcomes(
        &self,
        episode_id: &str,
    ) -> Result<Vec<ValidationOutcome>, EmilyError> {
        self.list_validation_outcomes_internal(episode_id).await
    }

    async fn upsert_integrity_snapshot(
        &self,
        snapshot: &IntegritySnapshot,
    ) -> Result<(), EmilyError> {
        self.upsert_integrity_snapshot_internal(snapshot).await
    }

    async fn latest_integrity_snapshot(&self) -> Result<Option<IntegritySnapshot>, EmilyError> {
        self.latest_integrity_snapshot_internal().await
    }

    async fn query_context(&self, query: &ContextQuery) -> Result<ContextPacket, EmilyError> {
        self.query_context_internal(query).await
    }

    async fn page_history_before(
        &self,
        request: &HistoryPageRequest,
    ) -> Result<HistoryPage, EmilyError> {
        self.page_history_before_internal(request).await
    }
}
