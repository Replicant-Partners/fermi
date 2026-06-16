//! Bootstrap confidence intervals.
//!
//! Given observations + optional weights, resample (with replacement, with
//! probabilities proportional to weights) `n_bootstrap` times. For each
//! resample, refit the requested family and record its central statistic.
//! Return the 5th and 95th percentiles of those central statistics.
//!
//! For Beta and Lognormal, the "central statistic" is the predictive 5th and 95th
//! percentile of the resampled fit — this directly gives the predictive CI on the
//! outcome scale, which is what feeds the FPL Driver.

use rand::distributions::WeightedIndex;
use rand::prelude::*;
use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::{
    weighted_mean, weighted_var, PosteriorError,
};

/// Which family the bootstrap should refit on each resample.
#[derive(Debug, Clone, Copy)]
pub enum BootstrapFit {
    Beta,
    Normal,
    Lognormal,
}

/// Bootstrap a 90% predictive CI by resampling `n_bootstrap` times.
///
/// `seed = None` uses thread-local randomness. Provide a seed for deterministic tests.
pub fn bootstrap_ci(
    observations: &[f64],
    weights: Option<&[f64]>,
    n_bootstrap: usize,
    seed: Option<u64>,
    fit: BootstrapFit,
) -> Result<(f64, f64), PosteriorError> {
    if observations.is_empty() {
        return Err(PosteriorError::NoObservations);
    }
    if n_bootstrap == 0 {
        return Err(PosteriorError::InsufficientData {
            need: 1,
            have: 0,
        });
    }

    let mut rng: StdRng = match seed {
        Some(s) => SeedableRng::seed_from_u64(s),
        None => SeedableRng::from_entropy(),
    };

    let dist = build_weighted_index(weights, observations.len())?;
    let n = observations.len();

    // Collect (lo, hi) pairs from each resample
    let mut lows: Vec<f64> = Vec::with_capacity(n_bootstrap);
    let mut highs: Vec<f64> = Vec::with_capacity(n_bootstrap);

    for _ in 0..n_bootstrap {
        // Resample
        let resampled: Vec<f64> = (0..n)
            .map(|_| observations[dist.sample(&mut rng)])
            .collect();

        if let Some((lo, hi)) = refit_predictive_5_95(&resampled, fit) {
            lows.push(lo);
            highs.push(hi);
        }
    }

    if lows.is_empty() {
        // All resamples failed (degenerate variance etc.). Fall back to empirical
        // percentiles of the original data, which is always defined for n >= 1.
        return empirical_5_95(observations);
    }

    // Take median of the lows and median of the highs as a robust pooled estimate.
    lows.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    highs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let ci_low = lows[lows.len() / 2];
    let ci_high = highs[highs.len() / 2];
    Ok((ci_low, ci_high))
}

/// Build a `WeightedIndex` over observation indices. If weights are `None` or all
/// zero, falls back to uniform.
fn build_weighted_index(
    weights: Option<&[f64]>,
    n: usize,
) -> Result<WeightedIndex<f64>, PosteriorError> {
    let w: Vec<f64> = match weights {
        None => vec![1.0; n],
        Some(w) => {
            let sum: f64 = w.iter().sum();
            if sum <= 0.0 {
                vec![1.0; n]
            } else {
                w.to_vec()
            }
        }
    };
    WeightedIndex::new(&w).map_err(|_| PosteriorError::InvalidWeights)
}

