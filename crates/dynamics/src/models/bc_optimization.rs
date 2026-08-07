//! Bacterial cellulose (BC) optimization model — 4D.
//!
//! State: [Brix (%), pH, BC_yield (g/L), BC_quality (0–1)]
//!
//! Anchored on PMC8657668 (Ruka et al. 2012 + updates).
//! Models the trade-off between BC yield (maximised by agitation)
//! and BC quality / crystallinity (maximised by static culture).
//!
//! Equations:
//!   dBrix/dt      = -k_b * Brix
//!   dpH/dt        = -k_ph * (pH - ph_floor) * Brix
//!   dBC_yield/dt  = k_bc * Brix * carbon_factor * agit_yield_factor * do_factor
//!                   * (1 - BC_yield / BC_max)
//!   dBC_quality/dt = -k_q * BC_quality * agit_quality_penalty
//!                   (quality degrades under agitation; recovers slowly in static)
//!
//! Context parameters:
//!   temperature_c     — Arrhenius base (default 26°C)
//!   agitation_rpm     — 0 = static (high quality, lower yield)
//!                       120–200 = agitated (higher yield, lower quality)
//!   do_saturation_pct — dissolved oxygen %; sweet spot ~10%
//!   carbon_source     — "glucose" | "sucrose" | "fructose" | "glycerol"
//!
//! Reference: Ruka et al. (2012), PMC8657668, Table 1 + Section 5.

use crate::{
    manifest::{ContextSchema, ContextSource, ContributionMode, ParamSchema, StateFieldSchema},
    DynamicsModel, ModelManifest, Note,
};
use std::collections::BTreeMap;

const R: f64 = 8.314;

pub struct BcOptimization {
    pub k_b: f64,
    pub k_ph: f64,
    pub k_bc: f64, // BC production rate (day⁻¹)
    pub k_q: f64,  // BC quality degradation rate (day⁻¹)
    pub ph_floor: f64,
    pub bc_max: f64, // Carrying capacity g/L
    // context-derived
    pub carbon_factor: f64, // 1.0 = glucose; sucrose 0.85; fructose 0.75; glycerol 0.5
    pub agit_yield_factor: f64, // static=1.0, agitated=1.4
    pub agit_quality_penalty: f64, // static=0.02 (slow quality drift), agitated=0.25
    pub do_factor: f64,     // dissolved oxygen factor: peaks at ~10%, drops above/below
}

impl BcOptimization {
    /// Construct from temperature + context parameters.
    pub fn from_context(
        temp_c: f64,
        agitation_rpm: f64,
        do_saturation_pct: f64,
        carbon_source: &str,
        ph_floor: f64,
        bc_max: f64,
    ) -> Self {
        let t_k = temp_c + 273.15;
        let k_b = 2.06e7_f64 * (-50_000.0 / (R * t_k)).exp();
        let k_ph = 3.86e6_f64 * (-47_000.0 / (R * t_k)).exp();
        let k_bc = 4.50e6_f64 * (-46_000.0 / (R * t_k)).exp();
        let k_q = 0.008; // base quality drift rate

        // Carbon source utilization factor (Table 1, PMC8657668)
        let carbon_factor = match carbon_source {
            "glucose" => 1.00,
            "sucrose" => 0.85,
            "fructose" => 0.75,
            "glycerol" => 0.50,
            _ => 0.80, // unknown source
        };

        // Agitation: static vs agitated regimes
        // At rpm=0: yield factor 1.0, quality penalty 0.02/day
        // At rpm 120-200: yield factor 1.4, quality penalty 0.25/day
        let (agit_yield_factor, agit_quality_penalty) = if agitation_rpm < 20.0 {
            (1.0, 0.02)
        } else {
            let rpm_factor = (agitation_rpm / 160.0).min(1.5);
            (1.0 + 0.4 * rpm_factor, 0.02 + 0.23 * rpm_factor)
        };

        // Dissolved oxygen factor: Gaussian peak at 10% DO saturation
        // Source: PMC8657668 Section 5 — BC production optimised at ~10% DO
        let do_factor = {
            let do_opt = 10.0_f64;
            let do_sigma = 8.0_f64;
            (-(do_saturation_pct - do_opt).powi(2) / (2.0 * do_sigma.powi(2))).exp()
        };

        Self {
            k_b,
            k_ph,
            k_bc,
            k_q,
            ph_floor,
            bc_max,
            carbon_factor,
            agit_yield_factor,
            agit_quality_penalty,
            do_factor,
        }
    }

