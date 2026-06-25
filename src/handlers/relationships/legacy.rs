//! Legacy forecast_relationships handlers — backward compat with mig 150.
//!
//! These handlers remain functional for existing data but new groups
//! should use the group-tag model (groups.rs + membership.rs).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use fermi_auth::AuthPrincipal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;
use super::propagation::{dispatch_propagation, PropagateRequest, PropagateResult};

#[derive(Debug, Deserialize)]
pub struct CreateRelationshipRequest {
    pub kind: String,
    pub forecast_ids: Vec<String>,
    #[serde(default)]
    pub parameters: JsonValue,
    pub description: Option<String>,
}

pub async fn create_relationship_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreateRelationshipRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();

    let known = matches!(
        req.kind.as_str(),
        "mutually_exclusive"
            | "logical_implies"
            | "conjunction"
            | "conditional"
            | "exhaustive_cover"
            | "mutex"
            | "at_most_n"
            | "implies"
    );
    if !known {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Unknown relationship kind '{}'.", req.kind),
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

    if !forecast_ids.contains(&req.trigger_forecast_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "trigger_forecast_id '{}' is not a member of relationship {}",
                req.trigger_forecast_id, rel_id
            ),
        ));
    }

    dispatch_propagation(&kind, &forecast_ids, &parameters, &req, &state.db, false)
        .await
        .map(Json)
}
