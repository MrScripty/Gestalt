use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("failed to create workspace directory {path:?}: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read workspace file {path:?}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write workspace temp file {path:?}: {source}")]
    WriteTempFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to flush workspace temp file {path:?}: {source}")]
    FlushTempFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to atomically replace workspace file {from:?} -> {to:?}: {source}")]
    AtomicRename {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize workspace payload: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("unsupported workspace schema version {version}")]
    UnsupportedSchemaVersion { version: u32 },
}
