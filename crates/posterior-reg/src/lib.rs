//! # posterior-reg — BayesOps Phase 2
//!
//! **Domain-neutral Bayesian regression with a NUTS sampler.**
//!
//! Takes `(features, outcome)` pairs and produces a [`ConditionalPosterior`] that
//! answers four what-if questions:
//!
//! 1. **A — Conditional prediction.** `P(outcome | features_new)` as a
//!    [`posterior::FittedDistribution`] suitable for direct injection into FPL.
//! 2. **B — Input sensitivity.** Sobol-style first/total-order indices over the
//!    posterior predictive — which features drive outcome variance most?
//! 3. **C — Scenario comparison.** Two input configurations compared on the same
//!    posterior predictive: `P(A > B)`, expected gain, risk ratio.
//! 4. **D — Planning under constraint.** `P(outcome ≥ threshold | features)` and
//!    `optimise_for_target` to recommend the input that maximises that probability.
//!
//! ## Domain-neutrality (Spec 14 §9)
//!
//! This crate has **zero** awareness of SimOps, FPL, or any domain. It operates
//! on a generic `HashMap<String, f64>` feature map. Conversions from
//! domain-specific types (e.g. `simops::CascadeResult` → [`WeightedSample`])
//! live in the *caller's* crate.
//!
//! ## Two-loop separation
//!
//! Like `crates/posterior`, this crate is entirely Loop A (parameter fitting).
//! The output [`posterior::FittedDistribution`] feeds Loop B (executor.rs) as a
//! Driver distribution. The executor never knows whether the parameters came
//! from a human, a marginal fit, or a conditional fit.
//!
//! ## Async model
//!
//! `fit_conditional` is `async` because multi-chain HMC runs each chain in its
//! own `tokio::task::spawn_blocking`. Chains are independent, CPU-bound, and
//! synchronise only at the diagnostics-aggregation step. Posterior query methods
//! (`predict`, `input_sensitivity`, etc.) are synchronous — they operate on the
//! already-collected samples.

use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Public re-exports from posterior so callers get one import path ──────────
pub use posterior::{DataQuality, FitMetadata, FittedDistribution};

// ── Public modules ───────────────────────────────────────────────────────────
pub mod diagnostics;
pub mod improvement;
pub mod models;
pub mod sampler;
pub mod whatif;

pub use diagnostics::{compute_r_hat, effective_sample_size_per_chain, SamplerDiagnostics};
pub use improvement::{improvement_loop, ImprovementConfig, ModelTrace};
pub use models::{LinearNormal, RegressionModel};
pub use sampler::{NutsSampler, SamplerConfig};

// ═════════════════════════════════════════════════════════════════════════════
// CORE TYPES — Spec 14 §5.1
// ═════════════════════════════════════════════════════════════════════════════

/// A single training observation with features, outcome, and trust weight.
///
/// `weight = 1.0` for real observations; `weight ∈ [0.0, 0.3]` for synthetic /
/// cascade-augmented observations (Spec 14 §6).
///
/// `features` may carry additional metadata keys beyond those declared in
/// [`RegressionConfig::feature_names`] — extra keys are silently ignored;
/// missing keys produce [`RegressionError::MissingFeature`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeightedSample {
    pub features: HashMap<String, f64>,
    pub outcome: f64,
    pub weight: f64,
}

impl WeightedSample {
    /// Construct a real (weight = 1.0) sample.
    pub fn real(features: HashMap<String, f64>, outcome: f64) -> Self {
        Self {
            features,
            outcome,
            weight: 1.0,
        }
    }

    /// Construct a synthetic (weight = 0.2) sample.
    pub fn synthetic(features: HashMap<String, f64>, outcome: f64) -> Self {
        Self {
            features,
            outcome,
            weight: 0.2,
        }
    }

    /// Extract feature values in the declared order. Errors on missing keys.
    pub(crate) fn feature_vector(
        &self,
        feature_names: &[String],
    ) -> Result<Vec<f64>, RegressionError> {
        let mut out = Vec::with_capacity(feature_names.len());
        for name in feature_names {
            let v = self.features.get(name).copied().ok_or_else(|| {
                RegressionError::MissingFeature {
                    name: name.clone(),
                    sample: format!("{:?}", self),
                }
            })?;
            if !v.is_finite() {
                return Err(RegressionError::NonFiniteFeature {
                    name: name.clone(),
                });
            }
            out.push(v);
        }
        Ok(out)
    }
}

