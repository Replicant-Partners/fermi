//! HMC (Hamiltonian Monte Carlo) sampler with dual-averaging step-size adaptation.
//!
//! Implementation notes:
//!
//! - **Algorithm:** Vanilla HMC with leapfrog integration, NOT full NUTS.
//!   Spec 14 §5.4 says "HMC sampler" — full NUTS is a future upgrade if
//!   trajectory tuning becomes a bottleneck.
//! - **Step-size adaptation:** Dual averaging during warmup (Hoffman & Gelman
//!   2014, §3.2), then frozen for sampling.
//! - **Mass matrix:** Identity. Diagonal adaptation is a follow-up; the model
//!   variants here (linear-ish, low-dimensional) don't critically need it.
//! - **Determinism:** Each chain takes a `seed: u64`. Same seed + same data +
//!   same model = bitwise-identical draws.
//! - **Concurrency:** Multi-chain via `tokio::task::spawn_blocking`. Chains
//!   are independent and CPU-bound; `spawn_blocking` is the canonical primitive.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::models::RegressionModel;
use crate::{RegressionError, WeightedSample};

/// HMC sampler configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplerConfig {
    /// Number of independent chains run in parallel. Default: 4.
    pub n_chains: u32,
    /// Warmup iterations per chain (step-size adaptation + burn-in). Default: 500.
    pub n_warmup: u32,
    /// Post-warmup draws per chain (these are what end up in the posterior). Default: 1000.
    pub n_draws: u32,
    /// Target Metropolis acceptance rate for step-size adaptation. Default: 0.80.
    pub target_accept_rate: f64,
    /// Number of leapfrog steps per HMC iteration. Default: 10.
    pub n_leapfrog: u32,
    /// Optional initial step size. `None` → auto-init from a heuristic.
    pub initial_step_size: Option<f64>,
}

impl Default for SamplerConfig {
    fn default() -> Self {
        Self {
            n_chains: 4,
            n_warmup: 500,
            n_draws: 1000,
            target_accept_rate: 0.80,
            n_leapfrog: 10,
            initial_step_size: None,
        }
    }
}

/// One HMC chain's output.
pub(crate) struct ChainOutput {
    /// Post-warmup samples: `[draw_idx][param_idx]`.
    pub samples: Vec<Vec<f64>>,
    /// Number of divergent transitions during sampling.
    pub divergences: u32,
    /// Final adapted step size (after warmup).
    pub final_step_size: f64,
    /// Realised acceptance rate over the sampling phase.
    pub accept_rate: f64,
}

/// The public sampler entry point. Runs `config.n_chains` independent HMC chains.
///
/// Each chain runs in a `tokio::task::spawn_blocking` task. Returns once every
/// chain completes; chain index `i` uses `seed.wrapping_add(i as u64)`.
pub struct NutsSampler;

impl NutsSampler {
    /// Run `n_chains` chains and collect their post-warmup samples.
    ///
    /// `init_params` provides the starting point for chain 0; subsequent chains
    /// perturb this with a small chain-index-dependent offset so they explore
    /// the space from different starting points (essential for R-hat to be meaningful).
    pub(crate) async fn run<M: RegressionModel + Clone + Send + 'static>(
        model: M,
        data: Vec<WeightedSample>,
        feature_names: Vec<String>,
        init_params: Vec<f64>,
        config: SamplerConfig,
        base_seed: u64,
    ) -> Result<Vec<ChainOutput>, RegressionError> {
        let mut handles = Vec::with_capacity(config.n_chains as usize);

        for chain_idx in 0..config.n_chains {
            let m = model.clone();
            let d = data.clone();
            let fnames = feature_names.clone();
            let init = perturb_init(&init_params, chain_idx, base_seed);
            let cfg = config.clone();
            let seed = base_seed.wrapping_add(chain_idx as u64);

            let handle = tokio::task::spawn_blocking(move || {
                run_single_chain(&m, &d, &fnames, init, &cfg, seed)
            });
            handles.push(handle);
        }

        let mut outputs = Vec::with_capacity(config.n_chains as usize);
        for h in handles {
            match h.await {
                Ok(Ok(out)) => outputs.push(out),
                Ok(Err(e)) => return Err(e),
                Err(join_err) => {
                    return Err(RegressionError::Io(format!(
                        "chain task panicked: {}",
                        join_err
                    )));
                }
            }
        }
        Ok(outputs)
    }
}

