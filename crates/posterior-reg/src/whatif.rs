//! What-if query methods on `ConditionalPosterior` (Spec 14 §5.2).
//!
//! Use cases A–D:
//! - A: `predict()` — full predictive distribution at new features
//! - B: `input_sensitivity()` — Sobol-style indices over the posterior predictive
//! - C: `compare_scenarios()` — full distribution comparison
//! - D: `prob_exceeds()` + `optimise_for_target()` — planning under constraint

use std::collections::HashMap;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::{ConditionalPosterior, FittedDistribution, RegressionError};

// ─── Use case A — conditional prediction ──────────────────────────────────────

/// Compute the posterior predictive distribution at `features`.
///
/// Mixture of Normals (one per posterior sample). We collapse it to a Normal
/// using the moments of the mixture (total expectation + variance of conditional
/// mean + variance of conditional std). This is what the downstream FPL Driver
/// needs as `Normal(μ, σ)`.
pub fn predict(
    posterior: &ConditionalPosterior,
    features: &HashMap<String, f64>,
) -> Result<FittedDistribution, RegressionError> {
    if posterior.samples.is_empty() {
        return Err(RegressionError::EmptyPosterior);
    }

    let model = posterior.recover_model()?;
    let fv = extract_features(features, &posterior.feature_names)?;

    // For each posterior sample, compute (predictive mean, predictive std)
    let mut means = Vec::with_capacity(posterior.samples.len());
    let mut stds = Vec::with_capacity(posterior.samples.len());
    for params in &posterior.samples {
        let m = model.predict_mean(params, &fv);
        let s = model.predict_std(params, &fv);
        if m.is_finite() && s.is_finite() && s > 0.0 {
            means.push(m);
            stds.push(s);
        }
    }

    if means.is_empty() {
        return Err(RegressionError::NonFinitePrediction {
            mean: f64::NAN,
            std: f64::NAN,
        });
    }

    // Mixture moments:
    //   E[Y]     = E[μ_i]
    //   Var[Y]   = E[σ_i²] + Var[μ_i]
    let mean_of_means: f64 = means.iter().sum::<f64>() / means.len() as f64;
    let var_of_means: f64 = means.iter().map(|m| (m - mean_of_means).powi(2)).sum::<f64>()
        / (means.len() as f64);
    let mean_of_vars: f64 = stds.iter().map(|s| s * s).sum::<f64>() / stds.len() as f64;
    let total_var = mean_of_vars + var_of_means;
    let total_std = total_var.sqrt();

    if !total_std.is_finite() || total_std <= 0.0 {
        return Err(RegressionError::InvalidStd { got: total_std });
    }

    // 90% predictive interval (Normal ±1.645σ)
    let ci_low = mean_of_means - 1.6449 * total_std;
    let ci_high = mean_of_means + 1.6449 * total_std;

    Ok(FittedDistribution::Normal {
        mean: mean_of_means,
        std_dev: total_std,
        ci_low,
        ci_high,
        n_eff: posterior.samples.len() as f64,
    })
}

// ─── Use case B — input sensitivity ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSensitivity {
    pub feature_name: String,
    /// First-order Sobol index — direct effect of this feature alone.
    pub first_order_index: f64,
    /// Total-order Sobol index — total effect including interactions.
    pub total_order_index: f64,
    /// 90% bootstrap CI on the total-order index.
    pub ci: (f64, f64),
}