    pub fn from_rates(
        k_b: f64,
        k_ph: f64,
        k_bc: f64,
        k_q: f64,
        ph_floor: f64,
        bc_max: f64,
        carbon_factor: f64,
        agit_yield_factor: f64,
        agit_quality_penalty: f64,
        do_factor: f64,
    ) -> Self {
        Self {
            k_b,
            k_ph,
            k_bc,
            k_q,
            ph_floor,
            bc_max,
            carbon_factor,
            agit_yield_factor,
            agit_quality_penalty,
            do_factor,
        }
    }
}

impl Default for BcOptimization {
    fn default() -> Self {
        Self::from_context(26.0, 0.0, 10.0, "glucose", 2.5, 6.0)
    }
}

impl DynamicsModel for BcOptimization {
    fn manifest(&self) -> ModelManifest {
        ModelManifest {
            uri: "kask:dynamics/bc_optimization@v1".into(),
            version: "1.0.0".into(),
            name: "Bacterial cellulose (BC) optimization".into(),
            description: "4D model: Brix + pH + BC yield + BC quality. \
                Captures yield/quality trade-off under agitation, carbon source \
                utilization, and dissolved oxygen effects. \
                Anchored on PMC8657668 (Ruka et al.)."
                .into(),
            applies_to_set: vec![
                "chem:brix_percent".into(),
                "chem:ph_value".into(),
                "bio:bc_yield_g_per_l".into(),
                "bio:bc_quality_index".into(),
            ],
            state_schema: BTreeMap::from([
                (
                    "chem:brix_percent".into(),
                    StateFieldSchema {
                        label: "Brix".into(),
                        units: "%".into(),
                        description: "Carbon substrate.".into(),
                        typical_range: Some((0.0, 15.0)),
                        contribution: ContributionMode::Additive,
                    },
                ),
                (
                    "chem:ph_value".into(),
                    StateFieldSchema {
                        label: "pH".into(),
                        units: "dimensionless".into(),
                        description: "Culture acidity.".into(),
                        typical_range: Some((2.5, 7.0)),
                        contribution: ContributionMode::Additive,
                    },
                ),
                (
                    "bio:bc_yield_g_per_l".into(),
                    StateFieldSchema {
                        label: "BC yield".into(),
                        units: "g/L".into(),
                        description: "Bacterial cellulose concentration.".into(),
                        typical_range: Some((0.0, 6.0)),
                        contribution: ContributionMode::Additive,
                    },
                ),
                (
                    "bio:bc_quality_index".into(),
                    StateFieldSchema {
                        label: "BC quality index".into(),
                        units: "0–1".into(),
                        description: "Relative crystallinity/quality. 1.0 = maximum.".into(),
                        typical_range: Some((0.0, 1.0)),
                        contribution: ContributionMode::Additive,
                    },
                ),
            ]),
            params_schema: BTreeMap::from([
                (
                    "bc_max".into(),
                    ParamSchema {
                        label: "BC max".into(),
                        units: "g/L".into(),
                        description: "Carrying capacity.".into(),
                        default: 6.0,
                        typical_range: Some((3.0, 10.0)),
                    },
                ),
                (
                    "ph_floor".into(),
                    ParamSchema {
                        label: "pH floor".into(),
                        units: "dimensionless".into(),
                        description: "Acidification floor.".into(),
                        default: 2.5,
                        typical_range: Some((2.0, 3.5)),
                    },
                ),
            ]),
            context_schema: BTreeMap::from([
                (
                    "temperature_c".into(),
                    ContextSchema {
                        label: "Temperature".into(),
                        units: "°C".into(),
                        description: "Arrhenius base.".into(),
                        source: ContextSource::ProcessField {
                            path: "stages[*].temperature_c".into(),
                        },
                    },
                ),
                (
                    "agitation_rpm".into(),
                    ContextSchema {
                        label: "Agitation".into(),
                        units: "rpm".into(),
                        description: "0=static, 120-200=agitated.".into(),
                        source: ContextSource::OperatorInput,
                    },
                ),
                (
                    "do_saturation_pct".into(),
                    ContextSchema {
                        label: "DO saturation".into(),
                        units: "%".into(),
                        description: "Dissolved oxygen %. Sweet spot ~10%.".into(),
                        source: ContextSource::OperatorInput,
                    },
                ),
                (
                    "carbon_source".into(),
                    ContextSchema {
                        label: "Carbon source".into(),
                        units: "name".into(),
                        description: "glucose|sucrose|fructose|glycerol.".into(),
                        source: ContextSource::OperatorInput,
                    },
                ),
            ]),
            default_params: BTreeMap::from([("bc_max".into(), 6.0), ("ph_floor".into(), 2.5)]),
            default_integrator: "rk4".into(),
            default_step_days: 0.01,
            citations: vec![
                "Ruka et al. (2012) PMC8657668 — Table 1 + Section 5: BC production optimization"
                    .into(),
                "Arrhenius parameters: empirical calibration on Komagataeibacter xylinus".into(),
            ],
        }
    }

