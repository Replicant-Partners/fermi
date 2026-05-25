//! Kombucha fermentation model — coupled Brix + pH dynamics.
//!
//! State: [Brix (%), pH]
//!
//! Equations (Arrhenius temperature-dependent):
//!   dBrix/dt = -k_b(T) * Brix
//!   dpH/dt   = -k_ph(T) * (pH - pH_floor) * Brix    (floor-clamped)
//!
//! Arrhenius:  k(T) = A * exp(-E / (R * T_K))
//!   A_b  = 2.06e7 day⁻¹  (Brix pre-exponential)
//!   E_b  = 50000 J/mol    (Brix activation energy)
//!   A_ph = 3.86e6 day⁻¹  (pH pre-exponential)
//!   E_ph = 47000 J/mol    (pH activation energy)
//!   R    = 8.314 J/(mol·K)

use std::collections::BTreeMap;
use crate::{
    DynamicsModel, ModelManifest, Note,
    manifest::{ContextSchema, ContextSource, ContributionMode, ParamSchema, StateFieldSchema},
};

const R: f64 = 8.314; // J/(mol·K)

pub struct KombuchaFermentation {
    pub k_b: f64,       // pre-computed Arrhenius rate for Brix (day⁻¹)
    pub k_ph: f64,      // pre-computed Arrhenius rate for pH (day⁻¹)
    pub ph_floor: f64,  // minimum pH (acidification floor)
}

impl KombuchaFermentation {
    /// Construct with default Arrhenius parameters at the given temperature.
    pub fn from_temperature(temp_c: f64, ph_floor: f64) -> Self {
        let t_k = temp_c + 273.15;
        let k_b  = 2.06e7_f64 * (-50_000.0 / (R * t_k)).exp();
        let k_ph = 3.86e6_f64 * (-47_000.0 / (R * t_k)).exp();
        Self { k_b, k_ph, ph_floor }
    }

    /// Construct from explicit rate constants (for testing / calibration).
    pub fn from_rates(k_b: f64, k_ph: f64, ph_floor: f64) -> Self {
        Self { k_b, k_ph, ph_floor }
    }
}

impl Default for KombuchaFermentation {
    fn default() -> Self {
        Self::from_temperature(26.0, 2.5)
    }
}

impl DynamicsModel for KombuchaFermentation {
    fn manifest(&self) -> ModelManifest {
        ModelManifest {
            uri: "kask:dynamics/kombucha_fermentation@v1".into(),
            version: "1.0.0".into(),
            name: "Kombucha fermentation".into(),
            description: "Coupled Brix and pH dynamics with Arrhenius temperature dependence and pH floor.".into(),
            applies_to_set: vec!["chem:brix_percent".into(), "chem:ph_value".into()],
            state_schema: BTreeMap::from([
                ("chem:brix_percent".into(), StateFieldSchema {
                    label: "Brix".into(),
                    units: "%".into(),
                    description: "Sugar content; consumed by SCOBY culture.".into(),
                    typical_range: Some((0.0, 15.0)),
                    contribution: ContributionMode::Additive,
                }),
                ("chem:ph_value".into(), StateFieldSchema {
                    label: "pH".into(),
                    units: "dimensionless".into(),
                    description: "Acidity; drops as organic acids accumulate.".into(),
                    typical_range: Some((2.5, 7.0)),
                    contribution: ContributionMode::Additive,
                }),
            ]),
            params_schema: BTreeMap::from([
                ("A_b".into(),     ParamSchema { label: "Brix pre-exponential".into(),    units: "day⁻¹".into(), description: "Arrhenius A for Brix consumption.".into(),         default: 2.06e7,  typical_range: None }),
                ("E_b".into(),     ParamSchema { label: "Brix activation energy".into(),  units: "J/mol".into(), description: "Arrhenius Ea for Brix consumption.".into(),        default: 50000.0, typical_range: Some((40000.0, 60000.0)) }),
                ("A_ph".into(),    ParamSchema { label: "pH pre-exponential".into(),     units: "day⁻¹".into(), description: "Arrhenius A for pH drop.".into(),                  default: 3.86e6,  typical_range: None }),
                ("E_ph".into(),    ParamSchema { label: "pH activation energy".into(),   units: "J/mol".into(), description: "Arrhenius Ea for pH drop.".into(),                 default: 47000.0, typical_range: Some((40000.0, 55000.0)) }),
                ("ph_floor".into(),ParamSchema { label: "pH floor".into(),               units: "dimensionless".into(), description: "Minimum pH — acidification stops here.".into(), default: 2.5, typical_range: Some((2.0, 3.5)) }),
            ]),
            context_schema: BTreeMap::from([(
                "temperature_c".into(),
                ContextSchema {
                    label: "Fermentation temperature".into(),
                    units: "°C".into(),
                    description: "Controls Arrhenius rates. Sourced from process stage config.".into(),
                    source: ContextSource::ProcessField { path: "stages[*].temperature_c".into() },
                },
            )]),
            default_params: BTreeMap::from([
                ("A_b".into(), 2.06e7),
                ("E_b".into(), 50_000.0),
                ("A_ph".into(), 3.86e6),
                ("E_ph".into(), 47_000.0),
                ("ph_floor".into(), 2.5),
            ]),
            default_integrator: "rk4".into(),
            default_step_days: 0.01,
            citations: vec![
                "Operator-provided Rust prototype (kask.bio)".into(),
                "Arrhenius parameters: empirical calibration on SCOBY culture at 26°C".into(),
            ],
        }
    }