/// Sobol sensitivity analysis on the posterior predictive mean.
///
/// Algorithm: Saltelli's pick-freeze with two independent feature matrices A, B
/// and crossed matrices C_i where column `i` of C_i comes from B and the rest
/// from A. First-order from `Var_x(E_y[Y|X_i])`, total-order from `Var(Y) -
/// Var_x(E_y[Y|X_~i])`.
///
/// We use the *posterior predictive mean* (averaged over posterior samples) as
/// the function under analysis. This is approximate — full Sobol over the
/// posterior would require sampling the posterior at each Saltelli evaluation
/// — but is fast and informative enough for what-if exploration.
pub fn input_sensitivity(
    posterior: &ConditionalPosterior,
    feature_ranges: &HashMap<String, (f64, f64)>,
    n_samples: usize,
) -> Result<HashMap<String, InputSensitivity>, RegressionError> {
    if posterior.samples.is_empty() {
        return Err(RegressionError::EmptyPosterior);
    }
    let n = n_samples.max(64);
    let model = posterior.recover_model()?;
    let feature_names = &posterior.feature_names;
    let p = feature_names.len();

    // Validate ranges
    let mut ranges_vec: Vec<(f64, f64)> = Vec::with_capacity(p);
    for fname in feature_names {
        let (lo, hi) = feature_ranges.get(fname).ok_or_else(|| {
            RegressionError::MissingFeature {
                name: fname.clone(),
                sample: format!("feature_ranges {:?}", feature_ranges.keys().collect::<Vec<_>>()),
            }
        })?;
        if !lo.is_finite() || !hi.is_finite() || hi <= lo {
            return Err(RegressionError::Internal(format!(
                "invalid range for {}: ({}, {})",
                fname, lo, hi
            )));
        }
        ranges_vec.push((*lo, *hi));
    }

    let mut rng = StdRng::seed_from_u64(0x50B0_50B0);

    // Predictive mean averaged over posterior — domain to do Sobol on.
    let f = |x: &[f64]| -> f64 {
        let mut total = 0.0;
        let mut cnt = 0;
        for params in &posterior.samples {
            let m = model.predict_mean(params, x);
            if m.is_finite() {
                total += m;
                cnt += 1;
            }
        }
        if cnt == 0 {
            0.0
        } else {
            total / cnt as f64
        }
    };

    // Sample matrices A and B uniformly over feature ranges
    let sample_matrix = |rng: &mut StdRng| -> Vec<Vec<f64>> {
        (0..n)
            .map(|_| {
                ranges_vec
                    .iter()
                    .map(|(lo, hi)| rng.gen_range(*lo..*hi))
                    .collect()
            })
            .collect()
    };
    let a = sample_matrix(&mut rng);
    let b = sample_matrix(&mut rng);

    // Evaluate f(A) and f(B)
    let f_a: Vec<f64> = a.iter().map(|x| f(x)).collect();
    let f_b: Vec<f64> = b.iter().map(|x| f(x)).collect();

    // Total variance (use combined A, B)
    let combined: Vec<f64> = f_a.iter().chain(f_b.iter()).cloned().collect();
    let mean_y: f64 = combined.iter().sum::<f64>() / combined.len() as f64;
    let var_y: f64 =
        combined.iter().map(|y| (y - mean_y).powi(2)).sum::<f64>() / combined.len() as f64;

    let mut out = HashMap::new();
    if var_y <= 0.0 {
        // Output is constant → all sensitivities are zero
        for fname in feature_names {
            out.insert(
                fname.clone(),
                InputSensitivity {
                    feature_name: fname.clone(),
                    first_order_index: 0.0,
                    total_order_index: 0.0,
                    ci: (0.0, 0.0),
                },
            );
        }
        return Ok(out);
    }

    for (i, fname) in feature_names.iter().enumerate() {
        // C_i: from A, but column i comes from B
        let c_i: Vec<Vec<f64>> = a
            .iter()
            .zip(b.iter())
            .map(|(ra, rb)| {
                let mut row = ra.clone();
                row[i] = rb[i];
                row
            })
            .collect();
        let f_ci: Vec<f64> = c_i.iter().map(|x| f(x)).collect();

        // Saltelli first-order: S_i = (1/N) Σ f(B) * (f(C_i) - f(A)) / Var(Y)
        let s_first: f64 = f_b
            .iter()
            .zip(f_a.iter())
            .zip(f_ci.iter())
            .map(|((fb, fa), fc)| fb * (fc - fa))
            .sum::<f64>()
            / (n as f64 * var_y);

        // Saltelli total-order: S_T_i = (1/(2N)) Σ (f(A) - f(C_i))² / Var(Y)
        let s_total: f64 = f_a
            .iter()
            .zip(f_ci.iter())
            .map(|(fa, fc)| (fa - fc).powi(2))
            .sum::<f64>()
            / (2.0 * n as f64 * var_y);

        // Bootstrap CI on total-order (200 resamples — kept small for speed)
        let n_boot = 200;
        let mut boot_totals = Vec::with_capacity(n_boot);
        for _ in 0..n_boot {
            let mut s = 0.0;
            for _ in 0..n {
                let idx = rng.gen_range(0..n);
                s += (f_a[idx] - f_ci[idx]).powi(2);
            }
            boot_totals.push(s / (2.0 * n as f64 * var_y));
        }
        boot_totals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let lo_idx = (0.05 * n_boot as f64) as usize;
        let hi_idx = (0.95 * n_boot as f64) as usize;

        out.insert(
            fname.clone(),
            InputSensitivity {
                feature_name: fname.clone(),
                first_order_index: s_first.max(0.0).min(1.0),
                total_order_index: s_total.max(0.0).min(1.0),
                ci: (boot_totals[lo_idx], boot_totals[hi_idx]),
            },
        );
    }

    Ok(out)
}