/// Configuration for a single `fit_conditional` call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionConfig {
    /// Fraction of `data` reserved for held-out NLPD computation.
    /// Default: `0.2`. Set to `0.0` to fit on all data (no NLPD).
    #[serde(default = "default_held_out")]
    pub held_out_fraction: f64,

    /// HMC sampler configuration.
    #[serde(default)]
    pub sampler: SamplerConfig,

    /// NLPD improvement loop configuration.
    #[serde(default)]
    pub improvement: ImprovementConfig,

    /// **The source of truth for feature column order.** Every
    /// [`WeightedSample::features`] map MUST contain every key in this vector.
    /// Extra keys are ignored.
    pub feature_names: Vec<String>,

    /// Top-level deterministic seed. Each chain derives its seed from this
    /// (`seed + chain_index`). `None` means use entropy.
    #[serde(default)]
    pub seed: Option<u64>,
}

fn default_held_out() -> f64 {
    0.2
}

impl RegressionConfig {
    /// Convenience: minimal config with just feature names.
    pub fn new(feature_names: Vec<String>) -> Self {
        Self {
            held_out_fraction: 0.2,
            sampler: SamplerConfig::default(),
            improvement: ImprovementConfig::default(),
            feature_names,
            seed: None,
        }
    }

    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    pub fn with_sampler(mut self, sampler: SamplerConfig) -> Self {
        self.sampler = sampler;
        self
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// THE CONDITIONAL POSTERIOR — Spec 14 §5.2
// ═════════════════════════════════════════════════════════════════════════════

/// The fitted result of a `fit_conditional` call.
///
/// Wraps the raw MCMC draws over model parameters and exposes the four what-if
/// query methods. All public fields are serializable for use across the HTTP /
/// MCP surfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalPosterior {
    /// MCMC samples: outer = sample index, inner = parameter index in
    /// `model.param_names()` order.
    pub samples: Vec<Vec<f64>>,

    /// Name of the winning model variant (e.g. "LinearNormal").
    /// We store the name rather than the trait object so the whole
    /// posterior round-trips through serde. `recover_model()` reconstructs
    /// the live trait object from the name + parameter names.
    pub model_name: String,

    /// Parameter names in the order the samples columns appear.
    pub param_names: Vec<String>,

    /// Feature names declared in the originating [`RegressionConfig`].
    pub feature_names: Vec<String>,

    pub diagnostics: SamplerDiagnostics,

    /// Negative log predictive density on held-out data. `None` if
    /// `held_out_fraction == 0.0`.
    pub nlpd: Option<f64>,

    pub metadata: FitMetadata,
}

impl ConditionalPosterior {
    /// Recover the live [`RegressionModel`] from the stored name.
    /// Returns an opaque `Box<dyn RegressionModel>` that supports `predict_mean`
    /// and `predict_std` — used internally by `predict()`, `input_sensitivity`, etc.
    pub fn recover_model(&self) -> Result<Box<dyn RegressionModel>, RegressionError> {
        models::recover(&self.model_name, &self.feature_names, &self.param_names)
    }

    /// **Use case A — conditional prediction.**
    /// Returns `P(outcome | features)` as a [`FittedDistribution`], directly
    /// pluggable into an FPL Driver via `.to_fpl_params()`.
    pub fn predict(
        &self,
        features: &HashMap<String, f64>,
    ) -> Result<FittedDistribution, RegressionError> {
        whatif::predict(self, features)
    }

    /// **Use case B — input sensitivity** (Sobol over posterior predictive).
    pub fn input_sensitivity(
        &self,
        feature_ranges: &HashMap<String, (f64, f64)>,
        n_samples: usize,
    ) -> Result<HashMap<String, whatif::InputSensitivity>, RegressionError> {
        whatif::input_sensitivity(self, feature_ranges, n_samples)
    }