    fn system(&self, _t: f64, y: &[f64], dy: &mut [f64]) {
        let brix = y[0].max(0.0);
        let ph   = y[1];

        // Brix consumed by the culture
        dy[0] = -self.k_b * brix;

        // pH drops proportional to available substrate and distance above floor
        if ph > self.ph_floor {
            dy[1] = -self.k_ph * (ph - self.ph_floor) * brix;
        } else {
            dy[1] = 0.0;
        }
    }

    fn state_order(&self) -> Vec<String> {
        vec!["chem:brix_percent".into(), "chem:ph_value".into()]
    }

    fn is_converged(&self, history: &[(f64, Vec<f64>)]) -> bool {
        if history.len() < 10 { return false; }
        let last = &history[history.len() - 10..];
        let brix_range = last.iter().map(|(_, y)| y[0]).fold(f64::INFINITY, f64::min)
            ..=last.iter().map(|(_, y)| y[0]).fold(f64::NEG_INFINITY, f64::max);
        let ph_range   = last.iter().map(|(_, y)| y[1]).fold(f64::INFINITY, f64::min)
            ..=last.iter().map(|(_, y)| y[1]).fold(f64::NEG_INFINITY, f64::max);
        (brix_range.end() - brix_range.start()) < 0.01
            && (ph_range.end() - ph_range.start()) < 0.01
    }

    fn generate_notes(&self, trajectory: &[(f64, Vec<f64>)]) -> Vec<Note> {
        let mut notes = Vec::new();
        // Detect when pH floor is first engaged
        for (i, (t, y)) in trajectory.iter().enumerate() {
            if i > 0 && y[1] <= self.ph_floor + 0.05 {
                let prev_ph = trajectory[i-1].1[1];
                if prev_ph > self.ph_floor + 0.05 {
                    notes.push(Note {
                        severity: "info".into(),
                        message: format!("pH floor ({:.2}) engaged — acidification stops.", self.ph_floor),
                        t_hours: Some(t * 24.0),
                    });
                }
                break;
            }
        }
        // Warn if Brix exhausted before end
        if let Some((t, y)) = trajectory.last() {
            if y[0] < 0.5 {
                notes.push(Note {
                    severity: "warning".into(),
                    message: format!("Brix exhausted ({:.2}%) by day {:.1} — fermentation substrate depleted.", y[0], t),
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
    use approx::assert_abs_diff_eq;

    #[test]
    fn brix_decreases_monotonically() {
        let model = KombuchaFermentation::from_rates(0.2, 0.1, 2.5);
        let mut y = vec![10.0_f64, 5.0_f64];
        let mut dy = vec![0.0; 2];
        model.system(0.0, &y, &mut dy);
        assert!(dy[0] < 0.0, "Brix should decrease, got dy[0]={}", dy[0]);
    }

    #[test]
    fn ph_does_not_drop_below_floor() {
        let model = KombuchaFermentation::from_rates(0.2, 0.5, 2.5);
        let y = vec![5.0_f64, 2.5_f64]; // pH exactly at floor
        let mut dy = vec![0.0; 2];
        model.system(0.0, &y, &mut dy);
        assert_abs_diff_eq!(dy[1], 0.0, epsilon = 1e-12);
    }

    #[test]
    fn ph_drops_when_above_floor_with_brix() {
        let model = KombuchaFermentation::from_rates(0.2, 0.1, 2.5);
        let y = vec![8.0_f64, 4.5_f64];
        let mut dy = vec![0.0; 2];
        model.system(0.0, &y, &mut dy);
        assert!(dy[1] < 0.0, "pH should drop, got dy[1]={}", dy[1]);
        // dy[1] = -0.1 * (4.5 - 2.5) * 8.0 = -1.6
        assert_abs_diff_eq!(dy[1], -1.6, epsilon = 1e-9);
    }

    #[test]
    fn arrhenius_higher_temp_faster_rate() {
        let m20 = KombuchaFermentation::from_temperature(20.0, 2.5);
        let m30 = KombuchaFermentation::from_temperature(30.0, 2.5);
        assert!(m30.k_b > m20.k_b, "Higher temperature should give higher k_b");
        assert!(m30.k_ph > m20.k_ph, "Higher temperature should give higher k_ph");
    }

    #[test]
    fn state_order_is_brix_then_ph() {
        let model = KombuchaFermentation::default();
        assert_eq!(model.state_order()[0], "chem:brix_percent");
        assert_eq!(model.state_order()[1], "chem:ph_value");
    }
}
