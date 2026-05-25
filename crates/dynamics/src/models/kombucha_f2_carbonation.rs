//! Kombucha F2 (second fermentation) carbonation model — 4D.
//!
//! Models a sealed bottle during secondary fermentation:
//! yeast consuming residual sugar, producing CO₂ that dissolves into the
//! kombucha liquid and pressurises the headspace.
//!
//! This model is **complementary to `kombucha_fermentation`** (F1):
//!   - F1 (`kombucha_fermentation`): open vessel, tracks Brix + pH during
//!     primary fermentation by the SCOBY culture.
//!   - F2 (this model): sealed bottle, tracks yeast + sugar + dissolved CO₂ +
//!     headspace pressure during carbonation.
//!
//! ## State
//!
//! | URI | Symbol | Units | Description |
//! |---|---|---|---|
//! | `bio:yeast_g_per_l` | X | g/L | Active yeast biomass concentration |
//! | `chem:sugar_g_per_l` | S | g/L | Residual fermentable sugar |
//! | `chem:co2_dissolved_g_per_l` | C | g/L | Dissolved CO₂ in liquid (carbonation) |
//! | `phys:headspace_pressure_bar` | P | bar | Headspace CO₂ partial pressure |
//!
//! ## Equations
//!
//! ### Yeast growth (Monod kinetics + death)
//! ```text
//! dX/dt = (μ(S,T) − k_d) · X
//! μ(S,T) = μ_max(T) · S / (K_s + S)
//! μ_max(T) = μ_base · exp(k_T · (T − T_ref))   [Q10-style Arrhenius]
//! ```
//!
//! ### Sugar consumption
//! ```text
//! dS/dt = −(1/Y_xs · μ · X + m · X)
//! ```
//!
//! ### Dissolved CO₂ (mass balance on liquid phase)
//! ```text
//! dC/dt = Y_co2_s · |dS/dt| − kLa · (C − C_sat(T,P))
//! C_sat(T,P) = K_H(T) · P
//! K_H(T) = K_H_std · exp((ΔH_R/R_gas) · (1/T − 1/T_std))   [van 't Hoff]
//! ```
//!
//! ### Headspace pressure (ideal gas law, CO₂ transfer from liquid)
//! ```text
//! dP/dt = kLa · (C − C_sat) · (V_liq/V_head) · (R_bar · T / M_co2)
//! ```
//!
//! ## Fixes applied to original code
//!
//! 1. **ODE form**: rewritten as proper dy/dt system (was Euler loop).
//!    RK4 integration now handles step control — higher accuracy.
//!
//! 2. **Sugar floor**: `max(S, 0)` inside system() instead of a
//!    conditional block outside the loop. Avoids half-step errors at
//!    sugar exhaustion.
//!
//! 3. **Dual gas constant**: original mixed `r = 0.08314 L·bar/(mol·K)`
//!    (pressure eq.) with `r_gas = 8.314 J/(mol·K)` (Henry's Law).
//!    Both are correct but confusing. Now explicit: `R_BAR` and `R_JOULE`.
//!
//! 4. **Temperature as context parameter**: hardcoded warm/cold schedule
//!    removed. Temperature comes from `process_context.temperature_c`.
//!    For a warm→cold schedule, run two sequential projections or use
//!    `Horizon::UntilPropertyReaches` to trigger the phase change.
//!
//! 5. **Physical unit clarity**: kLa is 1/day (not 1/h), all rates in
//!    day-consistent units matching the rest of the dynamics crate.

use std::collections::BTreeMap;
use crate::{
    DynamicsModel, ModelManifest, Note,
    manifest::{ContextSchema, ContextSource, ContributionMode, ParamSchema, StateFieldSchema},
};

/// Gas constant for pressure calculations: L·bar/(mol·K)
const R_BAR: f64 = 0.08314;
/// Gas constant for thermodynamic calculations: J/(mol·K)
const R_JOULE: f64 = 8.314;
/// Molecular weight of CO₂: g/mol
const MW_CO2: f64 = 44.01;
/// Henry's Law constant for CO₂ at 25°C (T_std): mol/(L·bar)
const K_H_STD_MOL: f64 = 0.034;
/// T_std for Henry's Law reference: K (= 25°C)
const T_H_STD_K: f64 = 298.15;
/// van 't Hoff enthalpy parameter for CO₂ dissolution: K
const D_H_R: f64 = 2400.0;

