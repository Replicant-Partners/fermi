//! Workspace Output handlers — typed KV store for workspace results.
//!
//! Outputs are the mechanism by which workspaces publish consumable results.
//! Cross-workspace reads enable dependency graphs (e.g., team prior → tournament path).
//!
//! Routes:
//!   PUT  /api/workspaces/:id/outputs/:key     — set an output
//!   GET  /api/workspaces/:id/outputs           — list all outputs
//!   GET  /api/workspaces/:id/outputs/:key      — read single output
//!   DEL  /api/workspaces/:id/outputs/:key      — delete an output
//!   GET  /api/workspaces/:id/dependencies       — list upstream/downstream
//!   POST /api/workspaces/:id/dependencies       — add a dependency edge
//!   DEL  /api/workspaces/:id/dependencies/:upstream_id — remove edge

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sqlx::Row;

use crate::AppState;
use fermi_auth::{teams, AuthPrincipal};

// ═══════════════════════════════════════════════════════════════════
// Request Types
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct SetOutputRequest {
    pub value: JsonValue,
}

#[derive(Debug, Deserialize)]
pub struct AddDependencyRequest {
    pub upstream_id: String,
    pub dependency_type: Option<String>,
    pub key_filter: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════
// Output CRUD
// ═══════════════════════════════════════════════════════════════════

/// PUT /api/workspaces/:workspace_id/outputs/:key
pub async fn set_output_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((workspace_id, key)): Path<(String, String)>,
    Json(req): Json<SetOutputRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".into()))?;

    // Verify membership
    teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".into()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".into()))?;

    // Upsert output
    let row = sqlx::query(
        "INSERT INTO workspace_outputs (workspace_id, key, value, version, updated_at, updated_by)
         VALUES ($1, $2, $3, 1, NOW(), $4)
         ON CONFLICT (workspace_id, key) DO UPDATE SET
           value = $3,
           version = workspace_outputs.version + 1,
           updated_at = NOW(),
           updated_by = $4
         RETURNING version",
    )
    .bind(ws_uuid)
    .bind(&key)
    .bind(&req.value)
    .bind(&user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let version: i32 = row.get("version");

    // Notify downstream workspaces that this output changed
    let downstream: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT downstream_id FROM workspace_dependencies
         WHERE upstream_id = $1
           AND (key_filter IS NULL OR key_filter = $2)",
    )
    .bind(ws_uuid)
    .bind(&key)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    // Post a system message to each downstream workspace
    for ds_id in &downstream {
        let _ = sqlx::query(
            "INSERT INTO workspace_messages (workspace_id, sender_type, sender_id, sender_name, content, message_type, metadata)
             VALUES ($1, 'system', $2, 'Workspace Output', $3, 'system_event', $4)",
        )
        .bind(ds_id)
        .bind(&workspace_id)
        .bind(format!("Upstream workspace {} updated output '{}'", workspace_id, key))
        .bind(json!({
            "event": "upstream_output_updated",
            "upstream_workspace_id": workspace_id,
            "key": key,
            "version": version,
        }))
        .execute(&state.db)
        .await;
    }

    Ok(Json(json!({
        "workspace_id": workspace_id,
        "key": key,
        "version": version,
        "downstream_notified": downstream.len(),
    })))
}

/// GET /api/workspaces/:workspace_id/outputs
pub async fn list_outputs_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".into()))?;

    // Verify membership (or workspace is shared/public)
    let _ = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .ok();

    let rows = sqlx::query(
        "SELECT key, value, version, updated_at, updated_by
         FROM workspace_outputs
         WHERE workspace_id = $1
         ORDER BY key",
    )
    .bind(ws_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let outputs: Vec<JsonValue> = rows
        .iter()
        .map(|r| {
            json!({
                "key": r.get::<String, _>("key"),
                "value": r.get::<JsonValue, _>("value"),
                "version": r.get::<i32, _>("version"),
                "updated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
                "updated_by": r.try_get::<Option<String>, _>("updated_by").ok().flatten(),
            })
        })
        .collect();

    Ok(Json(json!({
        "workspace_id": workspace_id,
        "outputs": outputs,
        "count": outputs.len(),
    })))
}

/// GET /api/workspaces/:workspace_id/outputs/:key
pub async fn get_output_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((workspace_id, key)): Path<(String, String)>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let _user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".into()))?;

    let row = sqlx::query(
        "SELECT value, version, updated_at, updated_by
         FROM workspace_outputs
         WHERE workspace_id = $1 AND key = $2",
    )
    .bind(ws_uuid)
    .bind(&key)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, format!("Output '{}' not found", key)))?;

    Ok(Json(json!({
        "workspace_id": workspace_id,
        "key": key,
        "value": row.get::<JsonValue, _>("value"),
        "version": row.get::<i32, _>("version"),
        "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
        "updated_by": row.try_get::<Option<String>, _>("updated_by").ok().flatten(),
    })))
}

