//! The `EvalModel` trait — the single contract every evaluator
//! implements.
//!
//! See `docs/architecture/EVALUATOR_DESIGN.md` for the per-evaluator
//! design proposals (Sotopia, LifelongBench, CharacterEval, WildGuard,
//! Faithfulness — all native Rust).

use async_trait::async_trait;

use crate::result::{Dimension, EvalResult};
use crate::tier::EvalTier;
use crate::EvalError;
use agent_bestiary_memory::EpisodeBundle;

/// One evaluator. Stateless or wraps its own configuration / providers.
///
/// `evaluate` returns `Result<EvalResult, EvalError>`; an
/// `EvalError::Inapplicable` is the canonical "this bundle doesn't
/// carry what I need, please skip me" return.
#[async_trait]
pub trait EvalModel: Send + Sync {
    /// Stable identifier for this evaluator. Matches the value stored
    /// on `eval_signals.evaluator_name` (Phase 2 table).
    fn name(&self) -> &'static str;

    /// Bumps when prompts / weights / version change so trend analysis
    /// can split before / after.
    fn version(&self) -> &'static str;

    /// Pre-filter (serial, can short-circuit) or dimensional
    /// (parallel).
    fn tier(&self) -> EvalTier;

    /// Dimensions this evaluator scores. The aggregator uses this
    /// list to know which keys to expect; missing dimensions in the
    /// `EvalResult` are silently ignored, but evaluators are expected
    /// to populate every dimension they declare.
    fn dimensions(&self) -> Vec<Dimension>;

    /// Score the bundle. Side-effect-free except for outbound
    /// provider I/O (LLM, classifier, db read).
    async fn evaluate(&self, bundle: &EpisodeBundle) -> Result<EvalResult, EvalError>;
}