pub struct KombuchaF2Carbonation {
    // Biological parameters
    /// Maximum growth rate at T_ref (day⁻¹)
    pub mu_base: f64,
    /// Reference temperature for μ_max (°C)
    pub t_ref_c: f64,
    /// Temperature sensitivity coefficient (°C⁻¹) — Q10-style exponential
    pub k_temp: f64,
    /// Substrate (sugar) affinity constant (g/L)
    pub k_s: f64,
    /// Yeast death rate (day⁻¹)
    pub k_d: f64,
    /// Yield: g yeast / g sugar
    pub y_xs: f64,
    /// Maintenance coefficient: g sugar / (g yeast · day)
    pub m_maint: f64,
    /// CO₂ yield from sugar: g CO₂ / g sugar consumed
    pub y_co2_s: f64,
    // Physical parameters
    /// Volumetric mass transfer coefficient (day⁻¹)
    pub kla: f64,
    /// Volume of liquid phase (L)
    pub v_liq: f64,
    /// Volume of headspace (L)
    pub v_head: f64,
    /// Current temperature (°C) — read from process_context
    pub temperature_c: f64,
}

impl KombuchaF2Carbonation {
    pub fn from_context(temperature_c: f64, params: &BTreeMap<String, f64>) -> Self {
        let get = |k: &str, default: f64| params.get(k).copied().unwrap_or(default);
        Self {
            mu_base:       get("mu_base",   0.5),
            t_ref_c:       get("t_ref_c",  22.0),
            k_temp:        get("k_temp",    0.07),
            k_s:           get("k_s",       5.0),
            k_d:           get("k_d",       0.05),
            y_xs:          get("y_xs",      0.1),
            m_maint:       get("m_maint",   0.01),
            y_co2_s:       get("y_co2_s",   0.48),
            kla:           get("kla",       4.0),
            v_liq:         get("v_liq",     0.45),
            v_head:        get("v_head",    0.05),
            temperature_c,
        }
    }

    /// Henry's Law saturation concentration at current temperature and pressure P.
    /// Returns C_sat in g/L.
    fn co2_sat_g_per_l(&self, pressure_bar: f64) -> f64 {
        let t_k = self.temperature_c + 273.15;
        // van 't Hoff: K_H(T) = K_H_std · exp(ΔH_R/R · (1/T − 1/T_std))
        let k_h_mol = K_H_STD_MOL * ((D_H_R / R_JOULE) * (1.0 / t_k - 1.0 / T_H_STD_K)).exp();
        // Convert mol/(L·bar) → g/(L·bar) by multiplying by MW_CO2
        let k_h_g = k_h_mol * MW_CO2;
        k_h_g * pressure_bar
    }

    /// Q10-style temperature-dependent maximum growth rate.
    fn mu_max(&self) -> f64 {
        self.mu_base * (self.k_temp * (self.temperature_c - self.t_ref_c)).exp()
    }
}

impl Default for KombuchaF2Carbonation {
    fn default() -> Self {
        Self::from_context(24.0, &BTreeMap::new())
    }
}

