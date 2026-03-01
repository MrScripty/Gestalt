use crate::error::EmilyError;
use crate::model::{
    ContextPacket, ContextQuery, DatabaseLocator, HistoryPage, HistoryPageRequest, TextObject,
};
use crate::store::EmilyStore;
use async_trait::async_trait;

/// Placeholder for embedded SurrealDB store implementation.
#[derive(Debug, Default)]
pub struct SurrealEmilyStore;

impl SurrealEmilyStore {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl EmilyStore for SurrealEmilyStore {
    async fn open(&self, _locator: &DatabaseLocator) -> Result<(), EmilyError> {
        Err(EmilyError::Runtime(
            "Surreal store is not initialized yet".to_string(),
        ))
    }

    async fn close(&self) -> Result<(), EmilyError> {
        Ok(())
    }

    async fn insert_text_object(&self, _object: &TextObject) -> Result<(), EmilyError> {
        Err(EmilyError::Runtime(
            "Surreal store is not initialized yet".to_string(),
        ))
    }

    async fn query_context(&self, _query: &ContextQuery) -> Result<ContextPacket, EmilyError> {
        Err(EmilyError::Runtime(
            "Surreal store is not initialized yet".to_string(),
        ))
    }

    async fn page_history_before(
        &self,
        _request: &HistoryPageRequest,
    ) -> Result<HistoryPage, EmilyError> {
        Err(EmilyError::Runtime(
            "Surreal store is not initialized yet".to_string(),
        ))
    }
}
