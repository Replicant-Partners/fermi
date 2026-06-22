//! Operator-gated cascade queue.
//!
//! When a forecast resolves (manually via /api/forecasts/:id/resolve OR
//! via an upstream workspace resolution that propagates here), we don't
//! auto-fire the relationship cascade. Instead, a `pending_cascades` row
//! is queued for each non-archived relationship the resolved forecast
//! is a member of. The operator reviews the queue and applies/dismisses
//! each entry.
//!
//! Operator-gate rule: every parameter mutation passes through a human.
//!
//! ## Lifecycle
//!
//! ```text
//!  resolve (manual or workspace_auto)
//!    │
//!    │ for each non-archived relationship involving this forecast:
//!    │
//!    ▼
//!  queue_pending_cascade()        → row inserted with status='pending'
//!    │                              and proposed_snapshot from dry-run
//!    │
//!  ┌─┴─────────────┐
//!  │               │
//!  ▼               ▼
//!  apply           dismiss
//!    │               │
//!    ▼               ▼
//!  status='applied' status='dismissed'
//!  + propagation    + no propagation
//!    fires (the same `dispatch_propagation` the manual button uses)
//! ```

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use fermi_auth::AuthPrincipal;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::handlers::relationships::{dispatch_propagation, PropagateRequest};
use crate::AppState;

/// Queue a pending cascade for each non-archived relationship the
/// resolved forecast belongs to.
///
/// Idempotent on (relationship_id, trigger_forecast_id) — if a pending
/// row already exists for the same trigger, this is a no-op so a retry
/// of the resolve handler doesn't duplicate queue entries.
///
/// Computes a dry-run propagation per relationship to populate
/// `proposed_snapshot` — the operator sees the projected deltas before
/// clicking Apply. The dry-run never writes to fermi_forecast_updates.
///
/// Failures are logged but don't propagate to the caller — the resolve
/// itself succeeded; the cascade queue is a best-effort follow-on.
pub async fn queue_pending_cascade(
    pool: &PgPool,
    trigger_forecast_id: &str,
    trigger_kind: &str,
    outcome: Option<bool>,
    source: &str,
    owner_id: &str,
) {
    // Find all relationships involving this forecast.
    let rels = match sqlx::query(
        "SELECT id, kind, forecast_ids, parameters
         FROM public.forecast_relationships
         WHERE archived_at IS NULL
           AND $1 = ANY(forecast_ids)",
    )
    .bind(trigger_forecast_id)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                forecast = %trigger_forecast_id,
                error = %e,
                "[cascade-queue] failed to fetch relationships"
            );
            return;
        }
    };

    if rels.is_empty() {
        return;
    }

    for rel in &rels {
        let rel_id: Uuid = match rel.try_get("id") {
            Ok(u) => u,
            Err(_) => continue,
        };
        let kind: String = rel.try_get("kind").unwrap_or_default();
        let forecast_ids: Vec<String> = rel.try_get("forecast_ids").unwrap_or_default();
        let parameters: JsonValue = rel.try_get("parameters").unwrap_or(JsonValue::Null);

        // Idempotency check.
        let already: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM public.pending_cascades
             WHERE relationship_id = $1
               AND trigger_forecast_id = $2
               AND status = 'pending'",
        )
        .bind(rel_id)
        .bind(trigger_forecast_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
        if already.is_some() {
            continue;
        }

        // Dry-run the propagation to compute proposed_snapshot.
        let dry_req = PropagateRequest {
            trigger_forecast_id: trigger_forecast_id.to_string(),
            trigger_kind: trigger_kind.to_string(),
            outcome,
        };
        let snapshot = match dispatch_propagation(
            &kind,
            &forecast_ids,
            &parameters,
            &dry_req,
            pool,
            true, // dry_run
        )
        .await
        {
            Ok(result) => json!({
                "n_projected": result.n_updated,
                "deltas": result.deltas.iter().map(|d| json!({
                    "forecast_id": d.forecast_id,
                    "previous_probability": d.previous_probability,
                    "new_probability": d.new_probability,
                    "delta_pp": d.delta_pp,
                })).collect::<Vec<_>>(),
                "note": result.note,
            }),
            Err((_, e)) => {
                tracing::warn!(
                    rel = %rel_id,
                    forecast = %trigger_forecast_id,
                    error = %e,
                    "[cascade-queue] dry-run failed"
                );
                continue;
            }
        };

        if let Err(e) = sqlx::query(
            "INSERT INTO public.pending_cascades (
                 relationship_id, trigger_forecast_id, trigger_kind,
                 outcome, source, status, owner_id, proposed_snapshot
             )
             VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7)",
        )
        .bind(rel_id)
        .bind(trigger_forecast_id)
        .bind(trigger_kind)
        .bind(outcome)
        .bind(source)
        .bind(owner_id)
        .bind(&snapshot)
        .execute(pool)
        .await
        {
            tracing::warn!(
                rel = %rel_id,
                forecast = %trigger_forecast_id,
                error = %e,
                "[cascade-queue] insert failed"
            );
        } else {
            tracing::info!(
                rel = %rel_id,
                forecast = %trigger_forecast_id,
                kind = %trigger_kind,
                source = %source,
                "[cascade-queue] queued cascade"
            );
        }
    }
}

