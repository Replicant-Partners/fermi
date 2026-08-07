//! Linear decay model: dv/dt = -k(v - target)
//!
//! General-purpose single-state relaxation. Useful for any sensor
//! that decays toward a floor (e.g. brix toward minimum, pH toward floor).
//! The registry constructs instances with the correct property URI.

use crate::{
    manifest::{ContextSchema, ContextSource, ContributionMode, ParamSchema, StateFieldSchema},
    DynamicsModel, ModelManifest, Note,
};
use std::collections::BTreeMap;

pub struct LinearDecay {
    /// The property URI this instance models (e.g. "chem:ph_value").
    pub property_uri: String,
    /// Decay rate k (day⁻¹). Computed from Arrhenius if temperature is available.
    pub k: f64,
    /// Equilibrium target the value decays toward.
    pub target: f64,
}

impl LinearDecay {
    pub fn new(property_uri: impl Into<String>, k: f64, target: f64) -> Self {
        Self {
            property_uri: property_uri.into(),
            k,
            target,
        }
    }
}

impl DynamicsModel for LinearDecay {
    fn manifest(&self) -> ModelManifest {
        ModelManifest {
            uri: "kask:dynamics/linear_decay@v1".into(),
            version: "1.0.0".into(),
            name: "Linear decay (first-order relaxation)".into(),
            description: "dv/dt = -k(v - target). General-purpose single-state relaxation toward a target value.".into(),
            applies_to_set: vec![self.property_uri.clone()],
            state_schema: BTreeMap::from([(
                self.property_uri.clone(),
                StateFieldSchema {
                    label: "Sensor value".into(),
                    units: "property-dependent".into(),
                    description: "Value relaxing toward target.".into(),
                    typical_range: None,
                    contribution: ContributionMode::Additive,
                },
            )]),
            params_schema: BTreeMap::from([
                ("k".into(), ParamSchema {
                    label: "Decay rate".into(),
                    units: "day⁻¹".into(),
                    description: "First-order rate constant.".into(),
                    default: 0.1,
                    typical_range: Some((0.01, 2.0)),
                }),
                ("target".into(), ParamSchema {
                    label: "Target value".into(),
                    units: "property-dependent".into(),
                    description: "Equilibrium value the state decays toward.".into(),
                    default: 0.0,
                    typical_range: None,
                }),
            ]),
            context_schema: BTreeMap::new(),
            default_params: BTreeMap::from([
                ("k".into(), 0.1),
                ("target".into(), 0.0),
            ]),
            default_integrator: "rk4".into(),
            default_step_days: 0.01,
            citations: vec![],
        }
    }

    fn system(&self, _t: f64, y: &[f64], dy: &mut [f64]) {
        dy[0] = -self.k * (y[0] - self.target);
    }

    fn state_order(&self) -> Vec<String> {
        vec![self.property_uri.clone()]
    }

    fn is_converged(&self, history: &[(f64, Vec<f64>)]) -> bool {
        if history.len() < 5 {
            return false;
        }
        let last_5 = &history[history.len() - 5..];
        let max_delta = last_5
            .windows(2)
            .map(|w| (w[1].1[0] - w[0].1[0]).abs())
            .fold(0.0_f64, f64::max);
        max_delta < 1e-4
    }

    fn generate_notes(&self, trajectory: &[(f64, Vec<f64>)]) -> Vec<Note> {
        let mut notes = Vec::new();
        if let Some((t, y)) = trajectory.last() {
            if (y[0] - self.target).abs() > 0.5 {
                notes.push(Note {
                    severity: "info".into(),
                    message: format!(
                        "Value {:.3} has not fully converged to target {:.3} by day {:.1}",
                        y[0], self.target, t
                    ),
                    t_hours: Some(t * 24.0),
                });
            }
        }
        notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decays_toward_target() {
        let model = LinearDecay::new("chem:ph_value", 0.3, 3.0);
        let mut y = vec![7.0_f64];
        let mut dy = vec![0.0_f64];
        model.system(0.0, &y, &mut dy);
        // dy should be negative (decaying from 7 toward 3)
        assert!(dy[0] < 0.0, "Expected negative derivative, got {}", dy[0]);
        // dy = -0.3 * (7 - 3) = -1.2
        assert!((dy[0] - (-1.2)).abs() < 1e-9);
    }

    #[test]
    fn at_target_derivative_is_zero() {
        let model = LinearDecay::new("chem:ph_value", 0.3, 3.0);
        let y = vec![3.0_f64];
        let mut dy = vec![0.0_f64];
        model.system(0.0, &y, &mut dy);
        assert!(dy[0].abs() < 1e-10);
    }

    #[test]
    fn state_order_matches_uri() {
        let model = LinearDecay::new("chem:brix_percent", 0.1, 2.0);
        assert_eq!(model.state_order(), vec!["chem:brix_percent"]);
    }
}
