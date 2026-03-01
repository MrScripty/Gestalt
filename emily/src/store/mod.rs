use crate::error::EmilyError;
use crate::model::{
    ContextPacket, ContextQuery, DatabaseLocator, HistoryPage, HistoryPageRequest, TextObject,
};
use async_trait::async_trait;

/// Storage contract implemented by Emily backends.
#[async_trait]
pub trait EmilyStore: Send + Sync {
    async fn open(&self, locator: &DatabaseLocator) -> Result<(), EmilyError>;
    async fn close(&self) -> Result<(), EmilyError>;
    async fn insert_text_object(&self, object: &TextObject) -> Result<(), EmilyError>;
    async fn query_context(&self, query: &ContextQuery) -> Result<ContextPacket, EmilyError>;
    async fn page_history_before(
        &self,
        request: &HistoryPageRequest,
    ) -> Result<HistoryPage, EmilyError>;
}

/// Surreal-backed store implementation.
pub mod surreal;
