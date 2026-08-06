//! Undo a prior applied cascade — reverse each delta atomically.
//!
//! Spec 25 §6.3 + §9 invariant 3 (Undo uses authoritative deltas).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use fermi_auth::AuthPrincipal;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct UndoRequest {
    pub notes: Option<String>,
}

pub async fn undo_pending_cascade_handler(
    Path(cascade_id): Path<String>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<UndoRequest>,
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
        "SELECT id, status, owner_id, applied_deltas
          FROM public.pending_cascades
          WHERE id = $1
          FOR UPDATE",
    )
    .bind(cid)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = match row {
        Some(r) => r,
        None => return Err((StatusCode::NOT_FOUND, "Cascade not found".into())),
    };

    let owner: String = row.try_get("owner_id").unwrap_or_default();
    // Undo stays owner-gated (see Spec 27 § note): reversing someone
    // else's applied decision is a different act from clearing a queue,
    // and the blast radius is already-propagated values. Widening this
    // wants its own decision, not a side effect of the ops board.
    if owner != user_id && !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Not your cascade".into()));
    }

    let status: String = row.try_get("status").unwrap_or_default();
    if status != "applied" {
        return Err((
            StatusCode::CONFLICT,
            format!("Can only undo 'applied' cascades; this one is '{}'", status),
        ));
    }

    let applied_deltas: JsonValue = row.try_get("applied_deltas").unwrap_or(JsonValue::Null);
    let deltas_arr = match applied_deltas.as_array() {
        Some(arr) => arr,
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "No applied_deltas recorded for this cascade; cannot undo".into(),
            ));
        }
    };

    tx.commit()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let reason = format!("cascade_undo of {}", cascade_id);
    let mut n_reverted = 0usize;

    for delta in deltas_arr {
        let forecast_id = match delta.get("forecast_id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => continue,
        };
        let prev_pp = delta.get("prev_pp").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

        sqlx::query(
            "INSERT INTO public.fermi_forecast_updates
                  (id, forecast_id, previous_probability, new_probability,
                   reason, revision_trigger, created_at)
              VALUES (gen_random_uuid()::text, $1, $2, $3, $4, 'cascade_undo', NOW())",
        )
        .bind(&forecast_id)
        .bind(delta.get("new_pp").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32)
        .bind(prev_pp)
        .bind(&reason)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        sqlx::query(
            "UPDATE public.fermi_forecasts
              SET predicted_probability = $1, updated_at = NOW()
              WHERE id = $2",
        )
        .bind(prev_pp)
        .bind(&forecast_id)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        n_reverted += 1;
    }

    sqlx::query(
        "UPDATE public.pending_cascades
          SET status = 'undone',
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
        "status": "undone",
        "n_reverted": n_reverted,
    })))
}
