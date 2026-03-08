use crate::state::{SessionId, SnippetId};
use crate::terminal::TerminalMemorySink;
use emily::api::EmilyApi;
use emily::inference::EmbeddingProvider;
use emily::model::{
    ContextPacket, ContextQuery, CreateEpisodeRequest, DatabaseLocator, EarlEvaluationRecord,
    EmbeddingProviderStatus, EpisodeRecord, EpisodeTraceLink, HistoryPageRequest,
    IngestTextRequest, TextObjectKind, TraceLinkRequest, VectorizationConfig,
    VectorizationConfigPatch, VectorizationJobSnapshot, VectorizationRunRequest,
    VectorizationStatus,
};
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::thread;
use tokio::sync::{mpsc as tokio_mpsc, oneshot, watch};

#[derive(Debug)]
enum BridgeCommand {
    IngestInput {
        session_id: SessionId,
        cwd: String,
        line: String,
        ts_unix_ms: i64,
    },
    IngestOutput {
        session_id: SessionId,
        cwd: String,
        line: String,
        ts_unix_ms: i64,
    },
    PageHistory {
        session_id: SessionId,
        before_sequence: Option<u64>,
        limit: usize,
        response_tx: oneshot::Sender<Result<HistoryChunk, String>>,
    },
    QueryContext {
        session_id: SessionId,
        query_text: String,
        top_k: usize,
        response_tx: oneshot::Sender<Result<ContextPacket, String>>,
    },
    IngestSnippet {
        request: SnippetIngestRequest,
        response_tx: oneshot::Sender<Result<SnippetIngestResult, String>>,
    },
    CreateEpisode {
        request: CreateEpisodeRequest,
        response_tx: oneshot::Sender<Result<EpisodeRecord, String>>,
    },
    LinkTextToEpisode {
        request: TraceLinkRequest,
        response_tx: oneshot::Sender<Result<EpisodeTraceLink, String>>,
    },
    Episode {
        episode_id: String,
        response_tx: oneshot::Sender<Result<Option<EpisodeRecord>, String>>,
    },
    LatestEarlEvaluation {
        episode_id: String,
        response_tx: oneshot::Sender<Result<Option<EarlEvaluationRecord>, String>>,
    },
    UpdateVectorizationConfig {
        patch: VectorizationConfigPatch,
        response_tx: oneshot::Sender<Result<VectorizationConfig, String>>,
    },
    StartBackfill {
        request: VectorizationRunRequest,
        response_tx: oneshot::Sender<Result<VectorizationJobSnapshot, String>>,
    },
    StartRevectorize {
        request: VectorizationRunRequest,
        response_tx: oneshot::Sender<Result<VectorizationJobSnapshot, String>>,
    },
    CancelVectorizationJob {
        job_id: String,
        response_tx: oneshot::Sender<Result<(), String>>,
    },
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct HistoryChunk {
    pub lines: Vec<String>,
    pub next_before_sequence: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct SnippetIngestRequest {
    pub snippet_id: SnippetId,
    pub source_session_id: SessionId,
    pub source_stream_id: String,
    pub source_cwd: String,
    pub source_start_offset: u64,
    pub source_end_offset: u64,
    pub source_start_row: u32,
    pub source_end_row: u32,
    pub text: String,
    pub ts_unix_ms: i64,
}

#[derive(Debug, Clone)]
pub struct SnippetIngestResult {
    pub object_id: String,
    pub embedding_profile_id: Option<String>,
    pub embedding_dimensions: Option<usize>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BridgeHealthSnapshot {
    pub ingest_input_success: u64,
    pub ingest_input_error: u64,
    pub ingest_output_success: u64,
    pub ingest_output_error: u64,
    pub history_query_error: u64,
    pub context_query_error: u64,
}

#[derive(Debug, Default)]
struct BridgeHealthCounters {
    ingest_input_success: AtomicU64,
    ingest_input_error: AtomicU64,
    ingest_output_success: AtomicU64,
    ingest_output_error: AtomicU64,
    history_query_error: AtomicU64,
    context_query_error: AtomicU64,
}

impl BridgeHealthCounters {
    fn snapshot(&self) -> BridgeHealthSnapshot {
        BridgeHealthSnapshot {
            ingest_input_success: self.ingest_input_success.load(AtomicOrdering::Relaxed),
            ingest_input_error: self.ingest_input_error.load(AtomicOrdering::Relaxed),
            ingest_output_success: self.ingest_output_success.load(AtomicOrdering::Relaxed),
            ingest_output_error: self.ingest_output_error.load(AtomicOrdering::Relaxed),
            history_query_error: self.history_query_error.load(AtomicOrdering::Relaxed),
            context_query_error: self.context_query_error.load(AtomicOrdering::Relaxed),
        }
    }
}

/// Gestalt-side adapter that feeds terminal text into Emily and exposes history queries.
pub struct EmilyBridge {
    command_tx: tokio_mpsc::UnboundedSender<BridgeCommand>,
    health: Arc<BridgeHealthCounters>,
    vectorization_status_rx: watch::Receiver<VectorizationStatus>,
}

impl EmilyBridge {
    /// Starts Emily runtime worker with a default local database locator.
    pub fn new_default() -> Self {
        let locator = Self::default_locator();
        Self::new(locator)
    }

    /// Starts Emily runtime worker with default locator and explicit embedding provider.
    pub fn new_default_with_embedding_provider(
        embedding_provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        let locator = Self::default_locator();
        Self::with_embedding_provider(locator, Some(embedding_provider))
    }

    /// Starts Emily runtime worker without provider and persists bootstrap error in status.
    pub fn new_default_with_provider_error(provider_error: String) -> Self {
        let locator = Self::default_locator();
        Self::with_embedding_provider_bootstrap_error(locator, None, Some(provider_error))
    }

    /// Starts Emily runtime worker with an explicit database locator.
    pub fn new(locator: DatabaseLocator) -> Self {
        Self::with_embedding_provider(locator, None)
    }

    /// Starts Emily runtime worker with an explicit embedding provider.
    pub fn with_embedding_provider(
        locator: DatabaseLocator,
        embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    ) -> Self {
        Self::with_embedding_provider_bootstrap_error(locator, embedding_provider, None)
    }

    fn with_embedding_provider_bootstrap_error(
        locator: DatabaseLocator,
        embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
        provider_bootstrap_error: Option<String>,
    ) -> Self {
        let initial_provider_status = fallback_provider_status(
            embedding_provider.is_some(),
            provider_bootstrap_error.as_deref(),
        );
        let (command_tx, command_rx) = tokio_mpsc::unbounded_channel::<BridgeCommand>();
        let initial_vectorization_status = VectorizationStatus {
            config: VectorizationConfig::default(),
            provider_available: embedding_provider.is_some(),
            provider_status: initial_provider_status,
            active_job: None,
            last_job: None,
        };
        let (vectorization_status_tx, vectorization_status_rx) =
            watch::channel(initial_vectorization_status);
        let health = Arc::new(BridgeHealthCounters::default());
        let worker_health = Arc::clone(&health);

        thread::spawn(move || {
            run_worker(
                locator,
                embedding_provider,
                provider_bootstrap_error,
                command_rx,
                vectorization_status_tx,
                worker_health,
            );
        });

        Self {
            command_tx,
            health,
            vectorization_status_rx,
        }
    }

    pub fn page_history_before(
        &self,
        session_id: SessionId,
        before_sequence: Option<u64>,
        limit: usize,
    ) -> Result<HistoryChunk, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::PageHistory {
                session_id,
                before_sequence,
                limit,
                response_tx,
            })
            .map_err(|error| format!("failed sending history request to Emily worker: {error}"))?;
        response_rx.blocking_recv().map_err(|error| {
            format!("failed receiving history response from Emily worker: {error}")
        })?
    }

    pub async fn page_history_before_async(
        &self,
        session_id: SessionId,
        before_sequence: Option<u64>,
        limit: usize,
    ) -> Result<HistoryChunk, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::PageHistory {
                session_id,
                before_sequence,
                limit,
                response_tx,
            })
            .map_err(|error| format!("failed sending history request to Emily worker: {error}"))?;
        response_rx.await.map_err(|error| {
            format!("failed receiving history response from Emily worker: {error}")
        })?
    }

    pub fn recent_lines(&self, session_id: SessionId, limit: usize) -> Vec<String> {
        self.recent_history(session_id, limit).lines
    }

    pub fn recent_history(&self, session_id: SessionId, limit: usize) -> HistoryChunk {
        self.page_history_before(session_id, None, limit)
            .map(|mut chunk| {
                // API returns newest-first for efficient paging. Terminal restore expects oldest-first.
                chunk.lines.reverse();
                chunk
            })
            .unwrap_or_else(|error| {
                eprintln!("Emily recent_history failed for session {session_id}: {error}");
                HistoryChunk {
                    lines: Vec::new(),
                    next_before_sequence: None,
                }
            })
    }

    pub fn health_snapshot(&self) -> BridgeHealthSnapshot {
        self.health.snapshot()
    }

    pub fn query_context(
        &self,
        session_id: SessionId,
        query_text: String,
        top_k: usize,
    ) -> Result<ContextPacket, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::QueryContext {
                session_id,
                query_text,
                top_k,
                response_tx,
            })
            .map_err(|error| format!("failed sending context request to Emily worker: {error}"))?;
        response_rx.blocking_recv().map_err(|error| {
            format!("failed receiving context response from Emily worker: {error}")
        })?
    }

    pub async fn query_context_async(
        &self,
        session_id: SessionId,
        query_text: String,
        top_k: usize,
    ) -> Result<ContextPacket, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::QueryContext {
                session_id,
                query_text,
                top_k,
                response_tx,
            })
            .map_err(|error| format!("failed sending context request to Emily worker: {error}"))?;
        response_rx.await.map_err(|error| {
            format!("failed receiving context response from Emily worker: {error}")
        })?
    }

    pub fn ingest_snippet(
        &self,
        request: SnippetIngestRequest,
    ) -> Result<SnippetIngestResult, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::IngestSnippet {
                request,
                response_tx,
            })
            .map_err(|error| format!("failed sending snippet ingest request: {error}"))?;
        response_rx.blocking_recv().map_err(|error| {
            format!("failed receiving snippet ingest response from Emily worker: {error}")
        })?
    }

    pub async fn ingest_snippet_async(
        &self,
        request: SnippetIngestRequest,
    ) -> Result<SnippetIngestResult, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::IngestSnippet {
                request,
                response_tx,
            })
            .map_err(|error| format!("failed sending snippet ingest request: {error}"))?;
        response_rx.await.map_err(|error| {
            format!("failed receiving snippet ingest response from Emily worker: {error}")
        })?
    }

    pub async fn create_episode_async(
        &self,
        request: CreateEpisodeRequest,
    ) -> Result<EpisodeRecord, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::CreateEpisode {
                request,
                response_tx,
            })
            .map_err(|error| format!("failed sending episode create request: {error}"))?;
        response_rx.await.map_err(|error| {
            format!("failed receiving episode create response from Emily worker: {error}")
        })?
    }

    pub async fn link_text_to_episode_async(
        &self,
        request: TraceLinkRequest,
    ) -> Result<EpisodeTraceLink, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::LinkTextToEpisode {
                request,
                response_tx,
            })
            .map_err(|error| format!("failed sending episode trace-link request: {error}"))?;
        response_rx.await.map_err(|error| {
            format!("failed receiving episode trace-link response from Emily worker: {error}")
        })?
    }

    pub async fn episode_async(&self, episode_id: String) -> Result<Option<EpisodeRecord>, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::Episode {
                episode_id,
                response_tx,
            })
            .map_err(|error| format!("failed sending episode read request: {error}"))?;
        response_rx.await.map_err(|error| {
            format!("failed receiving episode read response from Emily worker: {error}")
        })?
    }

    pub async fn latest_earl_evaluation_for_episode_async(
        &self,
        episode_id: String,
    ) -> Result<Option<EarlEvaluationRecord>, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::LatestEarlEvaluation {
                episode_id,
                response_tx,
            })
            .map_err(|error| format!("failed sending EARL read request: {error}"))?;
        response_rx.await.map_err(|error| {
            format!("failed receiving EARL read response from Emily worker: {error}")
        })?
    }

    pub fn vectorization_status(&self) -> VectorizationStatus {
        self.vectorization_status_rx.borrow().clone()
    }

    pub fn subscribe_vectorization_status(&self) -> watch::Receiver<VectorizationStatus> {
        self.vectorization_status_rx.clone()
    }

    pub fn update_vectorization_config(
        &self,
        patch: VectorizationConfigPatch,
    ) -> Result<VectorizationConfig, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::UpdateVectorizationConfig { patch, response_tx })
            .map_err(|error| {
                format!("failed sending vectorization config update request: {error}")
            })?;
        response_rx.blocking_recv().map_err(|error| {
            format!("failed receiving vectorization config update response: {error}")
        })?
    }

    pub async fn update_vectorization_config_async(
        &self,
        patch: VectorizationConfigPatch,
    ) -> Result<VectorizationConfig, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::UpdateVectorizationConfig { patch, response_tx })
            .map_err(|error| {
                format!("failed sending vectorization config update request: {error}")
            })?;
        response_rx.await.map_err(|error| {
            format!("failed receiving vectorization config update response: {error}")
        })?
    }

    pub fn start_backfill(
        &self,
        request: VectorizationRunRequest,
    ) -> Result<VectorizationJobSnapshot, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::StartBackfill {
                request,
                response_tx,
            })
            .map_err(|error| format!("failed sending start_backfill request: {error}"))?;
        response_rx.blocking_recv().map_err(|error| {
            format!("failed receiving start_backfill response from Emily worker: {error}")
        })?
    }

    pub async fn start_backfill_async(
        &self,
        request: VectorizationRunRequest,
    ) -> Result<VectorizationJobSnapshot, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::StartBackfill {
                request,
                response_tx,
            })
            .map_err(|error| format!("failed sending start_backfill request: {error}"))?;
        response_rx.await.map_err(|error| {
            format!("failed receiving start_backfill response from Emily worker: {error}")
        })?
    }

    pub fn start_revectorize(
        &self,
        request: VectorizationRunRequest,
    ) -> Result<VectorizationJobSnapshot, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::StartRevectorize {
                request,
                response_tx,
            })
            .map_err(|error| format!("failed sending start_revectorize request: {error}"))?;
        response_rx.blocking_recv().map_err(|error| {
            format!("failed receiving start_revectorize response from Emily worker: {error}")
        })?
    }

    pub async fn start_revectorize_async(
        &self,
        request: VectorizationRunRequest,
    ) -> Result<VectorizationJobSnapshot, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::StartRevectorize {
                request,
                response_tx,
            })
            .map_err(|error| format!("failed sending start_revectorize request: {error}"))?;
        response_rx.await.map_err(|error| {
            format!("failed receiving start_revectorize response from Emily worker: {error}")
        })?
    }

    pub fn cancel_vectorization_job(&self, job_id: String) -> Result<(), String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::CancelVectorizationJob {
                job_id,
                response_tx,
            })
            .map_err(|error| format!("failed sending cancel_vectorization_job request: {error}"))?;
        response_rx.blocking_recv().map_err(|error| {
            format!("failed receiving cancel_vectorization_job response from Emily worker: {error}")
        })?
    }

    pub async fn cancel_vectorization_job_async(&self, job_id: String) -> Result<(), String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(BridgeCommand::CancelVectorizationJob {
                job_id,
                response_tx,
            })
            .map_err(|error| format!("failed sending cancel_vectorization_job request: {error}"))?;
        response_rx.await.map_err(|error| {
            format!("failed receiving cancel_vectorization_job response from Emily worker: {error}")
        })?
    }

    fn default_locator() -> DatabaseLocator {
        DatabaseLocator {
            storage_path: default_storage_path(),
            namespace: "gestalt".to_string(),
            database: "default".to_string(),
        }
    }
}

