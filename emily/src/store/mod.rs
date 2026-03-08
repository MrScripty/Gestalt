use crate::error::EmilyError;
use crate::model::{
    ContextPacket, ContextQuery, DatabaseLocator, HistoryPage, HistoryPageRequest, TextEdge,
    TextObject, TextVector, VectorizationConfig,
};
use async_trait::async_trait;

/// Storage contract implemented by Emily backends.
#[async_trait]
pub trait EmilyStore: Send + Sync {
    async fn open(&self, locator: &DatabaseLocator) -> Result<(), EmilyError>;
    async fn close(&self) -> Result<(), EmilyError>;
    async fn insert_text_object(&self, object: &TextObject) -> Result<(), EmilyError>;
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
    async fn query_context(&self, query: &ContextQuery) -> Result<ContextPacket, EmilyError>;
    async fn page_history_before(
        &self,
        request: &HistoryPageRequest,
    ) -> Result<HistoryPage, EmilyError>;
}

/// Surreal-backed store implementation.
pub mod surreal;
