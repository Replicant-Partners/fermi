//! General-purpose solid–liquid extraction model.
//!
//! Models the kinetics of bioactive compound extraction from a solid matrix
//! into a liquid solvent, based on second-order kinetics with Arrhenius
//! temperature dependence and optional thermal degradation.
//!
//! ## References
//!
//! Hobbi et al. (2021) "Kinetic modelling of the solid–liquid extraction
//! process of polyphenolic compounds from apple pomace: influence of solvent
//! composition and temperature." *Bioresour Bioprocess* 8:114.
//! <https://doi.org/10.1186/s40643-021-00465-4> (PMC10991919)
//!
//! ## State vector
//!
//! | Index | Property URI              | Description                          |
//! |-------|---------------------------|--------------------------------------|
//! | 0     | `extract:ct_mg_per_g`    | Extracted concentration at time t    |
//! | 1     | `extract:cs_mg_per_g`    | Saturation capacity (slowly decays)  |
//!
//! ## Equations
//!
//! **Second-order extraction** (Eq. 6 in Hobbi et al.):
//!
//! ```text
//! dCt/dt = k(T) · (Cs − Ct)²
//! ```
//!
//! **Arrhenius temperature dependence** (Eq. 4):
//!
//! ```text
//! k(T) = Aₑ · exp(−Eₐ / (R · T_K))
//! ```
//!
//! **Thermal degradation** (post-peak, Eq. 12 — first-order decay on Ct):
//!
//! When Ct approaches Cs (Ct ≥ Cs · degradation_onset_fraction), the
//! extracted compounds begin to thermally degrade:
//!
//! ```text
//! dCt/dt += −k_deg(T) · Ct
//! ```
//!
//! **Saturation capacity decay** — prolonged heating reduces the maximum
//! extractable yield as heat-sensitive pools degrade in the solid matrix:
//!
//! ```text
//! dCs/dt = −k_deg(T) · Cs · (1 − Ct/Cs)⁺
//! ```

use crate::{
    manifest::{ContextSchema, ContextSource, ContributionMode, ParamSchema, StateFieldSchema},
    DynamicsModel, ModelManifest, Note,
};
use std::collections::BTreeMap;

const R: f64 = 8.314;

/// Solvent type — influences default Arrhenius parameters and behaviour.
///
/// Defaults calibrated against Hobbi et al. (2021) apple pomace TPC data:
///
/// | Solvent              | Aₑ (g/(mg·min)) | Eₐ (kJ/mol) | Cs range (mg GAE/g db) |
/// |----------------------|------------------|--------------|------------------------|
/// | Water (100%)         | 14.5             | 12.4         | 3.7 – 5.1              |
/// | Ethanol 50%–water    | 17.5             | 10.2         | 7.1 – 9.2              |
/// | Acetone 65%–water    | 7.9              | 17.4         | 9.0 – 11.1             |
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SolventKind {
    Water,
    EthanolWater50,
    AcetoneWater65,
    Custom,
}

impl Default for SolventKind {
    fn default() -> Self {
        Self::Water
    }
}

impl SolventKind {
    fn default_arrhenius(&self) -> (f64, f64) {
        match self {
            Self::Water => (14.5, 12_400.0),
            Self::EthanolWater50 => (17.5, 10_200.0),
            Self::AcetoneWater65 => (7.9, 17_400.0),
            Self::Custom => (14.5, 12_400.0),
        }
    }

    fn default_cs_at_ref(&self) -> f64 {
        match self {
            Self::Water => 5.1,
            Self::EthanolWater50 => 9.2,
            Self::AcetoneWater65 => 11.1,
            Self::Custom => 5.1,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Water => "Water (100%)",
            Self::EthanolWater50 => "Ethanol 50%–water",
            Self::AcetoneWater65 => "Acetone 65%–water",
            Self::Custom => "Custom solvent",
        }
    }
}

pub struct SolidLiquidExtraction {
    pub k: f64,
    pub k_deg: f64,
    pub cs_initial: f64,
    pub degradation_onset: f64,
    pub solvent: SolventKind,
}

