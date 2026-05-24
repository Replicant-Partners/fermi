use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProjectionError {
    #[error("Unknown executor kind: '{0}'. Register it with ExecutorRegistry::register().")]
    UnknownExecutor(String),

    #[error("Executor '{kind}' failed on run {run_index}: {message}")]
    ExecutorFailed {
        kind: String,
        run_index: usize,
        message: String,
    },

    #[error("Invalid sweep config: {0}")]
    InvalidSweep(String),

    #[error("Variable path '{path}' not found in model config: {message}")]
    VariableNotFound { path: String, message: String },

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
