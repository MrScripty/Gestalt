use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrchestrationLogError {
    #[error("failed opening orchestration db {path}: {source}")]
    OpenDb {
        path: String,
        #[source]
        source: rusqlite::Error,
    },
    #[error("failed to derive orchestration db parent directory for {0}")]
    MissingParent(String),
    #[error("failed creating orchestration db directory {path}: {source}")]
    CreateDirectory {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed ensuring orchestration schema: {0}")]
    EnsureSchema(rusqlite::Error),
    #[error("failed opening orchestration transaction: {0}")]
    BeginTransaction(rusqlite::Error),
    #[error("orchestration command {0} already exists")]
    DuplicateCommandId(String),
    #[error("orchestration command {0} does not exist")]
    MissingCommand(String),
    #[error("orchestration receipt for command {0} already exists")]
    ReceiptAlreadyFinalized(String),
    #[error("failed serializing orchestration payload: {0}")]
    SerializePayload(serde_json::Error),
    #[error("failed deserializing orchestration payload: {0}")]
    DeserializePayload(serde_json::Error),
    #[error("failed inserting orchestration command {command_id}: {source}")]
    InsertCommand {
        command_id: String,
        #[source]
        source: rusqlite::Error,
    },
    #[error("failed inserting orchestration event for command {command_id}: {source}")]
    InsertEvent {
        command_id: String,
        #[source]
        source: rusqlite::Error,
    },
    #[error("failed inserting orchestration receipt for command {command_id}: {source}")]
    InsertReceipt {
        command_id: String,
        #[source]
        source: rusqlite::Error,
    },
    #[error("failed updating orchestration timeline {timeline_id}: {source}")]
    UpdateTimeline {
        timeline_id: String,
        #[source]
        source: rusqlite::Error,
    },
    #[error("failed querying orchestration data: {0}")]
    Query(rusqlite::Error),
    #[error("failed decoding orchestration row: {0}")]
    DecodeRow(rusqlite::Error),
    #[error("failed committing orchestration transaction: {0}")]
    CommitTransaction(rusqlite::Error),
}