impl SolidLiquidExtraction {
    pub fn from_context(
        temp_c: f64,
        solvent: SolventKind,
        cs_initial: Option<f64>,
        ae: Option<f64>,
        ea: Option<f64>,
        ae_deg: Option<f64>,
        ea_deg: Option<f64>,
        degradation_onset: Option<f64>,
    ) -> Self {
        let t_k = temp_c + 273.15;
        let (ae_default, ea_default) = solvent.default_arrhenius();
        let ae = ae.unwrap_or(ae_default);
        let ea = ea.unwrap_or(ea_default);
        // Hobbi et al. Ae is in g/(mg·min). The integrator runs in days.
        // Convert: k [g/(mg·day)] = k_min [g/(mg·min)] × 1440 min/day
        let k_min = ae * (-ea / (R * t_k)).exp();
        let k = k_min * 1440.0;

        // Ae_deg default is in min⁻¹ units — same conversion.
        let ae_deg = ae_deg.unwrap_or(1.0e6);
        let ea_deg = ea_deg.unwrap_or(45_000.0);
        let k_deg_min = ae_deg * (-ea_deg / (R * t_k)).exp();
        let k_deg = k_deg_min * 1440.0;

        let cs_initial = cs_initial.unwrap_or_else(|| solvent.default_cs_at_ref());
        let degradation_onset = degradation_onset.unwrap_or(0.95);

        Self {
            k,
            k_deg,
            cs_initial,
            degradation_onset,
            solvent,
        }
    }

    pub fn from_explicit(
        k: f64,
        k_deg: f64,
        cs_initial: f64,
        degradation_onset: f64,
        solvent: SolventKind,
    ) -> Self {
        Self {
            k,
            k_deg,
            cs_initial,
            degradation_onset,
            solvent,
        }
    }
}

impl Default for SolidLiquidExtraction {
    fn default() -> Self {
        Self::from_context(60.0, SolventKind::Water, None, None, None, None, None, None)
    }
}

