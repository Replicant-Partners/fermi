use thiserror::Error;

#[derive(Debug, Error)]
pub enum SimOpsError {
    #[error("Insufficient training data: need at least {need} samples, have {have}")]
    InsufficientData { need: usize, have: usize },

    #[error("Feature '{0}' is missing from the observation")]
    MissingFeature(String),

    #[error("Matrix is singular — check for collinear features or zero-variance inputs")]
    SingularMatrix,

    #[error("Process validation failed: {0}")]
    InvalidProcess(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
