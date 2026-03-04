use crate::state::SessionId;
use crate::terminal::TerminalMemorySink;
use emily::api::EmilyApi;
use emily::inference::EmbeddingProvider;
use emily::model::{
    ContextPacket, ContextQuery, DatabaseLocator, HistoryPageRequest, IngestTextRequest,
    TextObjectKind,
};
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::mpsc;
use std::thread;

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
        response_tx: mpsc::Sender<Result<HistoryChunk, String>>,
    },
    QueryContext {
        session_id: SessionId,
        query_text: String,
        top_k: usize,
        response_tx: mpsc::Sender<Result<ContextPacket, String>>,
    },
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct HistoryChunk {
    pub lines: Vec<String>,
    pub next_before_sequence: Option<u64>,
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
    command_tx: mpsc::Sender<BridgeCommand>,
    health: Arc<BridgeHealthCounters>,
}

impl EmilyBridge {
    /// Starts Emily runtime worker with a default local database locator.
    pub fn new_default() -> Self {
        let locator = Self::default_locator();
        Self::new(locator)
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
        let (command_tx, command_rx) = mpsc::channel::<BridgeCommand>();
        let health = Arc::new(BridgeHealthCounters::default());
        let worker_health = Arc::clone(&health);

        thread::spawn(move || {
            run_worker(locator, embedding_provider, command_rx, worker_health);
        });

        Self { command_tx, health }
    }

    pub fn page_history_before(
        &self,
        session_id: SessionId,
        before_sequence: Option<u64>,
        limit: usize,
    ) -> Result<HistoryChunk, String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.command_tx
            .send(BridgeCommand::PageHistory {
                session_id,
                before_sequence,
                limit,
                response_tx,
            })
            .map_err(|error| format!("failed sending history request to Emily worker: {error}"))?;
        response_rx.recv().map_err(|error| {
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
        let (response_tx, response_rx) = mpsc::channel();
        self.command_tx
            .send(BridgeCommand::QueryContext {
                session_id,
                query_text,
                top_k,
                response_tx,
            })
            .map_err(|error| format!("failed sending context request to Emily worker: {error}"))?;
        response_rx.recv().map_err(|error| {
            format!("failed receiving context response from Emily worker: {error}")
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
    command_rx: mpsc::Receiver<BridgeCommand>,
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

        let mut sequence_by_stream = HashMap::<String, u64>::new();

        while let Ok(command) = command_rx.recv() {
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
    let stream_id = stream_id(session_id);
    let cached_sequence = sequence_by_stream.get(&stream_id).copied();
    let latest_sequence = match cached_sequence {
        Some(sequence) => Ok(sequence),
        None => latest_sequence_for_stream(emily_runtime, &stream_id).await,
    };
    let base_sequence = base_sequence_for_stream(sequence_by_stream, &stream_id, latest_sequence)?;
    let next_sequence = base_sequence.saturating_add(1);

    emily_runtime
        .ingest_text(IngestTextRequest {
            stream_id: stream_id.clone(),
            source_kind: "terminal".to_string(),
            object_kind,
            sequence: next_sequence,
            ts_unix_ms,
            text: line,
            metadata: json!({ "cwd": cwd }),
        })
        .await
        .map_err(|error| format!("Emily ingest failed: {error}"))?;

    sequence_by_stream.insert(stream_id, next_sequence);

    Ok(())
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
    use super::base_sequence_for_stream;
    use std::collections::HashMap;

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
}