impl DynamicsModel for KombuchaF2Carbonation {
    fn manifest(&self) -> ModelManifest {
        ModelManifest {
            uri: "kask:dynamics/kombucha_f2_carbonation@v1".into(),
            version: "1.0.0".into(),
            name: "Kombucha F2 carbonation (sealed bottle)".into(),
            description: "Second fermentation model for sealed kombucha bottles. Tracks yeast \
                biomass, residual sugar, dissolved CO₂ (carbonation level), and headspace \
                pressure. Monod kinetics with Q10 temperature dependence; Henry's Law \
                (van 't Hoff) for CO₂ solubility. Complementary to kombucha_fermentation \
                (F1 open-vessel model).".into(),
            applies_to_set: vec![
                "bio:yeast_g_per_l".into(),
                "chem:sugar_g_per_l".into(),
                "chem:co2_dissolved_g_per_l".into(),
                "phys:headspace_pressure_bar".into(),
            ],
            state_schema: BTreeMap::from([
                ("bio:yeast_g_per_l".into(), StateFieldSchema {
                    label: "Yeast biomass".into(),
                    units: "g/L".into(),
                    description: "Active yeast concentration. Drives sugar consumption and CO₂ production.".into(),
                    typical_range: Some((0.0, 2.0)),
                    contribution: ContributionMode::Additive,
                }),
                ("chem:sugar_g_per_l".into(), StateFieldSchema {
                    label: "Residual sugar".into(),
                    units: "g/L".into(),
                    description: "Fermentable sugar remaining. Depleted by yeast for growth and maintenance.".into(),
                    typical_range: Some((0.0, 30.0)),
                    contribution: ContributionMode::Additive,
                }),
                ("chem:co2_dissolved_g_per_l".into(), StateFieldSchema {
                    label: "Dissolved CO₂".into(),
                    units: "g/L".into(),
                    description: "CO₂ dissolved in the kombucha liquid. Determines carbonation mouth-feel. Equilibrium given by Henry's Law.".into(),
                    typical_range: Some((0.0, 12.0)),
                    contribution: ContributionMode::Additive,
                }),
                ("phys:headspace_pressure_bar".into(), StateFieldSchema {
                    label: "Headspace pressure".into(),
                    units: "bar".into(),
                    description: "CO₂ partial pressure in the bottle headspace. Drives carbonation equilibrium and bottle safety (burst risk above ~4 bar).".into(),
                    typical_range: Some((1.0, 5.0)),
                    contribution: ContributionMode::Additive,
                }),
            ]),
            params_schema: BTreeMap::from([
                ("mu_base".into(),  ParamSchema { label: "μ_base".into(), units: "day⁻¹".into(),               description: "Max growth rate at T_ref.".into(),                  default: 0.5,  typical_range: Some((0.1, 2.0)) }),
                ("t_ref_c".into(), ParamSchema { label: "T_ref".into(),  units: "°C".into(),                   description: "Reference temperature for μ_base.".into(),           default: 22.0, typical_range: Some((15.0, 30.0)) }),
                ("k_temp".into(),  ParamSchema { label: "k_T".into(),    units: "°C⁻¹".into(),                 description: "Temperature sensitivity of growth rate.".into(),     default: 0.07, typical_range: Some((0.03, 0.12)) }),
                ("k_s".into(),     ParamSchema { label: "K_s".into(),    units: "g/L".into(),                  description: "Substrate affinity constant (Monod).".into(),        default: 5.0,  typical_range: Some((0.5, 20.0)) }),
                ("k_d".into(),     ParamSchema { label: "k_d".into(),    units: "day⁻¹".into(),                description: "Yeast death rate.".into(),                           default: 0.05, typical_range: Some((0.01, 0.2)) }),
                ("y_xs".into(),    ParamSchema { label: "Y_xs".into(),   units: "g yeast/g sugar".into(),      description: "Biomass yield on sugar.".into(),                     default: 0.1,  typical_range: Some((0.05, 0.2)) }),
                ("m_maint".into(), ParamSchema { label: "m".into(),      units: "g sugar/g yeast/day".into(),  description: "Maintenance energy coefficient.".into(),             default: 0.01, typical_range: Some((0.001, 0.05)) }),
                ("y_co2_s".into(), ParamSchema { label: "Y_CO₂/S".into(),units: "g CO₂/g sugar".into(),       description: "CO₂ produced per gram sugar consumed.".into(),       default: 0.48, typical_range: Some((0.44, 0.51)) }),
                ("kla".into(),     ParamSchema { label: "kLa".into(),    units: "day⁻¹".into(),                description: "Volumetric mass transfer coefficient (liquid↔head).".into(), default: 4.0, typical_range: Some((0.5, 24.0)) }),
                ("v_liq".into(),   ParamSchema { label: "V_liq".into(),  units: "L".into(),                    description: "Volume of liquid in bottle.".into(),                 default: 0.45, typical_range: Some((0.2, 0.8)) }),
                ("v_head".into(),  ParamSchema { label: "V_head".into(), units: "L".into(),                    description: "Volume of headspace in bottle.".into(),               default: 0.05, typical_range: Some((0.02, 0.15)) }),
            ]),
            context_schema: BTreeMap::from([(
                "temperature_c".into(),
                ContextSchema {
                    label: "Fermentation temperature".into(),
                    units: "°C".into(),
                    description: "Bottle temperature. Controls both yeast growth rate and CO₂ \
                        solubility (colder = more CO₂ dissolved = finer bubbles). \
                        Typical F2: 20–25°C warm phase, 4–14°C cold maturation.".into(),
                    source: ContextSource::OperatorInput,
                },
            )]),
            default_params: BTreeMap::from([
                ("mu_base".into(), 0.5),
                ("t_ref_c".into(), 22.0),
                ("k_temp".into(), 0.07),
                ("k_s".into(), 5.0),
                ("k_d".into(), 0.05),
                ("y_xs".into(), 0.1),
                ("m_maint".into(), 0.01),
                ("y_co2_s".into(), 0.48),
                ("kla".into(), 4.0),
                ("v_liq".into(), 0.45),
                ("v_head".into(), 0.05),
            ]),
            default_integrator: "rk4".into(),
            default_step_days: 0.01,
            citations: vec![
                "Monod (1949) kinetics — substrate-limited microbial growth".into(),
                "van 't Hoff equation for Henry's Law temperature dependence".into(),
                "Ideal Gas Law for headspace pressure (R = 0.08314 L·bar/(mol·K))".into(),
                "Original F2 carbonation model: operator prototype (kask.bio)".into(),
            ],
        }
    }

