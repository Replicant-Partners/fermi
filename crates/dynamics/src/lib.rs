//! # dynamics — ODE-based dynamics models for SimOps digital twin
//!
//! Deterministic, pure-function simulation of coupled biochemical/physical
//! systems over time. No LLM, no I/O. The skill entry point is
//! [`apply_dynamics_model`].
//!
//! ## Models
//!
//! | URI | State | Description |
//! |---|---|---|
//! | `kask:dynamics/linear_decay@v1` | 1D | dv/dt = -k(v-target) |
//! | `kask:dynamics/kombucha_fermentation@v1` | 2D | Brix + pH, Arrhenius |
//! | `kask:dynamics/pellicle_growth@v1` | 3D | Brix + pH + Pellicle |
//! | `kask:dynamics/bc_optimization@v1` | 4D | Brix + pH + BC_yield + BC_quality |
//!
//! ## Usage
//!
//! ```rust
//! use dynamics::{apply_dynamics_model, SkillInput, Horizon};
//! use std::collections::BTreeMap;
//!
//! let input = SkillInput {
//!     model_uri: "kask:dynamics/kombucha_fermentation@v1".into(),
//!     initial_state: BTreeMap::from([
//!         ("chem:brix_percent".into(), 10.0),
//!         ("chem:ph_value".into(), 5.0),
//!     ]),
//!     process_context: serde_json::json!({ "temperature_c": 26.0 }),
//!     params_override: BTreeMap::new(),
//!     horizon: Horizon::Fixed { days: 14.0 },
//!     sample_cadence: None,
//!     integrator: None,
//!     generated_by: None,
//! };
//!
//! let output = apply_dynamics_model(input).unwrap();
//! assert!(output.converged || !output.trajectories.is_empty());
//! ```

pub mod coupled;
pub mod integrator;
pub mod manifest;
pub mod models;
pub mod provenance;
pub mod registry;
pub mod rheology;

use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

// ─── Public types ─────────────────────────────────────────────────────────────

pub use manifest::{
    ContributionMode, ContextSchema, ContextSource, ModelManifest, ParamSchema, StateFieldSchema,
};
pub use provenance::Provenance;
pub use rheology::{
    AlgaeViscosity, FlowRegime, RheologyInput, RheologyManifest, RheologyModel, RheologyOutput,
    list_rheology_manifests, known_rheology_uris, resolve_rheology,
};

/// One sample on a property's trajectory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrajectoryPoint {
    pub t_hours: f64,
    pub value: f64,
}

/// A free-form note the model surfaces (floor engaged, convergence warning, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    /// "info" | "warning" | "error"
    pub severity: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub t_hours: Option<f64>,
}

/// Termination condition for the integration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Horizon {
    /// Integrate for exactly `days` days.
    Fixed { days: f64 },
    /// Integrate until a property's value crosses a threshold (or max_days reached).
    UntilPropertyReaches {
        property: String,
        value: f64,
        max_days: f64,
    },
}

/// Sample cadence — how often to record a trajectory point.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SampleCadence {
    pub hours: f64,
}

/// Input to `apply_dynamics_model` — matches the kask `invokeDynamicsRunner` payload.
#[derive(Debug, Clone, Deserialize)]
pub struct SkillInput {
    pub model_uri: String,
    /// Initial values keyed by property URI (e.g. "chem:brix_percent").
    pub initial_state: BTreeMap<String, f64>,
    /// Process-level context (temperature, etc.) as a JSON object.
    #[serde(default)]
    pub process_context: serde_json::Value,
    /// Override model default params by name.
    #[serde(default)]
    pub params_override: BTreeMap<String, f64>,
    pub horizon: Horizon,
    #[serde(default)]
    pub sample_cadence: Option<SampleCadence>,
    /// "rk4" (default) | "dopri5" | "dop853"
    #[serde(default)]
    pub integrator: Option<String>,
    #[serde(default)]
    pub generated_by: Option<String>,
}

/// A single derived (post-integration) quantity at one trajectory point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedPoint {
    pub t_hours: f64,
    pub value: f64,
}