    /// **Use case C — scenario comparison.**
    pub fn compare_scenarios(
        &self,
        scenario_a: &HashMap<String, f64>,
        scenario_b: &HashMap<String, f64>,
    ) -> Result<whatif::ScenarioComparison, RegressionError> {
        whatif::compare_scenarios(self, scenario_a, scenario_b)
    }

    /// **Use case D — `P(outcome ≥ threshold | features)`.**
    pub fn prob_exceeds(
        &self,
        features: &HashMap<String, f64>,
        threshold: f64,
    ) -> Result<f64, RegressionError> {
        whatif::prob_exceeds(self, features, threshold)
    }

    /// **Use case D — find input value maximising `prob_exceeds`.**
    pub fn optimise_for_target(
        &self,
        fixed_features: &HashMap<String, f64>,
        free_feature: &str,
        search_range: (f64, f64),
        target_threshold: f64,
    ) -> Result<whatif::OptimisationResult, RegressionError> {
        whatif::optimise_for_target(
            self,
            fixed_features,
            free_feature,
            search_range,
            target_threshold,
        )
    }

    /// Number of post-warmup draws collected across all chains.
    pub fn n_samples(&self) -> usize {
        self.samples.len()
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// ERROR TYPE
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Error)]
pub enum RegressionError {
    #[error("no data provided")]
    NoData,

    #[error("insufficient data: need at least {need}, have {have}")]
    InsufficientData { need: usize, have: usize },

    #[error("feature_names is empty")]
    NoFeatures,

    #[error("sample is missing feature '{name}': {sample}")]
    MissingFeature { name: String, sample: String },

    #[error("feature '{name}' has non-finite value")]
    NonFiniteFeature { name: String },

    #[error("outcome is non-finite at sample index {index}")]
    NonFiniteOutcome { index: usize },

    #[error("weight is non-finite or negative at sample index {index}")]
    InvalidWeight { index: usize },

    #[error("held_out_fraction {got} out of [0, 1)")]
    InvalidHeldOut { got: f64 },

    #[error("unknown model variant '{name}'")]
    UnknownModel { name: String },

    #[error("sampler diverged: {divergences} divergences in {total} draws")]
    SamplerDiverged { divergences: u32, total: u32 },

    #[error("sampler did not converge: {reason}")]
    DidNotConverge { reason: String },

    #[error("posterior is empty (no samples collected)")]
    EmptyPosterior,

    #[error("model produced non-finite prediction (mean={mean}, std={std})")]
    NonFinitePrediction { mean: f64, std: f64 },

    #[error("fitted std deviation is non-positive: {got}")]
    InvalidStd { got: f64 },

    #[error("io error during fit: {0}")]
    Io(String),

