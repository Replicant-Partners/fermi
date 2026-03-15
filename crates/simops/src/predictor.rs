/// OLS linear regression predictor.
///
/// Implements the normal equations:  β = (XᵀX)⁻¹ Xᵀy
///
/// Design choices:
/// - No external linear-algebra dependency: we solve the n×n system (n ≤ ~10
///   features) using Gaussian elimination with partial pivoting.
/// - Features and target are identified by name strings, making the predictor
///   domain-agnostic and SOSA-friendly (feature names can be observable
///   property URIs).
/// - The model stores raw coefficients + intercept and exposes a `predict`
///   method that accepts a HashMap of feature values.
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::SimOpsError;

// ─── Training data ────────────────────────────────────────────────────────────

/// A single historical observation used for training.
/// `features` maps observable property names → measured values.
/// `target` is the outcome to predict (e.g. biomass yield in kg).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingObservation {
    pub features: HashMap<String, f64>,
    pub target: f64,
}

// ─── Model ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Predictor {
    /// Ordered feature names (defines the column order in X)
    pub feature_names: Vec<String>,
    /// Regression coefficients (one per feature, same order as feature_names)
    pub coefficients: Vec<f64>,
    /// Intercept (bias) term
    pub intercept: f64,
    /// R² on the training set (goodness of fit)
    pub r_squared: f64,
    /// Number of training samples used
    pub n_samples: usize,
}

impl Predictor {
    /// Fit the model from a slice of training observations.
    ///
    /// # Errors
    /// - Fewer than 2 samples
    /// - Features are inconsistent across samples
    /// - Matrix is singular (e.g. perfectly collinear features)
    pub fn fit(observations: &[TrainingObservation]) -> Result<Self, SimOpsError> {
        if observations.len() < 2 {
            return Err(SimOpsError::InsufficientData {
                need: 2,
                have: observations.len(),
            });
        }

        // Determine feature order from the first sample (sorted for determinism)
        let mut feature_names: Vec<String> =
            observations[0].features.keys().cloned().collect();
        feature_names.sort();
        let p = feature_names.len(); // number of features
        let n = observations.len(); // number of samples

        // Build design matrix X (n × (p+1)) with bias column prepended
        let mut x = vec![0.0_f64; n * (p + 1)];
        let mut y = vec![0.0_f64; n];

        for (i, obs) in observations.iter().enumerate() {
            x[i * (p + 1)] = 1.0; // intercept column
            for (j, name) in feature_names.iter().enumerate() {
                x[i * (p + 1) + j + 1] = *obs.features.get(name).ok_or_else(|| {
                    SimOpsError::MissingFeature(name.clone())
                })?;
            }
            y[i] = obs.target;
        }

        // Compute XᵀX  [(p+1) × (p+1)] and Xᵀy [(p+1)]
        let cols = p + 1;
        let mut xtx = vec![0.0_f64; cols * cols];
        let mut xty = vec![0.0_f64; cols];

        for i in 0..n {
            for j in 0..cols {
                xty[j] += x[i * cols + j] * y[i];
                for k in 0..cols {
                    xtx[j * cols + k] += x[i * cols + j] * x[i * cols + k];
                }
            }
        }

        // Ridge regularisation (λ = 1e-8) on feature columns only (not intercept).
        // Prevents singular matrix when features are collinear or n ≈ p.
        let lambda = 1e-8_f64;
        for j in 1..cols {
            xtx[j * cols + j] += lambda;
        }

        // Solve (XᵀX) β = Xᵀy via Gaussian elimination with partial pivoting
        let beta = gaussian_solve(&xtx, &xty, cols)
            .ok_or(SimOpsError::SingularMatrix)?;

        let intercept = beta[0];
        let coefficients = beta[1..].to_vec();

        // Compute R² on training data
        let y_mean = y.iter().sum::<f64>() / n as f64;
        let ss_res: f64 = (0..n)
            .map(|i| {
                let y_hat = intercept
                    + feature_names
                        .iter()
                        .enumerate()
                        .map(|(j, _)| coefficients[j] * x[i * cols + j + 1])
                        .sum::<f64>();
                (y[i] - y_hat).powi(2)
            })
            .sum();
        let ss_tot: f64 = y.iter().map(|yi| (yi - y_mean).powi(2)).sum();
        let r_squared = if ss_tot > 0.0 { 1.0 - ss_res / ss_tot } else { 1.0 };

        Ok(Predictor {
            feature_names,
            coefficients,
            intercept,
            r_squared,
            n_samples: n,
        })
    }

