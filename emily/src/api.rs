use crate::error::EmilyError;
use crate::model::{
    ContextPacket, ContextQuery, DatabaseLocator, HealthSnapshot, HistoryPage, HistoryPageRequest,
    IngestTextRequest, MemoryPolicy, TextObject,
};
use async_trait::async_trait;

/// Public Emily API exposed to host systems.
#[async_trait]
pub trait EmilyApi: Send + Sync {
    /// Open an embedded database target for this runtime.
    async fn open_db(&self, locator: DatabaseLocator) -> Result<(), EmilyError>;

    /// Switch to another embedded database target.
    async fn switch_db(&self, locator: DatabaseLocator) -> Result<(), EmilyError>;

    /// Close the currently active database target.
    async fn close_db(&self) -> Result<(), EmilyError>;

    /// Ingest one text object into memory.
    async fn ingest_text(&self, request: IngestTextRequest) -> Result<TextObject, EmilyError>;

    /// Retrieve ranked context items for the given query.
    async fn query_context(&self, query: ContextQuery) -> Result<ContextPacket, EmilyError>;

    /// Fetch one page of historical objects before a cursor sequence.
    async fn page_history_before(
        &self,
        request: HistoryPageRequest,
    ) -> Result<HistoryPage, EmilyError>;

    /// Read the current mutable memory policy.
    async fn memory_policy(&self) -> Result<MemoryPolicy, EmilyError>;

    /// Update policy used for retrieval/ranking.
    async fn set_memory_policy(&self, policy: MemoryPolicy) -> Result<(), EmilyError>;

    /// Report runtime and queue health.
    async fn health(&self) -> Result<HealthSnapshot, EmilyError>;
}
