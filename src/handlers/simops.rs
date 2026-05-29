//! Direct SimOps computation handlers — no LLM in the loop.
//!
//! `POST /api/simops/cascade` runs the deterministic energy/mass balance
//! cascade from `crates/simops` directly, bypassing the `simops_cascade`
//! LLM agent. Use this from the kask Compose mode for real-time (<1ms)
//! stage-edit feedback. See `docs/specs/01_APP_PRIMITIVE.md` §6.3.
//!
//! `POST /api/simops/project` runs the distributional projection engine:
//! N cascade runs with sampled inputs → distribution summaries per output
//! dimension. Powers the kask Digital Twin "Generate distribution" button.
//! See `docs/specs/16_KASK_HANDOFF_2026-05-24.md` §2.
//!
//! No credits charged — both endpoints are CPU-only and free at the platform level.

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use simops::{
    cascade::{cascade_backward, cascade_forward},
    process::ProcessConfig,
    cascade_v2::cascade_v2,
    process_v2::{CascadeRequestEnvelope, CascadeRequestV2},
};
use projections::{
    project_distribution, ExecutorRegistry, ProjectionRequest,
};
use dynamics::{
    apply_dynamics_model, registry as dynamics_registry, SkillInput as DynamicsInput,
    RheologyInput, resolve_rheology, list_rheology_manifests,
    coupled::{apply_coupled_dynamics_model, CoupledInput, CoupledParamsOverride},
};

use fermi_auth::AuthPrincipal;
use crate::AppState;

// ─── Request / response ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CascadeRequest {
    /// The ProcessConfig to run the cascade on.
    pub process: ProcessConfig,
    /// `"forward"` — propagate input_quantity through all stages.
    /// `"backward"` — back-calculate required input for target_output.
    pub direction: String,
    /// For `forward`: the primary input quantity at stage 0.
    /// For `backward`: the desired final output quantity at the last stage.
    pub quantity: f64,
}

// ─── POST /api/simops/cascade ────────────────────────────────────────────────
//
// Version-dispatching cascade handler.
//
// The request body is inspected for `process.schema_version`:
//   - schema_version == 2 (or absent with `inputs[]` present) → v2 engine
//   - schema_version absent / 1 with `input` singular → v1 engine (legacy)
//   - schema_version == 2 but `direction: backward` → 400 (deferred spec 30.6)
//
// v1 requests that include `schema_version: 1` explicitly are also rejected
// with the spec 30 migration message.

pub async fn cascade_handler(
    _state: State<AppState>,
    _principal: AuthPrincipal,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Probe schema_version without full deserialisation
    let schema_version = body
        .get("process")
        .and_then(|p| p.get("schema_version"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    // Also detect v2 by presence of `inputs` on the first stage
    let has_inputs_array = body
        .get("process")
        .and_then(|p| p.get("stages"))
        .and_then(|s| s.as_array())
        .and_then(|a| a.first())
        .map(|s| s.get("inputs").is_some())
        .unwrap_or(false);

    let is_v2 = schema_version == Some(2) || (schema_version.is_none() && has_inputs_array);

    if is_v2 {
        // ── v2 path ──────────────────────────────────────────────────────────
        let req: CascadeRequestV2 = serde_json::from_value(body)
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid v2 cascade request: {e}")))?;

        let response = cascade_v2(&req).map_err(|e| {
            use simops::cascade_v2::CascadeError;
            let status = if e.status_code() == 422 {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::BAD_REQUEST
            };
            // For structured errors (BasisUnresolved), return JSON body
            let body = serde_json::to_string(&e.to_json()).unwrap_or_else(|_| e.to_string());
            (status, body)
        })?;

        return Ok(Json(json!(response)));
    }

    // ── v1 rejection if schema_version explicitly set to non-2 ───────────────
    if let Some(v) = schema_version {
        if v != 2 {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("ProcessConfig schema_version must be 2 (got: {v}). See kask spec 30."),
            ));
        }
    }

    // ── v1 legacy path (no schema_version, singular input/output) ────────────
    // v1 processes have a singular `input` field on stages, no `inputs[]`.
    // They are still supported for existing integrations but are deprecated.
    // Any v1 process that reaches here was not rejected above (no schema_version
    // and no inputs[] array). Parse directly.
    let req: CascadeRequest = serde_json::from_value(body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid cascade request: {e}. \
            If this is a v2 process (inputs[]/outputs[]), ensure schema_version: 2 is set.")))?;

    // Guard: empty stage list would panic in cascade_forward/backward.
    if req.process.stages.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "ProcessConfig must have at least one stage".into()));
    }

    if req.quantity < 0.0 {
        return Err((StatusCode::BAD_REQUEST, "quantity must be >= 0".into()));
    }

    // cascade_forward / cascade_backward return CascadeResult directly (not Result<>).
    let result = match req.direction.as_str() {
        "forward" => cascade_forward(&req.process, req.quantity),
        "backward" => cascade_backward(&req.process, req.quantity),
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("direction must be 'forward' or 'backward', got '{}'", other),
            ))
        }
    };

    Ok(Json(json!(result)))
}

