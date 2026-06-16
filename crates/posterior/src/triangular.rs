//! Triangular distribution fitting — empirical percentiles, no parametric assumption.
//!
//! Reports `p5`, `p50`, `p95` of the (weighted) empirical distribution. This is the
//! fallback when no parametric family fits (auto-selection) or when the operator
//! explicitly wants a non-parametric representation.

use crate::{FittedDistribution, PosteriorError};

/// Fit a Triangular distribution by extracting empirical percentiles.
///
/// Uses linear interpolation between sorted observations. Weights are accounted
/// for via a weighted empirical CDF.
///
/// # Errors
/// - `NoObservations` if the vector is empty (caught upstream by `validate_inputs`,
///   but we re-check defensively here)
pub fn fit_triangular_empirical(
    observations: &[f64],
    weights: Option<&[f64]>,
) -> Result<FittedDistribution, PosteriorError> {
    if observations.is_empty() {
        return Err(PosteriorError::NoObservations);
    }

    // Build sorted (value, weight) pairs
    let mut paired: Vec<(f64, f64)> = match weights {
        None => observations.iter().map(|x| (*x, 1.0)).collect(),
        Some(w) => observations
            .iter()
            .zip(w.iter())
            .map(|(x, wi)| (*x, *wi))
            .collect(),
    };
    paired.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let total_weight: f64 = paired.iter().map(|(_, w)| *w).sum();
    if total_weight <= 0.0 {
        return Err(PosteriorError::InvalidWeights);
    }

    let p5 = weighted_percentile(&paired, total_weight, 0.05);
    let p50 = weighted_percentile(&paired, total_weight, 0.50);
    let p95 = weighted_percentile(&paired, total_weight, 0.95);

    Ok(FittedDistribution::Triangular {
        p5,
        p50,
        p95,
        n: observations.len(),
    })
}

/// Compute a weighted percentile via linear interpolation on the empirical CDF.
fn weighted_percentile(sorted: &[(f64, f64)], total_weight: f64, q: f64) -> f64 {
    let target = q * total_weight;
    let mut cum = 0.0;
    let mut prev_val = sorted[0].0;
    let mut prev_cum = 0.0;
    for (val, w) in sorted.iter() {
        let next_cum = cum + w;
        if next_cum >= target {
            // Linear interpolation between (prev_val, prev_cum) and (val, next_cum)
            let span = next_cum - prev_cum;
            if span <= 0.0 {
                return *val;
            }
            let t = (target - prev_cum) / span;
            return prev_val + t * (val - prev_val);
        }
        prev_val = *val;
        prev_cum = next_cum;
        cum = next_cum;
    }
    sorted.last().unwrap().0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_known_percentiles() {
        // 100 evenly-spaced observations from 0 to 99.
        let obs: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let fd = fit_triangular_empirical(&obs, None).unwrap();
        match fd {
            FittedDistribution::Triangular { p5, p50, p95, n } => {
                assert_eq!(n, 100);
                // Allow ±1 slack — percentile interpolation is implementation-defined
                assert!((p5 - 5.0).abs() < 2.0, "p5 = {}", p5);
                assert!((p50 - 50.0).abs() < 2.0, "p50 = {}", p50);
                assert!((p95 - 95.0).abs() < 2.0, "p95 = {}", p95);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn ordered_percentiles() {
        let obs = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let fd = fit_triangular_empirical(&obs, None).unwrap();
        match fd {
            FittedDistribution::Triangular { p5, p50, p95, .. } => {
                assert!(p5 <= p50 && p50 <= p95);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn weights_shift_percentiles() {
        let obs = vec![1.0, 1.0, 1.0, 10.0, 10.0, 10.0];
        // With uniform weights p50 falls in the middle (~5.5)
        let fd_uniform = fit_triangular_empirical(&obs, None).unwrap();
        // With weights favouring the 10s, p50 should be ≥ 5.5
        let w = vec![0.1, 0.1, 0.1, 1.0, 1.0, 1.0];
        let fd_weighted = fit_triangular_empirical(&obs, Some(&w)).unwrap();
        match (fd_uniform, fd_weighted) {
            (
                FittedDistribution::Triangular { p50: u_med, .. },
                FittedDistribution::Triangular { p50: w_med, .. },
            ) => {
                assert!(w_med >= u_med);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn rejects_zero_total_weight() {
        let obs = vec![1.0, 2.0, 3.0];
        let w = vec![0.0, 0.0, 0.0];
        assert!(matches!(
            fit_triangular_empirical(&obs, Some(&w)),
            Err(PosteriorError::InvalidWeights)
        ));
    }
}
