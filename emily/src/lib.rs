//! Emily reusable memory crate.
//!
//! This crate defines transport-agnostic contracts for:
//! - ingesting arbitrary text objects
//! - retrieving ranked context and history
//! - recording episode, outcome, audit, EARL, and ECGL artifacts
//! - running the in-process memory/runtime core against an addressable database
//!
//! Host applications own source-specific mapping concerns such as stream naming,
//! event-to-episode linkage policy, UI behavior, transport boundaries, and any
//! broader sovereign-dispatch orchestration layered above this crate.

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
    AppendAuditRecordRequest, AuditRecord, AuditRecordKind, ContextItem, ContextPacket,
    ContextQuery, CreateEpisodeRequest, DatabaseLocator, EarlDecision, EarlEvaluationRecord,
    EarlEvaluationRequest, EarlHostAction, EarlSignalVector, EmbeddingProviderStatus,
    EpisodeRecord, EpisodeState, EpisodeTraceKind, EpisodeTraceLink, HealthSnapshot, HistoryPage,
    HistoryPageRequest, IngestTextRequest, IntegritySnapshot, MemoryPolicy, MemoryState,
    OutcomeRecord, OutcomeStatus, RecordOutcomeRequest, TextEdge, TextEdgeType, TextObject,
    TextObjectKind, TextVector, TraceLinkRequest, VectorizationConfig, VectorizationConfigPatch,
    VectorizationJobKind, VectorizationJobSnapshot, VectorizationJobState, VectorizationRunRequest,
    VectorizationStatus,
};