    fn system(&self, _t: f64, y: &[f64], dy: &mut [f64]) {
        // State vector order matches state_order():
        //   y[0] = bio:yeast_g_per_l        (X)
        //   y[1] = chem:sugar_g_per_l       (S)
        //   y[2] = chem:co2_dissolved_g_per_l (C)
        //   y[3] = phys:headspace_pressure_bar (P)

        let x = y[0].max(0.0); // yeast — floor at 0
        let s = y[1].max(0.0); // sugar — floor at 0 (no negative sugar)
        let c = y[2].max(0.0); // dissolved CO₂ — floor at 0
        let p = y[3].max(0.0); // pressure — floor at 0 (sealed bottle)

        // ── Biological rates ──────────────────────────────────────────────────
        let mu_max = self.mu_max();
        let mu = mu_max * s / (self.k_s + s); // Monod: goes to 0 naturally as S → 0

        // dX/dt = (μ − k_d) · X
        let dx_dt = (mu - self.k_d) * x;

        // Sugar consumption rate: growth demand + maintenance (g/L/day)
        let sugar_consumption_rate = (mu / self.y_xs + self.m_maint) * x;

        // dS/dt = −consumption_rate (naturally → 0 as X → 0 or S → 0 via Monod)
        let ds_dt = -sugar_consumption_rate;

        // ── CO₂ gas transfer ──────────────────────────────────────────────────
        let c_sat = self.co2_sat_g_per_l(p);

        // Mass transfer: positive = outgassing (liquid → headspace) when C > C_sat
        let transfer_rate = self.kla * (c - c_sat); // g/(L_liq · day)

        // CO₂ produced by fermentation (g/L/day in liquid phase)
        let co2_production_rate = self.y_co2_s * sugar_consumption_rate;

        // dC/dt = production − transfer (net dissolution/outgassing)
        let dc_dt = co2_production_rate - transfer_rate;

        // ── Headspace pressure ────────────────────────────────────────────────
        // CO₂ transferred to headspace: transfer_rate [g/(L_liq·day)] × V_liq [L] = g/day total
        // Convert to bar/day via ideal gas law:
        //   dP/dt = (n_dot · R_bar · T) / V_head
        //   n_dot = transfer_rate · V_liq / MW_CO2   [mol/day]
        let t_k = self.temperature_c + 273.15;
        let dp_dt = transfer_rate
            * (self.v_liq / self.v_head)
            * (R_BAR * t_k / MW_CO2);

        dy[0] = dx_dt;
        dy[1] = ds_dt;
        dy[2] = dc_dt;
        dy[3] = dp_dt;
    }

    fn state_order(&self) -> Vec<String> {
        vec![
            "bio:yeast_g_per_l".into(),
            "chem:sugar_g_per_l".into(),
            "chem:co2_dissolved_g_per_l".into(),
            "phys:headspace_pressure_bar".into(),
        ]
    }

