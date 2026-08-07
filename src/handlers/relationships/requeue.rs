//! Re-queue a cascade — supersede prior cascades and queue a fresh one.
//!
//! Spec 25 §6.3.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use fermi_auth::AuthPrincipal;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use sqlx::Row;
use uuid::Uuid;

use super::propagation::{dispatch_propagation_group, PropagateRequest};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct RequeueRequest {
    pub group_id: String,
    pub trigger_forecast_id: String,
    pub trigger_kind: Option<String>,
    pub outcome: Option<bool>,
    pub supersede_ids: Option<Vec<Uuid>>,
}

pub async fn requeue_cascade_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<RequeueRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();

    let group_row = sqlx::query(
        "SELECT kind, parameters FROM public.forecast_relationship_groups
          WHERE group_id = $1 AND archived_at IS NULL",
    )
    .bind(&req.group_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let group_row = match group_row {
        Some(r) => r,
        None => return Err((StatusCode::NOT_FOUND, "Group not found".into())),
    };

    let kind: String = group_row.try_get("kind").unwrap_or_default();
    let parameters: JsonValue = group_row.try_get("parameters").unwrap_or(JsonValue::Null);

    let trigger_kind = req.trigger_kind.as_deref().unwrap_or("resolved");

    let prop_req = PropagateRequest {
        trigger_forecast_id: req.trigger_forecast_id.clone(),
        trigger_kind: trigger_kind.to_string(),
        outcome: req.outcome,
    };

    let snapshot = match dispatch_propagation_group(
        &kind,
        &req.group_id,
        &parameters,
        &prop_req,
        &state.db,
        true,
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
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Dry-run failed: {}", e),
            ));
        }
    };

    let supersede_ids = match req.supersede_ids {
        Some(ids) => ids,
        None => {
            let rows = sqlx::query(
                "SELECT id FROM public.pending_cascades
                  WHERE group_id = $1
                    AND trigger_forecast_id = $2
                    AND status NOT IN ('undone', 'superseded')",
            )
            .bind(&req.group_id)
            .bind(&req.trigger_forecast_id)
            .fetch_all(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            rows.iter()
                .filter_map(|r| r.try_get::<Uuid, _>("id").ok())
                .collect()
        }
    };

    let new_row = sqlx::query(
        "INSERT INTO public.pending_cascades (
              group_id, trigger_forecast_id, trigger_kind,
              outcome, source, status, owner_id, proposed_snapshot
          )
          VALUES ($1, $2, $3, $4, 'requeue', 'pending', $5, $6)
          RETURNING id",
    )
    .bind(&req.group_id)
    .bind(&req.trigger_forecast_id)
    .bind(trigger_kind)
    .bind(req.outcome)
    .bind(&user_id)
    .bind(&snapshot)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let new_id: Uuid = new_row.try_get("id").unwrap_or_default();

    if !supersede_ids.is_empty() {
        for old_id in &supersede_ids {
            let _ = sqlx::query(
                "UPDATE public.pending_cascades
                  SET status = 'superseded',
                      superseded_by = $2
                  WHERE id = $1
                    AND status NOT IN ('undone')",
            )
            .bind(old_id)
            .bind(new_id)
            .execute(&state.db)
            .await;
        }
    }

    Ok(Json(json!({
        "id": new_id.to_string(),
        "group_id": req.group_id,
        "trigger_forecast_id": req.trigger_forecast_id,
        "status": "pending",
        "superseded_count": supersede_ids.len(),
        "proposed_snapshot": snapshot,
    })))
}
