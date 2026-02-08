use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProjectorError {
    #[error("No embeddings found for projection")]
    NoEmbeddings,

    #[error("Insufficient embeddings: need at least {needed}, got {got}")]
    InsufficientEmbeddings { needed: usize, got: usize },

    #[error("Invalid dimensions: {0} (must be 2 or 3)")]
    InvalidDimensions(u8),

    #[error("Database error: {0}")]
    Database(#[from] anyhow::Error),

    #[error("Projection failed: {0}")]
    ProjectionFailed(String),

    #[error("Agent not found: {0}")]
    AgentNotFound(String),
}

pub type Result<T> = std::result::Result<T, ProjectorError>;