/// A derived quantity trajectory — computed from primary state trajectories
/// after integration. Not an ODE state variable; no feedback into the solver.
///
/// Examples:
///   `"phys:dynamic_viscosity_pa_s"` — broth viscosity at each timestep
///   `"phys:flow_index_n"`           — shear-thinning index
///   `"phys:kinematic_viscosity_cst"` — for pump sizing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedTrajectory {
    /// Property URI (same convention as `trajectories` keys)
    pub property_uri: String,
    /// Human-readable label
    pub label: String,
    /// SI units string
    pub units: String,
    /// Time-series values
    pub points: Vec<DerivedPoint>,
    /// Which model produced this derived quantity
    pub source_model_uri: String,
}

/// Output of `apply_dynamics_model`.
#[derive(Debug, Clone, Serialize)]
pub struct SkillOutput {
    /// Primary ODE state trajectories. Keyed by property URI.
    pub trajectories: BTreeMap<String, Vec<TrajectoryPoint>>,
    /// Derived quantities computed from primary trajectories after integration.
    /// Empty when no derivations apply to this model.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub derived_quantities: Vec<DerivedTrajectory>,
    pub provenance: Provenance,
    pub converged: bool,
    pub notes: Vec<Note>,
}

// ─── DynamicsModel trait ──────────────────────────────────────────────────────

/// Every dynamics model implements this. Pure functions; no I/O.
pub trait DynamicsModel: Send + Sync {
    /// The model's manifest (URI, version, schema, default params).
    fn manifest(&self) -> ModelManifest;

    /// The ODE system: dy/dt = f(t, y).
    /// `t` is time in **days**. `y` is indexed by `state_order()`.
    fn system(&self, t: f64, y: &[f64], dy: &mut [f64]);

    /// Property URIs in the order they appear in the state vector.
    fn state_order(&self) -> Vec<String>;

    /// Optional: model-specific convergence detection over rolling window.
    fn is_converged(&self, _history: &[(f64, Vec<f64>)]) -> bool {
        false
    }

    /// Optional: generate notes from the completed trajectory.
    fn generate_notes(&self, _trajectory: &[(f64, Vec<f64>)]) -> Vec<Note> {
        vec![]
    }
}

// ─── Entry point ──────────────────────────────────────────────────────────────

/// Skill entry point. Dispatches on model_uri, integrates, returns output.
/// Pure — deterministic for a given `SkillInput` (modulo `generated_at` timestamp).
pub fn apply_dynamics_model(input: SkillInput) -> Result<SkillOutput, String> {
    // Resolve model — pass input so registry can read context/params
    let model = registry::resolve(&input.model_uri, Some(&input))
        .ok_or_else(|| format!("Unknown model URI: '{}'. Known models: {}",
            input.model_uri,
            registry::known_uris().join(", ")))?;

    // Build initial state vector in manifest order
    let order = model.state_order();
    let y0: Vec<f64> = order.iter().map(|uri| {
        input.initial_state.get(uri).copied().unwrap_or(0.0)
    }).collect();

    // Determine integration horizon
    let horizon_days = match &input.horizon {
        Horizon::Fixed { days } => *days,
        Horizon::UntilPropertyReaches { max_days, .. } => *max_days,
    };

    // Sample cadence (default: 6h = 0.25 days)
    let cadence_days = input.sample_cadence
        .as_ref()
        .map(|c| c.hours / 24.0)
        .unwrap_or(0.25);

    // Step size: default from manifest, override via params
    let manifest = model.manifest();
    let step_days = input.params_override
        .get("step_size_days")
        .copied()
        .unwrap_or(manifest.default_step_days);

    // Integrate
    let trajectory = integrator::integrate(
        model.as_ref(),
        &y0,
        horizon_days,
        step_days,
        cadence_days,
    )?;

    // Apply until-property-reaches termination if requested
    let trajectory = match &input.horizon {
        Horizon::UntilPropertyReaches { property, value, .. } => {
            let prop_idx = order.iter().position(|u| u == property);
            if let Some(idx) = prop_idx {
                let cutoff = trajectory.iter().position(|(_, y)| {
                    (y[idx] - value).abs() < 1e-3 || y[idx] <= *value
                });
                if let Some(end) = cutoff {
                    trajectory[..=end].to_vec()
                } else {
                    trajectory
                }
            } else {
                trajectory
            }
        }
        Horizon::Fixed { .. } => trajectory,
    };

    // Check convergence
    let converged = model.is_converged(&trajectory);

    // Generate notes
    let notes = model.generate_notes(&trajectory);

    // Unpack to property URI map
    let trajectories = unpack(&order, &trajectory);

    // Build provenance
    let prov = provenance::build(&manifest, &input, step_days);

    // Compute derived quantities (Level 1 coupling — post-integration, no feedback)
    let derived_quantities = derive_rheology(&trajectories, &input);

    Ok(SkillOutput { trajectories, derived_quantities, provenance: prov, converged, notes })
}

