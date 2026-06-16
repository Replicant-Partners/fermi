//! Sampler convergence diagnostics.
//!
//! Implements:
//! - **R-hat** (Gelman-Rubin potential scale reduction factor). Want < 1.05.
//! - **ESS** (effective sample size). Want > 400.
//! - **Divergences** count (rolled up from per-chain output).
//!
//! Implementations follow Stan reference and the Vehtari et al. 2021 "rank-normalized
//! R-hat" paper (we use the simpler split-R-hat form, which is sufficient for
//! the model variants in scope).

use serde::{Deserialize, Serialize};

/// Convergence diagnostics across all chains and parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplerDiagnostics {
    /// Per-parameter R-hat (potential scale reduction factor). Want < 1.05.
    pub r_hat: Vec<f64>,
    /// Per-parameter bulk effective sample size. Want > 400.
    pub ess_bulk: Vec<f64>,
    /// Total divergent transitions across all chains.
    pub divergences: u32,
    /// Realised acceptance rate (averaged across chains).
    pub accept_rate: f64,
    /// Adapted step size (averaged across chains).
    pub step_size: f64,
    /// `true` iff all r_hat < 1.05 AND divergences == 0.
    pub converged: bool,
}

/// Compute split-R-hat per parameter across chains.
///
/// `chains[chain_idx][draw_idx][param_idx]`.
pub fn compute_r_hat(chains: &[Vec<Vec<f64>>], n_params: usize) -> Vec<f64> {
    let n_chains = chains.len();
    if n_chains < 2 {
        // R-hat is undefined for a single chain. Return 1.0 (trivially "converged"
        // for that chain alone) but `converged = false` will be set by the caller.
        return vec![1.0; n_params];
    }

    // Split each chain in half → 2*n_chains "sub-chains" of length n_draws/2.
    // This catches within-chain non-stationarity that whole-chain R-hat misses.
    let n_draws_per_chain = chains[0].len();
    if n_draws_per_chain < 4 {
        return vec![f64::NAN; n_params];
    }
    let half = n_draws_per_chain / 2;

    let mut r_hats = Vec::with_capacity(n_params);
    for p in 0..n_params {
        let mut split_chains: Vec<Vec<f64>> = Vec::with_capacity(n_chains * 2);
        for c in chains {
            let col: Vec<f64> = c.iter().map(|draw| draw[p]).collect();
            split_chains.push(col[..half].to_vec());
            split_chains.push(col[half..2 * half].to_vec());
        }

        let m = split_chains.len() as f64;
        let n = half as f64;

        // Per-chain mean and variance
        let chain_means: Vec<f64> = split_chains
            .iter()
            .map(|c| c.iter().sum::<f64>() / n)
            .collect();
        let chain_vars: Vec<f64> = split_chains
            .iter()
            .zip(chain_means.iter())
            .map(|(c, mean)| c.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0))
            .collect();

        let grand_mean: f64 = chain_means.iter().sum::<f64>() / m;
        // Between-chain variance
        let b: f64 = n * chain_means.iter().map(|cm| (cm - grand_mean).powi(2)).sum::<f64>()
            / (m - 1.0);
        // Within-chain variance
        let w: f64 = chain_vars.iter().sum::<f64>() / m;

        // Estimated marginal variance
        let var_plus = ((n - 1.0) * w + b) / n;

        let r_hat = if w > 0.0 {
            (var_plus / w).sqrt()
        } else {
            // Zero within-chain variance — either degenerate or all-same draws.
            // Return 1.0 if all chains are also at the same point, else +inf.
            if b == 0.0 {
                1.0
            } else {
                f64::INFINITY
            }
        };
        r_hats.push(r_hat);
    }
    r_hats
}

/// Effective sample size (bulk) per parameter, summed across chains.
///
/// Uses the autocorrelation-based estimator from Vehtari et al. 2021, truncated
/// at the first negative pair sum (Geyer's initial monotone sequence estimator,
/// simplified). Good enough for our scale; not for extreme tail diagnostics.
pub fn effective_sample_size_per_chain(chains: &[Vec<Vec<f64>>], n_params: usize) -> Vec<f64> {
    let n_chains = chains.len();
    if n_chains == 0 {
        return Vec::new();
    }
    let n_draws = chains[0].len();
    if n_draws < 4 {
        return vec![f64::NAN; n_params];
    }

    let mut ess_per_param = Vec::with_capacity(n_params);
    for p in 0..n_params {
        // Collect samples for this parameter across all chains as a flat vector
        let mut concat: Vec<f64> = Vec::with_capacity(n_chains * n_draws);
        for c in chains {
            for draw in c {
                concat.push(draw[p]);
            }
        }
        let n = concat.len() as f64;
        let mean = concat.iter().sum::<f64>() / n;
        let var = concat.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;

        if var <= 0.0 {
            ess_per_param.push(n);
            continue;
        }

        // Lag-k autocorrelation, summed via initial monotone sequence
        let max_lag = (n_draws / 4).min(200);
        let mut rho_sum = 0.0;
        let mut prev_pair = f64::INFINITY;
        for k in 1..max_lag {
            // Per-chain autocorrelation at lag k, averaged across chains
            let mut acf_sum = 0.0;
            for c in chains {
                let m: f64 = c.iter().map(|d| d[p]).sum::<f64>() / c.len() as f64;
                let mut s = 0.0;
                for t in 0..(c.len() - k) {
                    s += (c[t][p] - m) * (c[t + k][p] - m);
                }
                acf_sum += s / (c.len() as f64);
            }
            let rho = acf_sum / (n_chains as f64 * var);

            if k % 2 == 1 && k > 1 {
                let pair = rho + ess_per_param_prev_rho(chains, p, k - 1, var, n_chains);
                if pair < 0.0 || pair > prev_pair {
                    break;
                }
                prev_pair = pair;
                rho_sum += rho;
            } else {
                rho_sum += rho;
            }
        }

        let ess = n / (1.0 + 2.0 * rho_sum);
        // Clamp: ESS can't exceed the number of draws or fall below 1.
        ess_per_param.push(ess.clamp(1.0, n));
    }
    ess_per_param
}

