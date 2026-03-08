use crate::git::GitError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RunCheckpointError {
    #[error("Run checkpoint database path '{0}' has no parent directory.")]
    MissingParent(String),

    #[error("Failed creating run checkpoint directory '{path}': {source}")]
    CreateDirectory {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed opening run checkpoint database '{path}': {source}")]
    OpenDb {
        path: String,
        #[source]
        source: rusqlite::Error,
    },

    #[error("Failed ensuring run checkpoint schema: {0}")]
    EnsureSchema(#[source] rusqlite::Error),

    #[error("Run checkpoint query failed: {0}")]
    Query(#[source] rusqlite::Error),

    #[error("Failed decoding run checkpoint row: {0}")]
    DecodeRow(#[source] rusqlite::Error),

    #[error("Run checkpoint {0} already exists.")]
    DuplicateRunId(String),

    #[error("Failed serializing run checkpoint payload: {0}")]
    SerializePayload(#[from] serde_json::Error),

    #[error("Invalid run checkpoint data: {0}")]
    InvalidData(String),

    #[error(transparent)]
    Git(#[from] GitError),
}