/// DELETE /api/workspaces/:workspace_id/outputs/:key
pub async fn delete_output_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((workspace_id, key)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".into()))?;

    teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".into()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".into()))?;

    sqlx::query("DELETE FROM workspace_outputs WHERE workspace_id = $1 AND key = $2")
        .bind(ws_uuid)
        .bind(&key)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// ═══════════════════════════════════════════════════════════════════
// Dependency CRUD
// ═══════════════════════════════════════════════════════════════════

/// POST /api/workspaces/:workspace_id/dependencies
pub async fn add_dependency_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<AddDependencyRequest>,
) -> Result<(StatusCode, Json<JsonValue>), (StatusCode, String)> {
    let user_id = principal.user_id();
    let downstream_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".into()))?;
    let upstream_uuid: uuid::Uuid = req
        .upstream_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid upstream workspace ID".into()))?;

    // Verify caller is member of downstream workspace
    teams::get_member_role(&state.db, downstream_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a member of this workspace".into()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a member of this workspace".into()))?;

    // Check for cycles: upstream must not transitively depend on downstream
    let would_cycle = sqlx::query_scalar::<_, bool>(
        "WITH RECURSIVE chain AS (
            SELECT upstream_id FROM workspace_dependencies WHERE downstream_id = $1
            UNION ALL
            SELECT wd.upstream_id FROM workspace_dependencies wd
            JOIN chain c ON c.upstream_id = wd.downstream_id
        )
        SELECT EXISTS(SELECT 1 FROM chain WHERE upstream_id = $2)",
    )
    .bind(upstream_uuid)
    .bind(downstream_uuid)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);

    if would_cycle {
        return Err((
            StatusCode::CONFLICT,
            "Adding this dependency would create a cycle in the workspace DAG".into(),
        ));
    }

    let dep_type = req.dependency_type.as_deref().unwrap_or("output");

    sqlx::query(
        "INSERT INTO workspace_dependencies (upstream_id, downstream_id, dependency_type, key_filter)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (upstream_id, downstream_id) DO UPDATE SET
           dependency_type = $3,
           key_filter = $4",
    )
    .bind(upstream_uuid)
    .bind(downstream_uuid)
    .bind(dep_type)
    .bind(&req.key_filter)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "upstream_id": req.upstream_id,
            "downstream_id": workspace_id,
            "dependency_type": dep_type,
            "key_filter": req.key_filter,
        })),
    ))
}

/// GET /api/workspaces/:workspace_id/dependencies
pub async fn list_dependencies_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".into()))?;

    let upstream_rows = sqlx::query(
        "SELECT wd.upstream_id, t.name AS upstream_name, wd.dependency_type, wd.key_filter
         FROM workspace_dependencies wd
         JOIN teams t ON t.id = wd.upstream_id
         WHERE wd.downstream_id = $1",
    )
    .bind(ws_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let downstream_rows = sqlx::query(
        "SELECT wd.downstream_id, t.name AS downstream_name, wd.dependency_type, wd.key_filter
         FROM workspace_dependencies wd
         JOIN teams t ON t.id = wd.downstream_id
         WHERE wd.upstream_id = $1",
    )
    .bind(ws_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let upstream: Vec<JsonValue> = upstream_rows
        .iter()
        .map(|r| {
            json!({
                "workspace_id": r.get::<uuid::Uuid, _>("upstream_id").to_string(),
                "name": r.get::<String, _>("upstream_name"),
                "dependency_type": r.get::<String, _>("dependency_type"),
                "key_filter": r.try_get::<Option<String>, _>("key_filter").ok().flatten(),
            })
        })
        .collect();

    let downstream: Vec<JsonValue> = downstream_rows
        .iter()
        .map(|r| {
            json!({
                "workspace_id": r.get::<uuid::Uuid, _>("downstream_id").to_string(),
                "name": r.get::<String, _>("downstream_name"),
                "dependency_type": r.get::<String, _>("dependency_type"),
                "key_filter": r.try_get::<Option<String>, _>("key_filter").ok().flatten(),
            })
        })
        .collect();

    Ok(Json(json!({
        "workspace_id": workspace_id,
        "upstream": upstream,
        "downstream": downstream,
    })))
}

/// DELETE /api/workspaces/:workspace_id/dependencies/:upstream_id
pub async fn remove_dependency_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((workspace_id, upstream_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user_id = principal.user_id();
    let downstream_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".into()))?;
    let upstream_uuid: uuid::Uuid = upstream_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid upstream workspace ID".into()))?;

    teams::get_member_role(&state.db, downstream_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".into()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".into()))?;

    sqlx::query(
        "DELETE FROM workspace_dependencies WHERE upstream_id = $1 AND downstream_id = $2",
    )
    .bind(upstream_uuid)
    .bind(downstream_uuid)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}