// ─── Use case C — scenario comparison ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioComparison {
    pub a: FittedDistribution,
    pub b: FittedDistribution,
    /// P(outcome_A > outcome_B) under the joint posterior predictive.
    pub prob_a_better: f64,
    /// E[outcome_A - outcome_B].
    pub expected_gain: f64,
    /// std_a / std_b. < 1 means A is less risky.
    pub risk_ratio: f64,
}

pub fn compare_scenarios(
    posterior: &ConditionalPosterior,
    scenario_a: &HashMap<String, f64>,
    scenario_b: &HashMap<String, f64>,
) -> Result<ScenarioComparison, RegressionError> {
    let dist_a = predict(posterior, scenario_a)?;
    let dist_b = predict(posterior, scenario_b)?;

    let (mean_a, std_a) = unpack_normal(&dist_a)?;
    let (mean_b, std_b) = unpack_normal(&dist_b)?;

    // For two independent Normals: A - B ~ Normal(mean_a - mean_b, sqrt(std_a² + std_b²))
    let diff_mean = mean_a - mean_b;
    let diff_std = (std_a * std_a + std_b * std_b).sqrt();
    let prob_a_better = if diff_std > 0.0 {
        // P(A > B) = P(A - B > 0) = 1 - Φ(0; diff_mean, diff_std)
        //         = 1 - 0.5*(1 + erf(-diff_mean/(diff_std*sqrt(2))))
        //         = 0.5*(1 - erf(-diff_mean/(diff_std*sqrt(2))))
        //         = 0.5*(1 + erf(diff_mean/(diff_std*sqrt(2))))
        0.5 * (1.0 + erf(diff_mean / (diff_std * std::f64::consts::SQRT_2)))
    } else if diff_mean > 0.0 {
        1.0
    } else if diff_mean < 0.0 {
        0.0
    } else {
        0.5
    };

    let risk_ratio = if std_b > 0.0 { std_a / std_b } else { f64::INFINITY };

    Ok(ScenarioComparison {
        a: dist_a,
        b: dist_b,
        prob_a_better,
        expected_gain: diff_mean,
        risk_ratio,
    })
}

// ─── Use case D — planning under constraint ───────────────────────────────────