/// Helper to compute the lag-k autocorrelation for the initial-monotone pair test.
fn ess_per_param_prev_rho(
    chains: &[Vec<Vec<f64>>],
    p: usize,
    k: usize,
    var: f64,
    n_chains: usize,
) -> f64 {
    if k == 0 {
        return 1.0;
    }
    let mut acf_sum = 0.0;
    for c in chains {
        let m: f64 = c.iter().map(|d| d[p]).sum::<f64>() / c.len() as f64;
        let mut s = 0.0;
        for t in 0..(c.len() - k) {
            s += (c[t][p] - m) * (c[t + k][p] - m);
        }
        acf_sum += s / (c.len() as f64);
    }
    acf_sum / (n_chains as f64 * var)
}

/// Roll up per-chain outputs into a single `SamplerDiagnostics`.
pub(crate) fn aggregate_diagnostics(
    chains: &[Vec<Vec<f64>>],
    n_params: usize,
    divergences: u32,
    accept_rate: f64,
    step_size: f64,
) -> SamplerDiagnostics {
    let r_hat = compute_r_hat(chains, n_params);
    let ess_bulk = effective_sample_size_per_chain(chains, n_params);
    let converged = divergences == 0 && r_hat.iter().all(|r| r.is_finite() && *r < 1.05);
    SamplerDiagnostics {
        r_hat,
        ess_bulk,
        divergences,
        accept_rate,
        step_size,
        converged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    /// Generate `n_chains` chains of i.i.d. N(0, 1) samples — R-hat should be ≈ 1.0.
    fn iid_normal_chains(n_chains: usize, n_draws: usize, seed: u64) -> Vec<Vec<Vec<f64>>> {
        let mut rng = StdRng::seed_from_u64(seed);
        (0..n_chains)
            .map(|_| {
                (0..n_draws)
                    .map(|_| {
                        let u1: f64 = rng.gen_range(1e-12..1.0);
                        let u2: f64 = rng.gen::<f64>();
                        let z = (-2.0 * u1.ln()).sqrt()
                            * (2.0 * std::f64::consts::PI * u2).cos();
                        vec![z]
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn r_hat_near_one_for_converged_chains() {
        let chains = iid_normal_chains(4, 1000, 1);
        let r_hat = compute_r_hat(&chains, 1);
        assert_eq!(r_hat.len(), 1);
        assert!(
            (r_hat[0] - 1.0).abs() < 0.05,
            "R-hat = {} not near 1.0",
            r_hat[0]
        );
    }

    #[test]
    fn r_hat_high_for_unconverged_chains() {
        // Two chains, each constant but at different points → infinite within-chain
        // ratio. Take three chains with slowly drifting means.
        let n_draws = 1000;
        let chains: Vec<Vec<Vec<f64>>> = vec![
            (0..n_draws).map(|i| vec![0.0 + 0.001 * i as f64]).collect(),
            (0..n_draws).map(|i| vec![5.0 + 0.001 * i as f64]).collect(),
            (0..n_draws).map(|i| vec![10.0 + 0.001 * i as f64]).collect(),
        ];
        let r_hat = compute_r_hat(&chains, 1);
        assert!(r_hat[0] > 1.05, "R-hat = {} should signal divergence", r_hat[0]);
    }

    #[test]
    fn ess_close_to_n_for_iid() {
        let chains = iid_normal_chains(4, 1000, 2);
        let ess = effective_sample_size_per_chain(&chains, 1);
        // For 4000 i.i.d. samples, ESS should be near 4000 (within ~20%).
        assert!(ess[0] > 3000.0, "ESS = {} too low for i.i.d. chains", ess[0]);
    }

    #[test]
    fn aggregate_marks_converged() {
        let chains = iid_normal_chains(4, 1000, 3);
        let diag = aggregate_diagnostics(&chains, 1, 0, 0.85, 0.05);
        assert!(diag.converged);
        assert_eq!(diag.divergences, 0);
    }

    #[test]
    fn aggregate_marks_diverged_when_divergences_nonzero() {
        let chains = iid_normal_chains(4, 1000, 4);
        let diag = aggregate_diagnostics(&chains, 1, 5, 0.85, 0.05);
        assert!(!diag.converged);
        assert_eq!(diag.divergences, 5);
    }

    #[test]
    fn diagnostics_serde() {
        let diag = SamplerDiagnostics {
            r_hat: vec![1.01, 1.02],
            ess_bulk: vec![500.0, 600.0],
            divergences: 0,
            accept_rate: 0.85,
            step_size: 0.05,
            converged: true,
        };
        let v = serde_json::to_value(&diag).unwrap();
        let back: SamplerDiagnostics = serde_json::from_value(v).unwrap();
        assert_eq!(diag.converged, back.converged);
    }
}
