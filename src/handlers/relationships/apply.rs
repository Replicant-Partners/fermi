//! Apply a pending cascade — execute propagation and capture applied_deltas.
//!
//! Spec 25 §6.3 + §9 invariant 2 (Apply is recomputed at apply time).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use fermi_auth::AuthPrincipal;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use sqlx::Row;
use uuid::Uuid;

use super::propagation::{dispatch_propagation, dispatch_propagation_group, PropagateRequest};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ApplyDismissRequest {
    pub notes: Option<String>,
}

pub async fn apply_pending_cascade_handler(
    Path(cascade_id): Path<String>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<ApplyDismissRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();
    let cid = Uuid::parse_str(&cascade_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid cascade id".into()))?;

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = sqlx::query(
        "SELECT pc.relationship_id, pc.group_id, pc.trigger_forecast_id, pc.trigger_kind,
                pc.outcome, pc.status, pc.owner_id
          FROM public.pending_cascades pc
          WHERE pc.id = $1
          FOR UPDATE OF pc",
    )
    .bind(cid)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = match row {
        Some(r) => r,
        None => {
            return Err((StatusCode::NOT_FOUND, "Cascade not found".into()));
        }
    };

    // Spec 27: EDIT on the trigger forecast, not ownership of the queue
    // row. Applying a cascade is shared team work — see
    // pending_cascades::require_cascade_edit for the full rationale. The
    // check runs outside the transaction's row lock on purpose: it only
    // reads, and holding the lock across an ACL round-trip would serialise
    // the queue for every concurrent reviewer.
    crate::handlers::pending_cascades::require_cascade_edit(&state.db, cid, &principal).await?;
    let status: String = row.try_get("status").unwrap_or_default();
    if status != "pending" {
        return Err((
            StatusCode::CONFLICT,
            format!("Cascade is already {} (not pending)", status),
        ));
    }

    let group_id: Option<String> = row.try_get("group_id").ok().flatten();
    let relationship_id: Option<Uuid> = row.try_get("relationship_id").ok();
    let trigger_forecast_id: String = row.try_get("trigger_forecast_id").unwrap_or_default();
    let trigger_kind: String = row.try_get("trigger_kind").unwrap_or_default();
    let outcome: Option<bool> = row.try_get("outcome").ok().flatten();

    tx.commit()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let prop_req = PropagateRequest {
        trigger_forecast_id: trigger_forecast_id.clone(),
        trigger_kind: trigger_kind.clone(),
        outcome,
    };

    let result = if let Some(gid) = &group_id {
        let group_row = sqlx::query(
            "SELECT kind, parameters FROM public.forecast_relationship_groups
              WHERE group_id = $1 AND archived_at IS NULL",
        )
        .bind(gid)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let group_row = match group_row {
            Some(r) => r,
            None => return Err((StatusCode::NOT_FOUND, "Group not found".into())),
        };

        let kind: String = group_row.try_get("kind").unwrap_or_default();
        let parameters: JsonValue = group_row.try_get("parameters").unwrap_or(JsonValue::Null);

        dispatch_propagation_group(&kind, gid, &parameters, &prop_req, &state.db, false).await?
    } else if let Some(rel_id) = relationship_id {
        let rel_row = sqlx::query(
            "SELECT kind, forecast_ids, parameters
              FROM public.forecast_relationships
              WHERE id = $1 AND archived_at IS NULL",
        )
        .bind(rel_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let rel_row = match rel_row {
            Some(r) => r,
            None => {
                return Err((
                    StatusCode::NOT_FOUND,
                    "Legacy relationship not found".into(),
                ))
            }
        };

        let kind: String = rel_row.try_get("kind").unwrap_or_default();
        let forecast_ids: Vec<String> = rel_row.try_get("forecast_ids").unwrap_or_default();
        let parameters: JsonValue = rel_row.try_get("parameters").unwrap_or(JsonValue::Null);

        dispatch_propagation(
            &kind,
            &forecast_ids,
            &parameters,
            &prop_req,
            &state.db,
            false,
        )
        .await?
    } else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Cascade has neither group_id nor relationship_id".into(),
        ));
    };

    let applied_deltas: Vec<JsonValue> = result
        .deltas
        .iter()
        .map(|d| {
            json!({
                "forecast_id": d.forecast_id,
                "prev_pp": d.previous_probability,
                "new_pp": d.new_probability,
                "delta_pp": d.delta_pp,
            })
        })
        .collect();

    sqlx::query(
        "UPDATE public.pending_cascades
          SET status = 'applied',
              decided_at = NOW(),
              decided_by = $2,
              notes = COALESCE($3, notes),
              applied_deltas = $4
          WHERE id = $1",
    )
    .bind(cid)
    .bind(&user_id)
    .bind(&req.notes)
    .bind(serde_json::to_value(&applied_deltas).unwrap_or(JsonValue::Null))
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