    /// Predict the target for a new observation.
    /// Returns `0.0` as a floor (negative yields are not physical).
    pub fn predict(&self, features: &HashMap<String, f64>) -> Result<f64, SimOpsError> {
        let raw: f64 = self.intercept
            + self
                .feature_names
                .iter()
                .enumerate()
                .map(|(j, name)| {
                    let v = features.get(name).copied().unwrap_or(0.0);
                    self.coefficients[j] * v
                })
                .sum::<f64>();
        Ok(raw.max(0.0))
    }
}

// ─── Gaussian elimination (partial pivoting) ─────────────────────────────────

/// Solves Ax = b for x given A as a flat row-major array and b.
/// Returns None if A is singular.
fn gaussian_solve(a: &[f64], b: &[f64], n: usize) -> Option<Vec<f64>> {
    // Augmented matrix [A | b]
    let mut mat: Vec<f64> = Vec::with_capacity(n * (n + 1));
    for i in 0..n {
        for j in 0..n {
            mat.push(a[i * n + j]);
        }
        mat.push(b[i]);
    }
    let cols = n + 1;

    for col in 0..n {
        // Find pivot
        let pivot_row = (col..n)
            .max_by(|&r1, &r2| {
                mat[r1 * cols + col]
                    .abs()
                    .partial_cmp(&mat[r2 * cols + col].abs())
                    .unwrap()
            })?;

        if mat[pivot_row * cols + col].abs() < 1e-12 {
            return None; // singular
        }

        // Swap rows
        if pivot_row != col {
            for k in 0..cols {
                mat.swap(col * cols + k, pivot_row * cols + k);
            }
        }

        let pivot = mat[col * cols + col];
        // Eliminate below
        for row in (col + 1)..n {
            let factor = mat[row * cols + col] / pivot;
            for k in col..cols {
                let sub = factor * mat[col * cols + k];
                mat[row * cols + k] -= sub;
            }
        }
    }

    // Back substitution
    let mut x = vec![0.0_f64; n];
    for i in (0..n).rev() {
        let sum: f64 = (i + 1..n).map(|j| mat[i * cols + j] * x[j]).sum();
        x[i] = (mat[i * cols + n] - sum) / mat[i * cols + i];
    }
    Some(x)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn historical_logs() -> Vec<TrainingObservation> {
        // Format: [lighting_kwh, nutrients_kg, temp_c] → yield_kg
        vec![
            (100.0, 5.0, 24.0, 4.0),
            (110.0, 5.5, 25.0, 4.4),
            (120.0, 6.0, 26.0, 4.8),
            (95.0, 4.8, 23.0, 3.8),
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
        .collect()
    }

    #[test]
    fn fits_without_error() {
        let model = Predictor::fit(&historical_logs()).unwrap();
        assert_eq!(model.feature_names.len(), 3);
        assert_eq!(model.coefficients.len(), 3);
    }

    #[test]
    fn prediction_is_non_negative() {
        let model = Predictor::fit(&historical_logs()).unwrap();
        let features = HashMap::from([
            ("lighting_kwh".to_string(), 135.0),
            ("nutrients_kg".to_string(), 6.8),
            ("temp_c".to_string(), 27.5),
        ]);
        let pred = model.predict(&features).unwrap();
        assert!(pred >= 0.0);
    }

    #[test]
    fn r_squared_high_for_linear_data() {
        let model = Predictor::fit(&historical_logs()).unwrap();
        // The training data is perfectly linear in lighting_kwh, so R² should be high
        assert!(model.r_squared > 0.9, "R² was {}", model.r_squared);
    }

    #[test]
    fn insufficient_data_returns_error() {
        let result = Predictor::fit(&historical_logs()[..1]);
        assert!(matches!(result, Err(SimOpsError::InsufficientData { .. })));
    }

    #[test]
    fn gaussian_solve_simple() {
        // 2x + y = 5, x + 3y = 10 → x = 1, y = 3
        let a = vec![2.0, 1.0, 1.0, 3.0];
        let b = vec![5.0, 10.0];
        let x = gaussian_solve(&a, &b, 2).unwrap();
        assert!((x[0] - 1.0).abs() < 1e-9);
        assert!((x[1] - 3.0).abs() < 1e-9);
    }
}
