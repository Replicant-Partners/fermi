//! Lognormal distribution fitting via log-space method of moments.
//!
//! If `X ~ Lognormal(μ, σ)`, then `log(X) ~ Normal(μ, σ)`. We fit by taking
//! the (weighted) mean and stddev of the log-observations.
//!
//! Reported parameters:
//! - `median = exp(μ)` — what FPL `Lognormal(median, σ)` expects as its first arg
//! - `sigma = σ` — the log-space standard deviation
//!
//! Predictive 90% CI: `[exp(μ - 1.645σ), exp(μ + 1.645σ)]`.

use crate::{
    effective_sample_size, weighted_mean, weighted_var, FittedDistribution, PosteriorError,
};

/// Fit a Lognormal distribution via log-space moments.
///
/// All observations must be strictly positive.
///
/// # Errors
/// - `LognormalNonPositive` if any observation ≤ 0
/// - `DegenerateVariance` if the log-space variance is zero or non-finite
pub fn fit_lognormal_moments(
    observations: &[f64],
    weights: Option<&[f64]>,
) -> Result<FittedDistribution, PosteriorError> {
    // Positivity check + log transform
    let mut logs = Vec::with_capacity(observations.len());
    for (i, v) in observations.iter().enumerate() {
        if *v <= 0.0 {
            return Err(PosteriorError::LognormalNonPositive {
                value: *v,
                index: i,
            });
        }
        logs.push(v.ln());
    }

    let mu = weighted_mean(&logs, weights);
    let var = weighted_var(&logs, weights);
    // Use a meaningful epsilon: floating-point noise from summing identical values
    // produces tiny non-zero variances that should still be treated as degenerate.
    if !var.is_finite() || var < 1e-12 {
        return Err(PosteriorError::DegenerateVariance(var));
    }
    let sigma = var.sqrt();
    let n_eff = effective_sample_size(weights, observations.len());

    let median = mu.exp();
    let ci_low = (mu - 1.6449 * sigma).exp();
    let ci_high = (mu + 1.6449 * sigma).exp();

    Ok(FittedDistribution::Lognormal {
        median,
        sigma,
        ci_low,
        ci_high,
        n_eff,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_positive() {
        let bad = vec![1.0, 2.0, 0.0];
        assert!(matches!(
            fit_lognormal_moments(&bad, None),
            Err(PosteriorError::LognormalNonPositive { .. })
        ));
    }

    #[test]
    fn rejects_constant_data() {
        let constant = vec![5.0; 10];
        assert!(matches!(
            fit_lognormal_moments(&constant, None),
            Err(PosteriorError::DegenerateVariance(_))
        ));
    }

    #[test]
    fn recovers_known_lognormal() {
        // log(X) ~ Normal(ln 100, 0.3) → median = 100, sigma = 0.3
        use rand::SeedableRng;
        use rand_distr::Distribution;
        let mut rng = rand::rngs::StdRng::seed_from_u64(13);
        let log_dist = rand_distr::Normal::<f64>::new(100f64.ln(), 0.3).unwrap();
        let obs: Vec<f64> = (0..500).map(|_| log_dist.sample(&mut rng).exp()).collect();
        let fd = fit_lognormal_moments(&obs, None).unwrap();
        match fd {
            FittedDistribution::Lognormal { median, sigma, .. } => {
                assert!((median - 100.0).abs() / 100.0 < 0.05, "median = {}", median);
                assert!((sigma - 0.3).abs() < 0.05, "sigma = {}", sigma);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn ci_brackets_median() {
        let obs = vec![80.0, 100.0, 120.0, 90.0, 110.0, 95.0, 105.0];
        let fd = fit_lognormal_moments(&obs, None).unwrap();
        match fd {
            FittedDistribution::Lognormal {
                median,
                ci_low,
                ci_high,
                ..
            } => {
                assert!(ci_low < median && median < ci_high);
            }
            _ => unreachable!(),
        }
    }
}
