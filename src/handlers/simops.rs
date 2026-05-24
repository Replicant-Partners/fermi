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
};
use projections::{
    project_distribution, ExecutorRegistry, ProjectionRequest,
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

pub async fn cascade_handler(
    _state: State<AppState>,
    _principal: AuthPrincipal,
    Json(req): Json<CascadeRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Validate ProcessConfig (unit compatibility between adjacent stages).
    req.process
        .validate()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid ProcessConfig: {}", e)))?;

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
