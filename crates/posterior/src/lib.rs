//! # posterior — BayesOps Phase 1
//!
//! Marginal distribution fitting for the Fermi platform. Takes a vector of scalar
//! observations and produces a [`FittedDistribution`] whose parameters are directly
//! convertible to FPL `Driver` distribution parameters via [`FittedDistribution::to_fpl_params`].
//!
//! This crate is the **simple path** of the BayesOps architecture (see
//! `docs/specs/14_BAYESOPS_SPEC.md`). It handles the case where you have a vector of
//! outcome observations (possibly weighted) and want a calibrated distribution over
//! that outcome — no inputs, no regression, no sampler.
//!
//! ## What this crate does NOT do
//!
//! - It does not predict outcome as a function of inputs. That is `posterior-reg` (Phase 2+).
//! - It does not modify FPL, the executor, or `simops` in any way. It only produces
//!   `FittedDistribution` values; downstream code consumes them.
//! - It has no async, no I/O, no network calls. Pure compute.
//!
//! ## Two-loop separation
//!
//! Loop A (this crate): `observations → fit → FittedDistribution::Beta(α, β)`
//! Loop B (executor.rs, UNCHANGED): `Beta(α, β) → MC samples → outcome distribution`
//!
//! The seam is the FPL `Distribution` type. `FittedDistribution::to_fpl_params()` emits
//! a string that is valid FPL `Driver` syntax (e.g. `"Beta(9.4000, 13.6000)"`).
//!
//! ## Persistence
//!
//! `FittedDistribution` and `FitMetadata` are `serde`-serializable and round-trip cleanly
//! through `serde_json::Value`, matching the reserved `harness_snapshots.bayesops_params
//! JSONB` column (`migrations/140_forecast_benchmark.sql:42,48`). Phase 4 wires this in;
//! Phase 1 only guarantees the round-trip.
//!
//! ## Example
//!
//! ```
//! use posterior::{fit_marginal, DistFamily};
//!
//! // 12 historical batch success/failure outcomes (in [0, 1])
//! let obs = vec![0.4, 0.45, 0.42, 0.48, 0.50, 0.46, 0.41, 0.43, 0.47, 0.44, 0.49, 0.45];
//! let (fitted, meta) = fit_marginal(&obs, None, DistFamily::Beta).unwrap();
//!
//! // Plug directly into an FPL file as a Driver
//! let fpl = fitted.to_fpl_params(); // e.g. "beta(57.1234, 71.0000)"
//! assert!(fpl.starts_with("beta("));
//! assert_eq!(meta.n_observations, 12);
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Public modules ──────────────────────────────────────────────────────────
pub mod auto;
pub mod beta;
pub mod bootstrap;
pub mod extractors;
pub mod lognormal;
pub mod normal;
pub mod triangular;

// Re-export the public fit functions for ergonomic use.
pub use auto::{fit_auto, kl_to_empirical};
pub use beta::{fit_beta_conjugate, fit_beta_moments};
pub use bootstrap::bootstrap_ci;
pub use extractors::{
    BinaryFieldValue, BinaryWinnerIdMatch, Extractor, ExtractorDescription, ExtractorError,
    ExtractorRegistry, ScalarDifference, ScalarFieldValue, WorkspaceContext,
};
pub use lognormal::fit_lognormal_moments;
pub use normal::fit_normal_conjugate;
pub use triangular::fit_triangular_empirical;

// ═════════════════════════════════════════════════════════════════════════════
// CORE TYPES — the shared contract (Spec 14 §3)
// ═════════════════════════════════════════════════════════════════════════════

/// The output of any fitting operation. Represents a probability distribution whose
/// parameters were derived from data rather than elicited from a human.
///
/// Directly convertible to FPL `Driver` distribution parameters via
/// [`Self::to_fpl_params`].
///
/// The four variants mirror the FPL `Distribution` enum in `src/ast.rs:85-109`:
/// Beta, Normal, Lognormal, Triangular. Uniform is intentionally excluded — it
/// has no obvious "fit from data" semantics; use Triangular instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "family", rename_all = "lowercase")]
pub enum FittedDistribution {
    Beta {
        alpha: f64,
        beta: f64,
        /// 5th percentile of posterior predictive
        ci_low: f64,
        /// 95th percentile of posterior predictive
        ci_high: f64,
        /// Effective observation count (α + β for conjugate fits)
        n_eff: f64,
    },
    Normal {
        mean: f64,
        std_dev: f64,
        ci_low: f64,
        ci_high: f64,
        n_eff: f64,
    },
    Lognormal {
        /// exp(μ) where μ = mean of log-observations
        median: f64,
        /// σ = stddev of log-observations
        sigma: f64,
        ci_low: f64,
        ci_high: f64,
        n_eff: f64,
    },
    Triangular {
        p5: f64,
        p50: f64,
        p95: f64,
        /// Raw observation count (no n_eff concept for empirical fits)
        n: usize,
    },
}

