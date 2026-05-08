//! Convenience re-exports for downstream callers.

pub use crate::aggregator::{AggregatedSignal, Aggregator, ConflictFlag, DimensionAggregate};
pub use crate::error::EvalError;
pub use crate::judge::{JudgeOutcome, LlmJudge, LlmJudgeConfig, LlmJudgeEvaluator, NoopLlmJudge};
pub use crate::model::EvalModel;
pub use crate::registry::{EvaluatorRegistry, RegistryOutcome};
pub use crate::result::{Dimension, EvalFlag, EvalResult, RegistryResult};
pub use crate::scoring::{BrierEvaluator, BrierLookup, BrierObservation};
pub use crate::tier::EvalTier;
pub use agent_bestiary_memory::{
    AgentCardSnapshot, EpisodeBundle, Provenance, TranscriptRole, TranscriptTurn,
};