    #[error("internal error: {0}")]
    Internal(String),
}

// ═════════════════════════════════════════════════════════════════════════════
// PUBLIC FIT ENTRY POINT
// ═════════════════════════════════════════════════════════════════════════════

/// Fit a conditional model `P(outcome | features, data)`.
///
/// Runs the NLPD-driven improvement loop:
/// tries `LinearNormal` first; if NLPD on held-out improves with a more
/// flexible model, keeps the upgrade; otherwise reverts.
///
/// Returns a [`ConditionalPosterior`] that can answer the four what-if queries.
///
/// # Errors
///
/// Domain-neutral validation errors (missing features, non-finite values),
/// sampler errors (divergence, non-convergence), and config errors
/// (invalid held-out fraction).
pub async fn fit_conditional(
    data: &[WeightedSample],
    config: &RegressionConfig,
) -> Result<ConditionalPosterior, RegressionError> {
    validate_inputs(data, config)?;

    let (train, held_out) = split_train_holdout(data, config.held_out_fraction, config.seed);

    let (model, samples, diagnostics, nlpd) =
        improvement::improvement_loop(&train, &held_out, config).await?;

    let n_obs = data.len();
    let quality = DataQuality::classify(n_obs as f64);
    let metadata = FitMetadata {
        quality,
        nlpd,
        fitted_at: Utc::now(),
        n_observations: n_obs,
        source_description: format!(
            "fit_conditional n={} model={} chains={} draws={}",
            n_obs,
            model.name(),
            config.sampler.n_chains,
            config.sampler.n_draws
        ),
    };

    Ok(ConditionalPosterior {
        samples,
        model_name: model.name().to_string(),
        param_names: model.param_names(),
        feature_names: config.feature_names.clone(),
        diagnostics,
        nlpd,
        metadata,
    })
}

// ═════════════════════════════════════════════════════════════════════════════
// VALIDATION + DATA SPLIT
// ═════════════════════════════════════════════════════════════════════════════

pub(crate) fn validate_inputs(
    data: &[WeightedSample],
    config: &RegressionConfig,
) -> Result<(), RegressionError> {
    if data.is_empty() {
        return Err(RegressionError::NoData);
    }
    if config.feature_names.is_empty() {
        return Err(RegressionError::NoFeatures);
    }
    if !(0.0..1.0).contains(&config.held_out_fraction) {
        return Err(RegressionError::InvalidHeldOut {
            got: config.held_out_fraction,
        });
    }
    for (i, s) in data.iter().enumerate() {
        if !s.outcome.is_finite() {
            return Err(RegressionError::NonFiniteOutcome { index: i });
        }
        if !s.weight.is_finite() || s.weight < 0.0 {
            return Err(RegressionError::InvalidWeight { index: i });
        }
        // Verify every declared feature is present (early failure beats failing
        // mid-sampler with the same error).
        s.feature_vector(&config.feature_names)?;
    }
    let min_data = 5;
    if data.len() < min_data {
        return Err(RegressionError::InsufficientData {
            need: min_data,
            have: data.len(),
        });
    }
    Ok(())
}

/// Deterministic train/holdout split. Sorting by a hash of `(seed, index)`
/// gives a reproducible permutation without depending on RNG state.
pub(crate) fn split_train_holdout(
    data: &[WeightedSample],
    held_out_fraction: f64,
    seed: Option<u64>,
) -> (Vec<WeightedSample>, Vec<WeightedSample>) {
    if held_out_fraction <= 0.0 {
        return (data.to_vec(), Vec::new());
    }

    // Compute a deterministic permutation
    let s = seed.unwrap_or(0xBA1E50_5);
    let mut indexed: Vec<(u64, usize)> = (0..data.len())
        .map(|i| (splitmix64(s.wrapping_add(i as u64)), i))
        .collect();
    indexed.sort_by_key(|(h, _)| *h);

    let n_held = ((data.len() as f64) * held_out_fraction).round() as usize;
    let n_held = n_held.min(data.len().saturating_sub(1)).max(1);

    let mut train = Vec::with_capacity(data.len() - n_held);
    let mut held = Vec::with_capacity(n_held);
    for (rank, (_, idx)) in indexed.into_iter().enumerate() {
        if rank < n_held {
            held.push(data[idx].clone());
        } else {
            train.push(data[idx].clone());
        }
    }
    (train, held)
}

/// SplitMix64 — a fast deterministic hash for permutation generation.
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

// ═════════════════════════════════════════════════════════════════════════════
// TESTS
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(features: &[(&str, f64)], outcome: f64) -> WeightedSample {
        WeightedSample::real(
            features
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            outcome,
        )
    }