// ─── POST /api/simops/project ─────────────────────────────────────────────────
//
// Distributional projection: run the SimOps cascade N times with inputs
// sampled from declared distributions. Returns distribution summaries
// (percentiles + histogram) per output dimension.
//
// Powers the kask Digital Twin "Generate distribution" button.
// No LLM in the path. No credits charged.
//
// Request body: ProjectionRequest (see crates/projections/src/types.rs)
// Response:     ProjectionResponse
//
// Example minimal request:
// {
//   "model": {
//     "kind": "simops_cascade",
//     "config": { ...ProcessConfig JSON... }
//   },
//   "sweep": {
//     "kind": "monte_carlo",
//     "variables": [
//       { "path": "/stages/0/efficiency",
//         "distribution": { "type": "normal", "mean": 0.85, "std": 0.04 } }
//     ]
//   },
//   "n_runs": 100,
//   "seed": 42,
//   "output_format": "aggregate"
// }

pub async fn project_handler(
    _state: State<AppState>,
    _principal: AuthPrincipal,
    Json(req): Json<ProjectionRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Clamp n_runs server-side regardless of what the client sends
    if req.n_runs == 0 {
        return Err((StatusCode::BAD_REQUEST, "n_runs must be >= 1".into()));
    }

    // Build the registry with the simops-executor feature active
    let registry = ExecutorRegistry::default();

    // Run the projection — this is synchronous CPU work, no await needed
    let response = project_distribution(&req, &registry)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    Ok(Json(json!(response)))
}

// ─── POST /api/simops/dynamics ────────────────────────────────────────────────
//
// ODE-based dynamics model projection. Runs one of the registered models
// (kombucha_fermentation, pellicle_growth, bc_optimization, linear_decay)
// and returns trajectories for each state dimension.
//
// Powers the kask Digital Twin time-series projection panel.
// No LLM in the path. No credits charged.
//
// Request body: DynamicsInput (SkillInput from crates/dynamics)
// Response:     SkillOutput (trajectories + provenance + notes)
//
// Example:
// {
//   "model_uri": "kask:dynamics/kombucha_fermentation@v1",
//   "initial_state": { "chem:brix_percent": 10.0, "chem:ph_value": 5.0 },
//   "process_context": { "temperature_c": 26.0 },
//   "horizon": { "kind": "fixed", "days": 14 },
//   "sample_cadence": { "hours": 6 }
// }

pub async fn dynamics_handler(
    _state: State<AppState>,
    _principal: AuthPrincipal,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Normalise: `model_uris` (plural array) OR `model_uri` (singular string).
    // Single-model path uses the original SkillInput → apply_dynamics_model (unchanged).
    // Multi-model path uses CoupledInput → apply_coupled_dynamics_model.
    let has_plural = body.get("model_uris")
        .and_then(|v| v.as_array())
        .map(|a| a.len() > 1)
        .unwrap_or(false);

    if has_plural {
        // Multi-model coupled path
        let req: CoupledInput = serde_json::from_value(body)
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid coupled request: {e}")))?;
        let output = apply_coupled_dynamics_model(req)
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        Ok(Json(json!(output)))
    } else {
        // Single-model path — normalise model_uris: ["X"] to model_uri: "X" if needed
        let body = if body.get("model_uri").is_none() {
            // model_uris: ["X"] → extract first as model_uri
            if let Some(uri) = body.get("model_uris")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
            {
                let mut b = body.clone();
                b.as_object_mut().unwrap().insert("model_uri".into(), json!(uri));
                b
            } else {
                body
            }
        } else {
            body
        };

        let req: DynamicsInput = serde_json::from_value(body)
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid dynamics request: {e}")))?;
        let output = apply_dynamics_model(req)
            .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        Ok(Json(json!(output)))
    }
}

// ─── GET /api/simops/dynamics/models ──────────────────────────────────────────
//
// List all registered dynamics model manifests.
// Used by the dynamics_runner agent and kask model-picker UI.

pub async fn dynamics_list_handler(
    _state: State<AppState>,
    _principal: AuthPrincipal,
) -> Json<Value> {
    Json(json!(dynamics_registry::list_manifests()))
}

// ─── POST /api/simops/rheology ────────────────────────────────────────────────
//
// Instantaneous rheology calculation — no time integration.
// Given (temperature, shear_rate, volume_fraction, model_uri), returns
// viscosity, flow index, consistency index.
//
// Powers the kask Twin panel viscosity probe / pump sizing tool.
// No LLM. No credits charged.
//
// Request body: { model_uri, temperature_c, shear_rate_per_s, volume_fraction, params_override? }
// Response:     { viscosity_pa_s, flow_index_n, consistency_index_k, regime, kinematic_mm2_per_s }

pub async fn rheology_handler(
    _state: State<AppState>,
    _principal: AuthPrincipal,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let model_uri = body.get("model_uri")
        .and_then(|v| v.as_str())
        .unwrap_or("kask:rheology/algae_viscosity@v1");

    let model = resolve_rheology(model_uri)
        .ok_or_else(|| (
            StatusCode::BAD_REQUEST,
            format!("Unknown rheology model URI: '{}'. Known: {}",
                model_uri,
                dynamics::known_rheology_uris().join(", "))
        ))?;

    let input: RheologyInput = serde_json::from_value(body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid request: {e}")))?;

    let output = model.compute(&input)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Json(json!(output)))
}

// ─── GET /api/simops/rheology/models ─────────────────────────────────────────

pub async fn rheology_list_handler(
    _state: State<AppState>,
    _principal: AuthPrincipal,
) -> Json<Value> {
    Json(json!(list_rheology_manifests()))
}
