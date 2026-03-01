use crate::state::SessionId;
use crate::terminal::TerminalMemorySink;
use emily::api::EmilyApi;
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

/// Gestalt-side adapter that feeds terminal text into Emily and exposes history queries.
pub struct EmilyBridge {
    command_tx: mpsc::Sender<BridgeCommand>,
}

impl EmilyBridge {
    /// Starts Emily runtime worker with a default local database locator.
    pub fn new_default() -> Self {
        let locator = Self::default_locator();
        Self::new(locator)
    }

    /// Starts Emily runtime worker with an explicit database locator.
    pub fn new(locator: DatabaseLocator) -> Self {
        let (command_tx, command_rx) = mpsc::channel::<BridgeCommand>();

        thread::spawn(move || {
            run_worker(locator, command_rx);
        });

        Self { command_tx }
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
        self.page_history_before(session_id, None, limit)
            .map(|chunk| {
                // API returns newest-first for efficient paging. Terminal restore expects oldest-first.
                chunk.lines.into_iter().rev().collect::<Vec<_>>()
            })
            .unwrap_or_default()
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

fn run_worker(locator: DatabaseLocator, command_rx: mpsc::Receiver<BridgeCommand>) {
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
        let emily_runtime = Arc::new(EmilyRuntime::new(store));

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
                    let _ = ingest_line(
                        &emily_runtime,
                        &mut sequence_by_stream,
                        session_id,
                        cwd,
                        line,
                        ts_unix_ms,
                        TextObjectKind::UserInput,
                    )
                    .await;
                }
                BridgeCommand::IngestOutput {
                    session_id,
                    cwd,
                    line,
                    ts_unix_ms,
                } => {
                    let _ = ingest_line(
                        &emily_runtime,
                        &mut sequence_by_stream,
                        session_id,
                        cwd,
                        line,
                        ts_unix_ms,
                        TextObjectKind::SystemOutput,
                    )
                    .await;
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
    let next_sequence = sequence_by_stream.entry(stream_id.clone()).or_insert(0);
    *next_sequence = next_sequence.saturating_add(1);

    emily_runtime
        .ingest_text(IngestTextRequest {
            stream_id,
            source_kind: "terminal".to_string(),
            object_kind,
            sequence: *next_sequence,
            ts_unix_ms,
            text: line,
            metadata: json!({ "cwd": cwd }),
        })
        .await
        .map_err(|error| format!("Emily ingest failed: {error}"))?;

    Ok(())
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