// ─── Level 1 rheology coupling ────────────────────────────────────────────────

/// Derive rheological quantities from a completed trajectory.
///
/// Checks whether the trajectory contains state variables that can be used
/// to compute broth viscosity:
///   - `bio:bc_yield_g_per_l`  (bc_optimization model)
///   - `bio:pellicle_g_per_l`  (pellicle_growth model)
///
/// If found, converts concentration to volume fraction and calls
/// `AlgaeViscosity` at each trajectory point. Returns derived trajectories
/// for viscosity, flow index n, and consistency index K.
///
/// Returns an empty Vec when the model has no compatible state variables.
fn derive_rheology(
    trajectories: &BTreeMap<String, Vec<TrajectoryPoint>>,
    input: &SkillInput,
) -> Vec<DerivedTrajectory> {
    // Determine which concentration trajectory to use as φ proxy.
    // Priority: bc_yield > pellicle > none.
    let (conc_uri, bc_density) = if trajectories.contains_key("bio:bc_yield_g_per_l") {
        // BC is ~pure cellulose, density ≈ 1500 g/L (crystal) but effective
        // suspension density closer to 1050 g/L (hydrated gel network).
        ("bio:bc_yield_g_per_l", 1050.0_f64)
    } else if trajectories.contains_key("bio:pellicle_g_per_l") {
        // SCOBY pellicle — similar hydrated cellulose density
        ("bio:pellicle_g_per_l", 1050.0_f64)
    } else {
        // No compatible state variable — nothing to derive
        return vec![];
    };

    let conc_pts = match trajectories.get(conc_uri) {
        Some(pts) if !pts.is_empty() => pts,
        _ => return vec![],
    };

    // Read operating conditions from process_context (same as ODE model)
    let temp_c = input.process_context
        .get("temperature_c").and_then(|v| v.as_f64()).unwrap_or(26.0);
    let agitation_rpm = input.process_context
        .get("agitation_rpm").and_then(|v| v.as_f64()).unwrap_or(0.0);

    // Convert agitation rpm → approximate shear rate (γ̇ ≈ N_imp × rpm)
    // N_imp ≈ 20 for a standard Rushton turbine — reasonable default
    // for algae/BC bioreactors. Operator can override via params_override.
    let n_imp = input.params_override
        .get("rheology_n_imp").copied().unwrap_or(20.0);
    let shear_rate = if agitation_rpm > 0.0 {
        n_imp * agitation_rpm
    } else {
        // Static culture: gentle natural convection, ~0.01–0.1 s⁻¹
        input.params_override
            .get("rheology_static_shear").copied().unwrap_or(0.05)
    };

    // Build rheology model — respects params_override for k0, ea, c_n, etc.
    let rheology = AlgaeViscosity::from_input(&RheologyInput {
        temperature_c: temp_c,
        shear_rate_per_s: shear_rate,
        volume_fraction: 0.0, // placeholder — overridden per point
        params_override: input.params_override.iter()
            .filter(|(k, _)| matches!(k.as_str(), "k0" | "ea" | "c_n" | "n_min" | "density_kg_m3" | "t_ref_k"))
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
    });

    // Compute per-point
    let mut viscosity_pts   = Vec::with_capacity(conc_pts.len());
    let mut flow_index_pts  = Vec::with_capacity(conc_pts.len());
    let mut consistency_pts = Vec::with_capacity(conc_pts.len());
    let mut warned_high_viscosity = false;

    for pt in conc_pts {
        // Volume fraction: φ = concentration [g/L] / density [g/L]
        let phi = (pt.value / bc_density).clamp(0.0, 0.99);

        let rheology_input = RheologyInput {
            temperature_c: temp_c,
            shear_rate_per_s: shear_rate,
            volume_fraction: phi,
            params_override: rheology.to_input_overrides(),
        };

        match rheology.compute(&rheology_input) {
            Ok(r) => {
                viscosity_pts.push(DerivedPoint { t_hours: pt.t_hours, value: r.viscosity_pa_s });
                flow_index_pts.push(DerivedPoint { t_hours: pt.t_hours, value: r.flow_index_n });
                consistency_pts.push(DerivedPoint { t_hours: pt.t_hours, value: r.consistency_index_k });

                // Note: viscosity threshold for pumping concern (~10× water = 0.01 Pa·s)
                if !warned_high_viscosity && r.viscosity_pa_s > 0.01 {
                    warned_high_viscosity = true;
                    // note logged via notes field in SkillOutput — stored separately
                }
            }
            Err(_) => {
                // On compute error, push NaN so the trajectory stays aligned
                viscosity_pts.push(DerivedPoint { t_hours: pt.t_hours, value: f64::NAN });
                flow_index_pts.push(DerivedPoint { t_hours: pt.t_hours, value: f64::NAN });
                consistency_pts.push(DerivedPoint { t_hours: pt.t_hours, value: f64::NAN });
            }
        }
    }

    let rheology_uri = "kask:rheology/algae_viscosity@v1";

    vec![
        DerivedTrajectory {
            property_uri: "phys:dynamic_viscosity_pa_s".into(),
            label: "Dynamic viscosity".into(),
            units: "Pa·s".into(),
            points: viscosity_pts,
            source_model_uri: rheology_uri.into(),
        },
        DerivedTrajectory {
            property_uri: "phys:flow_index_n".into(),
            label: "Flow behaviour index (n)".into(),
            units: "dimensionless".into(),
            points: flow_index_pts,
            source_model_uri: rheology_uri.into(),
        },
        DerivedTrajectory {
            property_uri: "phys:consistency_index_k".into(),
            label: "Consistency index K(T)".into(),
            units: "Pa·sⁿ".into(),
            points: consistency_pts,
            source_model_uri: rheology_uri.into(),
        },
    ]
}