impl FittedDistribution {
    /// Emit the FPL `Driver` distribution syntax string.
    ///
    /// This is what gets written into a `.fpl` file or injected into the AST.
    /// FPL `Distribution` syntax is positional and lowercase (`beta(α, β)`,
    /// `normal(μ, σ)`, `lognormal(median, σ)`, `triangular(p5, p50, p95)` —
    /// confirmed against `src/lexer.rs:670-674`), so this string is directly
    /// parseable as the right-hand side of `distribution:` in a `driver`
    /// declaration.
    ///
    /// # Example
    /// ```
    /// use posterior::FittedDistribution;
    /// let fd = FittedDistribution::Beta {
    ///     alpha: 9.4, beta: 13.6, ci_low: 0.2, ci_high: 0.65, n_eff: 23.0,
    /// };
    /// assert_eq!(fd.to_fpl_params(), "beta(9.4000, 13.6000)");
    /// ```
    pub fn to_fpl_params(&self) -> String {
        match self {
            Self::Beta { alpha, beta, .. } => {
                format!("beta({:.4}, {:.4})", alpha, beta)
            }
            Self::Normal { mean, std_dev, .. } => {
                format!("normal({:.4}, {:.4})", mean, std_dev)
            }
            Self::Lognormal { median, sigma, .. } => {
                format!("lognormal({:.4}, {:.4})", median, sigma)
            }
            Self::Triangular { p5, p50, p95, .. } => {
                format!("triangular({:.4}, {:.4}, {:.4})", p5, p50, p95)
            }
        }
    }

    /// Width of the 90% CI (`ci_high - ci_low` for parametric fits, `p95 - p5` for
    /// triangular). The primary signal of how much uncertainty remains: wide = sparse
    /// data, narrow = well-fitted.
    pub fn ci_width(&self) -> f64 {
        match self {
            Self::Beta { ci_high, ci_low, .. } => ci_high - ci_low,
            Self::Normal { ci_high, ci_low, .. } => ci_high - ci_low,
            Self::Lognormal { ci_high, ci_low, .. } => ci_high - ci_low,
            Self::Triangular { p5, p95, .. } => p95 - p5,
        }
    }

    /// Effective observation count. For parametric conjugate fits this is the prior
    /// + data count (α + β for Beta, n for Normal). For Triangular it is the raw n.
    pub fn n_eff(&self) -> f64 {
        match self {
            Self::Beta { n_eff, .. } => *n_eff,
            Self::Normal { n_eff, .. } => *n_eff,
            Self::Lognormal { n_eff, .. } => *n_eff,
            Self::Triangular { n, .. } => *n as f64,
        }
    }
}

/// Metadata attached to any [`FittedDistribution`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FitMetadata {
    pub quality: DataQuality,
    /// Negative log predictive density on held-out data. `None` for the simple path
    /// (no held-out evaluation in Phase 1).
    pub nlpd: Option<f64>,
    pub fitted_at: DateTime<Utc>,
    pub n_observations: usize,
    /// Free-form description of the data source.
    /// Example: `"12 Ambu cultivation runs 2025-2026"`.
    pub source_description: String,
}

impl FitMetadata {
    /// Convenience constructor stamping `fitted_at = now()`.
    pub fn new(n_observations: usize, quality: DataQuality, source: impl Into<String>) -> Self {
        Self {
            quality,
            nlpd: None,
            fitted_at: Utc::now(),
            n_observations,
            source_description: source.into(),
        }
    }
}

/// Data quality classification based on effective observation count.
///
/// Thresholds (Spec 14 §3):
/// - `Sufficient`: `n_eff >= 20`
/// - `Sparse`: `5 <= n_eff < 20`
/// - `Insufficient`: `n_eff < 5`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataQuality {
    Sufficient,
    Sparse,
    Insufficient,
}