// ─── HTTP handlers ────────────────────────────────────────────────────

/// `GET /api/pending-cascades`
///
/// Lists pending cascades for the calling user. Default: status=pending
/// (the operator's actionable inbox). Query param `?status=` overrides.
pub async fn list_pending_cascades_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();
    let status = params.get("status").cloned().unwrap_or_else(|| "pending".to_string());

    let rows = sqlx::query(
        "SELECT pc.id, pc.relationship_id, pc.trigger_forecast_id,
                pc.trigger_kind, pc.outcome, pc.source, pc.status,
                pc.created_at, pc.decided_at, pc.decided_by, pc.notes,
                pc.proposed_snapshot,
                fr.kind AS relationship_kind,
                fr.description AS relationship_description,
                fr.forecast_ids AS relationship_forecast_ids,
                ff.question_text AS trigger_question_text,
                ff.predicted_probability AS trigger_probability
         FROM public.pending_cascades pc
         JOIN public.forecast_relationships fr ON fr.id = pc.relationship_id
         JOIN public.fermi_forecasts ff ON ff.id = pc.trigger_forecast_id
         WHERE pc.owner_id = $1 AND pc.status = $2
         ORDER BY pc.created_at DESC
         LIMIT 100",
    )
    .bind(&user_id)
    .bind(&status)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let entries: Vec<JsonValue> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.try_get::<Uuid, _>("id").ok().map(|u| u.to_string()),
                "relationship_id": r.try_get::<Uuid, _>("relationship_id").ok().map(|u| u.to_string()),
                "relationship_kind": r.try_get::<String, _>("relationship_kind").ok(),
                "relationship_description": r.try_get::<Option<String>, _>("relationship_description").ok().flatten(),
                "n_siblings": r
                    .try_get::<Vec<String>, _>("relationship_forecast_ids")
                    .ok()
                    .map(|v| v.len().saturating_sub(1)),
                "trigger_forecast_id": r.try_get::<String, _>("trigger_forecast_id").ok(),
                "trigger_question_text": r.try_get::<String, _>("trigger_question_text").ok(),
                "trigger_probability": r
                    .try_get::<Option<f32>, _>("trigger_probability")
                    .ok()
                    .flatten()
                    .map(|v| v as f64),
                "trigger_kind": r.try_get::<String, _>("trigger_kind").ok(),
                "outcome": r.try_get::<Option<bool>, _>("outcome").ok().flatten(),
                "source": r.try_get::<String, _>("source").ok(),
                "status": r.try_get::<String, _>("status").ok(),
                "proposed_snapshot": r.try_get::<JsonValue, _>("proposed_snapshot").ok(),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
                "decided_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("decided_at").ok().flatten().map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "pending": entries,
        "count": entries.len(),
        "status": status,
    })))
}

