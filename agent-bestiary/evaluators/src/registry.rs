//! `EvaluatorRegistry` — runs pre-filters serially (with optional
//! short-circuit on hard failure) and dimensional evaluators in
//! parallel.
//!
//! Architecture:
//!
//! ```text
//!   bundle ─► pre-filters (serial, ordered) ─► dimensional (parallel)
//!                       │
//!                       └─ short-circuit if any pre-filter returns a
//!                          score below `prefilter_block_threshold` on
//!                          its primary dimension. The registry still
//!                          returns the partial result so callers can
//!                          surface "blocked by safety filter."
//! ```

use std::sync::Arc;
use std::time::Instant;

use crate::aggregator::{AggregatedSignal, Aggregator};
use crate::model::EvalModel;
use crate::result::RegistryResult;
use crate::tier::EvalTier;
use agent_bestiary_memory::EpisodeBundle;

/// What the registry returns for a full run. In-process value — for
/// persistence/wire transmission, callers should serialize the
/// embedded `AggregatedSignal` and the `EvalResult`s (both
/// `Serialize`).
#[derive(Debug, Clone)]
pub struct RegistryOutcome {
    /// Per-evaluator results in order: pre-filters first (in
    /// registration order), then dimensional (also in registration
    /// order, but produced concurrently).
    pub results: Vec<RegistryResult>,
    /// Aggregated signal computed by the [`Aggregator`].
    pub signal: AggregatedSignal,
    /// True when a pre-filter triggered a short-circuit. Dimensional
    /// evaluators are skipped in that case; the partial signal is
    /// still computed from whatever has run so far.
    pub prefilter_blocked: bool,
    /// Total wall-clock time of the run.
    pub total_latency_ms: u64,
}

impl RegistryOutcome {
    pub fn is_blocked(&self) -> bool {
        self.prefilter_blocked
    }
}

/// Registry that owns a list of evaluators and runs them against a
/// bundle. Evaluators are stored as `Arc<dyn EvalModel>` so the same
/// registry instance can be cloned across tasks.
#[derive(Clone)]
pub struct EvaluatorRegistry {
    pre_filters: Vec<Arc<dyn EvalModel>>,
    dimensional: Vec<Arc<dyn EvalModel>>,
    aggregator: Aggregator,
    /// When a pre-filter scores its primary dimension below this
    /// threshold, dimensional evaluators are skipped. Set to `None`
    /// to disable short-circuiting (every evaluator always runs).
    prefilter_block_threshold: Option<f64>,
}

impl Default for EvaluatorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl EvaluatorRegistry {
    pub fn new() -> Self {
        Self {
            pre_filters: Vec::new(),
            dimensional: Vec::new(),
            aggregator: Aggregator::default(),
            prefilter_block_threshold: Some(0.5),
        }
    }

    /// Register an evaluator. Tier is read from `evaluator.tier()`;
    /// pre-filters are appended in registration order, dimensional
    /// likewise (order doesn't affect parallel execution semantics
    /// but does affect deterministic test output).
    pub fn register(&mut self, evaluator: Arc<dyn EvalModel>) -> &mut Self {
        match evaluator.tier() {
            EvalTier::PreFilter => self.pre_filters.push(evaluator),
            EvalTier::Dimensional => self.dimensional.push(evaluator),
        }
        self
    }

    /// Override the default conflict threshold (0.20).
    pub fn with_conflict_threshold(mut self, threshold: f64) -> Self {
        self.aggregator = Aggregator::new(threshold);
        self
    }

    /// Override the pre-filter short-circuit threshold. `None` ⇒
    /// no short-circuit; every evaluator always runs.
    pub fn with_prefilter_block_threshold(mut self, threshold: Option<f64>) -> Self {
        self.prefilter_block_threshold = threshold;
        self
    }

    pub fn pre_filter_count(&self) -> usize {
        self.pre_filters.len()
    }

    pub fn dimensional_count(&self) -> usize {
        self.dimensional.len()
    }

    /// Run the registry against a bundle.
    pub async fn run(&self, bundle: &EpisodeBundle) -> RegistryOutcome {
        let start = Instant::now();
        let mut results: Vec<RegistryResult> = Vec::new();
        let mut blocked = false;

        // ── Pre-filters: serial, ordered, can short-circuit ──
        for ev in &self.pre_filters {
            let r = run_one(ev.as_ref(), bundle).await;
            let score_below_block = match &r.outcome {
                Ok(eval) => self
                    .prefilter_block_threshold
                    .map(|t| eval.dimension_scores.values().any(|s| *s < t))
                    .unwrap_or(false),
                _ => false,
            };
            results.push(r);
            if score_below_block {
                blocked = true;
                break;
            }
        }

        // ── Dimensional: parallel ──
        if !blocked {
            let futs: Vec<_> = self
                .dimensional
                .iter()
                .map(|ev| {
                    let ev = ev.clone();
                    async move { run_one(ev.as_ref(), bundle).await }
                })
                .collect();
            let parallel_results = futures::future::join_all(futs).await;
            results.extend(parallel_results);
        }

        let signal = self.aggregator.aggregate(&results);
        let total_latency_ms = start.elapsed().as_millis() as u64;

        RegistryOutcome {
            results,
            signal,
            prefilter_blocked: blocked,
            total_latency_ms,
        }
    }
}

/// Run a single evaluator with timing instrumentation.
async fn run_one(ev: &dyn EvalModel, bundle: &EpisodeBundle) -> RegistryResult {
    let start = Instant::now();
    let outcome = ev.evaluate(bundle).await;
    let latency_ms = start.elapsed().as_millis() as u64;
    let outcome = match outcome {
        Ok(mut eval) => {
            // Stamp evaluator-reported latency if it didn't.
            if eval.latency_ms == 0 {
                eval.latency_ms = latency_ms;
            }
            Ok(eval)
        }
        Err(e) => Err(e),
    };
    RegistryResult {
        evaluator_name: ev.name().to_string(),
        tier: ev.tier(),
        outcome,
        latency_ms,
    }
}