impl DataQuality {
    /// Classify by effective observation count.
    pub fn classify(n_eff: f64) -> Self {
        if n_eff >= 20.0 {
            Self::Sufficient
        } else if n_eff >= 5.0 {
            Self::Sparse
        } else {
            Self::Insufficient
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// ERROR TYPE
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Error)]
pub enum PosteriorError {
    #[error("no observations provided")]
    NoObservations,

    #[error("weights length ({weights}) does not match observations length ({observations})")]
    WeightLengthMismatch { weights: usize, observations: usize },

    #[error("weights contain negative or non-finite values")]
    InvalidWeights,

    #[error("Beta requires all observations in (0, 1); found value {value} at index {index}")]
    BetaOutOfRange { value: f64, index: usize },

    #[error("Lognormal requires all observations > 0; found value {value} at index {index}")]
    LognormalNonPositive { value: f64, index: usize },

    #[error("observations contain NaN or infinity at index {0}")]
    NonFiniteObservation(usize),

    #[error("variance is zero or non-finite (sample_var = {0}); cannot fit parametric distribution")]
    DegenerateVariance(f64),

    #[error("insufficient data: need at least {need}, have {have}")]
    InsufficientData { need: usize, have: usize },

    #[error("auto family selection failed: no candidate family produced a valid fit")]
    AutoSelectionFailed,
}

// ═════════════════════════════════════════════════════════════════════════════
// PUBLIC API — fit_marginal
// ═════════════════════════════════════════════════════════════════════════════

/// Distribution family selection for [`fit_marginal`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DistFamily {
    /// Beta — outcomes must be in (0, 1).
    Beta,
    /// Normal — outcomes can be any real number.
    Normal,
    /// Lognormal — outcomes must be > 0.
    Lognormal,
    /// Triangular — empirical p5/p50/p95 percentiles, no parametric assumption.
    Triangular,
    /// Try all applicable families and select by lowest KL divergence to empirical CDF.
    Auto,
}

/// Fit a marginal distribution to a vector of scalar observations.
///
/// # Arguments
/// - `observations` — vector of scalar outcomes. Must be non-empty and finite.
/// - `weights` — optional same-length vector of trust weights. Real observations
///   should be `1.0`; synthetic/cascade observations should be `0.0–0.3`. `None`
///   means uniform weighting (all = 1.0).
/// - `family` — which distribution family to fit. `DistFamily::Auto` tries
///   `Beta` (if all obs in (0,1)), `Normal`, `Lognormal` (if all obs > 0) and
///   selects the lowest KL divergence to the empirical CDF.
///
/// # Returns
/// `(FittedDistribution, FitMetadata)` — the fitted distribution plus metadata
/// (quality classification, observation count, timestamp).
///
/// # Errors
/// - `NoObservations` if `observations.is_empty()`
/// - `WeightLengthMismatch` / `InvalidWeights`
/// - `BetaOutOfRange` if family is `Beta` and any obs is outside (0, 1)
/// - `LognormalNonPositive` if family is `Lognormal` and any obs ≤ 0
/// - `DegenerateVariance` if the sample variance is zero (constant data)
/// - `AutoSelectionFailed` if family is `Auto` and no candidate produced a valid fit
pub fn fit_marginal(
    observations: &[f64],
    weights: Option<&[f64]>,
    family: DistFamily,
) -> Result<(FittedDistribution, FitMetadata), PosteriorError> {
    validate_inputs(observations, weights)?;

    let fitted = match family {
        DistFamily::Beta => beta::fit_beta_moments(observations, weights)?,
        DistFamily::Normal => normal::fit_normal_conjugate(observations, weights)?,
        DistFamily::Lognormal => lognormal::fit_lognormal_moments(observations, weights)?,
        DistFamily::Triangular => triangular::fit_triangular_empirical(observations, weights)?,
        DistFamily::Auto => auto::fit_auto(observations, weights)?,
    };

    let quality = DataQuality::classify(fitted.n_eff());
    let meta = FitMetadata::new(
        observations.len(),
        quality,
        format!("fit_marginal({:?}, n={})", family, observations.len()),
    );

    Ok((fitted, meta))
}

// ═════════════════════════════════════════════════════════════════════════════
// SHARED HELPERS — used by all fit_*.rs modules
// ═════════════════════════════════════════════════════════════════════════════

