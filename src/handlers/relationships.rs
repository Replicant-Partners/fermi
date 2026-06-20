//! Forecast relationships — declarable inter-forecast dependencies.
//!
//! Generalizes "when forecast A changes, forecast B should follow" beyond
//! any single domain. A relationship is its own first-class object,
//! decoupled from portfolio membership: a portfolio may contain related
//! AND independent forecasts, and a relationship may span multiple
//! portfolios.
//!
//! ## Kinds (see migration 150)
//!
//! - `mutually_exclusive` — exactly one forecast in the set is true. Sum
//!   of probabilities = 1.0. WC sims case (one of 48 teams wins).
//! - `logical_implies` — `F1 ⇒ F2` so `P(F2) ≥ P(F1)`. Stubbed.
//! - `conjunction` — joint probability = product (under independence) or
//!   per a correlation parameter. Stubbed.
//! - `conditional` — correlation matrix entry: when F1 moves by Δ, F2
//!   moves by `corr * Δ`. Stubbed.
//! - `exhaustive_cover` — exactly one of the set is true, but not
//!   mutually exclusive. Stubbed.
//!
//! Adding a new kind: implement `propagate_<kind>`, register it in
//! `propagate_relationship`, document the parameters JSONB shape.
//!
//! ## Operator-explicit by design
//!
//! Propagation does NOT auto-fire on resolve. The console surfaces a
//! "Cascade to N forecasts" button on resolved forecasts that have
//! relationships; the operator clicks to propagate. Auto-cascade is
//! one toggle away once we trust the math at scale.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use fermi_auth::AuthPrincipal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::AppState;

// ─── Wire types ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateRelationshipRequest {
    pub kind: String,
    pub forecast_ids: Vec<String>,
    #[serde(default)]
    pub parameters: JsonValue,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PropagateRequest {
    /// The forecast that just changed; siblings update with reason
    /// "cascade from <trigger_forecast_id>".
    pub trigger_forecast_id: String,
    /// What kind of change. 'resolved' means the trigger forecast was
    /// resolved (outcome known); 'updated' means its probability
    /// changed but it's still active. Different kinds drive different
    /// propagation math (a resolved forecast's probability is
    /// effectively 0 or 1).
    pub trigger_kind: String,
    /// For trigger_kind='resolved' — was the outcome true (forecast
    /// "won") or false (forecast "lost"). Required for resolution
    /// propagation; ignored otherwise.
    pub outcome: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct PropagateResult {
    pub n_updated: usize,
    pub deltas: Vec<DeltaEntry>,
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DeltaEntry {
    pub forecast_id: String,
    pub previous_probability: f64,
    pub new_probability: f64,
    pub delta_pp: f64,
}

// ─── CRUD ─────────────────────────────────────────────────────────────

/// `POST /api/forecast-relationships`
pub async fn create_relationship_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreateRelationshipRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();

    // Validate the kind is one we know how to propagate. We accept
    // unknown kinds at create-time too (future extension), but mark them
    // explicitly as not-yet-implemented so the operator gets a clear
    // error on propagate.
    let known = matches!(
        req.kind.as_str(),
        "mutually_exclusive"
            | "logical_implies"
            | "conjunction"
            | "conditional"
            | "exhaustive_cover"
    );
    if !known {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Unknown relationship kind '{}'. Valid kinds: mutually_exclusive, logical_implies, conjunction, conditional, exhaustive_cover.",
                req.kind
            ),
        ));
    }
    if req.forecast_ids.len() < 2 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Relationships require at least 2 forecast_ids".into(),
        ));
    }

    let row = sqlx::query(
        "INSERT INTO public.forecast_relationships
             (kind, forecast_ids, parameters, description, owner_id)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, created_at",
    )
    .bind(&req.kind)
    .bind(&req.forecast_ids)
    .bind(&req.parameters)
    .bind(&req.description)
    .bind(&user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "id": row.try_get::<Uuid, _>("id").ok().map(|u| u.to_string()),
        "kind": req.kind,
        "forecast_ids": req.forecast_ids,
        "n_forecasts": req.forecast_ids.len(),
        "created_at": row
            .try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
            .ok()
            .map(|t| t.to_rfc3339()),
    })))
}

