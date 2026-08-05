//! BayesOps HTTP surface (Spec 14 §5.6).
//!
//! Domain-neutral parameter fitting endpoints under `/api/bayesops/*`. Backed
//! by `crates/posterior` (Phase 1, marginal fits) and `crates/posterior-reg`
//! (Phase 2, conditional fits with HMC).
//!
//! ## Endpoints
//!
//! | Method | Path | Purpose |
//! |---|---|---|
//! | POST | `/api/bayesops/fit_marginal` | Fit a marginal `FittedDistribution` from observations |
//! | POST | `/api/bayesops/fit_conditional` | Fit a conditional posterior; returns a `posterior_id` |
//! | POST | `/api/bayesops/predict` | Predictive distribution at new features (use case A) |
//! | POST | `/api/bayesops/input_sensitivity` | Sobol indices over the posterior predictive (use case B) |
//! | POST | `/api/bayesops/compare_scenarios` | Two-scenario comparison (use case C) |
//! | POST | `/api/bayesops/prob_exceeds` | `P(outcome ≥ threshold | features)` (use case D) |
//! | POST | `/api/bayesops/optimise_for_target` | Recommend free_feature to maximise prob_exceeds (use case D) |
//! | GET  | `/api/bayesops/posteriors` | List cached posterior IDs (debug / introspection) |
//! | DELETE | `/api/bayesops/posteriors/:id` | Evict a cached posterior |
//!
//! ## Persistence
//!
//! Posteriors are held in `AppState::posterior_cache` (DashMap keyed by Uuid).
//! Session-scoped — lost on restart. Persistent posterior store is Phase 5.
//!
//! ## Domain-neutrality
//!
//! These handlers do NOT know about SimOps, forecasts, evidence, or any other
//! domain. They take generic `WeightedSample`s (string-keyed feature maps) and
//! return generic `FittedDistribution`s. Domain-specific callers (the agent
//! runtime, the cockpit, the FPL composer, etc.) translate to/from
//! `WeightedSample` themselves.

use std::collections::HashMap;

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use posterior::{fit_marginal, DistFamily, FitMetadata, FittedDistribution};
use posterior_reg::{
    fit_conditional, ConditionalPosterior, RegressionConfig, SamplerDiagnostics, WeightedSample,
};

use crate::AppState;

// ═════════════════════════════════════════════════════════════════════════════
// REQUEST / RESPONSE TYPES
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct FitMarginalRequest {
    pub observations: Vec<f64>,
    #[serde(default)]
    pub weights: Option<Vec<f64>>,
    /// `"beta"`, `"normal"`, `"lognormal"`, `"triangular"`, or `"auto"`.
    /// Defaults to `"auto"`.
    #[serde(default = "default_family")]
    pub family: DistFamily,
    /// Optional human-readable description stored in the result metadata.
    #[serde(default)]
    pub source_description: Option<String>,
}

fn default_family() -> DistFamily {
    DistFamily::Auto
}

#[derive(Debug, Serialize)]
pub struct FitMarginalResponse {
    pub fitted: FittedDistribution,
    pub metadata: FitMetadata,
    /// Direct FPL Driver syntax string (e.g. `"beta(9.4000, 13.6000)"`).
    pub fpl_params: String,
}

#[derive(Debug, Deserialize)]
pub struct FitConditionalRequest {
    pub data: Vec<WeightedSample>,
    pub config: RegressionConfig,
}

#[derive(Debug, Serialize)]
pub struct FitConditionalResponse {
    pub posterior_id: Uuid,
    pub model_name: String,
    pub param_names: Vec<String>,
    pub feature_names: Vec<String>,
    pub diagnostics: SamplerDiagnostics,
    pub nlpd: Option<f64>,
    pub metadata: FitMetadata,
    pub n_samples: usize,
}

#[derive(Debug, Deserialize)]
pub struct PredictRequest {
    pub posterior_id: Uuid,
    pub features: HashMap<String, f64>,
}

#[derive(Debug, Serialize)]
pub struct PredictResponse {
    pub fitted: FittedDistribution,
    pub fpl_params: String,
}

