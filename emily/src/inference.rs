use crate::error::EmilyError;
use async_trait::async_trait;

/// Abstraction over embedding providers used by Emily ingestion.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed_text(&self, text: &str) -> Result<Vec<f32>, EmilyError>;
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
mod pantograph {
    use super::*;
    use pantograph_inference::SharedGateway;

    /// Pantograph-backed embedding provider.
    pub struct PantographEmbeddingProvider {
        gateway: SharedGateway,
        model_name: String,
    }

    impl PantographEmbeddingProvider {
        pub fn new(gateway: SharedGateway, model_name: impl Into<String>) -> Self {
            Self {
                gateway,
                model_name: model_name.into(),
            }
        }

        /// Verify gateway readiness for embedding requests.
        pub async fn validate(&self) -> Result<(), EmilyError> {
            let ready = self.gateway.is_ready().await;
            if !ready {
                return Err(EmilyError::Embedding(
                    "Pantograph gateway is not ready".to_string(),
                ));
            }
            let capabilities = self.gateway.capabilities().await;
            if !capabilities.embeddings {
                return Err(EmilyError::Embedding(
                    "Active Pantograph backend does not support embeddings".to_string(),
                ));
            }
            Ok(())
        }
    }

    #[async_trait]
    impl EmbeddingProvider for PantographEmbeddingProvider {
        async fn embed_text(&self, text: &str) -> Result<Vec<f32>, EmilyError> {
            let response = self
                .gateway
                .embeddings(vec![text.to_string()], &self.model_name)
                .await
                .map_err(|error| {
                    EmilyError::Embedding(format!("Pantograph embeddings request failed: {error}"))
                })?;
            let vector = response.into_iter().next().map(|item| item.vector).ok_or(
                EmilyError::Embedding("Pantograph returned no embedding vectors".to_string()),
            )?;
            Ok(vector)
        }
    }

    pub use PantographEmbeddingProvider as Provider;
}

#[cfg(feature = "pantograph")]
pub use pantograph::Provider as PantographEmbeddingProvider;
