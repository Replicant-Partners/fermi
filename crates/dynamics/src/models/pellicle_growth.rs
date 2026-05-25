//! Pellicle growth model — 3D: Brix + pH + Pellicle biomass.
//!
//! Extends `kombucha_fermentation` with pellicle (SCOBY mat) growth dynamics.
//!
//! State: [Brix (%), pH, Pellicle (g/L)]
//!
//! Equations (Arrhenius temperature-dependent):
//!   dBrix/dt    = -k_b(T) * Brix
//!   dpH/dt      = -k_ph(T) * (pH - pH_floor) * Brix
//!   dPellicle/dt = k_p(T) * Brix * (1 - Pellicle/P_max)   (logistic Monod)
//!
//! k_p(T) = A_p * exp(-E_p / (R * T_K))
//!   A_p  = 1.2e5 day⁻¹ (Pellicle pre-exponential)
//!   E_p  = 45000 J/mol  (Pellicle activation energy)

use std::collections::BTreeMap;
use crate::{
    DynamicsModel, ModelManifest, Note,
    manifest::{ContextSchema, ContextSource, ParamSchema, StateFieldSchema},
};

const R: f64 = 8.314;

pub struct PellicleGrowth {
    pub k_b: f64,
    pub k_ph: f64,
    pub k_p: f64,
    pub ph_floor: f64,
    pub p_max: f64,     // Maximum pellicle concentration (g/L)
}

impl PellicleGrowth {
    pub fn from_temperature(temp_c: f64, ph_floor: f64, p_max: f64) -> Self {
        let t_k = temp_c + 273.15;
        Self {
            k_b:  2.06e7_f64 * (-50_000.0 / (R * t_k)).exp(),
            k_ph: 3.86e6_f64 * (-47_000.0 / (R * t_k)).exp(),
            k_p:  1.20e5_f64 * (-45_000.0 / (R * t_k)).exp(),
            ph_floor,
            p_max,
        }
    }

    pub fn from_rates(k_b: f64, k_ph: f64, k_p: f64, ph_floor: f64, p_max: f64) -> Self {
        Self { k_b, k_ph, k_p, ph_floor, p_max }
    }
}

impl Default for PellicleGrowth {
    fn default() -> Self {
        Self::from_temperature(26.0, 2.5, 8.0)
    }
}

impl DynamicsModel for PellicleGrowth {
    fn manifest(&self) -> ModelManifest {
        ModelManifest {
            uri: "kask:dynamics/pellicle_growth@v1".into(),
            version: "1.0.0".into(),
            name: "Pellicle growth (3D)".into(),
            description: "Brix + pH + Pellicle biomass. Arrhenius temperature dependence, logistic pellicle growth.".into(),
            applies_to_set: vec![
                "chem:brix_percent".into(),
                "chem:ph_value".into(),
                "bio:pellicle_g_per_l".into(),
            ],
            state_schema: BTreeMap::from([
                ("chem:brix_percent".into(), StateFieldSchema {
                    label: "Brix".into(), units: "%".into(),
                    description: "Sugar content.".into(),
                    typical_range: Some((0.0, 15.0)),
                }),
                ("chem:ph_value".into(), StateFieldSchema {
                    label: "pH".into(), units: "dimensionless".into(),
                    description: "Acidity.".into(),
                    typical_range: Some((2.5, 7.0)),
                }),
                ("bio:pellicle_g_per_l".into(), StateFieldSchema {
                    label: "Pellicle".into(), units: "g/L".into(),
                    description: "SCOBY pellicle concentration.".into(),
                    typical_range: Some((0.0, 8.0)),
                }),
            ]),
            params_schema: BTreeMap::from([
                ("A_b".into(),     ParamSchema { label: "Brix A".into(),     units: "day⁻¹".into(), description: "Arrhenius pre-exp for Brix.".into(), default: 2.06e7, typical_range: None }),
                ("E_b".into(),     ParamSchema { label: "Brix Ea".into(),    units: "J/mol".into(), description: "Activation energy Brix.".into(),    default: 50000.0, typical_range: None }),
                ("A_ph".into(),    ParamSchema { label: "pH A".into(),       units: "day⁻¹".into(), description: "Arrhenius pre-exp for pH.".into(),  default: 3.86e6, typical_range: None }),
                ("E_ph".into(),    ParamSchema { label: "pH Ea".into(),      units: "J/mol".into(), description: "Activation energy pH.".into(),      default: 47000.0, typical_range: None }),
                ("A_p".into(),     ParamSchema { label: "Pellicle A".into(), units: "day⁻¹".into(), description: "Arrhenius pre-exp for pellicle.".into(), default: 1.2e5, typical_range: None }),
                ("E_p".into(),     ParamSchema { label: "Pellicle Ea".into(),units: "J/mol".into(), description: "Activation energy pellicle.".into(), default: 45000.0, typical_range: None }),
                ("ph_floor".into(),ParamSchema { label: "pH floor".into(),   units: "dimensionless".into(), description: "Acidification floor.".into(), default: 2.5, typical_range: Some((2.0, 3.5)) }),
                ("p_max".into(),   ParamSchema { label: "P_max".into(),      units: "g/L".into(), description: "Carrying capacity for pellicle.".into(), default: 8.0, typical_range: Some((4.0, 12.0)) }),
            ]),
            context_schema: BTreeMap::from([(
                "temperature_c".into(),
                ContextSchema {
                    label: "Temperature".into(), units: "°C".into(),
                    description: "Fermentation temperature (Arrhenius).".into(),
                    source: ContextSource::ProcessField { path: "stages[*].temperature_c".into() },
                },
            )]),
            default_params: BTreeMap::from([
                ("A_b".into(), 2.06e7), ("E_b".into(), 50_000.0),
                ("A_ph".into(), 3.86e6), ("E_ph".into(), 47_000.0),
                ("A_p".into(), 1.2e5), ("E_p".into(), 45_000.0),
                ("ph_floor".into(), 2.5), ("p_max".into(), 8.0),
            ]),
            default_integrator: "rk4".into(),
            default_step_days: 0.01,
            citations: vec![
                "Operator-provided Rust prototype (kask.bio)".into(),
            ],
        }
    }

