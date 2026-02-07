use thiserror::Error;

/// Error types for fermi-memory operations
#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Consolidation error: {0}")]
    Consolidation(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("UUID parsing error: {0}")]
    UuidParse(#[from] uuid::Error),
}

pub type Result<T> = std::result::Result<T, MemoryError>;