/// Refit a resample and return its predictive 5th/95th percentiles.
/// Returns `None` if the resample is degenerate (zero variance, etc.).
fn refit_predictive_5_95(resampled: &[f64], fit: BootstrapFit) -> Option<(f64, f64)> {
    match fit {
        BootstrapFit::Normal => {
            let mu = weighted_mean(resampled, None);
            let var = weighted_var(resampled, None);
            if !var.is_finite() || var <= 0.0 {
                return None;
            }
            let sigma = var.sqrt();
            Some((mu - 1.6449 * sigma, mu + 1.6449 * sigma))
        }
        BootstrapFit::Lognormal => {
            // Skip if any non-positive
            if resampled.iter().any(|v| *v <= 0.0) {
                return None;
            }
            let logs: Vec<f64> = resampled.iter().map(|v| v.ln()).collect();
            let mu = weighted_mean(&logs, None);
            let var = weighted_var(&logs, None);
            if !var.is_finite() || var <= 0.0 {
                return None;
            }
            let sigma = var.sqrt();
            Some(((mu - 1.6449 * sigma).exp(), (mu + 1.6449 * sigma).exp()))
        }
        BootstrapFit::Beta => {
            // For Beta, use the resample's empirical 5th/95th directly — refitting
            // Beta on each resample and then computing its inverse CDF is overkill
            // for a 1000-resample loop and adds nothing the empirical percentiles
            // don't already capture.
            empirical_5_95(resampled).ok()
        }
    }
}

/// 5th/95th percentile via linear interpolation on a sorted (unweighted) sample.
fn empirical_5_95(observations: &[f64]) -> Result<(f64, f64), PosteriorError> {
    if observations.is_empty() {
        return Err(PosteriorError::NoObservations);
    }
    let mut sorted: Vec<f64> = observations.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    Ok((
        interp_percentile(&sorted, 0.05, n),
        interp_percentile(&sorted, 0.95, n),
    ))
}

fn interp_percentile(sorted: &[f64], q: f64, n: usize) -> f64 {
    if n == 1 {
        return sorted[0];
    }
    let pos = q * (n as f64 - 1.0);
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(n - 1);
    let frac = pos - lo as f64;
    sorted[lo] + frac * (sorted[hi] - sorted[lo])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_normal_returns_finite_ci() {
        let obs: Vec<f64> = (1..=50).map(|i| i as f64).collect();
        let (lo, hi) = bootstrap_ci(&obs, None, 200, Some(1), BootstrapFit::Normal).unwrap();
        assert!(lo.is_finite() && hi.is_finite());
        assert!(lo < hi);
    }

    #[test]
    fn bootstrap_lognormal_finite() {
        let obs: Vec<f64> = (1..=50).map(|i| (i as f64).powf(1.1)).collect();
        let (lo, hi) =
            bootstrap_ci(&obs, None, 200, Some(2), BootstrapFit::Lognormal).unwrap();
        assert!(lo > 0.0 && hi > 0.0);
        assert!(lo < hi);
    }

    #[test]
    fn bootstrap_beta_in_unit_interval() {
        let obs: Vec<f64> = (1..=50).map(|i| i as f64 / 51.0).collect();
        let (lo, hi) = bootstrap_ci(&obs, None, 200, Some(3), BootstrapFit::Beta).unwrap();
        assert!(lo > 0.0 && hi < 1.0);
        assert!(lo < hi);
    }

    #[test]
    fn deterministic_with_seed() {
        let obs: Vec<f64> = (1..=20).map(|i| i as f64).collect();
        let a = bootstrap_ci(&obs, None, 100, Some(42), BootstrapFit::Normal).unwrap();
        let b = bootstrap_ci(&obs, None, 100, Some(42), BootstrapFit::Normal).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn rejects_zero_bootstrap() {
        let obs = vec![1.0, 2.0, 3.0];
        assert!(bootstrap_ci(&obs, None, 0, Some(1), BootstrapFit::Normal).is_err());
    }

    #[test]
    fn empirical_5_95_works() {
        let v: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        let (lo, hi) = empirical_5_95(&v).unwrap();
        // 5th of [1..100] interpolates to 1 + 0.05*99 ≈ 5.95
        assert!((lo - 5.95).abs() < 0.1, "lo = {}", lo);
        // 95th: 1 + 0.95*99 ≈ 95.05
        assert!((hi - 95.05).abs() < 0.1, "hi = {}", hi);
    }
}
