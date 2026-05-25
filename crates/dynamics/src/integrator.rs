//! RK4 integrator wrapping `ode_solvers::Rk4`.
//!
//! The `DynamicsModel::system` signature takes `&[f64]` slices for
//! model-agnostic dispatch. This module adapts that to the `ode_solvers`
//! `System<f64, VectorN>` trait using nalgebra dynamic vectors.

use ode_solvers::{DVector, Rk4, System};
use crate::DynamicsModel;
use crate::coupled::CoupledSystem;

/// Adapter that bridges `DynamicsModel` (slice-based) to `ode_solvers::System`.
struct ModelAdapter<'a> {
    model: &'a dyn DynamicsModel,
    n: usize,
}

impl<'a> System<f64, DVector<f64>> for ModelAdapter<'a> {
    fn system(&self, t: f64, y: &DVector<f64>, dy: &mut DVector<f64>) {
        let y_slice = y.as_slice();
        let dy_slice = dy.as_mut_slice();
        self.model.system(t, y_slice, dy_slice);
    }
}

/// Integrate the model from t=0 to t=`horizon_days` with step `step_days`.
/// Returns `(t_days, state_vec)` sampled every `cadence_days`.
pub fn integrate(
    model: &dyn DynamicsModel,
    y0: &[f64],
    horizon_days: f64,
    step_days: f64,
    cadence_days: f64,
) -> Result<Vec<(f64, Vec<f64>)>, String> {
    let n = y0.len();
    if n == 0 {
        return Err("State vector must have at least one dimension".into());
    }
    if horizon_days <= 0.0 {
        return Err(format!("horizon_days must be > 0, got {horizon_days}"));
    }
    if step_days <= 0.0 || step_days > horizon_days {
        return Err(format!("step_days must be in (0, horizon_days], got {step_days}"));
    }

    let y0_vec = DVector::from_vec(y0.to_vec());
    let adapter = ModelAdapter { model, n };
    let mut solver = Rk4::new(adapter, 0.0_f64, y0_vec, horizon_days, step_days);

    solver.integrate()
        .map_err(|e| format!("Integration failed: {:?}", e))?;

    let x_out = solver.x_out();
    let y_out = solver.y_out();

    // Subsample to cadence_days
    let mut result: Vec<(f64, Vec<f64>)> = Vec::new();
    let mut last_sampled = -f64::INFINITY;

    for (t, y) in x_out.iter().zip(y_out.iter()) {
        if t - last_sampled >= cadence_days - 1e-9 {
            result.push((*t, y.as_slice().to_vec()));
            last_sampled = *t;
        }
    }

    // Always include the final point
    if let (Some(t_last), Some(y_last)) = (x_out.last(), y_out.last()) {
        if result.last().map(|(t, _)| *t).unwrap_or(-1.0) < *t_last - 1e-9 {
            result.push((*t_last, y_last.as_slice().to_vec()));
        }
    }

    Ok(result)
}

/// Adapter that bridges `CoupledSystem` (slice-based) to `ode_solvers::System`.
struct CoupledAdapter<'a> {
    coupled: &'a CoupledSystem<'a>,
}

impl<'a> System<f64, DVector<f64>> for CoupledAdapter<'a> {
    fn system(&self, t: f64, y: &DVector<f64>, dy: &mut DVector<f64>) {
        self.coupled.system(t, y.as_slice(), dy.as_mut_slice());
    }
}

/// Integrate a coupled multi-model system over the union state vector.
/// Same RK4 algorithm as `integrate` — the system function is the
/// `CoupledSystem` adapter that sums contributions from all models.
pub fn integrate_coupled(
    coupled: &CoupledSystem<'_>,
    y0: &[f64],
    horizon_days: f64,
    step_days: f64,
    cadence_days: f64,
) -> Result<Vec<(f64, Vec<f64>)>, String> {
    let n = y0.len();
    if n == 0 { return Err("Union state vector is empty".into()); }
    if horizon_days <= 0.0 { return Err(format!("horizon_days must be > 0, got {horizon_days}")); }
    if step_days <= 0.0 || step_days > horizon_days {
        return Err(format!("step_days must be in (0, horizon_days], got {step_days}"));
    }

    let y0_vec = DVector::from_vec(y0.to_vec());
    let adapter = CoupledAdapter { coupled };
    let mut solver = Rk4::new(adapter, 0.0_f64, y0_vec, horizon_days, step_days);

    solver.integrate().map_err(|e| format!("RK4 integration failed: {:?}", e))?;

    let x_out = solver.x_out();
    let y_out = solver.y_out();
    let mut result: Vec<(f64, Vec<f64>)> = Vec::new();
    let mut last_sampled = -f64::INFINITY;

    for (t, y) in x_out.iter().zip(y_out.iter()) {
        if t - last_sampled >= cadence_days - 1e-9 {
            result.push((*t, y.as_slice().to_vec()));
            last_sampled = *t;
        }
    }
    if let (Some(t_last), Some(y_last)) = (x_out.last(), y_out.last()) {
        if result.last().map(|(t, _)| *t).unwrap_or(-1.0) < *t_last - 1e-9 {
            result.push((*t_last, y_last.as_slice().to_vec()));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ExponentialDecay { pub k: f64 }

    impl DynamicsModel for ExponentialDecay {
        fn manifest(&self) -> crate::ModelManifest { unimplemented!() }
        fn system(&self, _t: f64, y: &[f64], dy: &mut [f64]) {
            dy[0] = -self.k * y[0];
        }
        fn state_order(&self) -> Vec<String> { vec!["test:value".into()] }
    }

    #[test]
    fn exponential_decay_matches_closed_form() {
        // y(t) = y0 * exp(-k*t)
        let model = ExponentialDecay { k: 0.5 };
        let y0 = vec![10.0_f64];
        let result = integrate(&model, &y0, 4.0, 0.001, 1.0).unwrap();

        // At t=4 days: y = 10 * exp(-0.5 * 4) = 10 * exp(-2) ≈ 1.353
        let final_pt = result.last().unwrap();
        let expected = 10.0 * (-0.5 * 4.0_f64).exp();
        assert!(
            (final_pt.1[0] - expected).abs() < 0.01,
            "Got {}, expected {}", final_pt.1[0], expected
        );
    }

    #[test]
    fn sampled_points_respect_cadence() {
        let model = ExponentialDecay { k: 0.1 };
        let result = integrate(&model, &[5.0], 6.0, 0.01, 1.0).unwrap();
        // 6 days at 1-day cadence → approximately 7 points (0..=6)
        assert!(result.len() >= 6 && result.len() <= 8,
            "Expected ~7 points, got {}", result.len());
    }
}
