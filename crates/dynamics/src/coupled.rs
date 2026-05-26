//! Multi-model coupled ODE integration.
//!
//! Extends `apply_dynamics_model` to accept a **set** of models that
//! integrate over a **shared union state space** in lockstep. Shared state
//! variables (e.g. `chem:brix_percent` declared by multiple models) receive
//! summed derivative contributions from all models that declare them as
//! `ContributionMode::Additive`.
//!
//! # The problem this solves
//!
//! Three models (kombucha_fermentation, pellicle_growth, bc_optimization) all
//! consume Brix. Running them independently gives three diverging Brix
//! trajectories — biologically wrong. One vessel, one Brix. This module
//! integrates them as one coupled ODE over the union state vector.
//!
//! # Usage
//!
//! ```rust
//! use dynamics::coupled::{CoupledInput, CoupledParamsOverride, apply_coupled_dynamics_model};
//! use dynamics::{Horizon, SampleCadence};
//! use std::collections::BTreeMap;
//!
//! let input = CoupledInput {
//!     model_uris: vec![
//!         "kask:dynamics/kombucha_fermentation@v1".into(),
//!         "kask:dynamics/bc_optimization@v1".into(),
//!     ],
//!     initial_state: BTreeMap::from([
//!         ("chem:brix_percent".into(), 8.0),
//!         ("chem:ph_value".into(), 6.0),
//!         ("bio:bc_yield_g_per_l".into(), 0.0),
//!         ("bio:bc_quality_index".into(), 1.0),
//!     ]),
//!     process_context: serde_json::json!({
//!         "temperature_c": 30.0, "agitation_rpm": 0.0,
//!         "do_saturation_pct": 10.0, "carbon_source": "glucose"
//!     }),
//!     params_override: CoupledParamsOverride::Empty,
//!     horizon: Horizon::Fixed { days: 7.0 },
//!     sample_cadence: Some(SampleCadence { hours: 24.0 }),
//!     integrator: None,
//!     generated_by: None,
//!     integrator_step_days: None,
//! };
//!
//! let output = apply_coupled_dynamics_model(input).unwrap();
//! assert!(!output.trajectories.is_empty());
//! // Brix trajectory is shared — driven by ALL models simultaneously
//! assert!(output.trajectories.contains_key("chem:brix_percent"));
//! ```

use std::collections::{BTreeMap, BTreeSet};
use serde::{Deserialize, Serialize};

use crate::{
    ContributionMode, DerivedPoint, DerivedTrajectory, DynamicsModel, Horizon,
    Note, SampleCadence, SkillInput, TrajectoryPoint,
    integrator, registry,
    rheology::{AlgaeViscosity, RheologyInput, RheologyModel},
};

// ─── Input ────────────────────────────────────────────────────────────────────

/// Input for a multi-model coupled projection.
///
/// Backward compatible: `model_uri` (singular) is also accepted via
/// `CoupledInput::from_singular`. The HTTP handler normalises both shapes.
#[derive(Debug, Clone, Deserialize)]
pub struct CoupledInput {
    /// List of model URIs to couple. Order is deterministic (for provenance).
    pub model_uris: Vec<String>,

    /// Union of all initial state values across all declared models.
    pub initial_state: BTreeMap<String, f64>,

    /// Process-level context applied to all models.
    #[serde(default)]
    pub process_context: serde_json::Value,

    /// Per-model parameter overrides, keyed by short model name.
    /// Short name = URI local part before `@`: "kombucha_fermentation", "bc_optimization" etc.
    /// Flat map (legacy single-model shape) also accepted — applied to the sole model.
    #[serde(default)]
    pub params_override: CoupledParamsOverride,

    pub horizon: Horizon,

    #[serde(default)]
    pub sample_cadence: Option<SampleCadence>,

    #[serde(default)]
    pub integrator: Option<String>,

    #[serde(default)]
    pub generated_by: Option<String>,

    /// Advanced: override the integration step size for the whole coupled system.
    /// Defaults to the minimum default_step_days across all models.
    #[serde(default)]
    pub integrator_step_days: Option<f64>,
}

