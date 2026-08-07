//! Beta distribution fitting.
//!
//! Two paths:
//!
//! 1. **Conjugate** ([`fit_beta_conjugate`]): exact Bayesian update for binary
//!    success/failure data. Posterior is `Beta(1 + s, 1 + f)` under uniform
//!    `Beta(1, 1)` prior. Closed-form CI from the resulting Beta CDF.
//!
//! 2. **Method of moments** ([`fit_beta_moments`]): for continuous observations
//!    in (0, 1). Matches sample mean and variance to Beta parameters:
//!    ```text
//!    α = μ * (μ(1-μ)/σ² - 1)
//!    β = (1-μ) * (μ(1-μ)/σ² - 1)
//!    ```
//!    CI from bootstrap (1000 resamples by default).

use statrs::distribution::{Beta as StatrsBeta, ContinuousCDF};

use crate::bootstrap::{bootstrap_ci, BootstrapFit};
use crate::{
    effective_sample_size, weighted_mean, weighted_var, FittedDistribution, PosteriorError,
};

/// Exact conjugate update for binary outcome data with uniform `Beta(1, 1)` prior.
///
/// Returns `Beta(1 + successes, 1 + failures)` with CI from the posterior CDF.
///
/// # Errors
/// - `InsufficientData` if `successes + failures == 0`.
pub fn fit_beta_conjugate(
    successes: u32,
    failures: u32,
) -> Result<FittedDistribution, PosteriorError> {
    if successes == 0 && failures == 0 {
        return Err(PosteriorError::InsufficientData { need: 1, have: 0 });
    }
    let alpha = 1.0 + successes as f64;
    let beta = 1.0 + failures as f64;

    let dist = StatrsBeta::new(alpha, beta).map_err(|_| PosteriorError::DegenerateVariance(0.0))?;
    let ci_low = dist.inverse_cdf(0.05);
    let ci_high = dist.inverse_cdf(0.95);

    Ok(FittedDistribution::Beta {
        alpha,
        beta,
        ci_low,
        ci_high,
        n_eff: alpha + beta,
    })
}

/// Method-of-moments Beta fit for continuous outcomes in (0, 1).
///
/// CI computed by bootstrap (1000 resamples), so this function is non-trivially
/// stochastic — for deterministic tests, seed via `bootstrap::bootstrap_ci_seeded`.
///
/// # Errors
/// - `BetaOutOfRange` if any observation is not in (0, 1).
/// - `DegenerateVariance` if the sample variance is zero or the resulting α/β are non-positive.
pub fn fit_beta_moments(
    observations: &[f64],
    weights: Option<&[f64]>,
) -> Result<FittedDistribution, PosteriorError> {
    // Range check
    for (i, v) in observations.iter().enumerate() {
        if *v <= 0.0 || *v >= 1.0 {
            return Err(PosteriorError::BetaOutOfRange {
                value: *v,
                index: i,
            });
        }
    }

    let mu = weighted_mean(observations, weights);
    let var = weighted_var(observations, weights);
    if !var.is_finite() || var < 1e-12 {
        return Err(PosteriorError::DegenerateVariance(var));
    }

    // Method of moments:  k = μ(1-μ)/σ² - 1
    let k = mu * (1.0 - mu) / var - 1.0;
    if !k.is_finite() || k <= 0.0 {
        // variance too high for Beta to support these moments
        return Err(PosteriorError::DegenerateVariance(var));
    }
    let alpha = mu * k;
    let beta = (1.0 - mu) * k;

    if alpha <= 0.0 || beta <= 0.0 || !alpha.is_finite() || !beta.is_finite() {
        return Err(PosteriorError::DegenerateVariance(var));
    }

    let n_eff = effective_sample_size(weights, observations.len());

    // Bootstrap CI on the mean of the fitted distribution
    let (ci_low, ci_high) = bootstrap_ci(observations, weights, 1000, None, BootstrapFit::Beta)?;

    Ok(FittedDistribution::Beta {
        alpha,
        beta,
        ci_low,
        ci_high,
        n_eff,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conjugate_zero_zero_errors() {
        assert!(matches!(
            fit_beta_conjugate(0, 0),
            Err(PosteriorError::InsufficientData { .. })
        ));
    }

    #[test]
    fn conjugate_matches_analytical() {
        // 10 successes, 5 failures → Beta(11, 6)
        let fd = fit_beta_conjugate(10, 5).unwrap();
        match fd {
            FittedDistribution::Beta {
                alpha, beta, n_eff, ..
            } => {
                assert!((alpha - 11.0).abs() < 1e-12);
                assert!((beta - 6.0).abs() < 1e-12);
                assert!((n_eff - 17.0).abs() < 1e-12);
            }
            _ => panic!("expected Beta"),
        }
    }

    #[test]
    fn conjugate_ci_brackets_mean() {
        // Beta(11, 6) has mean ≈ 11/17 ≈ 0.647
        let fd = fit_beta_conjugate(10, 5).unwrap();
        match fd {
            FittedDistribution::Beta {
                ci_low, ci_high, ..
            } => {
                let mean = 11.0 / 17.0;
                assert!(ci_low < mean && mean < ci_high);
                assert!(ci_high - ci_low > 0.0 && ci_high - ci_low < 1.0);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn moments_rejects_out_of_range() {
        let bad = vec![0.5, 0.6, 1.0];
        assert!(matches!(
            fit_beta_moments(&bad, None),
            Err(PosteriorError::BetaOutOfRange { .. })
        ));
    }

    #[test]
    fn moments_rejects_zero_variance() {
        let constant = vec![0.5; 10];
        assert!(matches!(
            fit_beta_moments(&constant, None),
            Err(PosteriorError::DegenerateVariance(_))
        ));
    }

    #[test]
    fn moments_recovers_known_beta() {
        // Generate from Beta(2, 5) — mean = 2/7 ≈ 0.286
        use rand::SeedableRng;
        use rand_distr::Distribution;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let true_dist = rand_distr::Beta::new(2.0, 5.0).unwrap();
        let obs: Vec<f64> = (0..200).map(|_| true_dist.sample(&mut rng)).collect();
        let fd = fit_beta_moments(&obs, None).unwrap();
        match fd {
            FittedDistribution::Beta { alpha, beta, .. } => {
                // Method of moments with n=200 should be within 20% of true params
                assert!(
                    (alpha - 2.0).abs() / 2.0 < 0.2,
                    "alpha = {} not near 2.0",
                    alpha
                );
                assert!(
                    (beta - 5.0).abs() / 5.0 < 0.2,
                    "beta = {} not near 5.0",
                    beta
                );
            }
            _ => unreachable!(),
        }
    }
}