    fn system(&self, _t: f64, y: &[f64], dy: &mut [f64]) {
        let brix     = y[0].max(0.0);
        let ph       = y[1];
        let pellicle = y[2].max(0.0);

        dy[0] = -self.k_b * brix;

        dy[1] = if ph > self.ph_floor {
            -self.k_ph * (ph - self.ph_floor) * brix
        } else {
            0.0
        };

        // Logistic growth limited by Brix substrate and carrying capacity
        dy[2] = self.k_p * brix * (1.0 - pellicle / self.p_max).max(0.0);
    }

    fn state_order(&self) -> Vec<String> {
        vec![
            "chem:brix_percent".into(),
            "chem:ph_value".into(),
            "bio:pellicle_g_per_l".into(),
        ]
    }

    fn is_converged(&self, history: &[(f64, Vec<f64>)]) -> bool {
        if history.len() < 10 { return false; }
        let last = &history[history.len() - 10..];
        let delta_p = last.windows(2)
            .map(|w| (w[1].1[2] - w[0].1[2]).abs())
            .fold(0.0_f64, f64::max);
        let delta_b = last.windows(2)
            .map(|w| (w[1].1[0] - w[0].1[0]).abs())
            .fold(0.0_f64, f64::max);
        delta_p < 0.01 && delta_b < 0.01
    }

    fn generate_notes(&self, trajectory: &[(f64, Vec<f64>)]) -> Vec<Note> {
        let mut notes = Vec::new();
        // pH floor detection
        for (i, (t, y)) in trajectory.iter().enumerate() {
            if i > 0 && y[1] <= self.ph_floor + 0.05 {
                let prev = trajectory[i-1].1[1];
                if prev > self.ph_floor + 0.05 {
                    notes.push(Note {
                        severity: "info".into(),
                        message: format!("pH floor ({:.2}) engaged at day {:.1}.", self.ph_floor, t),
                        t_hours: Some(t * 24.0),
                    });
                    break;
                }
            }
        }
        // Pellicle near capacity
        if let Some((t, y)) = trajectory.last() {
            if y[2] >= self.p_max * 0.9 {
                notes.push(Note {
                    severity: "info".into(),
                    message: format!("Pellicle {:.2} g/L approaching capacity {:.1} g/L at day {:.1}.", y[2], self.p_max, t),
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
    fn pellicle_grows_with_brix_present() {
        let model = PellicleGrowth::from_rates(0.2, 0.1, 0.05, 2.5, 8.0);
        let y = vec![8.0_f64, 4.5_f64, 0.5_f64];
        let mut dy = vec![0.0; 3];
        model.system(0.0, &y, &mut dy);
        assert!(dy[2] > 0.0, "Pellicle should grow, got dy[2]={}", dy[2]);
    }

    #[test]
    fn pellicle_stops_at_capacity() {
        let model = PellicleGrowth::from_rates(0.2, 0.1, 0.05, 2.5, 8.0);
        let y = vec![8.0_f64, 4.5_f64, 8.0_f64]; // pellicle at P_max
        let mut dy = vec![0.0; 3];
        model.system(0.0, &y, &mut dy);
        assert!(dy[2].abs() < 1e-10, "Pellicle at capacity should not grow, got {}", dy[2]);
    }

    #[test]
    fn brix_absent_no_pellicle_growth() {
        let model = PellicleGrowth::from_rates(0.2, 0.1, 0.05, 2.5, 8.0);
        let y = vec![0.0_f64, 4.5_f64, 2.0_f64];
        let mut dy = vec![0.0; 3];
        model.system(0.0, &y, &mut dy);
        assert!(dy[2].abs() < 1e-10, "No brix → no pellicle growth");
    }
}
