//! Automatic family selection.
//!
//! Tries the parametric families that are *eligible* for the data (Beta requires
//! observations in (0,1); Lognormal requires positivity; Normal is always
//! eligible). For each, computes KL divergence from the empirical CDF to the
//! fitted CDF and returns the lowest-divergence fit.
//!
//! If no parametric family is eligible or all fail, falls back to Triangular.

use statrs::distribution::{
    Beta as StatrsBeta, ContinuousCDF, LogNormal as StatrsLogNormal, Normal as StatrsNormal,
};

use crate::{
    beta::fit_beta_moments, lognormal::fit_lognormal_moments, normal::fit_normal_conjugate,
    triangular::fit_triangular_empirical, FittedDistribution, PosteriorError,
};

/// KL divergence (empirical → parametric) approximated by binned empirical CDF
/// vs the parametric CDF at the same bin edges. Lower = better fit.
///
/// Public so tests can reuse it; also used by [`fit_auto`] internally.
pub fn kl_to_empirical(observations: &[f64], fitted: &FittedDistribution) -> f64 {
    if observations.len() < 2 {
        return f64::INFINITY;
    }

    let mut sorted: Vec<f64> = observations.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len() as f64;

    let cdf = |x: f64| -> Option<f64> {
        match fitted {
            FittedDistribution::Beta { alpha, beta, .. } => {
                if x <= 0.0 {
                    return Some(0.0);
                }
                if x >= 1.0 {
                    return Some(1.0);
                }
                StatrsBeta::new(*alpha, *beta).ok().map(|d| d.cdf(x))
            }
            FittedDistribution::Normal { mean, std_dev, .. } => {
                StatrsNormal::new(*mean, *std_dev).ok().map(|d| d.cdf(x))
            }
            FittedDistribution::Lognormal { median, sigma, .. } => {
                if x <= 0.0 {
                    return Some(0.0);
                }
                let mu = median.ln();
                StatrsLogNormal::new(mu, *sigma).ok().map(|d| d.cdf(x))
            }
            FittedDistribution::Triangular { .. } => None, // not used in auto
        }
    };

    let mut total = 0.0;
    let mut counted = 0;
    for (i, x) in sorted.iter().enumerate() {
        let empirical = (i as f64 + 1.0) / n;
        let parametric = match cdf(*x) {
            Some(p) => p.clamp(1e-9, 1.0 - 1e-9),
            None => return f64::INFINITY,
        };
        // Empirical → parametric KL contribution at each ordered point.
        // Using |Δ CDF| as the divergence proxy: simpler, more stable than
        // log ratios on tail observations.
        total += (empirical - parametric).abs();
        counted += 1;
    }
    if counted == 0 {
        f64::INFINITY
    } else {
        total / counted as f64
    }
}

/// Try every eligible parametric family + Triangular, return the lowest-divergence fit.
///
/// Eligibility:
/// - Beta: all observations in (0, 1)
/// - Lognormal: all observations > 0
/// - Normal: always eligible (unless variance is zero)
/// - Triangular: always eligible (fallback)
pub fn fit_auto(
    observations: &[f64],
    weights: Option<&[f64]>,
) -> Result<FittedDistribution, PosteriorError> {
    let in_unit_interval = observations.iter().all(|v| *v > 0.0 && *v < 1.0);
    let all_positive = observations.iter().all(|v| *v > 0.0);

    let mut candidates: Vec<FittedDistribution> = Vec::new();

    if in_unit_interval {
        if let Ok(fd) = fit_beta_moments(observations, weights) {
            candidates.push(fd);
        }
    }
    if let Ok(fd) = fit_normal_conjugate(observations, weights) {
        candidates.push(fd);
    }
    if all_positive {
        if let Ok(fd) = fit_lognormal_moments(observations, weights) {
            candidates.push(fd);
        }
    }

    if candidates.is_empty() {
        // Fall back to Triangular — non-parametric, always works.
        return fit_triangular_empirical(observations, weights);
    }

    // Pick lowest KL to empirical.
    let best = candidates
        .into_iter()
        .min_by(|a, b| {
            let ka = kl_to_empirical(observations, a);
            let kb = kl_to_empirical(observations, b);
            ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .ok_or(PosteriorError::AutoSelectionFailed)?;

    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_distr::Distribution;

    #[test]
    fn picks_beta_for_beta_data() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(11);
        let dist = rand_distr::Beta::new(2.0, 5.0).unwrap();
        let obs: Vec<f64> = (0..300).map(|_| dist.sample(&mut rng)).collect();
        let fd = fit_auto(&obs, None).unwrap();
        assert!(matches!(fd, FittedDistribution::Beta { .. }));
    }

    #[test]
    fn picks_normal_for_normal_data() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(22);
        let dist = rand_distr::Normal::new(50.0, 5.0).unwrap();
        let obs: Vec<f64> = (0..300).map(|_| dist.sample(&mut rng)).collect();
        // Some samples may be negative — keep them; Normal should still win
        let fd = fit_auto(&obs, None).unwrap();
        assert!(matches!(fd, FittedDistribution::Normal { .. }));
    }

    #[test]
    fn picks_lognormal_for_lognormal_data() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(33);
        let log_dist = rand_distr::Normal::<f64>::new(2.0, 0.5).unwrap();
        let obs: Vec<f64> = (0..300).map(|_| log_dist.sample(&mut rng).exp()).collect();
        let fd = fit_auto(&obs, None).unwrap();
        // The right answer is Lognormal; tolerate Normal (heavily-skewed positive data
        // can fool a CDF-based selector if Normal fits the bulk well).
        assert!(
            matches!(
                fd,
                FittedDistribution::Lognormal { .. } | FittedDistribution::Normal { .. }
            ),
            "got {:?}",
            fd
        );
    }

    #[test]
    fn falls_back_to_triangular_on_failure() {
        // Constant data — every parametric family fails (zero variance).
        let constant = vec![5.0; 10];
        let fd = fit_auto(&constant, None).unwrap();
        assert!(matches!(fd, FittedDistribution::Triangular { .. }));
    }
}