impl Drop for EmilyBridge {
    fn drop(&mut self) {
        let _ = self.command_tx.send(BridgeCommand::Shutdown);
    }
}

impl TerminalMemorySink for EmilyBridge {
    fn record_input_line(&self, session_id: SessionId, cwd: &str, line: String, ts_unix_ms: i64) {
        let _ = self.command_tx.send(BridgeCommand::IngestInput {
            session_id,
            cwd: cwd.to_string(),
            line,
            ts_unix_ms,
        });
    }

    fn record_output_line(&self, session_id: SessionId, cwd: &str, line: String, ts_unix_ms: i64) {
        let _ = self.command_tx.send(BridgeCommand::IngestOutput {
            session_id,
            cwd: cwd.to_string(),
            line,
            ts_unix_ms,
        });
    }
}

fn run_worker(
    locator: DatabaseLocator,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    provider_bootstrap_error: Option<String>,
    mut command_rx: tokio_mpsc::UnboundedReceiver<BridgeCommand>,
    vectorization_status_tx: watch::Sender<VectorizationStatus>,
    health: Arc<BridgeHealthCounters>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("failed starting Emily tokio runtime: {error}");
            return;
        }
    };

    runtime.block_on(async move {
        let store = Arc::new(SurrealEmilyStore::new());
        let emily_runtime = Arc::new(EmilyRuntime::with_embedding_provider(
            store,
            embedding_provider,
        ));

        if let Err(error) = emily_runtime.open_db(locator).await {
            eprintln!("failed opening Emily database: {error}");
            return;
        }

        if let Ok(status) = emily_runtime.vectorization_status().await {
            let status =
                apply_provider_bootstrap_fallback(status, provider_bootstrap_error.as_deref());
            let _ = vectorization_status_tx.send(status);
        }
        let mut runtime_status_rx = emily_runtime.subscribe_vectorization_status();
        let status_tx = vectorization_status_tx.clone();
        let provider_bootstrap_error = provider_bootstrap_error.clone();
        tokio::spawn(async move {
            loop {
                match runtime_status_rx.recv().await {
                    Ok(status) => {
                        let status = apply_provider_bootstrap_fallback(
                            status,
                            provider_bootstrap_error.as_deref(),
                        );
                        let _ = status_tx.send(status);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        let mut sequence_by_stream = HashMap::<String, u64>::new();

        while let Some(command) = command_rx.recv().await {
            match command {
                BridgeCommand::IngestInput {
                    session_id,
                    cwd,
                    line,
                    ts_unix_ms,
                } => {
                    match ingest_line(
                        &emily_runtime,
                        &mut sequence_by_stream,
                        session_id,
                        cwd,
                        line,
                        ts_unix_ms,
                        TextObjectKind::UserInput,
                    )
                    .await
                    {
                        Ok(()) => {
                            health
                                .ingest_input_success
                                .fetch_add(1, AtomicOrdering::Relaxed);
                        }
                        Err(error) => {
                            health
                                .ingest_input_error
                                .fetch_add(1, AtomicOrdering::Relaxed);
                            eprintln!(
                                "Emily ingest input failed for session {session_id}: {error}"
                            );
                        }
                    }
                }
                BridgeCommand::IngestOutput {
                    session_id,
                    cwd,
                    line,
                    ts_unix_ms,
                } => {
                    match ingest_line(
                        &emily_runtime,
                        &mut sequence_by_stream,
                        session_id,
                        cwd,
                        line,
                        ts_unix_ms,
                        TextObjectKind::SystemOutput,
                    )
                    .await
                    {
                        Ok(()) => {
                            health
                                .ingest_output_success
                                .fetch_add(1, AtomicOrdering::Relaxed);
                        }
                        Err(error) => {
                            health
                                .ingest_output_error
                                .fetch_add(1, AtomicOrdering::Relaxed);
                            eprintln!(
                                "Emily ingest output failed for session {session_id}: {error}"
                            );
                        }
                    }
                }
                BridgeCommand::PageHistory {
                    session_id,
                    before_sequence,
                    limit,
                    response_tx,
                } => {
                    let result = emily_runtime
                        .page_history_before(HistoryPageRequest {
                            stream_id: stream_id(session_id),
                            before_sequence,
                            limit,
                        })
                        .await
                        .map(|page| HistoryChunk {
                            lines: page.items.into_iter().map(|item| item.text).collect(),
                            next_before_sequence: page.next_before_sequence,
                        })
                        .map_err(|error| format!("Emily history query failed: {error}"));
                    if result.is_err() {
                        health
                            .history_query_error
                            .fetch_add(1, AtomicOrdering::Relaxed);
                    }
                    let _ = response_tx.send(result);
                }
                BridgeCommand::QueryContext {
                    session_id,
                    query_text,
                    top_k,
                    response_tx,
                } => {
                    let result = emily_runtime
                        .query_context(ContextQuery {
                            stream_id: Some(stream_id(session_id)),
                            query_text,
                            top_k,
                            neighbor_depth: 1,
                        })
                        .await
                        .map_err(|error| format!("Emily context query failed: {error}"));
                    if result.is_err() {
                        health
                            .context_query_error
                            .fetch_add(1, AtomicOrdering::Relaxed);
                    }
                    let _ = response_tx.send(result);
                }
                BridgeCommand::IngestSnippet {
                    request,
                    response_tx,
                } => {
                    let result =
                        ingest_snippet_object(&emily_runtime, &mut sequence_by_stream, request)
                            .await;
                    let _ = response_tx.send(result);
                }
                BridgeCommand::CreateEpisode {
                    request,
                    response_tx,
                } => {
                    let result = emily_runtime
                        .create_episode(request)
                        .await
                        .map_err(|error| format!("Emily create episode failed: {error}"));
                    let _ = response_tx.send(result);
                }
                BridgeCommand::LinkTextToEpisode {
                    request,
                    response_tx,
                } => {
                    let result = emily_runtime
                        .link_text_to_episode(request)
                        .await
                        .map_err(|error| format!("Emily trace-link failed: {error}"));
                    let _ = response_tx.send(result);
                }
                BridgeCommand::Episode {
                    episode_id,
                    response_tx,
                } => {
                    let result = emily_runtime
                        .episode(&episode_id)
                        .await
                        .map_err(|error| format!("Emily episode read failed: {error}"));
                    let _ = response_tx.send(result);
                }
                BridgeCommand::LatestEarlEvaluation {
                    episode_id,
                    response_tx,
                } => {
                    let result = emily_runtime
                        .latest_earl_evaluation_for_episode(&episode_id)
                        .await
                        .map_err(|error| format!("Emily latest EARL read failed: {error}"));
                    let _ = response_tx.send(result);
                }
                BridgeCommand::UpdateVectorizationConfig { patch, response_tx } => {
                    let result = emily_runtime
                        .update_vectorization_config(patch)
                        .await
                        .map_err(|error| {
                            format!("Emily vectorization config update failed: {error}")
                        });
                    let _ = response_tx.send(result);
                }
                BridgeCommand::StartBackfill {
                    request,
                    response_tx,
                } => {
                    let result = emily_runtime
                        .start_backfill(request)
                        .await
                        .map_err(|error| format!("Emily start_backfill failed: {error}"));
                    let _ = response_tx.send(result);
                }
                BridgeCommand::StartRevectorize {
                    request,
                    response_tx,
                } => {
                    let result = emily_runtime
                        .start_revectorize(request)
                        .await
                        .map_err(|error| format!("Emily start_revectorize failed: {error}"));
                    let _ = response_tx.send(result);
                }
                BridgeCommand::CancelVectorizationJob {
                    job_id,
                    response_tx,
                } => {
                    let result = emily_runtime
                        .cancel_vectorization_job(&job_id)
                        .await
                        .map_err(|error| format!("Emily cancel_vectorization_job failed: {error}"));
                    let _ = response_tx.send(result);
                }
                BridgeCommand::Shutdown => {
                    let _ = emily_runtime.close_db().await;
                    break;
                }
            }
        }
    });
}

async fn ingest_line(
    emily_runtime: &Arc<EmilyRuntime<SurrealEmilyStore>>,
    sequence_by_stream: &mut HashMap<String, u64>,
    session_id: SessionId,
    cwd: String,
    line: String,
    ts_unix_ms: i64,
    object_kind: TextObjectKind,
) -> Result<(), String> {
    let _ = ingest_object(
        emily_runtime,
        sequence_by_stream,
        stream_id(session_id),
        "terminal".to_string(),
        object_kind,
        ts_unix_ms,
        line,
        json!({ "cwd": cwd }),
    )
    .await?;
    Ok(())
}

async fn ingest_snippet_object(
    emily_runtime: &Arc<EmilyRuntime<SurrealEmilyStore>>,
    sequence_by_stream: &mut HashMap<String, u64>,
    request: SnippetIngestRequest,
) -> Result<SnippetIngestResult, String> {
    let object = ingest_object(
        emily_runtime,
        sequence_by_stream,
        snippet_stream_id(),
        "snippet".to_string(),
        TextObjectKind::Note,
        request.ts_unix_ms,
        request.text,
        json!({
            "snippet_id": request.snippet_id,
            "source_session_id": request.source_session_id,
            "source_stream_id": request.source_stream_id,
            "source_cwd": request.source_cwd,
            "source_start_offset": request.source_start_offset,
            "source_end_offset": request.source_end_offset,
            "source_start_row": request.source_start_row,
            "source_end_row": request.source_end_row,
        }),
    )
    .await?;
    let vectorization = emily_runtime
        .vectorization_status()
        .await
        .map_err(|error| format!("Emily vectorization status query failed: {error}"))?;
    let profile = if vectorization.config.enabled && vectorization.provider_available {
        Some(vectorization.config.profile_id)
    } else {
        None
    };
    let dimensions = if vectorization.config.enabled && vectorization.provider_available {
        Some(vectorization.config.expected_dimensions)
    } else {
        None
    };
    Ok(SnippetIngestResult {
        object_id: object.id,
        embedding_profile_id: profile,
        embedding_dimensions: dimensions,
    })
}

async fn ingest_object(
    emily_runtime: &Arc<EmilyRuntime<SurrealEmilyStore>>,
    sequence_by_stream: &mut HashMap<String, u64>,
    stream_id: String,
    source_kind: String,
    object_kind: TextObjectKind,
    ts_unix_ms: i64,
    text: String,
    metadata: serde_json::Value,
) -> Result<emily::model::TextObject, String> {
    let cached_sequence = sequence_by_stream.get(&stream_id).copied();
    let latest_sequence = match cached_sequence {
        Some(sequence) => Ok(sequence),
        None => latest_sequence_for_stream(emily_runtime, &stream_id).await,
    };
    let base_sequence = base_sequence_for_stream(sequence_by_stream, &stream_id, latest_sequence)?;
    let next_sequence = base_sequence.saturating_add(1);
    let object = emily_runtime
        .ingest_text(IngestTextRequest {
            stream_id: stream_id.clone(),
            source_kind,
            object_kind,
            sequence: next_sequence,
            ts_unix_ms,
            text,
            metadata,
        })
        .await
        .map_err(|error| format!("Emily ingest failed: {error}"))?;
    sequence_by_stream.insert(stream_id, next_sequence);
    Ok(object)
}

fn base_sequence_for_stream(
    sequence_by_stream: &HashMap<String, u64>,
    stream_id: &str,
    latest_sequence: Result<u64, String>,
) -> Result<u64, String> {
    if let Some(sequence) = sequence_by_stream.get(stream_id).copied() {
        return Ok(sequence);
    }

    latest_sequence
}

async fn latest_sequence_for_stream(
    emily_runtime: &Arc<EmilyRuntime<SurrealEmilyStore>>,
    stream_id: &str,
) -> Result<u64, String> {
    emily_runtime
        .page_history_before(HistoryPageRequest {
            stream_id: stream_id.to_string(),
            before_sequence: None,
            limit: 1,
        })
        .await
        .map(|page| page.items.first().map(|item| item.sequence).unwrap_or(0))
        .map_err(|error| format!("Emily seed sequence query failed: {error}"))
}

fn stream_id(session_id: SessionId) -> String {
    format!("terminal:{session_id}")
}

fn snippet_stream_id() -> String {
    "snippet:global".to_string()
}

fn fallback_provider_status(
    provider_available: bool,
    provider_bootstrap_error: Option<&str>,
) -> Option<EmbeddingProviderStatus> {
    if provider_available {
        return None;
    }
    let Some(error) = provider_bootstrap_error else {
        return None;
    };
    Some(EmbeddingProviderStatus {
        state: "unavailable".to_string(),
        session_id: None,
        queued_runs: None,
        queue_items: None,
        keep_alive: None,
        last_error: Some(error.to_string()),
    })
}

fn apply_provider_bootstrap_fallback(
    mut status: VectorizationStatus,
    provider_bootstrap_error: Option<&str>,
) -> VectorizationStatus {
    if status.provider_status.is_none() {
        status.provider_status =
            fallback_provider_status(status.provider_available, provider_bootstrap_error);
    }
    status
}

fn default_storage_path() -> PathBuf {
    if let Ok(value) = std::env::var("GESTALT_EMILY_DB_PATH") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .map(|home| home.join(".local/share/gestalt/emily"))
        .unwrap_or_else(|| std::env::temp_dir().join("gestalt-emily"))
}

#[cfg(test)]
mod tests {
    use super::{
        BridgeCommand, EmilyBridge, apply_provider_bootstrap_fallback, base_sequence_for_stream,
        fallback_provider_status, snippet_stream_id,
    };
    use emily::model::{DatabaseLocator, VectorizationConfig, VectorizationStatus};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn unique_locator(name: &str) -> DatabaseLocator {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        DatabaseLocator {
            storage_path: std::env::temp_dir().join(format!("gestalt-{name}-{nonce}")),
            namespace: "gestalt_test".to_string(),
            database: "default".to_string(),
        }
    }

    fn remove_storage_path(path: &PathBuf) {
        let _ = std::fs::remove_dir_all(path);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn base_sequence_prefers_cached_value() {
        let mut sequence_by_stream = HashMap::new();
        sequence_by_stream.insert("terminal:1".to_string(), 42);

        let sequence = base_sequence_for_stream(
            &sequence_by_stream,
            "terminal:1",
            Err("should not be used".to_string()),
        )
        .expect("base sequence should resolve");
        assert_eq!(sequence, 42);
    }

    #[test]
    fn base_sequence_propagates_seed_query_error() {
        let sequence_by_stream = HashMap::new();

        let error = base_sequence_for_stream(
            &sequence_by_stream,
            "terminal:9",
            Err("seed failure".to_string()),
        )
        .expect_err("seed errors must propagate");
        assert_eq!(error, "seed failure");
    }

    #[test]
    fn fallback_provider_status_includes_bootstrap_error_when_unavailable() {
        let status = fallback_provider_status(false, Some("workflow bootstrap failed"))
            .expect("fallback status should be present");
        assert_eq!(status.state, "unavailable");
        assert_eq!(
            status.last_error.as_deref(),
            Some("workflow bootstrap failed")
        );
    }

    #[test]
    fn apply_provider_bootstrap_fallback_keeps_existing_provider_status() {
        let status = VectorizationStatus {
            config: VectorizationConfig::default(),
            provider_available: false,
            provider_status: fallback_provider_status(false, Some("original")),
            active_job: None,
            last_job: None,
        };
        let updated = apply_provider_bootstrap_fallback(status, Some("replacement"));
        assert_eq!(
            updated.provider_status.unwrap().last_error.as_deref(),
            Some("original")
        );
    }

    #[test]
    fn new_bridge_surfaces_provider_bootstrap_error_in_status() {
        let locator = unique_locator("emily-bridge-provider-error");
        let storage_path = locator.storage_path.clone();
        let bridge = EmilyBridge::with_embedding_provider_bootstrap_error(
            locator,
            None,
            Some("workflow bootstrap failed".to_string()),
        );

        let status = bridge.vectorization_status();
        assert!(!status.provider_available);
        assert_eq!(
            status
                .provider_status
                .as_ref()
                .and_then(|provider| provider.last_error.as_deref()),
            Some("workflow bootstrap failed")
        );

        drop(bridge);
        thread::sleep(Duration::from_millis(50));
        remove_storage_path(&storage_path);
    }

    #[test]
    fn bridge_requests_fail_cleanly_after_worker_shutdown() {
        let locator = unique_locator("emily-bridge-shutdown");
        let storage_path = locator.storage_path.clone();
        let bridge = EmilyBridge::new(locator);

        let _ = bridge.command_tx.send(BridgeCommand::Shutdown);
        thread::sleep(Duration::from_millis(50));

        let history_error = bridge
            .page_history_before(1, None, 5)
            .expect_err("history query should fail after shutdown");
        assert!(
            history_error.contains("failed sending history request")
                || history_error.contains("failed receiving history response")
        );

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime should build");
        let context_error = runtime
            .block_on(bridge.query_context_async(1, "hello".to_string(), 3))
            .expect_err("context query should fail after shutdown");
        assert!(
            context_error.contains("failed sending context request")
                || context_error.contains("failed receiving context response")
        );

        let recent = bridge.recent_history(1, 5);
        assert!(recent.lines.is_empty());
        assert!(recent.next_before_sequence.is_none());

        drop(bridge);
        thread::sleep(Duration::from_millis(50));
        remove_storage_path(&storage_path);
    }

    #[test]
    fn snippet_stream_id_is_global_and_stable() {
        assert_eq!(snippet_stream_id(), "snippet:global");
    }
}