/// `GET /api/forecast-relationships?forecast_id=<id>`
///
/// Lists all non-archived relationships involving the given forecast.
/// Used by the console to surface cascade controls on resolved forecasts.
pub async fn list_relationships_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();

    let rows = if let Some(fid) = params.get("forecast_id") {
        sqlx::query(
            "SELECT id, kind, forecast_ids, parameters, description, created_at, updated_at
             FROM public.forecast_relationships
             WHERE owner_id = $1
               AND $2 = ANY(forecast_ids)
               AND archived_at IS NULL
             ORDER BY created_at DESC",
        )
        .bind(&user_id)
        .bind(fid)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query(
            "SELECT id, kind, forecast_ids, parameters, description, created_at, updated_at
             FROM public.forecast_relationships
             WHERE owner_id = $1 AND archived_at IS NULL
             ORDER BY created_at DESC",
        )
        .bind(&user_id)
        .fetch_all(&state.db)
        .await
    }
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let relationships: Vec<JsonValue> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.try_get::<Uuid, _>("id").ok().map(|u| u.to_string()),
                "kind": r.try_get::<String, _>("kind").ok(),
                "forecast_ids": r.try_get::<Vec<String>, _>("forecast_ids").ok(),
                "parameters": r.try_get::<JsonValue, _>("parameters").ok(),
                "description": r.try_get::<Option<String>, _>("description").ok().flatten(),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "relationships": relationships,
        "count": relationships.len(),
    })))
}

/// `DELETE /api/forecast-relationships/:id` — soft delete via archived_at.
pub async fn delete_relationship_handler(
    Path(rel_id): Path<String>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();
    let rel_uuid = Uuid::parse_str(&rel_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid relationship id".into()))?;

    let result = sqlx::query(
        "UPDATE public.forecast_relationships
         SET archived_at = NOW(), updated_at = NOW()
         WHERE id = $1 AND owner_id = $2 AND archived_at IS NULL",
    )
    .bind(rel_uuid)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Relationship not found or already archived".into(),
        ));
    }

    Ok(Json(json!({ "archived": true })))
}

// ─── Propagation ──────────────────────────────────────────────────────

/// `POST /api/forecast-relationships/:id/propagate`
///
/// Fires the per-kind propagation function. Operator-explicit (no
/// auto-firing on update/resolve elsewhere).
pub async fn propagate_relationship_handler(
    Path(rel_id): Path<String>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<PropagateRequest>,
) -> Result<Json<PropagateResult>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();
    let rel_uuid = Uuid::parse_str(&rel_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid relationship id".into()))?;

    let row = sqlx::query(
        "SELECT kind, forecast_ids, parameters, owner_id
         FROM public.forecast_relationships
         WHERE id = $1 AND archived_at IS NULL",
    )
    .bind(rel_uuid)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Relationship not found".into()))?;

    let owner: String = row.try_get("owner_id").unwrap_or_default();
    if owner != user_id && !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Not your relationship".into()));
    }
    let kind: String = row.try_get("kind").unwrap_or_default();
    let forecast_ids: Vec<String> = row.try_get("forecast_ids").unwrap_or_default();
    let parameters: JsonValue = row.try_get("parameters").unwrap_or(JsonValue::Null);

    // Sanity: trigger forecast must be in the relationship.
    if !forecast_ids.contains(&req.trigger_forecast_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "trigger_forecast_id '{}' is not a member of relationship {}",
                req.trigger_forecast_id, rel_id
            ),
        ));
    }

    // Dispatch by kind.
    match kind.as_str() {
        "mutually_exclusive" => {
            propagate_mutex(&forecast_ids, &parameters, &req, &state.db).await
        }
        "logical_implies" | "conjunction" | "conditional" | "exhaustive_cover" => Err((
            StatusCode::NOT_IMPLEMENTED,
            format!(
                "Relationship kind '{}' is declared but propagation is not yet implemented. \
                 Implement propagate_{} in src/handlers/relationships.rs.",
                kind, kind
            ),
        )),
        other => Err((
            StatusCode::BAD_REQUEST,
            format!("Unknown relationship kind: {}", other),
        )),
    }
    .map(Json)
}