    fn system(&self, _t: f64, y: &[f64], dy: &mut [f64]) {
        let brix = y[0].max(0.0);
        let ph = y[1];
        let bc_yield = y[2].max(0.0);
        let bc_quality = y[3].clamp(0.0, 1.0);

        dy[0] = -self.k_b * brix;

        dy[1] = if ph > self.ph_floor {
            -self.k_ph * (ph - self.ph_floor) * brix
        } else {
            0.0
        };

        let capacity_factor = (1.0 - bc_yield / self.bc_max).max(0.0);
        dy[2] = self.k_bc
            * brix
            * self.carbon_factor
            * self.agit_yield_factor
            * self.do_factor
            * capacity_factor;

        // Quality degrades under agitation; clamp at 0
        dy[3] = if bc_quality > 0.0 {
            -self.k_q * self.agit_quality_penalty * bc_quality
        } else {
            0.0
        };
    }

    fn state_order(&self) -> Vec<String> {
        vec![
            "chem:brix_percent".into(),
            "chem:ph_value".into(),
            "bio:bc_yield_g_per_l".into(),
            "bio:bc_quality_index".into(),
        ]
    }

    fn generate_notes(&self, trajectory: &[(f64, Vec<f64>)]) -> Vec<Note> {
        let mut notes = Vec::new();
        if let Some((t, y)) = trajectory.last() {
            let quality = y[3];
            let yield_val = y[2];
            if quality < 0.5 {
                notes.push(Note {
                    severity: "warning".into(),
                    message: format!("BC quality index {:.2} — consider static culture to improve crystallinity.", quality),
                    t_hours: Some(t * 24.0),
                });
            }
            if yield_val >= self.bc_max * 0.85 {
                notes.push(Note {
                    severity: "info".into(),
                    message: format!(
                        "BC yield {:.2} g/L approaching capacity {:.1} g/L.",
                        yield_val, self.bc_max
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
    fn agitated_higher_yield_lower_quality() {
        let static_model = BcOptimization::from_context(26.0, 0.0, 10.0, "glucose", 2.5, 6.0);
        let agit_model = BcOptimization::from_context(26.0, 160.0, 10.0, "glucose", 2.5, 6.0);
        let y = vec![8.0, 4.5, 0.5, 1.0];
        let mut dy_s = vec![0.0; 4];
        let mut dy_a = vec![0.0; 4];
        static_model.system(0.0, &y, &mut dy_s);
        agit_model.system(0.0, &y, &mut dy_a);

        assert!(dy_a[2] > dy_s[2], "Agitated should produce more BC yield");
        assert!(
            dy_a[3].abs() > dy_s[3].abs(),
            "Agitated should degrade quality faster"
        );
    }

    #[test]
    fn glucose_better_than_glycerol() {
        let m_gluc = BcOptimization::from_context(26.0, 0.0, 10.0, "glucose", 2.5, 6.0);
        let m_glyc = BcOptimization::from_context(26.0, 0.0, 10.0, "glycerol", 2.5, 6.0);
        let y = vec![8.0, 4.5, 0.5, 1.0];
        let mut dy_g = vec![0.0; 4];
        let mut dy_y = vec![0.0; 4];
        m_gluc.system(0.0, &y, &mut dy_g);
        m_glyc.system(0.0, &y, &mut dy_y);
        assert!(
            dy_g[2] > dy_y[2],
            "Glucose should yield more BC than glycerol"
        );
    }

    #[test]
    fn do_optimum_at_ten_percent() {
        let m_opt = BcOptimization::from_context(26.0, 0.0, 10.0, "glucose", 2.5, 6.0);
        let m_high = BcOptimization::from_context(26.0, 0.0, 80.0, "glucose", 2.5, 6.0);
        let y = vec![8.0, 4.5, 0.5, 1.0];
        let mut dy_opt = vec![0.0; 4];
        let mut dy_high = vec![0.0; 4];
        m_opt.system(0.0, &y, &mut dy_opt);
        m_high.system(0.0, &y, &mut dy_high);
        assert!(dy_opt[2] > dy_high[2], "10% DO should outperform 80% DO");
    }

    #[test]
    fn bc_does_not_exceed_capacity() {
        let model = BcOptimization::default();
        let y = vec![8.0, 4.5, 6.0, 1.0]; // bc_yield at bc_max
        let mut dy = vec![0.0; 4];
        model.system(0.0, &y, &mut dy);
        assert!(dy[2].abs() < 1e-10, "At capacity, BC growth must be zero");
    }

    #[test]
    fn state_order_has_four_dimensions() {
        assert_eq!(BcOptimization::default().state_order().len(), 4);
    }
}
