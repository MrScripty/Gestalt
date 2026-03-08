use crate::error::EmilyError;
use crate::model::EmbeddingProviderStatus;
use async_trait::async_trait;

/// Abstraction over embedding providers used by Emily ingestion.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed_text(&self, text: &str) -> Result<Vec<f32>, EmilyError>;

    async fn status(&self) -> Option<EmbeddingProviderStatus> {
        None
    }

    async fn shutdown(&self) -> Result<(), EmilyError> {
        Ok(())
    }
}

/// Default provider for deployments where embeddings are disabled.
#[derive(Debug, Default)]
pub struct NoopEmbeddingProvider;

#[async_trait]
impl EmbeddingProvider for NoopEmbeddingProvider {
    async fn embed_text(&self, _text: &str) -> Result<Vec<f32>, EmilyError> {
        Ok(Vec::new())
    }
}

#[cfg(feature = "pantograph")]
mod pantograph;

#[cfg(feature = "pantograph")]
pub use pantograph::Provider as PantographEmbeddingProvider;
#[cfg(feature = "pantograph")]
pub use pantograph::{
    Binding as PantographWorkflowBinding, Config as PantographWorkflowEmbeddingConfig,
    ServiceClient as PantographWorkflowServiceClient,
    SessionClient as PantographWorkflowSessionClient,
};
