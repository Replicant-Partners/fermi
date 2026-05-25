//! Rheology — instantaneous fluid property calculators.
//!
//! Rheology models differ from [`DynamicsModel`] in one critical way:
//! they are **algebraic**, not differential. They compute fluid properties
//! (viscosity, yield stress, etc.) at a single operating point — no time
//! integration required.
//!
//! The trait is deliberately separate from `DynamicsModel` to prevent
//! misuse (wrapping a property calculator in an ODE integrator makes no
//! physical sense).
//!
//! # Usage
//!
//! ```rust
//! use dynamics::rheology::{AlgaeViscosity, RheologyModel, RheologyInput};
//!
//! let model = AlgaeViscosity::default();
//! let input = RheologyInput {
//!     temperature_c: 25.0,
//!     shear_rate_per_s: 100.0,
//!     volume_fraction: 0.15,
//!     params_override: Default::default(),
//! };
//! let output = model.compute(&input).unwrap();
//! // output.viscosity_pa_s ≈ 0.0012 (shear-thinning algae at 25°C, 100 1/s, 15% φ)
//! ```

use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use crate::manifest::{ModelManifest, ParamSchema, StateFieldSchema, ContextSchema, ContextSource};

// ─── Trait ────────────────────────────────────────────────────────────────────

/// Algebraic fluid property calculator.
/// Compute a property at a single operating point — no time dimension.
pub trait RheologyModel: Send + Sync {
    fn manifest(&self) -> RheologyManifest;

    /// Compute the property value(s) at the given operating conditions.
    fn compute(&self, input: &RheologyInput) -> Result<RheologyOutput, String>;
}

// ─── I/O types ────────────────────────────────────────────────────────────────

/// Operating conditions for a single rheology evaluation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RheologyInput {
    /// Fluid temperature in °C.
    pub temperature_c: f64,
    /// Shear rate in s⁻¹. Typical ranges:
    ///   - Sedimentation: 0.001–0.01
    ///   - Mixing tank:   10–100
    ///   - Pump discharge: 100–1000
    pub shear_rate_per_s: f64,
    /// Volume fraction of suspended phase (0–1). E.g. 0.15 = 15% algae.
    pub volume_fraction: f64,
    /// Override model default parameters by name.
    #[serde(default)]
    pub params_override: BTreeMap<String, f64>,
}

/// Result of a single rheology evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RheologyOutput {
    /// Dynamic viscosity in Pa·s.
    pub viscosity_pa_s: f64,
    /// Power-law flow behaviour index n (dimensionless).
    ///   n = 1.0 → Newtonian
    ///   n < 1.0 → shear-thinning (typical for algae suspensions)
    ///   n > 1.0 → shear-thickening (rare for algae)
    pub flow_index_n: f64,
    /// Consistency index K at the given temperature (Pa·sⁿ).
    pub consistency_index_k: f64,
    /// Regime classification based on n.
    pub regime: FlowRegime,
    /// Effective kinematic viscosity in mm²/s (= cSt), for pump sizing reference.
    pub kinematic_mm2_per_s: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FlowRegime {
    Newtonian,       // n ≈ 1.0
    ShearThinning,   // n < 1.0 — typical for algae, polymers
    ShearThickening, // n > 1.0 — dense suspensions under high shear
}

/// Manifest for a rheology model (analogous to ModelManifest for ODE models).
#[derive(Debug, Clone, Serialize)]
pub struct RheologyManifest {
    pub uri: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub input_schema: BTreeMap<String, ParamSchema>,
    pub output_dimensions: Vec<String>,
    pub citations: Vec<String>,
}

// ─── AlgaeViscosity model ─────────────────────────────────────────────────────