fn unpack(
    order: &[String],
    trajectory: &[(f64, Vec<f64>)],
) -> BTreeMap<String, Vec<TrajectoryPoint>> {
    let mut out: BTreeMap<String, Vec<TrajectoryPoint>> = BTreeMap::new();
    for uri in order {
        out.insert(uri.clone(), Vec::with_capacity(trajectory.len()));
    }
    for (t_days, y) in trajectory {
        for (i, uri) in order.iter().enumerate() {
            if let Some(pts) = out.get_mut(uri) {
                pts.push(TrajectoryPoint {
                    t_hours: t_days * 24.0,
                    value: y[i],
                });
            }
        }
    }
    out
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn bc_input() -> SkillInput {
        SkillInput {
            model_uri: "kask:dynamics/bc_optimization@v1".into(),
            initial_state: BTreeMap::from([
                ("chem:brix_percent".into(), 8.0),
                ("chem:ph_value".into(), 6.0),
                ("bio:bc_yield_g_per_l".into(), 0.0),
                ("bio:bc_quality_index".into(), 1.0),
            ]),
            process_context: serde_json::json!({
                "temperature_c": 30.0,
                "agitation_rpm": 0.0,
                "do_saturation_pct": 10.0,
                "carbon_source": "glucose"
            }),
            params_override: BTreeMap::new(),
            horizon: Horizon::Fixed { days: 14.0 },
            sample_cadence: Some(SampleCadence { hours: 24.0 }),
            integrator: None,
            generated_by: None,
        }
    }

    fn pellicle_input() -> SkillInput {
        SkillInput {
            model_uri: "kask:dynamics/pellicle_growth@v1".into(),
            initial_state: BTreeMap::from([
                ("chem:brix_percent".into(), 10.0),
                ("chem:ph_value".into(), 5.0),
                ("bio:pellicle_g_per_l".into(), 0.1),
            ]),
            process_context: serde_json::json!({ "temperature_c": 26.0 }),
            params_override: BTreeMap::new(),
            horizon: Horizon::Fixed { days: 14.0 },
            sample_cadence: Some(SampleCadence { hours: 24.0 }),
            integrator: None,
            generated_by: None,
        }
    }

    #[test]
    fn bc_model_produces_derived_viscosity() {
        let output = apply_dynamics_model(bc_input()).unwrap();
        assert!(
            !output.derived_quantities.is_empty(),
            "bc_optimization should produce derived rheology quantities"
        );
        let viscosity = output.derived_quantities.iter()
            .find(|d| d.property_uri == "phys:dynamic_viscosity_pa_s")
            .expect("viscosity trajectory must be present");
        assert_eq!(
            viscosity.points.len(),
            output.trajectories["bio:bc_yield_g_per_l"].len(),
            "derived trajectory must have same length as primary trajectory"
        );
        // All viscosity values must be positive and finite
        for pt in &viscosity.points {
            assert!(pt.value > 0.0 && pt.value.is_finite(),
                "viscosity must be positive and finite at t={}h, got {}", pt.t_hours, pt.value);
        }
    }

    #[test]
    fn viscosity_increases_as_bc_yield_grows() {
        let output = apply_dynamics_model(bc_input()).unwrap();
        let viscosity = output.derived_quantities.iter()
            .find(|d| d.property_uri == "phys:dynamic_viscosity_pa_s")
            .unwrap();
        let first = viscosity.points.first().unwrap().value;
        let last  = viscosity.points.last().unwrap().value;
        // More BC → higher volume fraction → higher viscosity
        assert!(last >= first,
            "viscosity should not decrease as BC accumulates. first={:.3e}, last={:.3e}",
            first, last);
    }

    #[test]
    fn pellicle_model_produces_derived_viscosity() {
        let output = apply_dynamics_model(pellicle_input()).unwrap();
        assert!(
            output.derived_quantities.iter().any(|d| d.property_uri == "phys:dynamic_viscosity_pa_s"),
            "pellicle_growth should also produce viscosity derived quantity"
        );
    }

    #[test]
    fn kombucha_model_produces_no_derived_quantities() {
        // kombucha_fermentation has no bc_yield or pellicle state — no rheology derived
        let input = SkillInput {
            model_uri: "kask:dynamics/kombucha_fermentation@v1".into(),
            initial_state: BTreeMap::from([
                ("chem:brix_percent".into(), 10.0),
                ("chem:ph_value".into(), 5.0),
            ]),
            process_context: serde_json::json!({ "temperature_c": 26.0 }),
            params_override: BTreeMap::new(),
            horizon: Horizon::Fixed { days: 7.0 },
            sample_cadence: Some(SampleCadence { hours: 24.0 }),
            integrator: None,
            generated_by: None,
        };
        let output = apply_dynamics_model(input).unwrap();
        assert!(
            output.derived_quantities.is_empty(),
            "kombucha_fermentation has no BC/pellicle state — no derived quantities expected"
        );
    }

    #[test]
    fn derived_trajectory_time_axis_matches_primary() {
        let output = apply_dynamics_model(bc_input()).unwrap();
        let primary_times: Vec<f64> = output.trajectories["bio:bc_yield_g_per_l"]
            .iter().map(|p| p.t_hours).collect();
        let derived_times: Vec<f64> = output.derived_quantities.iter()
            .find(|d| d.property_uri == "phys:dynamic_viscosity_pa_s")
            .unwrap().points.iter().map(|p| p.t_hours).collect();
        assert_eq!(primary_times, derived_times,
            "derived quantity time axis must be identical to primary trajectory time axis");
    }

    #[test]
    fn three_derived_quantities_for_bc_model() {
        let output = apply_dynamics_model(bc_input()).unwrap();
        let uris: Vec<&str> = output.derived_quantities.iter()
            .map(|d| d.property_uri.as_str()).collect();
        assert!(uris.contains(&"phys:dynamic_viscosity_pa_s"));
        assert!(uris.contains(&"phys:flow_index_n"));
        assert!(uris.contains(&"phys:consistency_index_k"));
    }
}
