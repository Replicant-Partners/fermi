//! Evaluator tier — pre-filter or dimensional.
//!
//! Pre-filters run **first**, **serially**, and may short-circuit the
//! entire registry. WildGuard (safety) and faithfulness are pre-filters.
//!
//! Dimensional evaluators run **in parallel** via `tokio::join_all` and
//! contribute per-dimension scores to the aggregator. Sotopia,
//! LifelongBench, CharacterEval, and the existing LLM judge / Brier
//! wrappers are dimensional.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalTier {
    /// Cheap, runs first, can short-circuit on a hard failure.
    PreFilter,
    /// Parallel-execution evaluator that contributes dimensional
    /// scores to the aggregator.
    Dimensional,
}

impl EvalTier {
    pub fn is_pre_filter(self) -> bool {
        matches!(self, EvalTier::PreFilter)
    }

    pub fn is_dimensional(self) -> bool {
        matches!(self, EvalTier::Dimensional)
    }
}