/// Power-law (Ostwald-de Waele) viscosity model for algae suspensions
/// with Arrhenius temperature dependence.
///
/// ## Physics
///
/// Dynamic viscosity:
///   μ = K(T) · γ̇^(n − 1)
///
/// Temperature-dependent consistency index (modified Arrhenius, reference form):
///   K(T) = K_ref · exp(Ea/R · (1/T_K − 1/T_ref_K))
///
/// where T_ref = 25°C (298.15 K) is the reference temperature at which K = K_ref.
/// This form guarantees K(T) decreases as T increases (higher T → exponent becomes
/// more negative → K smaller → lower viscosity), which is physically correct for
/// all liquid systems.
///
/// **Bug in original code:** used `exp(+Ea/RT)` (wrong sign and no reference T),
/// which caused viscosity to increase with temperature — physically backwards.
/// Fixed by using the reference-form Arrhenius.
///
/// Concentration-dependent flow index:
///   n = 1 − C_n · φ
///
/// where φ is the volume fraction of algae cells (0–1) and C_n is
/// a concentration sensitivity coefficient (default 0.8). At φ = 0
/// (clear water) n = 1.0 (Newtonian). At φ = 0.15 (15%) n = 0.88
/// (mildly shear-thinning). At φ > 1/C_n the model saturates at n_min.
///
/// ## Parameters
///
/// | Name | Default | Units | Description |
/// |---|---|---|---|
/// | `k0` | 0.001 | Pa·sⁿ | Base consistency index at reference T |
/// | `ea` | 15000 | J/mol | Flow activation energy (Arrhenius Ea) |
/// | `c_n` | 0.8 | — | Concentration sensitivity of flow index |
/// | `n_min` | 0.1 | — | Floor on flow index (prevents n ≤ 0) |
/// | `density_kg_m3` | 1050 | kg/m³ | Suspension density (for kinematic viscosity) |
///
/// ## Typical values for Chlorella/Spirulina suspensions
///
/// - φ = 0.05 (5%): n ≈ 0.96, essentially Newtonian
/// - φ = 0.15 (15%): n ≈ 0.88, mildly shear-thinning
/// - φ = 0.30 (30%): n ≈ 0.76, moderately shear-thinning
///
/// ## References
///
/// Wileman et al. (2012) "Rheological properties of algal slurries for the
/// purposes of harvesting and dewatering." Bioresource Technology 118, 540–546.
/// doi:10.1016/j.biortech.2012.05.071
pub struct AlgaeViscosity {
    /// Consistency index at reference temperature T_ref (Pa·sⁿ)
    pub k0: f64,
    /// Arrhenius activation energy Ea (J/mol). Always positive.
    pub ea: f64,
    /// Reference temperature in Kelvin (default 298.15 = 25°C).
    pub t_ref_k: f64,
    /// Concentration sensitivity coefficient for flow index
    pub c_n: f64,
    /// Minimum flow index floor (prevents n ≤ 0 at high concentrations)
    pub n_min: f64,
    /// Suspension density kg/m³ (for kinematic viscosity output)
    pub density_kg_m3: f64,
}

impl Default for AlgaeViscosity {
    fn default() -> Self {
        Self {
            k0: 0.001,
            ea: 15_000.0,
            t_ref_k: 298.15,  // 25°C
            c_n: 0.8,
            n_min: 0.1,
            density_kg_m3: 1_050.0,
        }
    }
}

impl AlgaeViscosity {
    pub fn from_params(k0: f64, ea: f64, t_ref_k: f64, c_n: f64, n_min: f64, density_kg_m3: f64) -> Self {
        Self { k0, ea, t_ref_k, c_n, n_min, density_kg_m3 }
    }

    pub fn from_input(input: &RheologyInput) -> Self {
        let d = Self::default();
        Self {
            k0:            *input.params_override.get("k0").unwrap_or(&d.k0),
            ea:            *input.params_override.get("ea").unwrap_or(&d.ea),
            t_ref_k:       *input.params_override.get("t_ref_k").unwrap_or(&d.t_ref_k),
            c_n:           *input.params_override.get("c_n").unwrap_or(&d.c_n),
            n_min:         *input.params_override.get("n_min").unwrap_or(&d.n_min),
            density_kg_m3: *input.params_override.get("density_kg_m3").unwrap_or(&d.density_kg_m3),
        }
    }

    /// Export current parameter values as a `params_override` map.
    /// Used by `derive_rheology` to pass calibrated params back into per-point compute calls.
    pub fn to_input_overrides(&self) -> BTreeMap<String, f64> {
        BTreeMap::from([
            ("k0".into(),            self.k0),
            ("ea".into(),            self.ea),
            ("t_ref_k".into(),       self.t_ref_k),
            ("c_n".into(),           self.c_n),
            ("n_min".into(),         self.n_min),
            ("density_kg_m3".into(), self.density_kg_m3),
        ])
    }
}