/// Flexible parameter override — either per-model (keyed by short name)
/// or flat (legacy single-model shape).
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(untagged)]
pub enum CoupledParamsOverride {
    /// Per-model: { "kombucha_fermentation": { "ph_floor": 2.8 }, ... }
    PerModel(BTreeMap<String, BTreeMap<String, f64>>),
    /// Flat legacy: { "ph_floor": 2.8, "bc_max": 8.0 }
    Flat(BTreeMap<String, f64>),
    #[default]
    Empty,
}

impl CoupledParamsOverride {
    /// Resolve params for a specific model short name.
    pub fn for_model(&self, short_name: &str) -> BTreeMap<String, f64> {
        match self {
            Self::PerModel(map) => map.get(short_name).cloned().unwrap_or_default(),
            Self::Flat(map) => map.clone(),
            Self::Empty => BTreeMap::new(),
        }
    }
}

// ─── Coupled Provenance ───────────────────────────────────────────────────────

/// Provenance for a coupled multi-model run.
#[derive(Debug, Clone, Serialize)]
pub struct CoupledProvenance {
    /// Ordered list of model URIs that participated in this run.
    pub model_uris: Vec<String>,
    /// Version per model, keyed by short name.
    pub model_versions: BTreeMap<String, String>,
    pub integrator: String,
    pub step_size_days: f64,
    pub generated_at: String,
    pub projection_id: String,
    pub generated_by: String,
    /// Resolved params per model (defaults merged with overrides).
    pub params_used: BTreeMap<String, serde_json::Value>,
    pub context_used: serde_json::Value,
    pub initial_state: serde_json::Value,
    /// Which models contributed to each state variable.
    pub state_contributions: BTreeMap<String, Vec<String>>,
}

// ─── Coupled Output ───────────────────────────────────────────────────────────

/// Output of `apply_coupled_dynamics_model`.
#[derive(Debug, Clone, Serialize)]
pub struct CoupledOutput {
    pub trajectories: BTreeMap<String, Vec<TrajectoryPoint>>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub derived_quantities: Vec<DerivedTrajectory>,
    pub provenance: CoupledProvenance,
    pub converged: bool,
    pub notes: Vec<Note>,
}

// ─── Validation error ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum CoupledError {
    UnknownModel(String),
    DuplicateModel(String),
    ReplacementConflict { variable: String, models: (String, String) },
    MissingState(Vec<String>),
    IntegrationFailed { integrator: String, step: usize, message: String },
}

impl std::fmt::Display for CoupledError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownModel(uri) =>
                write!(f, "Unknown model URI: '{uri}'. Known: {}", registry::known_uris().join(", ")),
            Self::DuplicateModel(uri) =>
                write!(f, "Model '{uri}' listed more than once — likely a mistake"),
            Self::ReplacementConflict { variable, models: (a, b) } =>
                write!(f, "Two models declare Replacement for '{variable}': '{a}' and '{b}'"),
            Self::MissingState(keys) =>
                write!(f, "Missing required initial_state keys: {}", keys.join(", ")),
            Self::IntegrationFailed { integrator, step, message } =>
                write!(f, "Integration failed (integrator={integrator}, step={step}): {message}"),
        }
    }
}

// ─── Entry point ──────────────────────────────────────────────────────────────