pub fn prob_exceeds(
    posterior: &ConditionalPosterior,
    features: &HashMap<String, f64>,
    threshold: f64,
) -> Result<f64, RegressionError> {
    let dist = predict(posterior, features)?;
    let (mean, std) = unpack_normal(&dist)?;
    if std <= 0.0 {
        return Ok(if mean > threshold { 1.0 } else { 0.0 });
    }
    // P(Y >= threshold) = 1 - Φ(threshold; mean, std)
    let z = (threshold - mean) / (std * std::f64::consts::SQRT_2);
    Ok(0.5 * (1.0 - erf(z)))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimisationResult {
    pub recommended_value: f64,
    pub prob_at_recommended: f64,
    pub predictive_dist: FittedDistribution,
    /// `(feature_value, prob_exceeds)` grid sweep.
    pub sensitivity_curve: Vec<(f64, f64)>,
}

pub fn optimise_for_target(
    posterior: &ConditionalPosterior,
    fixed_features: &HashMap<String, f64>,
    free_feature: &str,
    search_range: (f64, f64),
    target_threshold: f64,
) -> Result<OptimisationResult, RegressionError> {
    let (lo, hi) = search_range;
    if !lo.is_finite() || !hi.is_finite() || hi <= lo {
        return Err(RegressionError::Internal(format!(
            "invalid search_range: ({}, {})",
            lo, hi
        )));
    }
    if !posterior.feature_names.iter().any(|n| n == free_feature) {
        return Err(RegressionError::MissingFeature {
            name: free_feature.to_string(),
            sample: format!("posterior features: {:?}", posterior.feature_names),
        });
    }

    let n_grid = 41;
    let mut best_value = lo;
    let mut best_prob = -1.0;
    let mut curve = Vec::with_capacity(n_grid);

    for i in 0..n_grid {
        let v = lo + (hi - lo) * (i as f64) / (n_grid as f64 - 1.0);
        let mut features = fixed_features.clone();
        features.insert(free_feature.to_string(), v);
        let p = prob_exceeds(posterior, &features, target_threshold)?;
        curve.push((v, p));
        if p > best_prob {
            best_prob = p;
            best_value = v;
        }
    }

    let mut features = fixed_features.clone();
    features.insert(free_feature.to_string(), best_value);
    let predictive_dist = predict(posterior, &features)?;

    Ok(OptimisationResult {
        recommended_value: best_value,
        prob_at_recommended: best_prob,
        predictive_dist,
        sensitivity_curve: curve,
    })
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn extract_features(
    features: &HashMap<String, f64>,
    feature_names: &[String],
) -> Result<Vec<f64>, RegressionError> {
    let mut out = Vec::with_capacity(feature_names.len());
    for name in feature_names {
        let v = features
            .get(name)
            .copied()
            .ok_or_else(|| RegressionError::MissingFeature {
                name: name.clone(),
                sample: format!("query features {:?}", features.keys().collect::<Vec<_>>()),
            })?;
        if !v.is_finite() {
            return Err(RegressionError::NonFiniteFeature { name: name.clone() });
        }
        out.push(v);
    }
    Ok(out)
}

fn unpack_normal(d: &FittedDistribution) -> Result<(f64, f64), RegressionError> {
    match d {
        FittedDistribution::Normal { mean, std_dev, .. } => Ok((*mean, *std_dev)),
        _ => Err(RegressionError::Internal(format!(
            "expected Normal predictive, got {:?}",
            d
        ))),
    }
}

/// Abramowitz–Stegun approximation of erf (max error ~1.5e-7). Sufficient for
/// Φ calculations in `compare_scenarios` and `prob_exceeds`.
fn erf(x: f64) -> f64 {
    // Constants from A&S 7.1.26
    const A1: f64 = 0.254829592;
    const A2: f64 = -0.284496736;
    const A3: f64 = 1.421413741;
    const A4: f64 = -1.453152027;
    const A5: f64 = 1.061405429;
    const P: f64 = 0.3275911;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + P * x);
    let y = 1.0 - (((((A5 * t + A4) * t) + A3) * t + A2) * t + A1) * t * (-x * x).exp();
    sign * y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erf_matches_known_values() {
        // erf(0) = 0
        assert!(erf(0.0).abs() < 1e-7);
        // erf(1) ≈ 0.8427
        assert!((erf(1.0) - 0.8427).abs() < 1e-3);
        // erf(-1) = -erf(1)
        assert!((erf(-1.0) + erf(1.0)).abs() < 1e-7);
        // erf(∞) → 1
        assert!((erf(5.0) - 1.0).abs() < 1e-7);
    }

    #[test]
    fn input_sensitivity_serde() {
        let s = InputSensitivity {
            feature_name: "x".to_string(),
            first_order_index: 0.6,
            total_order_index: 0.75,
            ci: (0.65, 0.85),
        };
        let v = serde_json::to_value(&s).unwrap();
        let back: InputSensitivity = serde_json::from_value(v).unwrap();
        assert_eq!(s.feature_name, back.feature_name);
    }

    #[test]
    fn scenario_comparison_serde() {
        let s = ScenarioComparison {
            a: FittedDistribution::Normal {
                mean: 5.0,
                std_dev: 1.0,
                ci_low: 3.35,
                ci_high: 6.65,
                n_eff: 100.0,
            },
            b: FittedDistribution::Normal {
                mean: 4.0,
                std_dev: 1.0,
                ci_low: 2.35,
                ci_high: 5.65,
                n_eff: 100.0,
            },
            prob_a_better: 0.76,
            expected_gain: 1.0,
            risk_ratio: 1.0,
        };
        let v = serde_json::to_value(&s).unwrap();
        let back: ScenarioComparison = serde_json::from_value(v).unwrap();
        assert!((s.prob_a_better - back.prob_a_better).abs() < 1e-12);
    }

    #[test]
    fn optimisation_result_serde() {
        let r = OptimisationResult {
            recommended_value: 4.2,
            prob_at_recommended: 0.83,
            predictive_dist: FittedDistribution::Normal {
                mean: 5.0,
                std_dev: 0.5,
                ci_low: 4.18,
                ci_high: 5.82,
                n_eff: 100.0,
            },
            sensitivity_curve: vec![(1.0, 0.1), (2.0, 0.4), (3.0, 0.8)],
        };
        let v = serde_json::to_value(&r).unwrap();
        let back: OptimisationResult = serde_json::from_value(v).unwrap();
        assert_eq!(r.sensitivity_curve.len(), back.sensitivity_curve.len());
    }
}
