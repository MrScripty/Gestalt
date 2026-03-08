use crate::error::EmilyError;
use crate::model::{
    AuditRecord, ContextPacket, ContextQuery, DatabaseLocator, EarlEvaluationRecord, EpisodeRecord,
    EpisodeTraceLink, HistoryPage, HistoryPageRequest, IntegritySnapshot, OutcomeRecord, TextEdge,
    TextObject, TextVector, VectorizationConfig,
};
use async_trait::async_trait;

/// Storage contract implemented by Emily backends.
#[async_trait]
pub trait EmilyStore: Send + Sync {
    async fn open(&self, locator: &DatabaseLocator) -> Result<(), EmilyError>;
    async fn close(&self) -> Result<(), EmilyError>;
    async fn insert_text_object(&self, object: &TextObject) -> Result<(), EmilyError>;
    async fn upsert_text_object(&self, object: &TextObject) -> Result<(), EmilyError>;
    async fn get_text_object(&self, object_id: &str) -> Result<Option<TextObject>, EmilyError>;
    async fn upsert_text_edge(&self, edge: &TextEdge) -> Result<(), EmilyError>;
    async fn upsert_text_vector(&self, vector: &TextVector) -> Result<(), EmilyError>;
    async fn get_text_vector(&self, object_id: &str) -> Result<Option<TextVector>, EmilyError>;
    async fn list_text_vectors(
        &self,
        stream_id: Option<&str>,
    ) -> Result<Vec<TextVector>, EmilyError>;
    async fn list_text_objects(
        &self,
        stream_id: Option<&str>,
    ) -> Result<Vec<TextObject>, EmilyError>;
    async fn list_text_edges(
        &self,
        object_ids: &[String],
        max_depth: u8,
    ) -> Result<Vec<TextEdge>, EmilyError>;
    async fn get_vectorization_config(&self) -> Result<Option<VectorizationConfig>, EmilyError>;
    async fn upsert_vectorization_config(
        &self,
        config: &VectorizationConfig,
    ) -> Result<(), EmilyError>;
    async fn upsert_episode(&self, episode: &EpisodeRecord) -> Result<(), EmilyError>;
    async fn get_episode(&self, episode_id: &str) -> Result<Option<EpisodeRecord>, EmilyError>;
    async fn upsert_episode_trace_link(&self, link: &EpisodeTraceLink) -> Result<(), EmilyError>;
    async fn get_episode_trace_link(
        &self,
        link_id: &str,
    ) -> Result<Option<EpisodeTraceLink>, EmilyError>;
    async fn list_episode_trace_links(
        &self,
        episode_id: &str,
    ) -> Result<Vec<EpisodeTraceLink>, EmilyError>;
    async fn upsert_outcome(&self, outcome: &OutcomeRecord) -> Result<(), EmilyError>;
    async fn get_outcome(&self, outcome_id: &str) -> Result<Option<OutcomeRecord>, EmilyError>;
    async fn list_outcomes(&self, episode_id: &str) -> Result<Vec<OutcomeRecord>, EmilyError>;
    async fn upsert_earl_evaluation(
        &self,
        evaluation: &EarlEvaluationRecord,
    ) -> Result<(), EmilyError>;
    async fn get_earl_evaluation(
        &self,
        evaluation_id: &str,
    ) -> Result<Option<EarlEvaluationRecord>, EmilyError>;
    async fn list_earl_evaluations(
        &self,
        episode_id: &str,
    ) -> Result<Vec<EarlEvaluationRecord>, EmilyError>;
    async fn upsert_audit_record(&self, audit: &AuditRecord) -> Result<(), EmilyError>;
    async fn get_audit_record(&self, audit_id: &str) -> Result<Option<AuditRecord>, EmilyError>;
    async fn list_audit_records(&self, episode_id: &str) -> Result<Vec<AuditRecord>, EmilyError>;
    async fn upsert_integrity_snapshot(
        &self,
        snapshot: &IntegritySnapshot,
    ) -> Result<(), EmilyError>;
    async fn latest_integrity_snapshot(&self) -> Result<Option<IntegritySnapshot>, EmilyError>;
    async fn query_context(&self, query: &ContextQuery) -> Result<ContextPacket, EmilyError>;
    async fn page_history_before(
        &self,
        request: &HistoryPageRequest,
    ) -> Result<HistoryPage, EmilyError>;
}

/// Surreal-backed store implementation.
pub mod surreal;
