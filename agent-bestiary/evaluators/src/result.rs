//! `EvalResult` — what every evaluator returns; `RegistryResult` —
//! the per-evaluator wrapper the registry produces.
//!
//! Dimensions are stored as `String` to keep the trait open: the
//! architecture-doc mock lists `goal_completion`, `social_capital`,
//! `rapport`, `persona_consistency`, `retention`, `persona_fidelity`,
//! `value_alignment`, `forecast_calibration`, but we don't want a
//! closed enum here — each evaluator declares its own dimensions.
//!
//! The `Dimension` newtype gives us cheap nominal typing while staying
//! string-shaped for storage / wire formats.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::EvalError;
use crate::EvalTier;

/// Stable name of a scoring dimension. Stored as a string so each
/// evaluator can declare its own without a central registry of
/// dimensions; the aggregator merges by name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Dimension(pub String);

impl Dimension {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Dimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Dimension {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for Dimension {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Free-form flag attached to an evaluator output. Examples:
/// `safety:violence`, `goal:partial`, `groundedness:contradicted`.
///
/// The kind is a stable category; the value is the specific instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EvalFlag {
    pub kind: String,
    pub value: String,
}

impl EvalFlag {
    pub fn new(kind: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            value: value.into(),
        }
    }
}

/// What a single `EvalModel` returns when it scores a bundle.
///
/// Per-dimension scores are clipped to `[0.0, 1.0]` before the result
/// reaches the aggregator. `confidence` is the evaluator's
/// self-reported confidence in its own output; the aggregator uses it
/// to weight conflicts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    /// Stable identifier matching `EvalModel::name()`.
    pub evaluator_name: String,
    /// Bumps when an evaluator's prompt / weights / version change
    /// (see EVALUATOR_DESIGN.md Q-CC4). Used by Phase 3 trend
    /// analyser to split before/after on prompt changes.
    pub evaluator_version: String,
    /// Per-dimension scores in `[0.0, 1.0]`.
    pub dimension_scores: HashMap<Dimension, f64>,
    /// Free-form flags (e.g. `safety:violence`, `groundedness:contradicted`).
    #[serde(default)]
    pub flags: Vec<EvalFlag>,
    /// Optional one-line rationale, useful for HITL review surfaces.
    #[serde(default)]
    pub rationale: Option<String>,
    /// Self-reported confidence in this evaluation in `[0.0, 1.0]`.
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    /// Wall-clock execution time of the evaluator's `evaluate()` call.
    #[serde(default)]
    pub latency_ms: u64,
    /// Optional cost in credits (for budget tracking).
    #[serde(default)]
    pub cost_credits: i32,
    /// The model identifier used by an LLM-backed evaluator, if any.
    #[serde(default)]
    pub model_used: Option<String>,
}

fn default_confidence() -> f64 {
    1.0
}

impl EvalResult {
    /// Construct a fresh result for evaluator `name@version` with no
    /// dimensions yet — caller adds them with [`Self::with_score`].
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            evaluator_name: name.into(),
            evaluator_version: version.into(),
            dimension_scores: HashMap::new(),
            flags: Vec::new(),
            rationale: None,
            confidence: 1.0,
            latency_ms: 0,
            cost_credits: 0,
            model_used: None,
        }
    }

    /// Builder helper. Score is clipped into `[0.0, 1.0]` to make
    /// callers' lives easy.
    pub fn with_score(mut self, dimension: impl Into<Dimension>, score: f64) -> Self {
        let s = score.clamp(0.0, 1.0);
        self.dimension_scores.insert(dimension.into(), s);
        self
    }

    pub fn with_flag(mut self, flag: EvalFlag) -> Self {
        self.flags.push(flag);
        self
    }

    pub fn with_rationale(mut self, rationale: impl Into<String>) -> Self {
        self.rationale = Some(rationale.into());
        self
    }

    pub fn with_confidence(mut self, conf: f64) -> Self {
        self.confidence = conf.clamp(0.0, 1.0);
        self
    }

    pub fn with_latency_ms(mut self, ms: u64) -> Self {
        self.latency_ms = ms;
        self
    }

    pub fn with_cost(mut self, credits: i32) -> Self {
        self.cost_credits = credits;
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model_used = Some(model.into());
        self
    }
}

/// What the registry produces for a single evaluator after running it.
/// The registry never fails the whole run on a single evaluator's
/// failure — instead it captures the outcome here and lets the
/// aggregator skip failed evaluators.
///
/// **Not serializable.** This is an in-process orchestration value;
/// for storage / wire formats use the aggregator's `AggregatedSignal`
/// and the per-evaluator `EvalResult` (both `Serialize`).
#[derive(Debug, Clone)]
pub struct RegistryResult {
    pub evaluator_name: String,
    pub tier: EvalTier,
    /// `Ok(EvalResult)` on success; `Err(EvalError)` for failure or
    /// inapplicability.
    pub outcome: Result<EvalResult, EvalError>,
    /// Wall-clock latency for this evaluator.
    pub latency_ms: u64,
}

impl RegistryResult {
    /// True when the evaluator ran successfully and contributed
    /// dimension scores.
    pub fn is_success(&self) -> bool {
        self.outcome.is_ok()
    }

    /// True when the evaluator opted out (inapplicable) — callers
    /// should treat these as "skip" rather than "failure."
    pub fn is_inapplicable(&self) -> bool {
        match &self.outcome {
            Err(e) => e.is_inapplicable(),
            _ => false,
        }
    }
}
