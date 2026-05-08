//! Evaluator registry and `EvalModel` trait — Plane B of the
//! Social Agent Observability Platform.
//!
//! See:
//! - [`docs/architecture/social_agent_observability_architecture.html`](../../../docs/architecture/social_agent_observability_architecture.html)
//! - [`docs/architecture/OBSERVABILITY_IMPL.md`](../../../docs/architecture/OBSERVABILITY_IMPL.md)
//! - [`docs/architecture/EVALUATOR_DESIGN.md`](../../../docs/architecture/EVALUATOR_DESIGN.md)
//!
//! ## Architecture
//!
//! ```text
//!   EpisodeBundle (from agent-bestiary-memory, Phase 0)
//!         │
//!         ▼
//!   ┌──────────────────────────────────────────────────────┐
//!   │                EvaluatorRegistry                     │
//!   │                                                      │
//!   │   tier = PreFilter   →  serial, can short-circuit    │
//!   │   tier = Dimensional →  parallel via tokio::join_all │
//!   └──────────────────────────────────────────────────────┘
//!         │
//!         ▼
//!   ┌──────────────────────────────────────────────────────┐
//!   │                   Aggregator                         │
//!   │                                                      │
//!   │   per-dimension mean, per-evaluator scores,          │
//!   │   conflict detection (max - min > θ)                 │
//!   └──────────────────────────────────────────────────────┘
//!         │
//!         ▼
//!   AggregatedSignal { per_dim, conflicts, prefilter_block }
//! ```
//!
//! Phase 1 ships the trait, the registry, the aggregator, and two
//! reference implementations (LLM judge + Brier wrapper) so the shape
//! is exercised end-to-end. Phase 2 wires the registry into the
//! existing `src/handlers/eval.rs::run_eval_cases` pipeline so per-dim
//! scores and conflict flags become visible in real eval-run results.

pub mod aggregator;
pub mod error;
pub mod judge;
pub mod model;
pub mod prelude;
pub mod registry;
pub mod result;
pub mod scoring;
pub mod tier;

#[cfg(test)]
mod tests;

pub use aggregator::{AggregatedSignal, Aggregator, ConflictFlag, DimensionAggregate};
pub use error::EvalError;
pub use judge::{JudgeOutcome, LlmJudge, LlmJudgeConfig, LlmJudgeEvaluator, NoopLlmJudge};
pub use model::EvalModel;
pub use registry::{EvaluatorRegistry, RegistryOutcome};
pub use result::{Dimension, EvalFlag, EvalResult, RegistryResult};
pub use scoring::{BrierEvaluator, BrierLookup, BrierObservation};
pub use tier::EvalTier;

// Re-export the input contract from the memory crate so consumers don't
// need a second import line just to call into the registry.
pub use agent_bestiary_memory::{
    AgentCardSnapshot, EpisodeBundle, Provenance, TranscriptRole, TranscriptTurn,
};