/// Perturb the init vector slightly per chain so chains start at different points.
fn perturb_init(init: &[f64], chain_idx: u32, seed: u64) -> Vec<f64> {
    let mut rng = StdRng::seed_from_u64(seed.wrapping_add(0xC0FFEE).wrapping_add(chain_idx as u64));
    init.iter().map(|x| x + rng.gen_range(-0.1..0.1)).collect()
}

/// Single-chain HMC implementation.
fn run_single_chain<M: RegressionModel>(
    model: &M,
    data: &[WeightedSample],
    feature_names: &[String],
    init: Vec<f64>,
    config: &SamplerConfig,
    seed: u64,
) -> Result<ChainOutput, RegressionError> {
    let mut rng = StdRng::seed_from_u64(seed);
    let n_params = init.len();

    // Pre-extract feature vectors once per sample to avoid repeated HashMap lookups
    let extracted: Vec<(Vec<f64>, f64, f64)> = data
        .iter()
        .map(|s| {
            let fv = s.feature_vector(feature_names)?;
            Ok::<_, RegressionError>((fv, s.outcome, s.weight))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Closure that computes total log-posterior + gradient at a point
    let log_post = |params: &[f64]| -> f64 {
        let mut lp = model.log_prior(params);
        for (fv, outcome, weight) in &extracted {
            lp += weight * model.log_likelihood_at(params, fv, *outcome);
        }
        lp
    };

    let grad_log_post = |params: &[f64]| -> Vec<f64> {
        let mut g = model.grad_log_prior(params);
        for (fv, outcome, weight) in &extracted {
            let glik = model.grad_log_likelihood_at(params, fv, *outcome);
            for (gi, gli) in g.iter_mut().zip(glik.iter()) {
                *gi += weight * *gli;
            }
        }
        g
    };

    // Step size adaptation state (dual averaging)
    let target_accept = config.target_accept_rate;
    let mut step_size = config.initial_step_size.unwrap_or(0.1);
    let mut log_step_avg = step_size.ln();
    let mu = (10.0 * step_size).ln();
    let mut h_bar: f64 = 0.0;
    let gamma: f64 = 0.05;
    let t0: f64 = 10.0;
    let kappa: f64 = 0.75;

    let mut current = init;
    let mut current_lp = log_post(&current);

    if !current_lp.is_finite() {
        return Err(RegressionError::Internal(format!(
            "initial log-posterior is non-finite at params={:?}",
            current
        )));
    }

    let n_total = config.n_warmup + config.n_draws;
    let mut samples = Vec::with_capacity(config.n_draws as usize);
    let mut divergences: u32 = 0;
    let mut accept_sum_sampling: f64 = 0.0;
    let mut n_accept_sampling: u32 = 0;

    for iter in 0..n_total {
        let is_warmup = iter < config.n_warmup;

        // Sample initial momentum from N(0, I)
        let momentum: Vec<f64> = (0..n_params)
            .map(|_| sample_standard_normal(&mut rng))
            .collect();
        let init_kinetic: f64 = 0.5 * momentum.iter().map(|m| m * m).sum::<f64>();
        let init_h = -current_lp + init_kinetic;

        // Leapfrog integration
        let (proposed, proposed_momentum) = leapfrog(
            &current,
            &momentum,
            step_size,
            config.n_leapfrog,
            &grad_log_post,
        );

        let proposed_lp = log_post(&proposed);
        let proposed_kinetic: f64 = 0.5 * proposed_momentum.iter().map(|m| m * m).sum::<f64>();
        let proposed_h = -proposed_lp + proposed_kinetic;

        // Energy divergence check: catch unstable trajectories
        let dh = proposed_h - init_h;
        let diverged = !proposed_lp.is_finite() || dh.abs() > 1000.0;

        let accept_prob = if diverged { 0.0 } else { (-dh).exp().min(1.0) };

        let accepted = !diverged && rng.gen::<f64>() < accept_prob;

        if accepted {
            current = proposed;
            current_lp = proposed_lp;
        }

        if !is_warmup {
            if diverged {
                divergences += 1;
            }
            accept_sum_sampling += accept_prob;
            n_accept_sampling += 1;
            samples.push(current.clone());
        } else {
            // Dual-averaging step-size update during warmup
            let m = (iter + 1) as f64;
            h_bar =
                (1.0 - 1.0 / (m + t0)) * h_bar + (1.0 / (m + t0)) * (target_accept - accept_prob);
            let log_step = mu - (m.sqrt() / gamma) * h_bar;
            let frac = m.powf(-kappa);
            log_step_avg = frac * log_step + (1.0 - frac) * log_step_avg;
            step_size = log_step.exp();

            // Guard against pathological step size
            if !step_size.is_finite() || step_size <= 1e-10 {
                step_size = 1e-3;
                log_step_avg = step_size.ln();
            }
        }
    }

    // Freeze step size to its averaged value after warmup (Hoffman & Gelman 2014)
    let final_step_size = log_step_avg.exp();

    let accept_rate = if n_accept_sampling > 0 {
        accept_sum_sampling / n_accept_sampling as f64
    } else {
        0.0
    };

    Ok(ChainOutput {
        samples,
        divergences,
        final_step_size,
        accept_rate,
    })
}

/// Leapfrog integrator for HMC. Returns the proposed (params, momentum) after
/// `n_steps` leapfrog steps with step size `eps`.
fn leapfrog<F: Fn(&[f64]) -> Vec<f64>>(
    params: &[f64],
    momentum: &[f64],
    eps: f64,
    n_steps: u32,
    grad: &F,
) -> (Vec<f64>, Vec<f64>) {
    let mut q = params.to_vec();
    let mut p = momentum.to_vec();

    // Half-step momentum, then alternating full-step q and full-step p, then half-step p
    let g0 = grad(&q);
    for (pi, gi) in p.iter_mut().zip(g0.iter()) {
        *pi += 0.5 * eps * gi;
    }

    for step in 0..n_steps {
        // Full step in q
        for (qi, pi) in q.iter_mut().zip(p.iter()) {
            *qi += eps * pi;
        }
        // Full step in p (except the last leapfrog step, which does a half step)
        let g = grad(&q);
        if step < n_steps - 1 {
            for (pi, gi) in p.iter_mut().zip(g.iter()) {
                *pi += eps * gi;
            }
        } else {
            for (pi, gi) in p.iter_mut().zip(g.iter()) {
                *pi += 0.5 * eps * gi;
            }
        }
    }

    // Negate momentum to make the proposal symmetric (canonical HMC detail).
    // Since we discard momentum after each iteration, this is equivalent
    // to not negating, but we keep it for canonical correctness.
    for pi in p.iter_mut() {
        *pi = -*pi;
    }

    (q, p)
}

/// Sample from N(0, 1) via Box-Muller.
fn sample_standard_normal(rng: &mut StdRng) -> f64 {
    let u1: f64 = rng.gen_range(1e-12..1.0);
    let u2: f64 = rng.gen::<f64>();
    let r = (-2.0 * u1.ln()).sqrt();
    r * (2.0 * std::f64::consts::PI * u2).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampler_config_default() {
        let cfg = SamplerConfig::default();
        assert_eq!(cfg.n_chains, 4);
        assert_eq!(cfg.n_warmup, 500);
        assert_eq!(cfg.n_draws, 1000);
        assert!((cfg.target_accept_rate - 0.80).abs() < 1e-9);
    }

    #[test]
    fn sampler_config_serde() {
        let cfg = SamplerConfig::default();
        let v = serde_json::to_value(&cfg).unwrap();
        let back: SamplerConfig = serde_json::from_value(v).unwrap();
        assert_eq!(cfg.n_chains, back.n_chains);
    }

    #[test]
    fn box_muller_produces_normal() {
        let mut rng = StdRng::seed_from_u64(7);
        let samples: Vec<f64> = (0..10_000)
            .map(|_| sample_standard_normal(&mut rng))
            .collect();
        let mean: f64 = samples.iter().sum::<f64>() / samples.len() as f64;
        let var: f64 =
            samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (samples.len() as f64 - 1.0);
        assert!(mean.abs() < 0.05, "mean = {}", mean);
        assert!((var - 1.0).abs() < 0.1, "var = {}", var);
    }

    #[test]
    fn perturb_init_is_deterministic() {
        let a = perturb_init(&[0.0, 0.0], 1, 42);
        let b = perturb_init(&[0.0, 0.0], 1, 42);
        assert_eq!(a, b);
        let c = perturb_init(&[0.0, 0.0], 2, 42);
        assert_ne!(a, c);
    }
}
