//! Errors raised by Plane C components.

use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum ObservabilityError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Embedding mismatch: {0}")]
    Embedding(String),

    #[error("Invalid input: {0}")]
    Invalid(String),

    /// The component had nothing to do for this input — analogous to
    /// `EvalError::Inapplicable`. Callers should treat this as a
    /// silent skip, not a failure.
    #[error("Inapplicable: {0}")]
    Inapplicable(String),
}

impl ObservabilityError {
    pub fn is_inapplicable(&self) -> bool {
        matches!(self, ObservabilityError::Inapplicable(_))
    }
}