/// Validate observations and weights up front. Centralised so each fit_* module
/// can rely on a clean input.
pub(crate) fn validate_inputs(
    observations: &[f64],
    weights: Option<&[f64]>,
) -> Result<(), PosteriorError> {
    if observations.is_empty() {
        return Err(PosteriorError::NoObservations);
    }
    for (i, v) in observations.iter().enumerate() {
        if !v.is_finite() {
            return Err(PosteriorError::NonFiniteObservation(i));
        }
    }
    if let Some(w) = weights {
        if w.len() != observations.len() {
            return Err(PosteriorError::WeightLengthMismatch {
                weights: w.len(),
                observations: observations.len(),
            });
        }
        for wi in w {
            if !wi.is_finite() || *wi < 0.0 {
                return Err(PosteriorError::InvalidWeights);
            }
        }
    }
    Ok(())
}

/// Compute weighted mean. Assumes inputs already validated.
pub(crate) fn weighted_mean(observations: &[f64], weights: Option<&[f64]>) -> f64 {
    match weights {
        None => observations.iter().sum::<f64>() / observations.len() as f64,
        Some(w) => {
            let sw: f64 = w.iter().sum();
            if sw <= 0.0 {
                return f64::NAN;
            }
            observations
                .iter()
                .zip(w.iter())
                .map(|(x, wi)| x * wi)
                .sum::<f64>()
                / sw
        }
    }
}

/// Compute weighted (sample-style) variance with Bessel-equivalent correction
/// using the effective sample size. Assumes inputs already validated.
///
/// For uniform weights this reduces to the standard sample variance with
/// (n - 1) denominator.
pub(crate) fn weighted_var(observations: &[f64], weights: Option<&[f64]>) -> f64 {
    let mu = weighted_mean(observations, weights);
    match weights {
        None => {
            let n = observations.len();
            if n < 2 {
                return 0.0;
            }
            observations
                .iter()
                .map(|x| (x - mu).powi(2))
                .sum::<f64>()
                / (n as f64 - 1.0)
        }
        Some(w) => {
            let sw: f64 = w.iter().sum();
            let sw2: f64 = w.iter().map(|wi| wi * wi).sum();
            // Effective sample size correction (Kish): denominator = sw - sw2/sw
            let denom = sw - sw2 / sw;
            if denom <= 0.0 {
                return 0.0;
            }
            observations
                .iter()
                .zip(w.iter())
                .map(|(x, wi)| wi * (x - mu).powi(2))
                .sum::<f64>()
                / denom
        }
    }
}

