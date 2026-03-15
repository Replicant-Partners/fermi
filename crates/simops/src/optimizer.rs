/// What-if optimizer: given a target output quantity, solve for the required
/// input feature values using a trained `Predictor`.
///
/// Strategy:
///   The predictor learned  y = β₀ + β₁x₁ + β₂x₂ + … + βₙxₙ
///   We want to find x* such that y(x*) = target.
///
///   With multiple free inputs this is under-determined.  We solve it in two
///   complementary modes:
///
///   1. `scale_from_reference` — proportionally scale a reference operating
///      point until the prediction hits the target.  Simple, interpretable,
///      physically plausible.
///
///   2. `single_input_solve` — hold all but one input fixed and compute
///      analytically the single free variable that hits the target (direct
///      division of the residual by its coefficient).
///
/// Both modes clamp results to `[0, max_scale × reference]` to prevent
/// physically absurd solutions.
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::SimOpsError;
use crate::predictor::Predictor;

// ─── Result ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationResult {
    /// Whether the solver hit the target within `tolerance`
    pub converged: bool,
    /// Solved input feature values
    pub inputs: HashMap<String, f64>,
    /// Predicted output for the solved inputs
    pub predicted_output: f64,
    /// Absolute error vs target
    pub residual: f64,
    /// Target that was requested
    pub target: f64,
    pub method: String,
}

// ─── Solver ───────────────────────────────────────────────────────────────────

/// Proportionally scale a reference operating point to hit `target_output`.
///
/// The scale factor s is found by solving:
///   β₀ + s·(β₁r₁ + β₂r₂ + … + βₙrₙ) = target
///   s = (target − β₀) / Σ(βⱼ rⱼ)
///
/// All input values are then multiplied by s.
pub fn scale_from_reference(
    predictor: &Predictor,
    reference: &HashMap<String, f64>,
    target_output: f64,
    max_scale: f64,
) -> Result<OptimizationResult, SimOpsError> {
    // Weighted sum of reference inputs
    let weighted_sum: f64 = predictor
        .feature_names
        .iter()
        .enumerate()
        .map(|(j, name)| {
            let r = reference.get(name).copied().unwrap_or(0.0);
            predictor.coefficients[j] * r
        })
        .sum();

    if weighted_sum.abs() < 1e-12 {
        return Err(SimOpsError::SingularMatrix); // degenerate reference
    }

    let scale = ((target_output - predictor.intercept) / weighted_sum)
        .max(0.0)
        .min(max_scale);

    let inputs: HashMap<String, f64> = predictor
        .feature_names
        .iter()
        .map(|name| {
            let r = reference.get(name).copied().unwrap_or(0.0);
            (name.clone(), r * scale)
        })
        .collect();

    let predicted_output = predictor.predict(&inputs)?;
    let residual = (predicted_output - target_output).abs();

    Ok(OptimizationResult {
        converged: residual / target_output.max(1e-9) < 0.01,
        inputs,
        predicted_output,
        residual,
        target: target_output,
        method: "scale_from_reference".into(),
    })
}

/// Hold all inputs fixed except `free_feature`, then solve analytically.
///
/// y = β₀ + βᶠ·xᶠ + Σ(βⱼxⱼ for j≠f)
/// xᶠ = (target − β₀ − fixed_sum) / βᶠ
pub fn single_input_solve(
    predictor: &Predictor,
    fixed_inputs: &HashMap<String, f64>,
    free_feature: &str,
    target_output: f64,
    min_value: f64,
    max_value: f64,
) -> Result<OptimizationResult, SimOpsError> {
    let free_idx = predictor
        .feature_names
        .iter()
        .position(|n| n == free_feature)
        .ok_or_else(|| SimOpsError::MissingFeature(free_feature.to_string()))?;

    let beta_free = predictor.coefficients[free_idx];
    if beta_free.abs() < 1e-12 {
        return Err(SimOpsError::SingularMatrix);
    }

    // Sum contributions of all fixed features
    let fixed_sum: f64 = predictor
        .feature_names
        .iter()
        .enumerate()
        .filter(|(j, _)| *j != free_idx)
        .map(|(j, name)| {
            predictor.coefficients[j] * fixed_inputs.get(name).copied().unwrap_or(0.0)
        })
        .sum();

    let free_value = ((target_output - predictor.intercept - fixed_sum) / beta_free)
        .max(min_value)
        .min(max_value);

    let mut inputs = fixed_inputs.clone();
    inputs.insert(free_feature.to_string(), free_value);

    let predicted_output = predictor.predict(&inputs)?;
    let residual = (predicted_output - target_output).abs();

    Ok(OptimizationResult {
        converged: residual / target_output.max(1e-9) < 0.01,
        inputs,
        predicted_output,
        residual,
        target: target_output,
        method: format!("single_input_solve({})", free_feature),
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::predictor::TrainingObservation;

    fn trained_model() -> Predictor {
        let logs = vec![
            (100.0, 5.0, 24.0, 4.0),
            (110.0, 5.5, 25.0, 4.4),
            (120.0, 6.0, 26.0, 4.8),
            (95.0, 4.8, 23.0, 3.8),
            (130.0, 6.5, 27.0, 5.2),
        ]
        .into_iter()
        .map(|(l, n, t, y)| TrainingObservation {
            features: HashMap::from([
                ("lighting_kwh".to_string(), l),
                ("nutrients_kg".to_string(), n),
                ("temp_c".to_string(), t),
            ]),
            target: y,
        })
        .collect::<Vec<_>>();
        Predictor::fit(&logs).unwrap()
    }

    fn reference_ops() -> HashMap<String, f64> {
        HashMap::from([
            ("lighting_kwh".to_string(), 120.0),
            ("nutrients_kg".to_string(), 6.0),
            ("temp_c".to_string(), 26.0),
        ])
    }

    #[test]
    fn scale_solver_converges() {
        let model = trained_model();
        let result = scale_from_reference(&model, &reference_ops(), 5.5, 3.0).unwrap();
        assert!(result.converged, "residual was {}", result.residual);
        assert!(result.predicted_output >= 0.0);
    }

    #[test]
    fn single_input_solver_converges() {
        let model = trained_model();
        let fixed = HashMap::from([
            ("nutrients_kg".to_string(), 6.0),
            ("temp_c".to_string(), 26.0),
        ]);
        let result =
            single_input_solve(&model, &fixed, "lighting_kwh", 5.2, 0.0, 500.0).unwrap();
        assert!(result.converged, "residual was {}", result.residual);
        // lighting should be higher than reference to hit a higher target
        assert!(*result.inputs.get("lighting_kwh").unwrap() > 120.0);
    }

    #[test]
    fn missing_free_feature_returns_error() {
        let model = trained_model();
        let result = single_input_solve(&model, &reference_ops(), "co2_ppm", 5.0, 0.0, 1000.0);
        assert!(matches!(result, Err(SimOpsError::MissingFeature(_))));
    }
}
