//! Membership queries — "for forecast F, what groups is it in?" /
//! "for group G, who are members?"
//!
//! Spec 25 §6.2.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use fermi_auth::AuthPrincipal;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use sqlx::Row;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct SetGroupsRequest {
    pub groups: Vec<String>,
}

pub async fn get_group_members_handler(
    Path(group_id): Path<String>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let _user_id = principal.user_id().to_string();

    let members = get_group_members(&group_id, &state.db).await?;

    Ok(Json(json!({
        "members": members,
        "count": members.len(),
    })))
}

pub async fn get_forecast_groups_handler(
    Path(forecast_id): Path<String>,
    State(state): State<AppState>,
    _principal: AuthPrincipal,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let row = sqlx::query("SELECT relationship_groups FROM public.fermi_forecasts WHERE id = $1")
        .bind(&forecast_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = match row {
        Some(r) => r,
        None => return Err((StatusCode::NOT_FOUND, "Forecast not found".into())),
    };

    let groups: Vec<String> = row.try_get("relationship_groups").unwrap_or_default();

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "groups": groups,
    })))
}

pub async fn set_forecast_groups_handler(
    Path(forecast_id): Path<String>,
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Json(req): Json<SetGroupsRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.fermi_forecasts WHERE id = $1)")
            .bind(&forecast_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !exists {
        return Err((StatusCode::NOT_FOUND, "Forecast not found".into()));
    }

    sqlx::query(
        "UPDATE public.fermi_forecasts
          SET relationship_groups = $2, updated_at = NOW()
          WHERE id = $1",
    )
    .bind(&forecast_id)
    .bind(&req.groups)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "groups": req.groups,
    })))
}

pub async fn add_forecast_to_group_handler(
    Path((forecast_id, group_id)): Path<(String, String)>,
    State(state): State<AppState>,
    _principal: AuthPrincipal,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.fermi_forecasts WHERE id = $1)")
            .bind(&forecast_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !exists {
        return Err((StatusCode::NOT_FOUND, "Forecast not found".into()));
    }

    sqlx::query(
        "UPDATE public.fermi_forecasts
          SET relationship_groups = array_append(relationship_groups, $2),
              updated_at = NOW()
          WHERE id = $1
            AND NOT ($2 = ANY(relationship_groups))",
    )
    .bind(&forecast_id)
    .bind(&group_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = sqlx::query("SELECT relationship_groups FROM public.fermi_forecasts WHERE id = $1")
        .bind(&forecast_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let groups: Vec<String> = row.try_get("relationship_groups").unwrap_or_default();

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "groups": groups,
    })))
}

pub async fn remove_forecast_from_group_handler(
    Path((forecast_id, group_id)): Path<(String, String)>,
    State(state): State<AppState>,
    _principal: AuthPrincipal,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.fermi_forecasts WHERE id = $1)")
            .bind(&forecast_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !exists {
        return Err((StatusCode::NOT_FOUND, "Forecast not found".into()));
    }

    sqlx::query(
        "UPDATE public.fermi_forecasts
          SET relationship_groups = array_remove(relationship_groups, $2),
              updated_at = NOW()
          WHERE id = $1",
    )
    .bind(&forecast_id)
    .bind(&group_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = sqlx::query("SELECT relationship_groups FROM public.fermi_forecasts WHERE id = $1")
        .bind(&forecast_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let groups: Vec<String> = row.try_get("relationship_groups").unwrap_or_default();

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "groups": groups,
    })))
}

pub async fn get_group_members(
    group_id: &str,
    pool: &sqlx::PgPool,
) -> Result<Vec<JsonValue>, (StatusCode, String)> {
    let rows = sqlx::query(
        "SELECT id, question_text, predicted_probability, status
          FROM public.fermi_forecasts
          WHERE relationship_groups @> ARRAY[$1]
          ORDER BY question_text",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(rows
        .iter()
        .map(|r| {
            json!({
                "id": r.try_get::<String, _>("id").ok(),
                "question_text": r.try_get::<String, _>("question_text").ok(),
                "predicted_probability": r
                    .try_get::<f32, _>("predicted_probability")
                    .ok()
                    .map(|v| v as f64),
                "status": r.try_get::<String, _>("status").ok(),
            })
        })
        .collect())
}