/// Effective sample size. For uniform weights = n. For non-uniform = (Σw)² / Σw².
pub(crate) fn effective_sample_size(weights: Option<&[f64]>, n: usize) -> f64 {
    match weights {
        None => n as f64,
        Some(w) => {
            let sw: f64 = w.iter().sum();
            let sw2: f64 = w.iter().map(|wi| wi * wi).sum();
            if sw2 <= 0.0 {
                0.0
            } else {
                sw * sw / sw2
            }
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// TESTS
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_quality_thresholds() {
        assert_eq!(DataQuality::classify(50.0), DataQuality::Sufficient);
        assert_eq!(DataQuality::classify(20.0), DataQuality::Sufficient);
        assert_eq!(DataQuality::classify(19.9), DataQuality::Sparse);
        assert_eq!(DataQuality::classify(5.0), DataQuality::Sparse);
        assert_eq!(DataQuality::classify(4.9), DataQuality::Insufficient);
        assert_eq!(DataQuality::classify(0.0), DataQuality::Insufficient);
    }

    #[test]
    fn to_fpl_params_beta() {
        let fd = FittedDistribution::Beta {
            alpha: 9.4,
            beta: 13.6,
            ci_low: 0.2,
            ci_high: 0.65,
            n_eff: 23.0,
        };
        assert_eq!(fd.to_fpl_params(), "beta(9.4000, 13.6000)");
    }

    #[test]
    fn to_fpl_params_normal() {
        let fd = FittedDistribution::Normal {
            mean: 4.8,
            std_dev: 0.7,
            ci_low: 3.7,
            ci_high: 5.9,
            n_eff: 12.0,
        };
        assert_eq!(fd.to_fpl_params(), "normal(4.8000, 0.7000)");
    }

    #[test]
    fn to_fpl_params_lognormal() {
        let fd = FittedDistribution::Lognormal {
            median: 100.0,
            sigma: 0.3,
            ci_low: 60.0,
            ci_high: 160.0,
            n_eff: 14.0,
        };
        assert_eq!(fd.to_fpl_params(), "lognormal(100.0000, 0.3000)");
    }

    #[test]
    fn to_fpl_params_triangular() {
        let fd = FittedDistribution::Triangular {
            p5: 3.1,
            p50: 4.8,
            p95: 6.9,
            n: 11,
        };
        assert_eq!(fd.to_fpl_params(), "triangular(3.1000, 4.8000, 6.9000)");
    }

    /// Round-trip a FittedDistribution + FitMetadata through serde_json::Value.
    /// This is what Phase 4 will use to write into the
    /// `harness_snapshots.bayesops_params JSONB` column (migration 140).
    #[test]
    fn jsonb_round_trip() {
        let fd = FittedDistribution::Beta {
            alpha: 9.4,
            beta: 13.6,
            ci_low: 0.2,
            ci_high: 0.65,
            n_eff: 23.0,
        };
        let v = serde_json::to_value(&fd).unwrap();
        let back: FittedDistribution = serde_json::from_value(v).unwrap();
        assert_eq!(fd, back);

        let meta = FitMetadata::new(12, DataQuality::Sparse, "test fixture");
        let v = serde_json::to_value(&meta).unwrap();
        let back: FitMetadata = serde_json::from_value(v).unwrap();
        // fitted_at round-trips at JSON precision (RFC3339, ns truncation possible)
        assert_eq!(meta.quality, back.quality);
        assert_eq!(meta.n_observations, back.n_observations);
        assert_eq!(meta.source_description, back.source_description);
    }

    #[test]
    fn ci_width_matches() {
        let fd = FittedDistribution::Beta {
            alpha: 9.4,
            beta: 13.6,
            ci_low: 0.2,
            ci_high: 0.65,
            n_eff: 23.0,
        };
        assert!((fd.ci_width() - 0.45).abs() < 1e-9);
    }

    #[test]
    fn n_eff_dispatches() {
        let fd = FittedDistribution::Triangular {
            p5: 1.0,
            p50: 2.0,
            p95: 3.0,
            n: 11,
        };
        assert!((fd.n_eff() - 11.0).abs() < 1e-9);
    }

    #[test]
    fn validate_rejects_empty() {
        let err = validate_inputs(&[], None).unwrap_err();
        assert!(matches!(err, PosteriorError::NoObservations));
    }

    #[test]
    fn validate_rejects_weight_mismatch() {
        let err = validate_inputs(&[1.0, 2.0], Some(&[1.0])).unwrap_err();
        assert!(matches!(err, PosteriorError::WeightLengthMismatch { .. }));
    }

    #[test]
    fn validate_rejects_negative_weights() {
        let err = validate_inputs(&[1.0, 2.0], Some(&[1.0, -0.5])).unwrap_err();
        assert!(matches!(err, PosteriorError::InvalidWeights));
    }

    #[test]
    fn validate_rejects_nan() {
        let err = validate_inputs(&[1.0, f64::NAN], None).unwrap_err();
        assert!(matches!(err, PosteriorError::NonFiniteObservation(1)));
    }

    #[test]
    fn weighted_mean_uniform() {
        let obs = [1.0, 2.0, 3.0, 4.0];
        assert!((weighted_mean(&obs, None) - 2.5).abs() < 1e-9);
    }

    #[test]
    fn weighted_mean_weighted() {
        let obs = [1.0, 5.0];
        let w = [1.0, 3.0];
        // (1*1 + 5*3) / 4 = 16/4 = 4.0
        assert!((weighted_mean(&obs, Some(&w)) - 4.0).abs() < 1e-9);
    }

    #[test]
    fn effective_sample_size_uniform() {
        assert!((effective_sample_size(None, 10) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn effective_sample_size_weighted() {
        // weights [1, 1, 1, 1]: ess = 16/4 = 4
        let w = [1.0, 1.0, 1.0, 1.0];
        assert!((effective_sample_size(Some(&w), 4) - 4.0).abs() < 1e-9);
        // weights [1, 0]: ess = 1 (only one observation effectively counts)
        let w = [1.0, 0.0];
        assert!((effective_sample_size(Some(&w), 2) - 1.0).abs() < 1e-9);
    }
}