    fn is_converged(&self, history: &[(f64, Vec<f64>)]) -> bool {
        // Converged when: sugar exhausted AND pressure stabilised
        if history.len() < 10 { return false; }
        let last = &history[history.len() - 10..];
        let sugar_exhausted = last.iter().all(|(_, y)| y[1] < 0.1);
        let pressure_stable = {
            let pressures: Vec<f64> = last.iter().map(|(_, y)| y[3]).collect();
            let max = pressures.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let min = pressures.iter().cloned().fold(f64::INFINITY, f64::min);
            (max - min) < 0.01
        };
        sugar_exhausted && pressure_stable
    }

    fn generate_notes(&self, trajectory: &[(f64, Vec<f64>)]) -> Vec<Note> {
        let mut notes = Vec::new();
        let mut warned_pressure = false;
        let mut noted_sugar_out = false;
        let mut noted_carbonation = false;

        for (t, y) in trajectory {
            let _x = y[0];
            let s  = y[1];
            let c  = y[2];
            let p  = y[3];

            // Burst risk: > 3.5 bar is approaching glass bottle safety limit
            if !warned_pressure && p > 3.5 {
                notes.push(Note {
                    severity: "warning".into(),
                    message: format!(
                        "Headspace pressure {:.2} bar exceeds safe limit for glass bottles (~3.5 bar). \
                         Risk of burst. Refrigerate immediately or burp bottle.",
                        p
                    ),
                    t_hours: Some(t * 24.0),
                });
                warned_pressure = true;
            }

            // Sugar exhausted
            if !noted_sugar_out && s < 0.1 {
                notes.push(Note {
                    severity: "info".into(),
                    message: format!(
                        "Sugar exhausted ({:.3} g/L) — fermentation stopping. \
                         Final carbonation determined by current CO₂ level.",
                        s
                    ),
                    t_hours: Some(t * 24.0),
                });
                noted_sugar_out = true;
            }

            // Good carbonation reached: dissolved CO₂ > 5 g/L (well-carbonated)
            if !noted_carbonation && c > 5.0 {
                notes.push(Note {
                    severity: "info".into(),
                    message: format!(
                        "Dissolved CO₂ reached {:.2} g/L — well-carbonated level. \
                         Refrigerate to lock in carbonation and promote fine bubbles.",
                        c
                    ),
                    t_hours: Some(t * 24.0),
                });
                noted_carbonation = true;
            }
        }
        notes
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    fn default_model() -> KombuchaF2Carbonation {
        KombuchaF2Carbonation::from_context(24.0, &BTreeMap::new())
    }

    fn initial_state() -> Vec<f64> {
        vec![
            0.1,  // yeast g/L
            20.0, // sugar g/L
            1.5,  // dissolved CO₂ g/L
            1.0,  // headspace pressure bar
        ]
    }

    #[test]
    fn yeast_grows_with_sugar_present() {
        let model = default_model();
        let y = initial_state();
        let mut dy = vec![0.0; 4];
        model.system(0.0, &y, &mut dy);
        assert!(dy[0] > 0.0, "Yeast must grow when sugar present, got dy[0]={}", dy[0]);
    }

    #[test]
    fn sugar_decreases_monotonically() {
        let model = default_model();
        let y = initial_state();
        let mut dy = vec![0.0; 4];
        model.system(0.0, &y, &mut dy);
        assert!(dy[1] < 0.0, "Sugar must decrease, got dy[1]={}", dy[1]);
    }

    #[test]
    fn no_growth_without_sugar() {
        let model = default_model();
        let y = vec![0.5, 0.0, 3.0, 2.0]; // zero sugar
        let mut dy = vec![0.0; 4];
        model.system(0.0, &y, &mut dy);
        // Monod: mu = mu_max * 0 / (K_s + 0) = 0, so dX/dt = -k_d * X
        assert!(dy[0] <= 0.0, "No growth without sugar: dX/dt must be <= 0");
        // dS/dt should also be ~0 (maintenance × X is small but non-zero)
        // But no growth demand. Total consumption = m_maint * X only.
        assert!(dy[1] <= 0.0, "Sugar maintenance still consumed");
    }

    #[test]
    fn pressure_increases_when_supersaturated() {
        // C > C_sat → outgassing → pressure rises
        let model = default_model();
        let c_sat = model.co2_sat_g_per_l(1.0);
        let y = vec![0.1, 20.0, c_sat + 2.0, 1.0]; // CO₂ above saturation
        let mut dy = vec![0.0; 4];
        model.system(0.0, &y, &mut dy);
        assert!(dy[3] > 0.0, "Pressure must rise when CO₂ supersaturated: dy[3]={}", dy[3]);
    }

    #[test]
    fn co2_dissolves_when_undersaturated() {
        // C < C_sat → absorption from headspace into liquid
        let model = default_model();
        let c_sat = model.co2_sat_g_per_l(3.0); // high pressure → high saturation
        // Put liquid well below saturation, with sugar depleted (no co2 production)
        let y = vec![0.0, 0.0, 0.0, 3.0]; // C=0 << C_sat at P=3 bar
        let mut dy = vec![0.0; 4];
        model.system(0.0, &y, &mut dy);
        // transfer_rate = kLa * (0 - C_sat) < 0 → dc_dt = 0 - negative = positive
        assert!(dy[2] > 0.0, "CO₂ must dissolve into liquid when undersaturated: dy[2]={}", dy[2]);
        // pressure must fall as CO₂ transfers from headspace to liquid
        assert!(dy[3] < 0.0, "Pressure must drop when CO₂ dissolves: dy[3]={}", dy[3]);
    }

    #[test]
    fn colder_temperature_higher_co2_saturation() {
        // Lower T → higher K_H → higher C_sat → more CO₂ dissolves → finer bubbles
        let cold = KombuchaF2Carbonation::from_context(4.0, &BTreeMap::new());
        let warm = KombuchaF2Carbonation::from_context(24.0, &BTreeMap::new());
        let c_sat_cold = cold.co2_sat_g_per_l(2.0);
        let c_sat_warm = warm.co2_sat_g_per_l(2.0);
        assert!(
            c_sat_cold > c_sat_warm,
            "Cold liquid must hold more CO₂. cold={:.3}, warm={:.3}",
            c_sat_cold, c_sat_warm
        );
    }

    #[test]
    fn higher_temperature_faster_yeast_growth() {
        let warm = KombuchaF2Carbonation::from_context(28.0, &BTreeMap::new());
        let cool = KombuchaF2Carbonation::from_context(14.0, &BTreeMap::new());
        assert!(
            warm.mu_max() > cool.mu_max(),
            "Warmer must give higher mu_max. warm={:.4}, cool={:.4}",
            warm.mu_max(), cool.mu_max()
        );
    }

    #[test]
    fn pressure_equilibrium_at_saturation() {
        // When C == C_sat exactly, transfer = 0, so dP/dt = 0 (from gas transfer)
        // (there's still biological CO₂ production unless sugar=0)
        let model = default_model();
        let p = 2.0;
        let c_sat = model.co2_sat_g_per_l(p);
        let y = vec![0.0, 0.0, c_sat, p]; // no yeast/sugar, exactly at saturation
        let mut dy = vec![0.0; 4];
        model.system(0.0, &y, &mut dy);
        assert_abs_diff_eq!(dy[3], 0.0, epsilon = 1e-10);
    }

    #[test]
    fn state_order_has_four_dimensions() {
        let model = default_model();
        assert_eq!(model.state_order().len(), 4);
        assert_eq!(model.state_order()[0], "bio:yeast_g_per_l");
        assert_eq!(model.state_order()[3], "phys:headspace_pressure_bar");
    }

    #[test]
    fn burst_warning_note_generated() {
        // Build a trajectory with pressure > 3.5 bar at some point
        let model = default_model();
        let traj: Vec<(f64, Vec<f64>)> = vec![
            (0.0, vec![0.1, 20.0, 1.5, 1.0]),
            (1.0, vec![0.3, 15.0, 4.0, 2.5]),
            (2.0, vec![0.5, 10.0, 6.0, 3.8]), // >3.5 bar → warning
        ];
        let notes = model.generate_notes(&traj);
        assert!(
            notes.iter().any(|n| n.severity == "warning" && n.message.contains("burst")),
            "Expected burst warning note"
        );
    }

    #[test]
    fn co2_yield_from_sugar_correct() {
        // Theoretical: 1 mol glucose (180 g) → 2 mol CO₂ (88 g) = 0.489 g CO₂/g sugar
        // Our default Y_co2_s = 0.48 is appropriately close.
        let model = default_model();
        assert!((model.y_co2_s - 0.48).abs() < 0.01);
    }
}