#[derive(Debug, Deserialize)]
pub struct InputSensitivityRequest {
    pub posterior_id: Uuid,
    /// `feature_name -> (lo, hi)` for the analysis grid.
    pub feature_ranges: HashMap<String, (f64, f64)>,
    #[serde(default = "default_n_samples")]
    pub n_samples: usize,
}

fn default_n_samples() -> usize {
    256
}

#[derive(Debug, Deserialize)]
pub struct CompareScenariosRequest {
    pub posterior_id: Uuid,
    pub a: HashMap<String, f64>,
    pub b: HashMap<String, f64>,
}

#[derive(Debug, Deserialize)]
pub struct ProbExceedsRequest {
    pub posterior_id: Uuid,
    pub features: HashMap<String, f64>,
    pub threshold: f64,
}

#[derive(Debug, Serialize)]
pub struct ProbExceedsResponse {
    pub probability: f64,
}

#[derive(Debug, Deserialize)]
pub struct OptimiseForTargetRequest {
    pub posterior_id: Uuid,
    pub fixed_features: HashMap<String, f64>,
    pub free_feature: String,
    pub search_range: (f64, f64),
    pub target_threshold: f64,
}

// ═════════════════════════════════════════════════════════════════════════════
// HANDLERS
// ═════════════════════════════════════════════════════════════════════════════

/// `POST /api/bayesops/fit_marginal`
pub async fn fit_marginal_handler(
    State(state): State<AppState>,
    Json(req): Json<FitMarginalRequest>,
) -> Result<Json<FitMarginalResponse>, (StatusCode, Json<Value>)> {
    let _ = state; // unused: marginal fits are stateless

    let weights_slice = req.weights.as_deref();
    let (fitted, mut metadata) =
        fit_marginal(&req.observations, weights_slice, req.family).map_err(bad_request)?;
    if let Some(desc) = req.source_description {
        metadata.source_description = desc;
    }
    let fpl_params = fitted.to_fpl_params();
    Ok(Json(FitMarginalResponse {
        fitted,
        metadata,
        fpl_params,
    }))
}

/// `POST /api/bayesops/fit_conditional`
pub async fn fit_conditional_handler(
    State(state): State<AppState>,
    Json(req): Json<FitConditionalRequest>,
) -> Result<Json<FitConditionalResponse>, (StatusCode, Json<Value>)> {
    let posterior = fit_conditional(&req.data, &req.config)
        .await
        .map_err(bad_request)?;

    let id = Uuid::new_v4();
    let response = FitConditionalResponse {
        posterior_id: id,
        model_name: posterior.model_name.clone(),
        param_names: posterior.param_names.clone(),
        feature_names: posterior.feature_names.clone(),
        diagnostics: posterior.diagnostics.clone(),
        nlpd: posterior.nlpd,
        metadata: posterior.metadata.clone(),
        n_samples: posterior.n_samples(),
    };

    state.posterior_cache.insert(id, posterior);
    Ok(Json(response))
}

/// `POST /api/bayesops/predict`
pub async fn predict_handler(
    State(state): State<AppState>,
    Json(req): Json<PredictRequest>,
) -> Result<Json<PredictResponse>, (StatusCode, Json<Value>)> {
    let posterior = state
        .posterior_cache
        .get(&req.posterior_id)
        .ok_or_else(|| posterior_not_found(req.posterior_id))?;
    let fitted = posterior.predict(&req.features).map_err(bad_request)?;
    let fpl_params = fitted.to_fpl_params();
    Ok(Json(PredictResponse { fitted, fpl_params }))
}

/// `POST /api/bayesops/input_sensitivity`
pub async fn input_sensitivity_handler(
    State(state): State<AppState>,
    Json(req): Json<InputSensitivityRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let posterior = state
        .posterior_cache
        .get(&req.posterior_id)
        .ok_or_else(|| posterior_not_found(req.posterior_id))?;
    let result = posterior
        .input_sensitivity(&req.feature_ranges, req.n_samples)
        .map_err(bad_request)?;
    Ok(Json(json!({ "sensitivity": result })))
}