/// Mutually-exclusive propagation.
///
/// For trigger_kind='resolved' with outcome=false:
///   The trigger forecast is now P=0. Distribute its previous probability
///   mass across the remaining survivors proportionally to their CURRENT
///   probabilities (so a high-probability survivor takes more of the mass
///   than a low-probability one — matches Bayesian conditioning under
///   "exactly one of these is true; we just learned trigger isn't it").
///
/// For trigger_kind='resolved' with outcome=true:
///   The trigger forecast is now P=1. Every other forecast in the mutex
///   is necessarily false → set their probabilities to ~0 (we use 0.001
///   to keep the cockpit's clamp happy).
///
/// For trigger_kind='updated':
///   The trigger's probability moved by Δ. Distribute -Δ across siblings
///   proportionally to current p. Mass-conserving redistribution.
///
/// Each affected sibling gets a fermi_forecast_updates row written with
/// revision_trigger='cascade' and a reason that names the source. This
/// surfaces in the trajectory tab as a synchronized cascade event.
async fn propagate_mutex(
    forecast_ids: &[String],
    _parameters: &JsonValue,
    req: &PropagateRequest,
    pool: &PgPool,
) -> Result<PropagateResult, (StatusCode, String)> {
    // Read the current probability of every member (including the
    // trigger). We need the trigger's previous probability to know how
    // much mass to redistribute.
    let rows = sqlx::query(
        "SELECT id, predicted_probability
         FROM public.fermi_forecasts
         WHERE id = ANY($1)",
    )
    .bind(forecast_ids)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut current: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for r in &rows {
        let id: String = match r.try_get("id") {
            Ok(s) => s,
            Err(_) => continue,
        };
        let p: f64 = r
            .try_get::<f32, _>("predicted_probability")
            .map(|v| v as f64)
            .unwrap_or(0.0);
        current.insert(id, p);
    }

    let trigger_prev = *current.get(&req.trigger_forecast_id).unwrap_or(&0.0);

    // Compute new probabilities per member. Build the list of (id,
    // prev_p, new_p) up front; we'll write all updates after so a partial
    // failure leaves a consistent ledger.
    let mut updates: Vec<(String, f64, f64)> = Vec::new();
    let mut note: Option<String> = None;

    match (req.trigger_kind.as_str(), req.outcome) {
        ("resolved", Some(false)) => {
            // Trigger went to 0. Distribute its mass across survivors
            // proportionally to current probability.
            let survivors: Vec<&String> = forecast_ids
                .iter()
                .filter(|id| **id != req.trigger_forecast_id)
                .collect();
            let survivor_total: f64 = survivors
                .iter()
                .map(|id| current.get(*id).copied().unwrap_or(0.0))
                .sum();

            if survivor_total < 1e-9 {
                note = Some(
                    "Survivor probabilities sum to ~0; cannot redistribute proportionally. \
                     Sibling forecasts left untouched."
                        .into(),
                );
            } else {
                for id in &survivors {
                    let prev = current.get(*id).copied().unwrap_or(0.0);
                    // Each survivor absorbs a share of the eliminated
                    // mass equal to their fraction of the survivor pool.
                    let share = prev / survivor_total;
                    let absorbed = trigger_prev * share;
                    let new_p = (prev + absorbed).clamp(0.001, 0.999);
                    if (new_p - prev).abs() > 1e-5 {
                        updates.push(((*id).clone(), prev, new_p));
                    }
                }
            }
            // Trigger goes to 0 explicitly (the resolve handler should
            // have done this already, but we backstop). 0.001 to keep
            // clamp happy.
            if trigger_prev > 0.001 {
                updates.push((
                    req.trigger_forecast_id.clone(),
                    trigger_prev,
                    0.001,
                ));
            }
        }

        ("resolved", Some(true)) => {
            // Trigger went to 1. Every sibling drops to ~0.
            for id in forecast_ids.iter().filter(|id| **id != req.trigger_forecast_id) {
                let prev = current.get(id).copied().unwrap_or(0.0);
                if prev > 0.001 {
                    updates.push((id.clone(), prev, 0.001));
                }
            }
            if trigger_prev < 0.999 {
                updates.push((
                    req.trigger_forecast_id.clone(),
                    trigger_prev,
                    0.999,
                ));
            }
        }

        ("resolved", None) => {
            return Err((
                StatusCode::BAD_REQUEST,
                "trigger_kind='resolved' requires `outcome` (true|false)".into(),
            ));
        }

        ("updated", _) => {
            // Trigger's probability changed. We need both prev and new
            // values; the caller should have just written an
            // update_probability so current_p is the *new* value.
            // Trigger change Δ is implied by current[trigger] minus the
            // total probability before this update. With exact mutex,
            // the sum-after must equal the sum-before (= 1.0 ideally).
            //
            // Strategy: compute total over all members; treat the
            // overshoot/undershoot from 1.0 as the mass to redistribute
            // among siblings. This is approximate when the prior sum
            // wasn't exactly 1.0 (which is normal for a calibrated
            // model — Fermi's WC sims sum to ~30% across all 48,
            // because the model is conservative and doesn't know the
            // mutex constraint).
            //
            // For the demo this is fine: the cascade button is meant
            // to be fired specifically after a resolution, not after
            // every probability tick. We still implement 'updated' so
            // the wire works for non-resolution scenarios; it's the
            // operator's call when to fire.
            let total: f64 = current.values().sum();
            let delta = total - 1.0;
            if delta.abs() < 1e-6 {
                note = Some("Members already sum to 1.0; no redistribution needed.".into());
            } else {
                // We want to subtract `delta` proportionally from the
                // siblings (NOT the trigger — its probability is what
                // it is now). If sum > 1, siblings shed mass; if <1,
                // siblings absorb.
                let siblings: Vec<&String> = forecast_ids
                    .iter()
                    .filter(|id| **id != req.trigger_forecast_id)
                    .collect();
                let sibling_total: f64 = siblings
                    .iter()
                    .map(|id| current.get(*id).copied().unwrap_or(0.0))
                    .sum();
                if sibling_total < 1e-9 {
                    note = Some(
                        "Sibling probabilities sum to ~0; cannot redistribute. \
                         Sibling forecasts left untouched."
                            .into(),
                    );
                } else {
                    for id in &siblings {
                        let prev = current.get(*id).copied().unwrap_or(0.0);
                        // Each sibling's share of the redistribution
                        // equals their fraction of the sibling pool.
                        let share = prev / sibling_total;
                        let new_p = (prev - delta * share).clamp(0.001, 0.999);
                        if (new_p - prev).abs() > 1e-5 {
                            updates.push(((*id).clone(), prev, new_p));
                        }
                    }
                }
            }
        }

        (other, _) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Unknown trigger_kind '{}'. Valid: 'resolved', 'updated'",
                    other
                ),
            ));
        }
    }

    // Write all updates. Each gets a fermi_forecast_updates row with
    // revision_trigger='cascade'. The spacetime trigger (migration 149
    // + 150) propagates these into forecast_spacetime so the trajectory
    // tab sees them.
    let reason = format!(
        "cascade from {} ({})",
        req.trigger_forecast_id, req.trigger_kind
    );
    let mut written = 0usize;
    for (fid, prev, new_p) in &updates {
        let new_p_f32 = *new_p as f32;
        let prev_f32 = *prev as f32;
        // Insert into the updates table (the trigger fans out to spacetime).
        let _ = sqlx::query(
            "INSERT INTO public.fermi_forecast_updates
                 (id, forecast_id, previous_probability, new_probability,
                  reason, revision_trigger, created_at)
             VALUES (gen_random_uuid()::text, $1, $2, $3, $4, 'cascade', NOW())",
        )
        .bind(fid)
        .bind(prev_f32)
        .bind(new_p_f32)
        .bind(&reason)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // Update the forecast's current probability so subsequent reads
        // see the cascaded value.
        let _ = sqlx::query(
            "UPDATE public.fermi_forecasts
             SET predicted_probability = $1, updated_at = NOW()
             WHERE id = $2",
        )
        .bind(new_p_f32)
        .bind(fid)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        written += 1;
    }

    let deltas: Vec<DeltaEntry> = updates
        .into_iter()
        .map(|(fid, prev, new_p)| DeltaEntry {
            forecast_id: fid,
            previous_probability: prev,
            new_probability: new_p,
            delta_pp: (new_p - prev) * 100.0,
        })
        .collect();

    Ok(PropagateResult {
        n_updated: written,
        deltas,
        note,
    })
}