impl DynamicsModel for SolidLiquidExtraction {
    fn manifest(&self) -> ModelManifest {
        let (ae_def, ea_def) = self.solvent.default_arrhenius();
        ModelManifest {
            uri: "kask:dynamics/solid_liquid_extraction@v1".into(),
            version: "1.0.0".into(),
            name: "Solid–liquid extraction".into(),
            description: "Second-order kinetic extraction model with Arrhenius temperature dependence and thermal degradation. General-purpose for bioactive compound recovery from solid matrices.".into(),
            applies_to_set: vec!["extract:ct_mg_per_g".into(), "extract:cs_mg_per_g".into()],
            state_schema: BTreeMap::from([
                ("extract:ct_mg_per_g".into(), StateFieldSchema {
                    label: "Ct — extracted concentration".into(),
                    units: "mg GAE/g db".into(),
                    description: "Concentration of extracted compound in solvent at time t. Units are mg of gallic-acid-equivalent per gram dry basis of solid matrix.".into(),
                    typical_range: Some((0.0, 15.0)),
                    contribution: ContributionMode::Additive,
                }),
                ("extract:cs_mg_per_g".into(), StateFieldSchema {
                    label: "Cs — saturation capacity".into(),
                    units: "mg GAE/g db".into(),
                    description: "Maximum extractable concentration at saturation. Decays under prolonged heating as heat-sensitive compound pools degrade in the solid matrix.".into(),
                    typical_range: Some((0.0, 15.0)),
                    contribution: ContributionMode::Additive,
                }),
            ]),
            params_schema: BTreeMap::from([
                ("Ae".into(), ParamSchema {
                    label: "Pre-exponential factor (extraction)".into(),
                    units: "g/(mg·min)".into(),
                    description: "Arrhenius Aₑ for the second-order extraction rate constant. \
                        Literature values are in g/(mg·min); the engine converts to per-day \
                        internally (×1440) before integration.".into(),
                    default: ae_def,
                    typical_range: Some((1.0, 50.0)),
                }),
                ("Ea".into(), ParamSchema {
                    label: "Activation energy (extraction)".into(),
                    units: "J/mol".into(),
                    description: "Arrhenius Eₐ for the extraction rate. <20 kJ/mol → diffusion-controlled; 20–40 → mixed; >40 → solubilization-controlled.".into(),
                    default: ea_def,
                    typical_range: Some((5_000.0, 50_000.0)),
                }),
                ("Ae_deg".into(), ParamSchema {
                    label: "Pre-exponential factor (degradation)".into(),
                    units: "day⁻¹".into(),
                    description: "Arrhenius Aₑ for the first-order thermal degradation rate.".into(),
                    default: 1.0e6,
                    typical_range: None,
                }),
                ("Ea_deg".into(), ParamSchema {
                    label: "Activation energy (degradation)".into(),
                    units: "J/mol".into(),
                    description: "Arrhenius Eₐ for thermal degradation of extracted compounds.".into(),
                    default: 45_000.0,
                    typical_range: Some((30_000.0, 70_000.0)),
                }),
                ("cs_initial".into(), ParamSchema {
                    label: "Initial saturation capacity".into(),
                    units: "mg GAE/g db".into(),
                    description: "Cs at t=0. Maximum extractable yield under current solvent/temperature conditions.".into(),
                    default: self.solvent.default_cs_at_ref(),
                    typical_range: Some((1.0, 20.0)),
                }),
                ("degradation_onset".into(), ParamSchema {
                    label: "Degradation onset fraction".into(),
                    units: "dimensionless".into(),
                    description: "Fraction of Cs at which thermal degradation begins. Ct ≥ Cs × degradation_onset activates decay.".into(),
                    default: 0.95,
                    typical_range: Some((0.8, 1.0)),
                }),
            ]),
            context_schema: BTreeMap::from([
                ("temperature_c".into(), ContextSchema {
                    label: "Extraction temperature".into(),
                    units: "°C".into(),
                    description: "Controls Arrhenius rates for both extraction and degradation.".into(),
                    source: ContextSource::ProcessField { path: "stages[*].temperature_c".into() },
                }),
                ("solvent".into(), ContextSchema {
                    label: "Solvent type".into(),
                    units: "dimensionless".into(),
                    description: "Solvent kind: water, ethanol_water_50, acetone_water_65, or custom. Determines default Arrhenius parameters.".into(),
                    source: ContextSource::OperatorInput,
                }),
            ]),
            default_params: BTreeMap::from([
                ("Ae".into(), ae_def),
                ("Ea".into(), ea_def),
                ("Ae_deg".into(), 1.0e6),
                ("Ea_deg".into(), 45_000.0),
                ("cs_initial".into(), self.solvent.default_cs_at_ref()),
                ("degradation_onset".into(), 0.95),
            ]),
            default_integrator: "rk4".into(),
            default_step_days: 0.0001,
            citations: vec![
                "Hobbi et al. (2021) Bioresour Bioprocess 8:114. doi:10.1186/s40643-021-00465-4".into(),
                "Harouna-Oumarou et al. (2007) — second-order extraction kinetics formulation".into(),
                "Gonzalez-Centeno et al. (2015) — Eₐ classification for extraction mechanism".into(),
            ],
        }
    }

    fn system(&self, _t: f64, y: &[f64], dy: &mut [f64]) {
        let ct = y[0].max(0.0);
        let cs = y[1].max(0.0);

        let driving_force = (cs - ct).max(0.0);

        // dCt/dt = k · (Cs − Ct)²   [second-order extraction, Hobbi Eq. 6]
        let mut dct_dt = self.k * driving_force * driving_force;

        // Thermal degradation: first-order decay on Ct once near saturation
        // Activates when Ct ≥ Cs × degradation_onset (or Ct ≥ Cs)
        if ct >= cs * self.degradation_onset {
            dct_dt -= self.k_deg * ct;
        }

        dy[0] = dct_dt;

        // dCs/dt = −k_deg · Cs · max(1 − Ct/Cs, 0)
        // Saturation capacity decays as unextracted heat-sensitive pools degrade
        let unextracted_fraction = if cs > 1e-12 {
            (1.0 - ct / cs).max(0.0)
        } else {
            0.0
        };
        dy[1] = -self.k_deg * cs * unextracted_fraction;
    }

    fn state_order(&self) -> Vec<String> {
        vec!["extract:ct_mg_per_g".into(), "extract:cs_mg_per_g".into()]
    }

