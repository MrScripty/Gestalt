use thiserror::Error;

/// Top-level Emily errors exposed through the public API.
#[derive(Debug, Error)]
pub enum EmilyError {
    #[error("database is not open")]
    DatabaseNotOpen,
    #[error("database locator is invalid: {0}")]
    InvalidDatabaseLocator(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("store error: {0}")]
    Store(String),
    #[error("embedding provider error: {0}")]
    Embedding(String),
    #[error("internal runtime error: {0}")]
    Runtime(String),
}