    fn config(feature_names: &[&str]) -> RegressionConfig {
        RegressionConfig::new(feature_names.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn weighted_sample_constructors() {
        let s = WeightedSample::real(HashMap::new(), 1.0);
        assert_eq!(s.weight, 1.0);
        let s = WeightedSample::synthetic(HashMap::new(), 1.0);
        assert_eq!(s.weight, 0.2);
    }

    #[test]
    fn feature_vector_extracts_in_declared_order() {
        let s = sample(&[("a", 1.0), ("b", 2.0), ("c", 3.0)], 0.0);
        let names = vec!["c".to_string(), "a".to_string()];
        let v = s.feature_vector(&names).unwrap();
        assert_eq!(v, vec![3.0, 1.0]);
    }

    #[test]
    fn feature_vector_silently_ignores_extras() {
        let s = sample(&[("a", 1.0), ("z_extra", 999.0)], 0.0);
        let names = vec!["a".to_string()];
        let v = s.feature_vector(&names).unwrap();
        assert_eq!(v, vec![1.0]);
    }

    #[test]
    fn feature_vector_errors_on_missing() {
        let s = sample(&[("a", 1.0)], 0.0);
        let names = vec!["a".to_string(), "missing".to_string()];
        let err = s.feature_vector(&names).unwrap_err();
        assert!(matches!(err, RegressionError::MissingFeature { .. }));
    }

    #[test]
    fn feature_vector_errors_on_nan() {
        let s = sample(&[("a", f64::NAN)], 0.0);
        let names = vec!["a".to_string()];
        let err = s.feature_vector(&names).unwrap_err();
        assert!(matches!(err, RegressionError::NonFiniteFeature { .. }));
    }

    #[test]
    fn validate_rejects_empty() {
        let cfg = config(&["a"]);
        assert!(matches!(
            validate_inputs(&[], &cfg),
            Err(RegressionError::NoData)
        ));
    }

    #[test]
    fn validate_rejects_no_features() {
        let cfg = config(&[]);
        let data = vec![sample(&[("a", 1.0)], 1.0); 10];
        assert!(matches!(
            validate_inputs(&data, &cfg),
            Err(RegressionError::NoFeatures)
        ));
    }

    #[test]
    fn validate_rejects_invalid_held_out() {
        let mut cfg = config(&["a"]);
        cfg.held_out_fraction = 1.0;
        let data = vec![sample(&[("a", 1.0)], 1.0); 10];
        assert!(matches!(
            validate_inputs(&data, &cfg),
            Err(RegressionError::InvalidHeldOut { .. })
        ));
    }

    #[test]
    fn validate_rejects_nan_outcome() {
        let cfg = config(&["a"]);
        let mut data = vec![sample(&[("a", 1.0)], 1.0); 6];
        data[3].outcome = f64::NAN;
        assert!(matches!(
            validate_inputs(&data, &cfg),
            Err(RegressionError::NonFiniteOutcome { index: 3 })
        ));
    }

    #[test]
    fn validate_rejects_too_little_data() {
        let cfg = config(&["a"]);
        let data = vec![sample(&[("a", 1.0)], 1.0); 3];
        assert!(matches!(
            validate_inputs(&data, &cfg),
            Err(RegressionError::InsufficientData { .. })
        ));
    }

    #[test]
    fn split_is_deterministic_with_same_seed() {
        let data: Vec<_> = (0..100)
            .map(|i| sample(&[("a", i as f64)], i as f64))
            .collect();
        let (a_train, a_held) = split_train_holdout(&data, 0.2, Some(42));
        let (b_train, b_held) = split_train_holdout(&data, 0.2, Some(42));
        assert_eq!(a_train.len(), b_train.len());
        assert_eq!(a_held.len(), b_held.len());
        for (a, b) in a_train.iter().zip(b_train.iter()) {
            assert_eq!(a.outcome, b.outcome);
        }
        for (a, b) in a_held.iter().zip(b_held.iter()) {
            assert_eq!(a.outcome, b.outcome);
        }
    }

    #[test]
    fn split_holds_out_correct_fraction() {
        let data: Vec<_> = (0..100)
            .map(|i| sample(&[("a", i as f64)], i as f64))
            .collect();
        let (train, held) = split_train_holdout(&data, 0.2, Some(1));
        assert_eq!(train.len() + held.len(), 100);
        assert_eq!(held.len(), 20);
    }

    #[test]
    fn split_zero_holdout_returns_all_train() {
        let data: Vec<_> = (0..10)
            .map(|i| sample(&[("a", i as f64)], i as f64))
            .collect();
        let (train, held) = split_train_holdout(&data, 0.0, None);
        assert_eq!(train.len(), 10);
        assert_eq!(held.len(), 0);
    }

    #[test]
    fn config_serde_round_trip() {
        let cfg = RegressionConfig::new(vec!["x".into(), "y".into()]).with_seed(7);
        let v = serde_json::to_value(&cfg).unwrap();
        let back: RegressionConfig = serde_json::from_value(v).unwrap();
        assert_eq!(back.feature_names, cfg.feature_names);
        assert_eq!(back.seed, cfg.seed);
    }

    #[test]
    fn weighted_sample_serde_round_trip() {
        let s = sample(&[("a", 1.0), ("b", 2.0)], 5.0);
        let v = serde_json::to_value(&s).unwrap();
        let back: WeightedSample = serde_json::from_value(v).unwrap();
        assert_eq!(s, back);
    }
}