#[derive(Debug, Deserialize)]
pub struct ApplyDismissRequest {
    pub notes: Option<String>,
}

/// `POST /api/pending-cascades/:id/apply`
///
/// Fires the cascade for this queue entry. Recomputes the propagation
/// against current sibling probabilities (so a stale snapshot doesn't
/// produce wrong values if siblings shifted in the meantime). Marks the
/// row 'applied' on success.
pub async fn apply_pending_cascade_handler(
    Path(cascade_id): Path<String>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<ApplyDismissRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();
    let cid = Uuid::parse_str(&cascade_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid cascade id".into()))?;

    // Load + lock the queue row so two concurrent Apply clicks can't
    // double-fire.
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = sqlx::query(
        "SELECT pc.relationship_id, pc.trigger_forecast_id, pc.trigger_kind,
                pc.outcome, pc.status, pc.owner_id,
                fr.kind AS relationship_kind,
                fr.forecast_ids AS relationship_forecast_ids,
                fr.parameters AS relationship_parameters
         FROM public.pending_cascades pc
         JOIN public.forecast_relationships fr ON fr.id = pc.relationship_id
         WHERE pc.id = $1
         FOR UPDATE OF pc",
    )
    .bind(cid)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Cascade not found".into()))?;

    let owner: String = row.try_get("owner_id").unwrap_or_default();
    if owner != user_id && !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Not your cascade".into()));
    }
    let status: String = row.try_get("status").unwrap_or_default();
    if status != "pending" {
        return Err((
            StatusCode::CONFLICT,
            format!("Cascade is already {} (not pending)", status),
        ));
    }

    let rel_kind: String = row.try_get("relationship_kind").unwrap_or_default();
    let forecast_ids: Vec<String> = row.try_get("relationship_forecast_ids").unwrap_or_default();
    let parameters: JsonValue = row.try_get("relationship_parameters").unwrap_or(JsonValue::Null);
    let trigger_forecast_id: String = row.try_get("trigger_forecast_id").unwrap_or_default();
    let trigger_kind: String = row.try_get("trigger_kind").unwrap_or_default();
    let outcome: Option<bool> = row.try_get("outcome").ok().flatten();

    // Commit txn before propagation — the propagation function uses
    // its own connections and we don't want to hold the lock during
    // its writes.
    tx.commit()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Apply the propagation for real (dry_run = false).
    let prop_req = PropagateRequest {
        trigger_forecast_id: trigger_forecast_id.clone(),
        trigger_kind: trigger_kind.clone(),
        outcome,
    };
    let result = dispatch_propagation(
        &rel_kind,
        &forecast_ids,
        &parameters,
        &prop_req,
        &state.db,
        false,
    )
    .await?;

    // Mark the queue row applied.
    let _ = sqlx::query(
        "UPDATE public.pending_cascades
         SET status = 'applied',
             decided_at = NOW(),
             decided_by = $2,
             notes = COALESCE($3, notes)
         WHERE id = $1",
    )
    .bind(cid)
    .bind(&user_id)
    .bind(&req.notes)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "id": cascade_id,
        "status": "applied",
        "n_updated": result.n_updated,
        "deltas": result.deltas,
        "note": result.note,
    })))
}

/// `POST /api/pending-cascades/:id/dismiss`
///
/// Mark the queue entry dismissed without firing the cascade.
pub async fn dismiss_pending_cascade_handler(
    Path(cascade_id): Path<String>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<ApplyDismissRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();
    let cid = Uuid::parse_str(&cascade_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid cascade id".into()))?;

    let result = sqlx::query(
        "UPDATE public.pending_cascades
         SET status = 'dismissed',
             decided_at = NOW(),
             decided_by = $2,
             notes = $3
         WHERE id = $1
           AND (owner_id = $2 OR $4)
           AND status = 'pending'",
    )
    .bind(cid)
    .bind(&user_id)
    .bind(&req.notes)
    .bind(principal.can_admin())
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Cascade not found, not yours, or not pending".into(),
        ));
    }

    Ok(Json(json!({
        "id": cascade_id,
        "status": "dismissed",
    })))
}
