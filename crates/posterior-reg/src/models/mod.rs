//! Regression model variants for the HMC sampler.
//!
//! Each model exposes a [`RegressionModel`] trait implementation. The sampler
//! is model-agnostic: it queries `log_likelihood_at` + `grad_log_likelihood_at`
//! and runs HMC over the parameter space.
//!
//! Phase 2a ships **LinearNormal** only (linear regression with Normal
//! likelihood, hand-coded analytical gradient). The improvement-loop ladder
//! (LinearStudentT, NonlinearNormal, HeteroscedasticNormal, HierarchicalNormal)
//! is Phase 2b and lives behind the `ad` feature flag.

pub mod linear_normal;

pub use linear_normal::LinearNormal;

use crate::RegressionError;

/// Domain-neutral regression model. The HMC sampler only needs:
///
/// - `log_likelihood_at(params, features_row, outcome)` — per-observation
///   log-likelihood contribution.
/// - `grad_log_likelihood_at(params, features_row, outcome)` — per-observation
///   gradient with respect to params.
/// - `log_prior` / `grad_log_prior` — over params, independent of data.
/// - `predict_mean` / `predict_std` — used at posterior-query time.
///
/// Models with `Clone + Send + 'static` can be sent across `spawn_blocking`
/// boundaries. All built-in variants satisfy these bounds.
pub trait RegressionModel: Send + Sync {
    /// Stable identifier (e.g. "LinearNormal"). Stored in
    /// [`ConditionalPosterior::model_name`] for serde round-trip.
    fn name(&self) -> &str;

    /// Number of parameters in the model.
    fn n_params(&self) -> usize;

    /// Names of the parameters in their canonical order. The HMC sampler operates
    /// on `Vec<f64>` of this length.
    fn param_names(&self) -> Vec<String>;

    /// Log-likelihood of a single (features_row, outcome) pair at `params`.
    ///
    /// `features_row` is pre-extracted by the caller in declared feature_names
    /// order — the model does not look at strings.
    fn log_likelihood_at(&self, params: &[f64], features_row: &[f64], outcome: f64) -> f64;

    /// Gradient of `log_likelihood_at` w.r.t. params. Same length as `n_params()`.
    fn grad_log_likelihood_at(
        &self,
        params: &[f64],
        features_row: &[f64],
        outcome: f64,
    ) -> Vec<f64>;

    /// Log-prior on params. Should be log of a proper prior (otherwise NLPD
    /// is undefined). Defaults to weakly informative `N(0, 10)` per param.
    fn log_prior(&self, params: &[f64]) -> f64 {
        // Default: independent N(0, 10) on every parameter
        let sigma = 10.0;
        let two_var = 2.0 * sigma * sigma;
        let log_norm = -0.5 * (2.0 * std::f64::consts::PI * sigma * sigma).ln();
        params.iter().map(|p| log_norm - p * p / two_var).sum()
    }

    /// Gradient of `log_prior` w.r.t. params.
    fn grad_log_prior(&self, params: &[f64]) -> Vec<f64> {
        let sigma = 10.0;
        let var = sigma * sigma;
        params.iter().map(|p| -p / var).collect()
    }

    /// Predicted mean outcome at `features_row` given `params`.
    fn predict_mean(&self, params: &[f64], features_row: &[f64]) -> f64;

    /// Predicted (aleatoric) standard deviation at `features_row` given `params`.
    /// For homoscedastic models this does not depend on features_row.
    fn predict_std(&self, params: &[f64], features_row: &[f64]) -> f64;

    /// Initial parameter guess given the training data (used as HMC starting point).
    /// Default: zeros — implementations should override for faster convergence.
    fn init_params(&self, _n_features: usize) -> Vec<f64> {
        vec![0.0; self.n_params()]
    }
}

/// Reconstruct a `Box<dyn RegressionModel>` from its persisted name + feature/param
/// names. Used by `ConditionalPosterior::recover_model()`.
pub fn recover(
    model_name: &str,
    feature_names: &[String],
    _param_names: &[String],
) -> Result<Box<dyn RegressionModel>, RegressionError> {
    match model_name {
        "LinearNormal" => Ok(Box::new(LinearNormal::new(feature_names.len()))),
        other => Err(RegressionError::UnknownModel {
            name: other.to_string(),
        }),
    }
}