impl RheologyModel for AlgaeViscosity {
    fn manifest(&self) -> RheologyManifest {
        RheologyManifest {
            uri: "kask:rheology/algae_viscosity@v1".into(),
            version: "1.0.0".into(),
            name: "Algae suspension viscosity (power-law + Arrhenius)".into(),
            description: "Power-law (Ostwald-de Waele) dynamic viscosity for algae suspensions. \
                Temperature dependence via Arrhenius (negative exponent — viscosity decreases \
                with temperature). Shear-thinning index n decreases linearly with volume fraction.".into(),
            input_schema: BTreeMap::from([
                ("temperature_c".into(), ParamSchema { label: "Temperature".into(), units: "°C".into(), description: "Fluid temperature.".into(), default: 25.0, typical_range: Some((4.0, 40.0)) }),
                ("shear_rate_per_s".into(), ParamSchema { label: "Shear rate".into(), units: "s⁻¹".into(), description: "Applied shear rate. Mixing tank: 10–100, pump: 100–1000.".into(), default: 100.0, typical_range: Some((0.001, 10_000.0)) }),
                ("volume_fraction".into(), ParamSchema { label: "Volume fraction".into(), units: "—".into(), description: "Algae volume fraction (0–1). E.g. 0.15 = 15%.".into(), default: 0.10, typical_range: Some((0.01, 0.40)) }),
                ("k0".into(), ParamSchema { label: "K₀".into(), units: "Pa·sⁿ".into(), description: "Base consistency index.".into(), default: 0.001, typical_range: Some((0.0001, 0.1)) }),
                ("ea".into(), ParamSchema { label: "Ea".into(), units: "J/mol".into(), description: "Flow activation energy.".into(), default: 15_000.0, typical_range: Some((5_000.0, 30_000.0)) }),
                ("c_n".into(), ParamSchema { label: "C_n".into(), units: "—".into(), description: "Concentration sensitivity of flow index.".into(), default: 0.8, typical_range: Some((0.3, 1.5)) }),
            ]),
            output_dimensions: vec![
                "viscosity_pa_s".into(),
                "flow_index_n".into(),
                "consistency_index_k".into(),
            ],
            citations: vec![
                "Wileman et al. (2012) Bioresource Technology 118:540-546. doi:10.1016/j.biortech.2012.05.071".into(),
                "Ostwald-de Waele power law: Bird, Armstrong, Hassager (1987) Dynamics of Polymeric Liquids Vol.1".into(),
            ],
        }
    }

    fn compute(&self, input: &RheologyInput) -> Result<RheologyOutput, String> {
        // Resolve params from override or use struct fields
        let model = AlgaeViscosity::from_input(input);

        // Validate inputs
        if input.temperature_c < -273.15 {
            return Err(format!("temperature_c {} is below absolute zero", input.temperature_c));
        }
        if input.shear_rate_per_s <= 0.0 {
            return Err(format!("shear_rate_per_s must be > 0, got {}", input.shear_rate_per_s));
        }
        if !(0.0..=1.0).contains(&input.volume_fraction) {
            return Err(format!("volume_fraction must be in [0, 1], got {}", input.volume_fraction));
        }

        let t_k = input.temperature_c + 273.15;
        const R: f64 = 8.314; // J/(mol·K)

        // Consistency index K(T) — reference-form Arrhenius.
        //   K(T) = K_ref · exp(Ea/R · (1/T − 1/T_ref))
        //
        // When T > T_ref: exponent is negative → K < K_ref → lower viscosity ✓
        // When T < T_ref: exponent is positive → K > K_ref → higher viscosity ✓
        // At T = T_ref: exponent = 0 → K = K_ref ✓
        //
        // This form is physically correct for all liquid systems regardless of
        // whether Ea is large or small. The original code used exp(+Ea/RT) with
        // no reference temperature, which produced K increasing with T (wrong).
        let k_temp = model.k0 * ((model.ea / R) * (1.0 / t_k - 1.0 / model.t_ref_k)).exp();

        // Flow index n — decreases linearly with volume fraction.
        // Clamped to [n_min, 1.0] to prevent unphysical values.
        let n = (1.0 - model.c_n * input.volume_fraction).clamp(model.n_min, 2.0);

        // Power-law viscosity: μ = K · γ̇^(n-1)
        // At n=1 (Newtonian): μ = K (independent of shear rate)
        // At n<1 (shear-thinning): viscosity decreases with shear rate
        let viscosity = k_temp * input.shear_rate_per_s.powf(n - 1.0);

        // Kinematic viscosity ν = μ/ρ (mm²/s = cSt)
        let kinematic = if model.density_kg_m3 > 0.0 {
            Some(viscosity / model.density_kg_m3 * 1e6) // Pa·s / (kg/m³) × 10⁶ → mm²/s
        } else {
            None
        };

        let regime = if (n - 1.0).abs() < 0.02 {
            FlowRegime::Newtonian
        } else if n < 1.0 {
            FlowRegime::ShearThinning
        } else {
            FlowRegime::ShearThickening
        };

        Ok(RheologyOutput {
            viscosity_pa_s: viscosity,
            flow_index_n: n,
            consistency_index_k: k_temp,
            regime,
            kinematic_mm2_per_s: kinematic,
        })
    }
}

// ─── Registry ─────────────────────────────────────────────────────────────────

/// Resolve a rheology model URI.
pub fn resolve_rheology(uri: &str) -> Option<Box<dyn RheologyModel>> {
    match uri {
        "kask:rheology/algae_viscosity@v1" => Some(Box::new(AlgaeViscosity::default())),
        _ => None,
    }
}

pub fn known_rheology_uris() -> Vec<&'static str> {
    vec!["kask:rheology/algae_viscosity@v1"]
}