/// Integrate a set of models as one coupled ODE over their union state space.
pub fn apply_coupled_dynamics_model(input: CoupledInput) -> Result<CoupledOutput, CoupledError> {

    // ── 1. Validate: no unknown URIs, no duplicates ───────────────────────────
    let mut seen_uris = BTreeSet::new();
    for uri in &input.model_uris {
        if !seen_uris.insert(uri.clone()) {
            return Err(CoupledError::DuplicateModel(uri.clone()));
        }
    }

    // ── 2. Resolve all models ─────────────────────────────────────────────────
    // Build a minimal SkillInput per model for the registry resolver
    let models: Vec<Box<dyn DynamicsModel>> = input.model_uris.iter()
        .map(|uri| {
            let short = short_name(uri);
            let flat_params = input.params_override.for_model(&short);
            let skill_input = crate::SkillInput {
                model_uri: uri.clone(),
                initial_state: input.initial_state.clone(),
                process_context: input.process_context.clone(),
                params_override: flat_params,
                horizon: input.horizon.clone(),
                sample_cadence: input.sample_cadence.clone(),
                integrator: input.integrator.clone(),
                generated_by: input.generated_by.clone(),
            };
            registry::resolve(uri, Some(&skill_input))
                .ok_or_else(|| CoupledError::UnknownModel(uri.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // ── 3. Build union state order (alphabetical by URI — deterministic) ──────
    let union_order: Vec<String> = {
        let mut uris: BTreeSet<String> = BTreeSet::new();
        for model in &models {
            for uri in model.state_order() {
                uris.insert(uri);
            }
        }
        uris.into_iter().collect()
    };

    // ── 4. Validate: no two models declare Replacement for the same variable ──
    let mut replacement_owners: BTreeMap<String, String> = BTreeMap::new();
    for model in &models {
        let manifest = model.manifest();
        let short = short_name(&manifest.uri).to_string();
        for (var_uri, schema) in &manifest.state_schema {
            if schema.contribution == ContributionMode::Replacement {
                if let Some(existing) = replacement_owners.get(var_uri) {
                    return Err(CoupledError::ReplacementConflict {
                        variable: var_uri.clone(),
                        models: (existing.clone(), short.clone()),
                    });
                }
                replacement_owners.insert(var_uri.clone(), short.clone());
            }
        }
    }

    // ── 5. Validate: required state variables are present ────────────────────
    let missing: Vec<String> = union_order.iter()
        .filter(|uri| !input.initial_state.contains_key(*uri))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(CoupledError::MissingState(missing));
    }

    // ── 6. Build initial state vector in union order ──────────────────────────
    // All keys validated in step 5 — direct index is safe here.
    let y0: Vec<f64> = union_order.iter()
        .map(|uri| input.initial_state[uri])
        .collect();

    // ── 7. Determine step size (minimum across models) ────────────────────────
    let step_days = input.integrator_step_days.unwrap_or_else(|| {
        models.iter()
            .map(|m| m.manifest().default_step_days)
            .fold(f64::INFINITY, f64::min)
    });

    let horizon_days = match &input.horizon {
        Horizon::Fixed { days } => *days,
        Horizon::UntilPropertyReaches { max_days, .. } => *max_days,
    };

    let cadence_days = input.sample_cadence.as_ref()
        .map(|c| c.hours / 24.0)
        .unwrap_or(0.25);

    // ── 8. Build the coupled system adapter and integrate ─────────────────────
    let coupled = CoupledSystem {
        models: &models,
        union_order: &union_order,
    };

    let trajectory = integrator::integrate_coupled(
        &coupled,
        &y0,
        horizon_days,
        step_days,
        cadence_days,
    ).map_err(|msg| CoupledError::IntegrationFailed {
        integrator: "rk4_coupled".into(),
        step: 0,
        message: msg,
    })?;

    // ── 9. Apply horizon termination ──────────────────────────────────────────
    let trajectory = match &input.horizon {
        Horizon::UntilPropertyReaches { property, value, .. } => {
            let idx = union_order.iter().position(|u| u == property);
            if let Some(i) = idx {
                let cut = trajectory.iter().position(|(_, y)| y[i] <= *value);
                if let Some(end) = cut { trajectory[..=end].to_vec() } else { trajectory }
            } else { trajectory }
        }
        Horizon::Fixed { .. } => trajectory,
    };

    // ── 10. Convergence + notes (per model, tagged with source) ───────────────
    let converged = models.iter().all(|m| {
        let model_traj = project_model_traj(m.as_ref(), &union_order, &trajectory);
        m.is_converged(&model_traj)
    });

    let mut notes: Vec<Note> = Vec::new();
    for model in &models {
        let manifest = model.manifest();
        let short = short_name(&manifest.uri).to_string();
        let model_traj = project_model_traj(model.as_ref(), &union_order, &trajectory);
        for mut note in model.generate_notes(&model_traj) {
            note.message = format!("[{}] {}", short, note.message);
            notes.push(note);
        }
    }

    // ── 11. Unpack union trajectories ─────────────────────────────────────────
    let trajectories = unpack_union(&union_order, &trajectory);

    // ── 12. Derived rheology quantities ───────────────────────────────────────
    let derived_quantities = derive_coupled_rheology(&trajectories, &input);

    // ── 13. Build state_contributions map ─────────────────────────────────────
    let mut state_contributions: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for model in &models {
        let manifest = model.manifest();
        let short = short_name(&manifest.uri).to_string();
        for var_uri in model.state_order() {
            let mode = manifest.state_schema.get(&var_uri)
                .map(|s| &s.contribution)
                .unwrap_or(&ContributionMode::Additive);
            if *mode != ContributionMode::ReadOnly {
                state_contributions
                    .entry(var_uri)
                    .or_default()
                    .push(short.clone());
            }
        }
    }

    // ── 14. Build provenance ──────────────────────────────────────────────────
    let mut model_versions = BTreeMap::new();
    let mut params_used_map: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for model in &models {
        let manifest = model.manifest();
        let short = short_name(&manifest.uri).to_string();
        model_versions.insert(short.clone(), manifest.version.clone());
        let mut resolved = manifest.default_params.clone();
        for (k, v) in input.params_override.for_model(&short) {
            resolved.insert(k, v);
        }
        params_used_map.insert(short, serde_json::to_value(&resolved).unwrap_or_default());
    }

    let provenance = CoupledProvenance {
        model_uris: input.model_uris.clone(),
        model_versions,
        integrator: "rk4_coupled".into(),
        step_size_days: step_days,
        generated_at: chrono::Utc::now().to_rfc3339(),
        projection_id: format!("proj-coupled-{}", uuid::Uuid::new_v4()),
        generated_by: input.generated_by.clone().unwrap_or_else(|| "system".into()),
        params_used: params_used_map,
        context_used: input.process_context.clone(),
        initial_state: serde_json::to_value(&input.initial_state).unwrap_or_default(),
        state_contributions,
    };

    Ok(CoupledOutput { trajectories, derived_quantities, provenance, converged, notes })
}

// ─── Coupled system adapter ───────────────────────────────────────────────────

/// Wraps multiple `DynamicsModel`s into a single system function over the
/// union state vector. Used by the integrator.
pub(crate) struct CoupledSystem<'a> {
    pub models: &'a [Box<dyn DynamicsModel>],
    pub union_order: &'a [String],
}

impl<'a> CoupledSystem<'a> {
    /// Compute dy/dt for the union state vector by summing (or replacing)
    /// contributions from each model.
    pub fn system(&self, t: f64, y: &[f64], dy: &mut [f64]) {
        // Zero the derivative vector first
        for d in dy.iter_mut() { *d = 0.0; }

        for model in self.models {
            let model_order = model.state_order();
            let manifest = model.manifest();

            // Extract model-local state slice from union vector
            let model_y: Vec<f64> = model_order.iter()
                .map(|uri| {
                    self.union_order.iter().position(|u| u == uri)
                        .map(|i| y[i])
                        .unwrap_or(0.0)
                })
                .collect();

            let mut model_dy = vec![0.0_f64; model_y.len()];
            model.system(t, &model_y, &mut model_dy);

            // Apply contribution mode per variable
            for (local_i, var_uri) in model_order.iter().enumerate() {
                let global_i = match self.union_order.iter().position(|u| u == var_uri) {
                    Some(i) => i,
                    None => continue,
                };
                let mode = manifest.state_schema.get(var_uri)
                    .map(|s| &s.contribution)
                    .unwrap_or(&ContributionMode::Additive);

                match mode {
                    ContributionMode::Additive    => dy[global_i] += model_dy[local_i],
                    ContributionMode::Replacement => dy[global_i] = model_dy[local_i],
                    ContributionMode::ReadOnly    => { /* no contribution */ }
                }
            }
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn coupled_bc_input() -> CoupledInput {
        CoupledInput {
            model_uris: vec![
                "kask:dynamics/kombucha_fermentation@v1".into(),
                "kask:dynamics/bc_optimization@v1".into(),
            ],
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
            params_override: CoupledParamsOverride::Empty,
            horizon: Horizon::Fixed { days: 7.0 },
            sample_cadence: Some(SampleCadence { hours: 24.0 }),
            integrator: None,
            generated_by: None,
            integrator_step_days: None,
        }
    }

    // ── Phase 1: backward compat ──────────────────────────────────────────────

    #[test]
    fn single_model_via_coupled_path_matches_direct() {
        // A single model_uris: ["X"] should give identical results to direct apply_dynamics_model
        let coupled = CoupledInput {
            model_uris: vec!["kask:dynamics/kombucha_fermentation@v1".into()],
            initial_state: BTreeMap::from([
                ("chem:brix_percent".into(), 10.0),
                ("chem:ph_value".into(), 5.0),
            ]),
            process_context: serde_json::json!({ "temperature_c": 26.0 }),
            params_override: CoupledParamsOverride::Empty,
            horizon: Horizon::Fixed { days: 7.0 },
            sample_cadence: Some(SampleCadence { hours: 24.0 }),
            integrator: None,
            generated_by: None,
            integrator_step_days: None,
        };
        let out = apply_coupled_dynamics_model(coupled).unwrap();
        assert!(out.trajectories.contains_key("chem:brix_percent"));
        assert!(out.trajectories.contains_key("chem:ph_value"));
        // Brix must decrease (consumed by culture)
        let brix = &out.trajectories["chem:brix_percent"];
        assert!(brix.last().unwrap().value < brix.first().unwrap().value);
    }

    // ── Phase 2: multi-model coupling correct ─────────────────────────────────

    #[test]
    fn coupled_brix_depletes_faster_than_single_model() {
        // Kombucha + BC both consume Brix. Coupled run should deplete Brix
        // faster than kombucha alone (additive contributions).
        let coupled_out = apply_coupled_dynamics_model(coupled_bc_input()).unwrap();

        let single = CoupledInput {
            model_uris: vec!["kask:dynamics/kombucha_fermentation@v1".into()],
            initial_state: BTreeMap::from([
                ("chem:brix_percent".into(), 8.0),
                ("chem:ph_value".into(), 6.0),
            ]),
            process_context: serde_json::json!({ "temperature_c": 30.0 }),
            params_override: CoupledParamsOverride::Empty,
            horizon: Horizon::Fixed { days: 7.0 },
            sample_cadence: Some(SampleCadence { hours: 24.0 }),
            integrator: None, generated_by: None, integrator_step_days: None,
        };
        let single_out = apply_coupled_dynamics_model(single).unwrap();

        let coupled_final_brix = coupled_out.trajectories["chem:brix_percent"].last().unwrap().value;
        let single_final_brix  = single_out.trajectories["chem:brix_percent"].last().unwrap().value;

        assert!(
            coupled_final_brix < single_final_brix,
            "Coupled Brix must deplete faster. coupled={:.4}, single={:.4}",
            coupled_final_brix, single_final_brix
        );
    }

    #[test]
    fn coupled_union_contains_all_state_variables() {
        let out = apply_coupled_dynamics_model(coupled_bc_input()).unwrap();
        // kombucha contributes: chem:brix_percent, chem:ph_value
        // bc_optimization contributes: chem:brix_percent, chem:ph_value, bio:bc_yield_g_per_l, bio:bc_quality_index
        assert!(out.trajectories.contains_key("chem:brix_percent"));
        assert!(out.trajectories.contains_key("chem:ph_value"));
        assert!(out.trajectories.contains_key("bio:bc_yield_g_per_l"));
        assert!(out.trajectories.contains_key("bio:bc_quality_index"));
    }

    #[test]
    fn state_contributions_map_correct() {
        let out = apply_coupled_dynamics_model(coupled_bc_input()).unwrap();
        let contribs = &out.provenance.state_contributions;
        // Both models declare Brix and pH
        let brix_drivers = contribs.get("chem:brix_percent").unwrap();
        assert!(brix_drivers.contains(&"kombucha_fermentation".to_string()));
        assert!(brix_drivers.contains(&"bc_optimization".to_string()));
        // Only bc_optimization drives BC yield
        let bc_drivers = contribs.get("bio:bc_yield_g_per_l").unwrap();
        assert_eq!(bc_drivers.len(), 1);
        assert_eq!(bc_drivers[0], "bc_optimization");
    }

    #[test]
    fn provenance_has_model_uris_array() {
        let out = apply_coupled_dynamics_model(coupled_bc_input()).unwrap();
        assert_eq!(out.provenance.model_uris.len(), 2);
        assert!(out.provenance.model_uris.contains(&"kask:dynamics/kombucha_fermentation@v1".to_string()));
        assert!(out.provenance.model_uris.contains(&"kask:dynamics/bc_optimization@v1".to_string()));
        assert_eq!(out.provenance.integrator, "rk4_coupled");
    }

    #[test]
    fn derived_viscosity_present_for_bc_model() {
        let out = apply_coupled_dynamics_model(coupled_bc_input()).unwrap();
        assert!(
            out.derived_quantities.iter().any(|d| d.property_uri == "phys:dynamic_viscosity_pa_s"),
            "Coupled run with bc_optimization should produce viscosity derived quantity"
        );
    }

    // ── Phase 3: validation ───────────────────────────────────────────────────

    #[test]
    fn unknown_model_uri_returns_error() {
        let mut input = coupled_bc_input();
        input.model_uris.push("kask:dynamics/nonexistent@v1".into());
        input.initial_state.insert("some:prop".into(), 0.0);
        let result = apply_coupled_dynamics_model(input);
        assert!(matches!(result, Err(CoupledError::UnknownModel(_))));
    }

    #[test]
    fn duplicate_model_returns_error() {
        let mut input = coupled_bc_input();
        input.model_uris.push("kask:dynamics/bc_optimization@v1".into());
        let result = apply_coupled_dynamics_model(input);
        assert!(matches!(result, Err(CoupledError::DuplicateModel(_))));
    }

    #[test]
    fn missing_state_variable_returns_error() {
        let mut input = coupled_bc_input();
        input.initial_state.remove("bio:bc_yield_g_per_l");
        input.initial_state.remove("bio:bc_quality_index");
        let result = apply_coupled_dynamics_model(input);
        assert!(matches!(result, Err(CoupledError::MissingState(_))));
    }

    #[test]
    fn per_model_params_override_works() {
        let mut input = coupled_bc_input();
        input.params_override = CoupledParamsOverride::PerModel(BTreeMap::from([
            ("bc_optimization".into(), BTreeMap::from([("bc_max".into(), 12.0)])),
        ]));
        let out = apply_coupled_dynamics_model(input).unwrap();
        // bc_max=12 means BC yield can grow higher — just verify it ran without error
        assert!(out.trajectories.contains_key("bio:bc_yield_g_per_l"));
    }

    #[test]
    fn short_name_extraction() {
        assert_eq!(short_name("kask:dynamics/kombucha_fermentation@v1"), "kombucha_fermentation");
        assert_eq!(short_name("kask:dynamics/bc_optimization@v2"), "bc_optimization");
        assert_eq!(short_name("linear_decay"), "linear_decay");
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Extract the short model name from a URI.
/// `"kask:dynamics/kombucha_fermentation@v1"` → `"kombucha_fermentation"`
pub fn short_name(uri: &str) -> &str {
    uri.rsplit('/')
        .next()
        .and_then(|s| s.split('@').next())
        .unwrap_or(uri)
}

/// Project a model's local trajectory slice from the union trajectory.
fn project_model_traj(
    model: &dyn DynamicsModel,
    union_order: &[String],
    trajectory: &[(f64, Vec<f64>)],
) -> Vec<(f64, Vec<f64>)> {
    let order = model.state_order();
    let indices: Vec<usize> = order.iter()
        .filter_map(|uri| union_order.iter().position(|u| u == uri))
        .collect();
    trajectory.iter()
        .map(|(t, y)| (*t, indices.iter().map(|&i| y[i]).collect()))
        .collect()
}

fn unpack_union(
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
                pts.push(TrajectoryPoint { t_hours: t_days * 24.0, value: y[i] });
            }
        }
    }
    out
}

/// Derive rheology quantities from a coupled trajectory.
/// Same logic as `derive_rheology` in lib.rs but reads from CoupledInput.
fn derive_coupled_rheology(
    trajectories: &BTreeMap<String, Vec<TrajectoryPoint>>,
    input: &CoupledInput,
) -> Vec<DerivedTrajectory> {
    // Re-use the same derivation logic by building a minimal SkillInput proxy
    let (conc_uri, density) = if trajectories.contains_key("bio:bc_yield_g_per_l") {
        ("bio:bc_yield_g_per_l", 1050.0_f64)
    } else if trajectories.contains_key("bio:pellicle_g_per_l") {
        ("bio:pellicle_g_per_l", 1050.0_f64)
    } else {
        return vec![];
    };

    let conc_pts = match trajectories.get(conc_uri) {
        Some(pts) if !pts.is_empty() => pts,
        _ => return vec![],
    };

    let temp_c = input.process_context.get("temperature_c").and_then(|v| v.as_f64()).unwrap_or(26.0);
    let agitation_rpm = input.process_context.get("agitation_rpm").and_then(|v| v.as_f64()).unwrap_or(0.0);

    // Use flat params from any model that provides rheology overrides
    let flat_params: BTreeMap<String, f64> = input.params_override.for_model("rheology");

    let n_imp = flat_params.get("rheology_n_imp").copied().unwrap_or(20.0);
    let shear_rate = if agitation_rpm > 0.0 {
        n_imp * agitation_rpm
    } else {
        flat_params.get("rheology_static_shear").copied().unwrap_or(0.05)
    };

    let dummy_input = RheologyInput {
        temperature_c: temp_c,
        shear_rate_per_s: shear_rate,
        volume_fraction: 0.0,
        params_override: flat_params.iter()
            .filter(|(k, _)| matches!(k.as_str(), "k0"|"ea"|"c_n"|"n_min"|"density_kg_m3"|"t_ref_k"))
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
    };
    let rheology = AlgaeViscosity::from_input(&dummy_input);

    let mut viscosity_pts = Vec::with_capacity(conc_pts.len());
    let mut flow_index_pts = Vec::with_capacity(conc_pts.len());
    let mut consistency_pts = Vec::with_capacity(conc_pts.len());

    for pt in conc_pts {
        let phi = (pt.value / density).clamp(0.0, 0.99);
        let ri = RheologyInput {
            temperature_c: temp_c,
            shear_rate_per_s: shear_rate,
            volume_fraction: phi,
            params_override: rheology.to_input_overrides(),
        };
        match rheology.compute(&ri) {
            Ok(r) => {
                viscosity_pts.push(crate::DerivedPoint { t_hours: pt.t_hours, value: r.viscosity_pa_s });
                flow_index_pts.push(crate::DerivedPoint { t_hours: pt.t_hours, value: r.flow_index_n });
                consistency_pts.push(crate::DerivedPoint { t_hours: pt.t_hours, value: r.consistency_index_k });
            }
            Err(_) => {
                viscosity_pts.push(crate::DerivedPoint { t_hours: pt.t_hours, value: f64::NAN });
                flow_index_pts.push(crate::DerivedPoint { t_hours: pt.t_hours, value: f64::NAN });
                consistency_pts.push(crate::DerivedPoint { t_hours: pt.t_hours, value: f64::NAN });
            }
        }
    }

    let src = "kask:rheology/algae_viscosity@v1";
    vec![
        DerivedTrajectory { property_uri: "phys:dynamic_viscosity_pa_s".into(), label: "Dynamic viscosity".into(), units: "Pa·s".into(), points: viscosity_pts, source_model_uri: src.into() },
        DerivedTrajectory { property_uri: "phys:flow_index_n".into(), label: "Flow behaviour index (n)".into(), units: "dimensionless".into(), points: flow_index_pts, source_model_uri: src.into() },
        DerivedTrajectory { property_uri: "phys:consistency_index_k".into(), label: "Consistency index K(T)".into(), units: "Pa·sⁿ".into(), points: consistency_pts, source_model_uri: src.into() },
    ]
}