    fn is_converged(&self, history: &[(f64, Vec<f64>)]) -> bool {
        if history.len() < 6 {
            return false;
        }
        let tail = &history[history.len() - 6..];
        let ct_range = tail.iter().map(|(_, y)| y[0]).fold(f64::INFINITY, f64::min)
            ..=tail
                .iter()
                .map(|(_, y)| y[0])
                .fold(f64::NEG_INFINITY, f64::max);
        (ct_range.end() - ct_range.start()) < 0.01
    }

    fn generate_notes(&self, trajectory: &[(f64, Vec<f64>)]) -> Vec<Note> {
        let mut notes = Vec::new();

        let mut peak_ct = 0.0_f64;
        let mut peak_t = 0.0_f64;
        let mut peak_idx = 0;
        for (i, (t, y)) in trajectory.iter().enumerate() {
            if y[0] > peak_ct {
                peak_ct = y[0];
                peak_t = *t;
                peak_idx = i;
            }
        }

        if peak_idx > 0 && peak_idx < trajectory.len() - 1 {
            let end_ct = trajectory.last().map(|(_, y)| y[0]).unwrap_or(0.0);
            let loss_pct = if peak_ct > 0.0 {
                (1.0 - end_ct / peak_ct) * 100.0
            } else {
                0.0
            };
            if loss_pct > 5.0 {
                notes.push(Note {
                    severity: "warning".into(),
                    message: format!(
                        "Thermal degradation detected: peak Ct = {:.2} mg/g at day {:.2}, final Ct = {:.2} mg/g ({:.1}% loss). Consider reducing temperature or extraction time.",
                        peak_ct, peak_t, end_ct, loss_pct
                    ),
                    t_hours: Some(peak_t * 24.0),
                });
            }
        }

        if let (Some((_, y_initial)), Some((t_end, y_end))) =
            (trajectory.first(), trajectory.last())
        {
            let cs = y_initial[1];
            let ct_final = y_end[0];
            if cs > 0.0 && ct_final / cs > 0.98 {
                notes.push(Note {
                    severity: "info".into(),
                    message: "Extraction reached saturation — further extraction time yields diminishing returns.".into(),
                    t_hours: Some(t_end * 24.0), // use end time, not start time
                });
            }
        }

        let (_ae, ea) = self.solvent.default_arrhenius();
        let ea_kj = ea / 1000.0;
        let mechanism = if ea_kj < 20.0 {
            "diffusion"
        } else if ea_kj < 40.0 {
            "mixed diffusion + solubilization"
        } else {
            "solubilization"
        };
        notes.push(Note {
            severity: "info".into(),
            message: format!(
                "Eₐ = {:.1} kJ/mol → extraction is {}-controlled (solvent: {}).",
                ea_kj,
                mechanism,
                self.solvent.label()
            ),
            t_hours: None,
        });

        notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    fn water_model_at_60c() -> SolidLiquidExtraction {
        SolidLiquidExtraction::from_context(
            60.0,
            SolventKind::Water,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }

    #[test]
    fn ct_increases_initially() {
        let model = water_model_at_60c();
        let y = vec![0.0, model.cs_initial];
        let mut dy = vec![0.0; 2];
        model.system(0.0, &y, &mut dy);
        assert!(
            dy[0] > 0.0,
            "Ct should increase at t=0, got dCt/dt = {}",
            dy[0]
        );
    }

    #[test]
    fn ct_stops_at_saturation() {
        let model = SolidLiquidExtraction::from_explicit(0.2, 0.0, 5.0, 0.95, SolventKind::Water);
        let y = vec![5.0, 5.0];
        let mut dy = vec![0.0; 2];
        model.system(0.0, &y, &mut dy);
        assert_abs_diff_eq!(dy[0], 0.0, epsilon = 1e-9);
    }

    #[test]
    fn second_order_rate_is_correct() {
        let model = SolidLiquidExtraction::from_explicit(0.1, 0.0, 10.0, 0.95, SolventKind::Custom);
        let y = vec![2.0, 10.0];
        let mut dy = vec![0.0; 2];
        model.system(0.0, &y, &mut dy);
        let expected = 0.1_f64 * (10.0_f64 - 2.0_f64).powi(2);
        assert_abs_diff_eq!(dy[0], expected, epsilon = 1e-9);
    }

    #[test]
    fn thermal_degradation_activates_near_saturation() {
        let model = SolidLiquidExtraction::from_explicit(0.1, 0.5, 10.0, 0.90, SolventKind::Custom);
        let y = vec![9.5, 10.0];
        let mut dy = vec![0.0; 2];
        model.system(0.0, &y, &mut dy);
        let extraction = 0.1_f64 * (10.0_f64 - 9.5_f64).powi(2);
        let degradation = 0.5 * 9.5;
        assert!((dy[0] - (extraction - degradation)).abs() < 1e-9);
    }

    #[test]
    fn cs_decays_with_degradation() {
        let model = SolidLiquidExtraction::from_explicit(0.1, 0.5, 10.0, 0.95, SolventKind::Custom);
        let y = vec![5.0, 10.0];
        let mut dy = vec![0.0; 2];
        model.system(0.0, &y, &mut dy);
        let unextracted = (1.0_f64 - 5.0_f64 / 10.0_f64).max(0.0_f64);
        let expected = -0.5 * 10.0 * unextracted;
        assert_abs_diff_eq!(dy[1], expected, epsilon = 1e-9);
    }

    #[test]
    fn arrhenius_higher_temp_faster_extraction() {
        let m40 = SolidLiquidExtraction::from_context(
            40.0,
            SolventKind::Water,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let m85 = SolidLiquidExtraction::from_context(
            85.0,
            SolventKind::Water,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(
            m85.k > m40.k,
            "Higher temperature should yield higher k: k40={}, k85={}",
            m40.k,
            m85.k
        );
    }

    #[test]
    fn acetone_higher_degradation_at_high_temp() {
        let m20 = SolidLiquidExtraction::from_context(
            20.0,
            SolventKind::AcetoneWater65,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let m60 = SolidLiquidExtraction::from_context(
            60.0,
            SolventKind::AcetoneWater65,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(m60.k_deg > m20.k_deg,
            "Acetone: degradation rate should increase with temperature. k_deg20={:.4}, k_deg60={:.4}",
            m20.k_deg, m60.k_deg);
    }

    #[test]
    fn ethanol_faster_than_water_at_same_temp() {
        let mw = SolidLiquidExtraction::from_context(
            60.0,
            SolventKind::Water,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let me = SolidLiquidExtraction::from_context(
            60.0,
            SolventKind::EthanolWater50,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(
            me.k > mw.k,
            "Ethanol should have higher k than water at 60°C"
        );
    }

    #[test]
    fn state_order() {
        let model = SolidLiquidExtraction::default();
        assert_eq!(model.state_order()[0], "extract:ct_mg_per_g");
        assert_eq!(model.state_order()[1], "extract:cs_mg_per_g");
    }

    #[test]
    fn manifest_uri() {
        let model = SolidLiquidExtraction::default();
        assert_eq!(
            model.manifest().uri,
            "kask:dynamics/solid_liquid_extraction@v1"
        );
    }

    #[test]
    fn time_units_are_days_not_minutes() {
        // Sanity check: a water extraction at 60°C should reach ~80% of Cs
        // within a few hours (0.05–0.2 days), NOT within a few minutes × 1440.
        // With k in per-day units, Ct at t=0.1 days (2.4h) should be
        // meaningfully > 0 and < Cs.
        use crate::integrator::integrate;
        let model = SolidLiquidExtraction::from_context(
            60.0,
            SolventKind::Water,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let cs = model.cs_initial;
        let y0 = vec![0.0, cs];
        // Integrate for 0.1 days (2.4 hours) — typical extraction time
        let result = integrate(&model, &y0, 0.1, 0.0001, 0.01).unwrap();
        let ct_final = result.last().unwrap().1[0];
        // At 60°C water extraction, ~50–90% of Cs should be extracted in 2.4h
        assert!(
            ct_final > cs * 0.3,
            "After 2.4h, Ct should be >30% of Cs. Got Ct={:.3}, Cs={:.3}",
            ct_final,
            cs
        );
        assert!(
            ct_final <= cs,
            "Ct cannot exceed Cs. Got Ct={:.3}, Cs={:.3}",
            ct_final,
            cs
        );
    }
}
