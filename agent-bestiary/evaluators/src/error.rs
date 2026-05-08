//! Errors raised by `EvalModel` implementations and the registry.
//!
//! The `Inapplicable` variant is the registry-friendly way for an
//! evaluator to say "this bundle doesn't carry the inputs I need" —
//! e.g. Sotopia without a `goal_spec`. `Inapplicable` is *not* an
//! error: the registry treats it as "skip me," not "evaluation failed."
//!
//! See `docs/architecture/EVALUATOR_DESIGN.md` Q-CC5.

use thiserror::Error;

/// `Clone` is derived because `RegistryResult` carries a
/// `Result<EvalResult, EvalError>` and the registry hands cloned
/// outcomes to multiple consumers (aggregator, signal store, HITL
/// surfaces) without re-running the evaluator.
#[derive(Debug, Clone, Error)]
pub enum EvalError {
    /// The evaluator did not run because the bundle lacks required
    /// inputs (e.g. no goal_spec for Sotopia, no transcript history
    /// for LifelongBench). Aggregator skips these silently.
    #[error("Inapplicable to this bundle: {0}")]
    Inapplicable(String),

    /// The evaluator's underlying provider (LLM, classifier, db query)
    /// failed.
    #[error("Provider error: {0}")]
    Provider(String),

    /// The evaluator returned a malformed response.
    #[error("Malformed response: {0}")]
    Malformed(String),

    /// Validation of inputs failed (e.g. dimension score out of range
    /// before the evaluator gets to clip it).
    #[error("Invalid input: {0}")]
    Invalid(String),

    /// A transient error worth retrying. The registry currently does
    /// not auto-retry but Phase 2 may.
    #[error("Transient error: {0}")]
    Transient(String),
}

impl EvalError {
    pub fn is_inapplicable(&self) -> bool {
        matches!(self, EvalError::Inapplicable(_))
    }
}
