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

pub mod integrator;
pub mod manifest;
pub mod models;
pub mod provenance;
pub mod registry;

use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

// ─── Public types ─────────────────────────────────────────────────────────────

pub use manifest::{
    ContextSchema, ContextSource, ModelManifest, ParamSchema, StateFieldSchema,
};
pub use provenance::Provenance;

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

/// Output of `apply_dynamics_model`.
#[derive(Debug, Clone, Serialize)]
pub struct SkillOutput {
    /// Keyed by property URI — one Vec<TrajectoryPoint> per state dimension.
    pub trajectories: BTreeMap<String, Vec<TrajectoryPoint>>,
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

    Ok(SkillOutput { trajectories, provenance: prov, converged, notes })
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
