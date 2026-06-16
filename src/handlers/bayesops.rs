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
    fit_conditional, ConditionalPosterior, RegressionConfig, SamplerDiagnostics,
    WeightedSample,
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
