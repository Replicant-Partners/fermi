use thiserror::Error;

pub type Result<T> = std::result::Result<T, MemoryError>;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Lock acquisition failed")]
    LockFailed,

    #[error("Lock unavailable: {0}")]
    LockUnavailable(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("External API error: {0}")]
    ExternalError(String),
}