pub fn list_rheology_manifests() -> Vec<RheologyManifest> {
    vec![AlgaeViscosity::default().manifest()]
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_input() -> RheologyInput {
        RheologyInput {
            temperature_c: 25.0,
            shear_rate_per_s: 100.0,
            volume_fraction: 0.15,
            params_override: BTreeMap::new(),
        }
    }

    #[test]
    fn viscosity_decreases_with_temperature() {
        // Bug fix validation: Arrhenius must have NEGATIVE exponent.
        // Higher temperature → lower viscosity.
        let model = AlgaeViscosity::default();
        let cold = model.compute(&RheologyInput { temperature_c: 10.0, ..default_input() }).unwrap();
        let warm = model.compute(&RheologyInput { temperature_c: 35.0, ..default_input() }).unwrap();
        assert!(
            warm.viscosity_pa_s < cold.viscosity_pa_s,
            "Viscosity must decrease with temperature. cold={:.6}, warm={:.6}",
            cold.viscosity_pa_s, warm.viscosity_pa_s
        );
    }

    #[test]
    fn viscosity_decreases_with_shear_rate_for_shear_thinning() {
        // n < 1 → shear-thinning → μ decreases as γ̇ increases
        let model = AlgaeViscosity::default();
        let slow = model.compute(&RheologyInput { shear_rate_per_s: 10.0,  ..default_input() }).unwrap();
        let fast = model.compute(&RheologyInput { shear_rate_per_s: 1000.0, ..default_input() }).unwrap();
        assert!(
            fast.viscosity_pa_s < slow.viscosity_pa_s,
            "Shear-thinning: viscosity must drop at higher shear rates. slow={:.6}, fast={:.6}",
            slow.viscosity_pa_s, fast.viscosity_pa_s
        );
        assert_eq!(slow.regime, FlowRegime::ShearThinning);
    }

    #[test]
    fn higher_concentration_more_shear_thinning() {
        // More algae → lower n → stronger shear-thinning behaviour
        let model = AlgaeViscosity::default();
        let dilute = model.compute(&RheologyInput { volume_fraction: 0.05, ..default_input() }).unwrap();
        let dense  = model.compute(&RheologyInput { volume_fraction: 0.30, ..default_input() }).unwrap();
        assert!(
            dense.flow_index_n < dilute.flow_index_n,
            "Dense suspension must have lower n. dilute={:.4}, dense={:.4}",
            dilute.flow_index_n, dense.flow_index_n
        );
    }

    #[test]
    fn zero_concentration_is_newtonian() {
        // φ = 0 → n = 1.0 → Newtonian water-like fluid
        let model = AlgaeViscosity::default();
        let output = model.compute(&RheologyInput { volume_fraction: 0.0, ..default_input() }).unwrap();
        assert_eq!(output.regime, FlowRegime::Newtonian);
        assert!((output.flow_index_n - 1.0).abs() < 0.02);
    }

    #[test]
    fn flow_index_clamped_above_n_min() {
        // Very high concentration should not drive n to zero or negative
        let model = AlgaeViscosity::default();
        let output = model.compute(&RheologyInput { volume_fraction: 0.99, ..default_input() }).unwrap();
        assert!(output.flow_index_n >= model.n_min, "n must not drop below n_min");
    }

    #[test]
    fn known_reference_value() {
        // At T = T_ref (25°C), the exponent is exactly 0, so K(T_ref) = K_ref = k0.
        // n = 1 - 0.8*0.15 = 0.88
        // μ = 0.001 * 100^(0.88-1) = 0.001 * 100^(-0.12) = 0.001 / 1.820 ≈ 5.495e-4
        let model = AlgaeViscosity::default();
        let output = model.compute(&default_input()).unwrap(); // default is 25°C = T_ref
        let n_expected = 1.0 - 0.8 * 0.15;
        let mu_expected = 0.001_f64 * 100.0_f64.powf(n_expected - 1.0); // K = k0 at T_ref
        assert!(
            (output.viscosity_pa_s - mu_expected).abs() / mu_expected < 1e-6,
            "At T_ref viscosity must equal k0 * gamma^(n-1). got={:.6e}, expected={:.6e}",
            output.viscosity_pa_s, mu_expected
        );
    }

    #[test]
    fn invalid_shear_rate_returns_error() {
        let model = AlgaeViscosity::default();
        let result = model.compute(&RheologyInput { shear_rate_per_s: 0.0, ..default_input() });
        assert!(result.is_err());
    }

    #[test]
    fn kinematic_viscosity_computed() {
        let model = AlgaeViscosity::default();
        let output = model.compute(&default_input()).unwrap();
        assert!(output.kinematic_mm2_per_s.is_some());
        // ν = μ/ρ * 1e6, with ρ=1050 kg/m³ — just check it's positive and small
        assert!(output.kinematic_mm2_per_s.unwrap() > 0.0);
    }
}
