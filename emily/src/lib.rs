//! Emily reusable memory crate.
//!
//! This crate defines transport-agnostic memory contracts for ingesting arbitrary
//! text objects and querying history/context from an addressable database.

pub mod api;
pub mod error;
pub mod inference;
pub mod model;
pub mod runtime;
pub mod store;

pub use api::EmilyApi;
pub use error::EmilyError;
pub use inference::{EmbeddingProvider, NoopEmbeddingProvider};
#[cfg(feature = "pantograph")]
pub use inference::{
    PantographEmbeddingProvider, PantographWorkflowBinding, PantographWorkflowEmbeddingConfig,
    PantographWorkflowServiceClient, PantographWorkflowSessionClient,
};
pub use model::{
    ContextItem, ContextPacket, ContextQuery, DatabaseLocator, HealthSnapshot, HistoryPage,
    HistoryPageRequest, IngestTextRequest, MemoryPolicy, TextEdge, TextEdgeType, TextObject,
    TextObjectKind,
};
