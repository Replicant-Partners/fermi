//! Group CRUD — create, read, update, archive relationship groups.
//!
//! Spec 25 §6.1.
//!
//! Also hosts `preview_group_propagation_handler`
//! (Phase 2.5 Slice B) — the dry-run propagate endpoint that powers
//! the cascade detail panel's "what if I resolve this member NO?"
//! preview. Lives here because it's a group-scoped read operation on
//! the same primitive; keeping it in one file mirrors how the CRUD
//! endpoints are laid out.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use fermi_auth::AuthPrincipal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sqlx::Row;

use super::propagation::{dispatch_propagation_group, PropagateRequest, PropagateResult};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateGroupRequest {
    pub group_id: String,
    pub kind: String,
    #[serde(default)]
    pub parameters: JsonValue,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PatchGroupRequest {
    pub kind: Option<String>,
    pub parameters: Option<JsonValue>,
    pub description: Option<String>,
}

pub async fn create_group_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreateGroupRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();

    let valid_kinds = ["mutex", "at_most_n", "implies"];
    if !valid_kinds.contains(&req.kind.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Unknown group kind '{}'. Valid: mutex, at_most_n, implies",
                req.kind
            ),
        ));
    }

    if req.kind == "at_most_n" && req.parameters.get("n").is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            "at_most_n requires parameters.n".into(),
        ));
    }

    if req.kind == "implies"
        && (req.parameters.get("antecedent").is_none()
            || req.parameters.get("consequent").is_none())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "implies requires parameters.antecedent and parameters.consequent".into(),
        ));
    }

    let row = sqlx::query(
        "INSERT INTO public.forecast_relationship_groups
              (group_id, kind, parameters, description, owner_id)
          VALUES ($1, $2, $3, $4, $5)
          ON CONFLICT (group_id) DO NOTHING
          RETURNING group_id, kind, parameters, description, created_at",
    )
    .bind(&req.group_id)
    .bind(&req.kind)
    .bind(&req.parameters)
    .bind(&req.description)
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = match row {
        Some(r) => r,
        None => {
            return Err((
                StatusCode::CONFLICT,
                format!("Group '{}' already exists", req.group_id),
            ));
        }
    };

    Ok(Json(json!({
        "group_id": row.try_get::<String, _>("group_id").ok(),
        "kind": row.try_get::<String, _>("kind").ok(),
        "parameters": row.try_get::<JsonValue, _>("parameters").ok(),
        "description": row.try_get::<Option<String>, _>("description").ok().flatten(),
        "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
    })))
}

pub async fn list_groups_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();

    let rows = sqlx::query(
        "SELECT frg.group_id, frg.kind, frg.parameters, frg.description,
                frg.created_at, frg.updated_at, frg.archived_at,
                (SELECT COUNT(*) FROM public.fermi_forecasts ff
                 WHERE ff.relationship_groups @> ARRAY[frg.group_id]
                   AND (ff.status IS NULL OR ff.status != 'archived')) AS member_count
          FROM public.forecast_relationship_groups frg
          WHERE frg.owner_id = $1
          ORDER BY frg.created_at DESC",
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let groups: Vec<JsonValue> = rows
        .iter()
        .map(|r| {
            json!({
                "group_id": r.try_get::<String, _>("group_id").ok(),
                "kind": r.try_get::<String, _>("kind").ok(),
                "parameters": r.try_get::<JsonValue, _>("parameters").ok(),
                "description": r.try_get::<Option<String>, _>("description").ok().flatten(),
                "member_count": r.try_get::<i64, _>("member_count").ok(),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
                "updated_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at").ok().map(|t| t.to_rfc3339()),
                "archived_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("archived_at").ok().flatten().map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "groups": groups,
        "count": groups.len(),
    })))
}

pub async fn get_group_handler(
    Path(group_id): Path<String>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();

    let row = sqlx::query(
        "SELECT frg.group_id, frg.kind, frg.parameters, frg.description,
                frg.owner_id, frg.created_at, frg.updated_at, frg.archived_at
          FROM public.forecast_relationship_groups frg
          WHERE frg.group_id = $1 AND frg.archived_at IS NULL",
    )
    .bind(&group_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = match row {
        Some(r) => r,
        None => return Err((StatusCode::NOT_FOUND, "Group not found".into())),
    };

    let owner: String = row.try_get("owner_id").unwrap_or_default();
    if owner != user_id && !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Not your group".into()));
    }

    let members = super::membership::get_group_members(&group_id, &state.db).await?;

    Ok(Json(json!({
        "group": {
            "group_id": row.try_get::<String, _>("group_id").ok(),
            "kind": row.try_get::<String, _>("kind").ok(),
            "parameters": row.try_get::<JsonValue, _>("parameters").ok(),
            "description": row.try_get::<Option<String>, _>("description").ok().flatten(),
            "owner_id": owner,
            "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
            "updated_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at").ok().map(|t| t.to_rfc3339()),
        },
        "member_count": members.len(),
        "members": members,
    })))
}

pub async fn get_group_members_handler(
    Path(group_id): Path<String>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let _user_id = principal.user_id().to_string();

    let group_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM public.forecast_relationship_groups WHERE group_id = $1 AND archived_at IS NULL)",
    )
    .bind(&group_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !group_exists {
        return Err((StatusCode::NOT_FOUND, "Group not found".into()));
    }

    let members = super::membership::get_group_members(&group_id, &state.db).await?;

    Ok(Json(json!({
        "members": members,
        "count": members.len(),
    })))
}

