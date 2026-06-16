//! Normal distribution fitting via weighted conjugate update.
//!
//! With a flat (non-informative) prior, the posterior over `μ` is `N(μ_hat, σ/√n_eff)`
//! and the predictive distribution for a new observation is `N(μ_hat, σ_hat)` where
//! `μ_hat`, `σ_hat` are the weighted sample mean and standard deviation.
//!
//! The CI returned is the **predictive** 90% interval `μ_hat ± 1.645 · σ_hat`, not
//! the (much tighter) credible interval on `μ` itself. This is what the downstream
//! consumer wants — the spread that will feed FPL's executor as `Normal(μ, σ)`.

use crate::{
    effective_sample_size, weighted_mean, weighted_var, FittedDistribution, PosteriorError,
};

/// Fit a Normal distribution via weighted conjugate update.
///
/// Returns `Normal(μ_hat, σ_hat)` with predictive 90% CI `μ_hat ± 1.645·σ_hat`.
///
/// # Errors
/// - `DegenerateVariance` if the sample variance is zero or non-finite.
pub fn fit_normal_conjugate(
    observations: &[f64],
    weights: Option<&[f64]>,
) -> Result<FittedDistribution, PosteriorError> {
    let mu = weighted_mean(observations, weights);
    let var = weighted_var(observations, weights);
    if !var.is_finite() || var < 1e-12 {
        return Err(PosteriorError::DegenerateVariance(var));
    }
    let std_dev = var.sqrt();
    let n_eff = effective_sample_size(weights, observations.len());

    // 90% predictive CI (1.645 = inverse Φ at 0.95)
    let half_width = 1.6449 * std_dev;
    let ci_low = mu - half_width;
    let ci_high = mu + half_width;

    Ok(FittedDistribution::Normal {
        mean: mu,
        std_dev,
        ci_low,
        ci_high,
        n_eff,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_weighted_sample_stats() {
        let obs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let fd = fit_normal_conjugate(&obs, None).unwrap();
        match fd {
            FittedDistribution::Normal { mean, std_dev, .. } => {
                assert!((mean - 3.0).abs() < 1e-9);
                // sample stddev of 1..5 = √(10/4) = √2.5 ≈ 1.5811
                assert!((std_dev - 2.5_f64.sqrt()).abs() < 1e-9);
            }
            _ => panic!("expected Normal"),
        }
    }

    #[test]
    fn rejects_constant_data() {
        let constant = vec![3.0; 8];
        assert!(matches!(
            fit_normal_conjugate(&constant, None),
            Err(PosteriorError::DegenerateVariance(_))
        ));
    }

    #[test]
    fn recovers_known_normal() {
        use rand::SeedableRng;
        use rand_distr::Distribution;
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let true_dist = rand_distr::Normal::new(4.8, 0.7).unwrap();
        let obs: Vec<f64> = (0..500).map(|_| true_dist.sample(&mut rng)).collect();
        let fd = fit_normal_conjugate(&obs, None).unwrap();
        match fd {
            FittedDistribution::Normal { mean, std_dev, .. } => {
                assert!((mean - 4.8).abs() < 0.1, "mean = {}", mean);
                assert!((std_dev - 0.7).abs() < 0.1, "std_dev = {}", std_dev);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn weighted_mean_respects_weights() {
        let obs = vec![1.0, 5.0, 1.0, 5.0];
        let w = vec![1.0, 0.0, 1.0, 0.0];
        // With only the [1.0, 1.0] effectively counting, mean ≈ 1.0, var = 0 → error
        assert!(matches!(
            fit_normal_conjugate(&obs, Some(&w)),
            Err(PosteriorError::DegenerateVariance(_))
        ));
    }

    #[test]
    fn ci_brackets_mean() {
        let obs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let fd = fit_normal_conjugate(&obs, None).unwrap();
        match fd {
            FittedDistribution::Normal {
                mean,
                ci_low,
                ci_high,
                ..
            } => {
                assert!(ci_low < mean && mean < ci_high);
            }
            _ => unreachable!(),
        }
    }
}