/// `POST /api/bayesops/compare_scenarios`
pub async fn compare_scenarios_handler(
    State(state): State<AppState>,
    Json(req): Json<CompareScenariosRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let posterior = state
        .posterior_cache
        .get(&req.posterior_id)
        .ok_or_else(|| posterior_not_found(req.posterior_id))?;
    let comp = posterior
        .compare_scenarios(&req.a, &req.b)
        .map_err(bad_request)?;
    Ok(Json(json!(comp)))
}

/// `POST /api/bayesops/prob_exceeds`
pub async fn prob_exceeds_handler(
    State(state): State<AppState>,
    Json(req): Json<ProbExceedsRequest>,
) -> Result<Json<ProbExceedsResponse>, (StatusCode, Json<Value>)> {
    let posterior = state
        .posterior_cache
        .get(&req.posterior_id)
        .ok_or_else(|| posterior_not_found(req.posterior_id))?;
    let probability = posterior
        .prob_exceeds(&req.features, req.threshold)
        .map_err(bad_request)?;
    Ok(Json(ProbExceedsResponse { probability }))
}

/// `POST /api/bayesops/optimise_for_target`
pub async fn optimise_for_target_handler(
    State(state): State<AppState>,
    Json(req): Json<OptimiseForTargetRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let posterior = state
        .posterior_cache
        .get(&req.posterior_id)
        .ok_or_else(|| posterior_not_found(req.posterior_id))?;
    let result = posterior
        .optimise_for_target(
            &req.fixed_features,
            &req.free_feature,
            req.search_range,
            req.target_threshold,
        )
        .map_err(bad_request)?;
    Ok(Json(json!(result)))
}

/// `GET /api/bayesops/posteriors` — list cached posterior IDs (introspection).
pub async fn list_posteriors_handler(State(state): State<AppState>) -> Json<Value> {
    let ids: Vec<Value> = state
        .posterior_cache
        .iter()
        .map(|entry| {
            json!({
                "posterior_id": entry.key(),
                "model_name": entry.value().model_name,
                "feature_names": entry.value().feature_names,
                "fitted_at": entry.value().metadata.fitted_at,
                "n_samples": entry.value().n_samples(),
                "nlpd": entry.value().nlpd,
            })
        })
        .collect();
    Json(json!({ "posteriors": ids, "count": ids.len() }))
}

/// `DELETE /api/bayesops/posteriors/:id` — evict.
pub async fn evict_posterior_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if state.posterior_cache.remove(&id).is_some() {
        Ok(Json(json!({ "evicted": id })))
    } else {
        Err(posterior_not_found(id))
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// ERROR HELPERS
// ═════════════════════════════════════════════════════════════════════════════

fn bad_request<E: std::fmt::Display>(e: E) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": e.to_string() })),
    )
}

fn posterior_not_found(id: Uuid) -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": format!("posterior_id {} not found in cache", id),
            "hint": "POST /api/bayesops/fit_conditional first; posteriors are session-scoped"
        })),
    )
}

// Helper used by ConditionalPosterior internals — re-export so callers
// keying off this shape don't have to import posterior_reg directly.
// (Used to surface `prob_exceeds` return shape in OpenAPI clients.)
// Intentionally empty; serde tags do the work.

// ═════════════════════════════════════════════════════════════════════════════
// R-2: Sparkline UX endpoints (Spec 23 §4.3)
//
// These power the console's forecast-editor sparkline affordances:
//   GET  /api/workspaces/:id/bayesops/state   — per-driver pending + snapshot
//   POST /api/bayesops/pending/:id/accept     — write params, mark accepted
//   POST /api/bayesops/pending/:id/reject     — mark rejected, no params write
//
// State is the single round-trip the editor needs to render every sparkline
// in a forecast. Accept/reject are inline-button targets.
// ═════════════════════════════════════════════════════════════════════════════

use sqlx::Row as _;

#[derive(Debug, Serialize)]
pub struct WorkspaceBayesopsState {
    pub workspace_id: Uuid,
    pub drivers: Vec<DriverState>,
}