pub async fn patch_group_handler(
    Path(group_id): Path<String>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<PatchGroupRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();

    let row = sqlx::query(
        "SELECT owner_id, kind, parameters, description
          FROM public.forecast_relationship_groups
          WHERE group_id = $1 AND archived_at IS NULL",
    )
    .bind(&group_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = match row {
        Some(r) => r,
        None => return Err((StatusCode::NOT_FOUND, "Group not found".into())),
    };

    let owner: String = row.try_get("owner_id").unwrap_or_default();
    if owner != user_id && !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Not your group".into()));
    }

    let current_kind: String = row.try_get("kind").unwrap_or_default();
    let new_kind = req.kind.as_deref().unwrap_or(&current_kind);

    let valid_kinds = ["mutex", "at_most_n", "implies"];
    if !valid_kinds.contains(&new_kind) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Unknown group kind '{}'. Valid: mutex, at_most_n, implies",
                new_kind
            ),
        ));
    }

    let current_params: JsonValue = row.try_get("parameters").unwrap_or(json!({}));
    let new_params = req.parameters.as_ref().unwrap_or(&current_params);

    let current_desc: Option<String> = row.try_get("description").ok().flatten();
    let new_desc = req.description.as_deref().or(current_desc.as_deref());

    let result = sqlx::query(
        "UPDATE public.forecast_relationship_groups
          SET kind = $2, parameters = $3, description = $4, updated_at = NOW()
          WHERE group_id = $1",
    )
    .bind(&group_id)
    .bind(new_kind)
    .bind(new_params)
    .bind(new_desc)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Group not found".into()));
    }

    Ok(Json(json!({
        "group_id": group_id,
        "kind": new_kind,
        "parameters": new_params,
        "description": new_desc,
        "updated": true,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/relationship-groups/:group_id/propagate
//
// Group-scoped propagation endpoint. Two use cases:
//   * dry_run=true (default) — the cascade detail panel's preview:
//     "if I resolve <trigger_forecast_id> as YES/NO, here's how the
//     other members shift." Zero side effects; returns a
//     `PropagateResult` with the proposed `deltas`. This is the
//     read-side of the cascade authoring surface.
//   * dry_run=false — direct apply, bypassing the pending_cascades
//     queue. Kept for symmetry with the legacy propagate route and
//     for CLI tooling; the normal apply flow still routes through
//     /api/pending-cascades/:id/apply, which enforces the operator-
//     gate. Callers who bypass do so knowingly.
//
// This endpoint does NOT queue a pending_cascade; it just executes
// `dispatch_propagation_group` on the caller's behalf. The read-only
// (dry_run=true) path is safe to call repeatedly.
//
// Auth: caller must own the group (or be admin), same as the CRUD
// handlers above. Prevents leaking probability shifts on other
// operators' groups.

#[derive(Debug, Deserialize)]
pub struct PreviewPropagateRequest {
    /// The forecast whose resolution / update we're simulating.
    /// Must be a member of the target group (server validates).
    pub trigger_forecast_id: String,
    /// "resolved" (outcome fixed to true/false) or "updated" (soft
    /// probability shift). Matches PropagateRequest semantics.
    pub trigger_kind: String,
    /// For trigger_kind="resolved": the outcome (true=YES, false=NO).
    /// Ignored for "updated".
    pub outcome: Option<bool>,
    /// Default true — previews are the common case. Set false only
    /// when explicitly bypassing the pending_cascades operator-gate
    /// (CLI tooling, admin one-shots).
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
}

fn default_dry_run() -> bool {
    true
}

pub async fn preview_group_propagation_handler(
    Path(group_id): Path<String>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<PreviewPropagateRequest>,
) -> Result<Json<PropagateResult>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();

    // Group + auth. Same shape as get_group_handler; a preview leaks
    // the group's kind + parameters + members, so it must be
    // owner-gated.
    let row = sqlx::query(
        "SELECT kind, parameters, owner_id
          FROM public.forecast_relationship_groups
          WHERE group_id = $1 AND archived_at IS NULL",
    )
    .bind(&group_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Group not found".into()))?;

    let owner: String = row.try_get("owner_id").unwrap_or_default();
    if owner != user_id && !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Not your group".into()));
    }
    let kind: String = row.try_get("kind").unwrap_or_default();
    let parameters: JsonValue = row.try_get("parameters").unwrap_or(JsonValue::Null);

    let prop_req = PropagateRequest {
        trigger_forecast_id: req.trigger_forecast_id.clone(),
        trigger_kind: req.trigger_kind,
        outcome: req.outcome,
    };

    dispatch_propagation_group(
        &kind,
        &group_id,
        &parameters,
        &prop_req,
        &state.db,
        req.dry_run,
    )
    .await
    .map(Json)
}

pub async fn delete_group_handler(
    Path(group_id): Path<String>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();

    let result = sqlx::query(
        "UPDATE public.forecast_relationship_groups
          SET archived_at = NOW(), updated_at = NOW()
          WHERE group_id = $1 AND owner_id = $2 AND archived_at IS NULL",
    )
    .bind(&group_id)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Group not found or already archived".into(),
        ));
    }

    Ok(Json(json!({ "archived": true, "group_id": group_id })))
}
