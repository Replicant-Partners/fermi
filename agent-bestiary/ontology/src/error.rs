use thiserror::Error;

#[derive(Error, Debug)]
pub enum OntologyError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Git error: {0}")]
    GitError(#[from] git2::Error),

    #[error("Memory store error: {0}")]
    MemoryError(#[from] agent_bestiary_memory::error::MemoryError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Invalid configuration: {0}")]
    ConfigError(String),

    #[error("Mermaid generation error: {0}")]
    MermaidError(String),

    #[error("Git repository not found at: {0}")]
    RepoNotFound(String),

    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("No entities found for agent: {0}")]
    NoEntities(String),

    #[error("Ontology snapshot not found: {0}")]
    SnapshotNotFound(String),
}

pub type Result<T> = std::result::Result<T, OntologyError>;