#[derive(Debug, Serialize)]
pub struct DriverState {
    pub driver_name: String,
    /// Most-recent snapshot for this driver, if any. Empty for drivers that
    /// have never been fit (cold start).
    pub latest_snapshot: Option<SnapshotSummary>,
    /// Currently-pending fit, if any. At most one per (workspace, driver)
    /// per the EXCLUDE constraint on bayesops_pending_fits.
    pub pending_fit: Option<PendingFit>,
}

#[derive(Debug, Serialize)]
pub struct SnapshotSummary {
    pub snapshot_id: Uuid,
    pub fitted: Value,
    pub n_observations: i32,
    pub n_eff: f64,
    pub ci_width: f64,
    pub quality: String,
    pub rate_before: Option<f64>,
    pub rate_after: Option<f64>,
    pub decision: String,
    pub fitted_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct PendingFit {
    pub pending_id: Uuid,
    pub snapshot_id: Uuid,
    pub fitted: Value,
    pub n_observations: i32,
    pub n_eff: f64,
    pub ci_width: f64,
    pub quality: String,
    pub rate_before: Option<f64>,
    pub rate_after: Option<f64>,
    pub delta_pp: Option<f64>,
    pub staged_at: chrono::DateTime<chrono::Utc>,
}

/// GET /api/workspaces/:workspace_id/bayesops/state
///
/// Single round-trip for the editor: returns per-driver state for every
/// learnable driver that has either a snapshot or a pending fit (or both).
/// Drivers that have never been fit don't appear here — the editor uses the
/// FPL declaration plus the local executor's `learnable_drivers` log to
/// render their pre-fit baseline.
///
/// No auth — read-only against tables the resolution + refit hooks already
/// gate on workspace membership at write time.
pub async fn workspace_bayesops_state_handler(
    State(state): State<AppState>,
    axum::extract::Path(workspace_id): axum::extract::Path<String>,
) -> Result<Json<WorkspaceBayesopsState>, (StatusCode, Json<Value>)> {
    let ws_uuid: Uuid = workspace_id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid workspace_id" })),
        )
    })?;

    // Pull the latest snapshot per driver (one row per driver via DISTINCT ON).
    let snapshot_rows = sqlx::query(
        "SELECT DISTINCT ON (driver_name)
            driver_name, snapshot_id, fitted, n_observations,
            n_eff, ci_width, quality, rate_before, rate_after,
            decision, fitted_at
         FROM bayesops_posterior_snapshots
         WHERE workspace_id = $1
         ORDER BY driver_name, fitted_at DESC",
    )
    .bind(ws_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(internal_err)?;

    // Pull every pending fit (still 'pending' status). EXCLUDE constraint
    // guarantees at most one per (workspace, driver).
    let pending_rows = sqlx::query(
        "SELECT
            pf.pending_id, pf.snapshot_id, pf.driver_name, pf.staged_at,
            s.fitted, s.n_observations, s.n_eff, s.ci_width, s.quality,
            s.rate_before, s.rate_after
         FROM bayesops_pending_fits pf
         JOIN bayesops_posterior_snapshots s ON s.snapshot_id = pf.snapshot_id
         WHERE pf.workspace_id = $1 AND pf.status = 'pending'
         ORDER BY pf.driver_name",
    )
    .bind(ws_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(internal_err)?;

    // Index by driver_name, then merge
    let mut by_driver: HashMap<String, DriverState> = HashMap::new();
    for r in &snapshot_rows {
        let driver_name: String = r.get("driver_name");
        by_driver.insert(
            driver_name.clone(),
            DriverState {
                driver_name,
                latest_snapshot: Some(SnapshotSummary {
                    snapshot_id: r.get("snapshot_id"),
                    fitted: r.get("fitted"),
                    n_observations: r.get("n_observations"),
                    n_eff: r.get("n_eff"),
                    ci_width: r.get("ci_width"),
                    quality: r.get("quality"),
                    rate_before: r.get("rate_before"),
                    rate_after: r.get("rate_after"),
                    decision: r.get("decision"),
                    fitted_at: r.get("fitted_at"),
                }),
                pending_fit: None,
            },
        );
    }
    for r in &pending_rows {
        let driver_name: String = r.get("driver_name");
        let rate_before: Option<f64> = r.get("rate_before");
        let rate_after: Option<f64> = r.get("rate_after");
        let delta_pp = match (rate_before, rate_after) {
            (Some(b), Some(a)) => Some((a - b).abs() * 100.0),
            _ => None,
        };
        let pending = PendingFit {
            pending_id: r.get("pending_id"),
            snapshot_id: r.get("snapshot_id"),
            fitted: r.get("fitted"),
            n_observations: r.get("n_observations"),
            n_eff: r.get("n_eff"),
            ci_width: r.get("ci_width"),
            quality: r.get("quality"),
            rate_before,
            rate_after,
            delta_pp,
            staged_at: r.get("staged_at"),
        };
        by_driver
            .entry(driver_name.clone())
            .and_modify(|d| d.pending_fit = Some(pending.clone()))
            .or_insert_with(|| DriverState {
                driver_name,
                latest_snapshot: None,
                pending_fit: Some(pending),
            });
    }

    let mut drivers: Vec<DriverState> = by_driver.into_values().collect();
    drivers.sort_by(|a, b| a.driver_name.cmp(&b.driver_name));

    Ok(Json(WorkspaceBayesopsState {
        workspace_id: ws_uuid,
        drivers,
    }))
}

// PendingFit is cloned during the merge above — derive Clone.
impl Clone for PendingFit {
    fn clone(&self) -> Self {
        Self {
            pending_id: self.pending_id,
            snapshot_id: self.snapshot_id,
            fitted: self.fitted.clone(),
            n_observations: self.n_observations,
            n_eff: self.n_eff,
            ci_width: self.ci_width,
            quality: self.quality.clone(),
            rate_before: self.rate_before,
            rate_after: self.rate_after,
            delta_pp: self.delta_pp,
            staged_at: self.staged_at,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct DecisionRequest {
    /// Optional free-form notes from the operator.
    #[serde(default)]
    pub notes: Option<String>,
}

/// POST /api/bayesops/pending/:pending_id/accept
///
/// Mark a pending fit accepted, write the params, post an evidence event.
/// Idempotent on already-accepted rows (returns 200 with status='already_accepted').
pub async fn accept_pending_handler(
    State(state): State<AppState>,
    axum::extract::Path(pending_id): axum::extract::Path<String>,
    principal: fermi_auth::AuthPrincipal,
    Json(req): Json<DecisionRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pending_uuid: Uuid = pending_id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid pending_id" })),
        )
    })?;
    let user_id = principal.user_id();

    // Pull the pending row + snapshot in one query for context
    let row = sqlx::query(
        "SELECT pf.workspace_id, pf.driver_name, pf.status,
                s.fitted, s.snapshot_id,
                s.rate_before, s.rate_after, s.n_observations
         FROM bayesops_pending_fits pf
         JOIN bayesops_posterior_snapshots s ON s.snapshot_id = pf.snapshot_id
         WHERE pf.pending_id = $1",
    )
    .bind(pending_uuid)
    .fetch_optional(&state.db)
    .await
    .map_err(internal_err)?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "pending fit not found" })),
        )
    })?;

    let status: String = row.get("status");
    if status == "accepted" {
        return Ok(Json(json!({
            "status": "already_accepted",
            "pending_id": pending_uuid
        })));
    }
    if status != "pending" {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!("pending fit is in '{}' state; only 'pending' can be accepted", status)
            })),
        ));
    }

    let workspace_id: Uuid = row.get("workspace_id");
    let driver_name: String = row.get("driver_name");
    let fitted_json: Value = row.get("fitted");
    let snapshot_id: Uuid = row.get("snapshot_id");
    let rate_before: Option<f64> = row.get("rate_before");
    let rate_after: Option<f64> = row.get("rate_after");
    let n_observations: i32 = row.get("n_observations");

    // Membership check on the workspace
    fermi_auth::teams::get_member_role(&state.db, workspace_id, &user_id)
        .await
        .map_err(|_| {
            (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "not a workspace member" })),
            )
        })?
        .ok_or((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "not a workspace member" })),
        ))?;

    // Read current params, merge in the fit, write back. Same pattern as
    // the refit hook's auto-accept path.
    let current_params: Value = sqlx::query(
        "SELECT value FROM workspace_outputs WHERE workspace_id = $1 AND key = 'params'",
    )
    .bind(workspace_id)
    .fetch_optional(&state.db)
    .await
    .map_err(internal_err)?
    .map(|r| r.get::<Value, _>("value"))
    .unwrap_or(Value::Object(serde_json::Map::new()));

    let mut merged = current_params.as_object().cloned().unwrap_or_default();
    merged.insert(format!("{}_fitted", driver_name), fitted_json.clone());
    let merged_value = Value::Object(merged);

    sqlx::query(
        "INSERT INTO workspace_outputs
            (workspace_id, key, value, version, updated_at, updated_by)
         VALUES ($1, 'params', $2, 1, NOW(), $3)
         ON CONFLICT (workspace_id, key) DO UPDATE SET
            value = EXCLUDED.value,
            version = workspace_outputs.version + 1,
            updated_at = NOW(),
            updated_by = EXCLUDED.updated_by",
    )
    .bind(workspace_id)
    .bind(&merged_value)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(internal_err)?;

    // Mark the pending row as accepted
    sqlx::query(
        "UPDATE bayesops_pending_fits
         SET status='accepted', decided_at=NOW(), decided_by=$2, decision_notes=$3
         WHERE pending_id=$1",
    )
    .bind(pending_uuid)
    .bind(&user_id)
    .bind(req.notes.as_deref())
    .execute(&state.db)
    .await
    .map_err(internal_err)?;

    // Spec 23 R-3 Piece 1: write a fermi_forecast_updates row so the
    // forecast_spacetime trigger surfaces this acceptance in the timeline.
    // We look up the linked forecast by workspace_id; if there isn't one
    // (rare — pending fits are created by the refit hook which already
    // skips workspaces with no forecast), we log and continue.
    if let Ok(Some(forecast_row)) = sqlx::query(
        "SELECT id, predicted_probability
         FROM fermi_forecasts
         WHERE workspace_id = $1
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(workspace_id)
    .fetch_optional(&state.db)
    .await
    {
        let forecast_id: String = forecast_row.get("id");
        let current_prob: f32 = forecast_row.try_get("predicted_probability").unwrap_or(0.5);
        let prev = current_prob as f64;
        let new_p = rate_after.unwrap_or(prev);

        if (new_p - prev).abs() >= 1e-4 {
            let update_id = Uuid::new_v4().to_string();
            let reason = format!(
                "BayesOps refit accepted (operator review): driver '{}' fitted from {} observations",
                driver_name, n_observations
            );
            let evidence_added = json!({
                "kind": "bayesops_refit",
                "decision": "operator_accepted",
                "pending_id": pending_uuid,
                "driver_name": driver_name,
                "snapshot_id": snapshot_id,
                "rate_before": rate_before,
                "rate_after": rate_after,
                "n_observations": n_observations,
                "decided_by": user_id,
            });
            // Spec 26 §4.1: a refit ACCEPT is an operator decision, not a
            // systemic event — someone chose to take this fit. Attribute
            // it so the team feed shows "Alice accepted a BayesOps refit"
            // rather than an unowned jump in the number.
            let _ = sqlx::query(
                "INSERT INTO fermi_forecast_updates
                    (id, forecast_id, previous_probability, new_probability,
                     reason, agent_id, evidence_added, actor_user_id,
                     revision_trigger, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'bayesops_refit', NOW())",
            )
            .bind(&update_id)
            .bind(&forecast_id)
            .bind(prev as f32)
            .bind(new_p as f32)
            .bind(&reason)
            .bind(Option::<String>::None)
            .bind(&evidence_added)
            .bind(&user_id)
            .execute(&state.db)
            .await;

            let _ = sqlx::query(
                "UPDATE fermi_forecasts
                 SET predicted_probability = $2, updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(&forecast_id)
            .bind(new_p as f32)
            .execute(&state.db)
            .await;
        }
    }

    // Post evidence event
    let _ = sqlx::query(
        "INSERT INTO workspace_messages
            (workspace_id, sender_type, sender_id, sender_name, content,
             message_type, metadata)
         VALUES ($1, 'system', 'bayesops', 'BayesOps', $2, 'system_event', $3)",
    )
    .bind(workspace_id)
    .bind(format!(
        "✓ Fit accepted for driver '{}' by {}.",
        driver_name, user_id
    ))
    .bind(json!({
        "event": "bayesops_fit_decision",
        "decision": "accepted",
        "pending_id": pending_uuid,
        "snapshot_id": snapshot_id,
        "driver_name": driver_name,
        "decided_by": user_id,
        "notes": req.notes,
    }))
    .execute(&state.db)
    .await;

    Ok(Json(json!({
        "status": "accepted",
        "pending_id": pending_uuid,
        "workspace_id": workspace_id,
        "driver_name": driver_name,
    })))
}

/// POST /api/bayesops/pending/:pending_id/reject
///
/// Mark a pending fit rejected. No params write. Posts an evidence event so
/// the rejection is visible in the workspace history.
pub async fn reject_pending_handler(
    State(state): State<AppState>,
    axum::extract::Path(pending_id): axum::extract::Path<String>,
    principal: fermi_auth::AuthPrincipal,
    Json(req): Json<DecisionRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pending_uuid: Uuid = pending_id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid pending_id" })),
        )
    })?;
    let user_id = principal.user_id();

    let row = sqlx::query(
        "SELECT workspace_id, driver_name, status, snapshot_id
         FROM bayesops_pending_fits WHERE pending_id = $1",
    )
    .bind(pending_uuid)
    .fetch_optional(&state.db)
    .await
    .map_err(internal_err)?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "pending fit not found" })),
        )
    })?;

    let status: String = row.get("status");
    if status == "rejected" {
        return Ok(Json(json!({
            "status": "already_rejected",
            "pending_id": pending_uuid
        })));
    }
    if status != "pending" {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!("pending fit is in '{}' state; only 'pending' can be rejected", status)
            })),
        ));
    }

    let workspace_id: Uuid = row.get("workspace_id");
    let driver_name: String = row.get("driver_name");
    let snapshot_id: Uuid = row.get("snapshot_id");

    fermi_auth::teams::get_member_role(&state.db, workspace_id, &user_id)
        .await
        .map_err(|_| {
            (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "not a workspace member" })),
            )
        })?
        .ok_or((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "not a workspace member" })),
        ))?;

    sqlx::query(
        "UPDATE bayesops_pending_fits
         SET status='rejected', decided_at=NOW(), decided_by=$2, decision_notes=$3
         WHERE pending_id=$1",
    )
    .bind(pending_uuid)
    .bind(&user_id)
    .bind(req.notes.as_deref())
    .execute(&state.db)
    .await
    .map_err(internal_err)?;

    let _ = sqlx::query(
        "INSERT INTO workspace_messages
            (workspace_id, sender_type, sender_id, sender_name, content,
             message_type, metadata)
         VALUES ($1, 'system', 'bayesops', 'BayesOps', $2, 'system_event', $3)",
    )
    .bind(workspace_id)
    .bind(format!(
        "✗ Fit dismissed for driver '{}' by {}.",
        driver_name, user_id
    ))
    .bind(json!({
        "event": "bayesops_fit_decision",
        "decision": "rejected",
        "pending_id": pending_uuid,
        "snapshot_id": snapshot_id,
        "driver_name": driver_name,
        "decided_by": user_id,
        "notes": req.notes,
    }))
    .execute(&state.db)
    .await;

    Ok(Json(json!({
        "status": "rejected",
        "pending_id": pending_uuid,
        "workspace_id": workspace_id,
        "driver_name": driver_name,
    })))
}

fn internal_err<E: std::fmt::Display>(e: E) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": e.to_string() })),
    )
}
