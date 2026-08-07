//! NLPD-driven improvement loop (Spec 14 §5.5).
//!
//! Phase 2a ships **a single model** (LinearNormal) — the improvement loop walks
//! a one-element ladder. Phase 2b adds LinearStudentT, NonlinearNormal,
//! HeteroscedasticNormal, HierarchicalNormal and the actual model-selection
//! logic.

use serde::{Deserialize, Serialize};

use crate::diagnostics::{aggregate_diagnostics, SamplerDiagnostics};
use crate::models::{LinearNormal, RegressionModel};
use crate::sampler::{ChainOutput, NutsSampler};
use crate::{RegressionConfig, RegressionError, WeightedSample};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImprovementConfig {
    /// Maximum number of model variants to try. Default: 10.
    pub max_iterations: u32,
    /// Stop after N consecutive non-improving trials. Default: 3.
    pub stop_after_n_flat: u32,
}

impl Default for ImprovementConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            stop_after_n_flat: 3,
        }
    }
}

/// Trace of the improvement loop: which models were tried and their NLPDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelTrace {
    pub model_name: String,
    pub nlpd: Option<f64>,
    pub accepted: bool,
}

/// Run the improvement loop, returning the winning model + its samples +
/// diagnostics + NLPD.
///
/// **Phase 2a:** single-model loop (LinearNormal). The trace will always be
/// `[("LinearNormal", Some(nlpd), true)]` unless held_out_fraction == 0
/// (then NLPD is None).
pub async fn improvement_loop(
    train: &[WeightedSample],
    held_out: &[WeightedSample],
    config: &RegressionConfig,
) -> Result<
    (
        Box<dyn RegressionModel>,
        Vec<Vec<f64>>,
        SamplerDiagnostics,
        Option<f64>,
    ),
    RegressionError,
> {
    // Phase 2a: one model variant
    let model = LinearNormal::new(config.feature_names.len());
    let (samples, diagnostics) = fit_one(&model, train, config).await?;
    let nlpd = if held_out.is_empty() {
        None
    } else {
        Some(compute_nlpd(
            &model,
            &samples,
            held_out,
            &config.feature_names,
        )?)
    };

    Ok((Box::new(model), samples, diagnostics, nlpd))
}

/// Fit a single model: run multi-chain HMC, aggregate samples + diagnostics.
async fn fit_one<M: RegressionModel + Clone + Send + 'static>(
    model: &M,
    train: &[WeightedSample],
    config: &RegressionConfig,
) -> Result<(Vec<Vec<f64>>, SamplerDiagnostics), RegressionError> {
    let init = model.init_params(config.feature_names.len());
    let base_seed = config.seed.unwrap_or(0xFE_8A1E_5);

    let chain_outputs: Vec<ChainOutput> = NutsSampler::run(
        model.clone(),
        train.to_vec(),
        config.feature_names.clone(),
        init,
        config.sampler.clone(),
        base_seed,
    )
    .await?;

    // Flatten chains into a single sample vector for the posterior, while keeping
    // per-chain separation for diagnostics.
    let per_chain_samples: Vec<Vec<Vec<f64>>> =
        chain_outputs.iter().map(|c| c.samples.clone()).collect();

    let total_divergences: u32 = chain_outputs.iter().map(|c| c.divergences).sum();
    let mean_accept: f64 =
        chain_outputs.iter().map(|c| c.accept_rate).sum::<f64>() / chain_outputs.len() as f64;
    let mean_step: f64 =
        chain_outputs.iter().map(|c| c.final_step_size).sum::<f64>() / chain_outputs.len() as f64;

    let diagnostics = aggregate_diagnostics(
        &per_chain_samples,
        model.n_params(),
        total_divergences,
        mean_accept,
        mean_step,
    );

    // Flat samples (used by the predict/whatif layer)
    let mut flat: Vec<Vec<f64>> = Vec::new();
    for c in chain_outputs {
        flat.extend(c.samples);
    }

    if flat.is_empty() {
        return Err(RegressionError::EmptyPosterior);
    }

    Ok((flat, diagnostics))
}

/// Negative log predictive density on held-out data.
///
/// For each held-out sample, marginalises over the posterior samples by
/// averaging the per-sample predictive density (mixture-style), then takes
/// the negative log and averages over held-out samples.
fn compute_nlpd<M: RegressionModel>(
    model: &M,
    posterior_samples: &[Vec<f64>],
    held_out: &[WeightedSample],
    feature_names: &[String],
) -> Result<f64, RegressionError> {
    if posterior_samples.is_empty() {
        return Err(RegressionError::EmptyPosterior);
    }
    let n_samples = posterior_samples.len() as f64;
    let log_n = n_samples.ln();

    let mut total = 0.0;
    for s in held_out {
        let fv = s.feature_vector(feature_names)?;

        // log-sum-exp of per-sample log-densities → log mean predictive density
        let mut log_terms = Vec::with_capacity(posterior_samples.len());
        for params in posterior_samples {
            let mean = model.predict_mean(params, &fv);
            let std = model.predict_std(params, &fv);
            if !mean.is_finite() || !std.is_finite() || std <= 0.0 {
                continue;
            }
            // log N(outcome; mean, std)
            let resid = s.outcome - mean;
            let log_p =
                -0.5 * (2.0 * std::f64::consts::PI).ln() - std.ln() - 0.5 * (resid / std).powi(2);
            log_terms.push(log_p);
        }
        if log_terms.is_empty() {
            continue;
        }
        let max = log_terms.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let sum_exp: f64 = log_terms.iter().map(|lp| (lp - max).exp()).sum();
        let log_mean_p = max + sum_exp.ln() - log_n;
        total += -log_mean_p;
    }

    Ok(total / held_out.len() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn improvement_config_default() {
        let cfg = ImprovementConfig::default();
        assert_eq!(cfg.max_iterations, 10);
        assert_eq!(cfg.stop_after_n_flat, 3);
    }

    #[test]
    fn improvement_config_serde() {
        let cfg = ImprovementConfig::default();
        let v = serde_json::to_value(&cfg).unwrap();
        let back: ImprovementConfig = serde_json::from_value(v).unwrap();
        assert_eq!(cfg.max_iterations, back.max_iterations);
    }

    #[test]
    fn model_trace_serde() {
        let trace = ModelTrace {
            model_name: "LinearNormal".to_string(),
            nlpd: Some(0.42),
            accepted: true,
        };
        let v = serde_json::to_value(&trace).unwrap();
        let back: ModelTrace = serde_json::from_value(v).unwrap();
        assert_eq!(trace.model_name, back.model_name);
        assert_eq!(trace.nlpd, back.nlpd);
    }
}
