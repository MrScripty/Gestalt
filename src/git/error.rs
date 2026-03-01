use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum GitError {
    #[error("No Git repository found at '{path}'.")]
    NotRepo { path: String },

    #[error("Git command failed ({command}) with code {code:?}: {stderr}")]
    CommandFailed {
        command: String,
        code: Option<i32>,
        stderr: String,
    },

    #[error("Failed to parse git output ({command}): {details}")]
    ParseError { command: String, details: String },

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("I/O error while running git command: {details}")]
    Io { details: String },
}
