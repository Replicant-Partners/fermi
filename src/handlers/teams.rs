//! Team management and membership handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use fermi_auth::{
    credit_deposit, get_or_create_wallet, teams, AuthPrincipal, MemberType, ObjectType, Permission,
    ShareType, TeamRole,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::AppState;
// ─── Team management ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateTeamRequest {
    name: String,
    slug: String,
    description: Option<String>,
}

pub async fn create_team_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<CreateTeamRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    let team = teams::create_team(
        &state.db,
        &body.name,
        &body.slug,
        body.description.as_deref(),
        &principal.user_id(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Seed workspace with 100 starter credits
    let ws_id_str = team.id.to_string();
    let initial_credits: i32 = 100;
    if let Ok(ws_wallet) = get_or_create_wallet(&state.db, "workspace", &ws_id_str).await {
        if credit_deposit(
            &state.db,
            ws_wallet.wallet_id,
            initial_credits,
            "Workspace starter credits",
        )
        .await
        .is_ok()
        {
            let _ = sqlx::query("UPDATE teams SET workspace_budget = $1 WHERE id = $2")
                .bind(initial_credits)
                .bind(team.id)
                .execute(&state.db)
                .await;
        }
    }

    Ok((StatusCode::CREATED, Json(json!(team))))
}

pub async fn list_teams_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_teams = teams::get_user_teams(&state.db, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "teams": user_teams })))
}

pub async fn get_team_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(team_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Verify membership
    let role = teams::get_member_role(&state.db, team_id, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if role.is_none() && !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Not a team member".to_string()));
    }

    let team = teams::get_team(&state.db, team_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    let members = teams::get_team_members(&state.db, team_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "team": team,
        "members": members,
    })))
}

pub async fn delete_team_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(team_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    teams::delete_team(&state.db, team_id, &principal.user_id())
        .await
        .map_err(|e| match e {
            fermi_auth::AuthError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            other => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
        })?;

    Ok(Json(json!({ "status": "deleted" })))
}

// ─── Team membership ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    member_id: String,
    member_type: Option<String>,
    role: Option<String>,
}

pub async fn add_member_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(team_id): Path<uuid::Uuid>,
    Json(body): Json<AddMemberRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    // Check requester has invite permission (admin or owner)
    let requester_role = teams::get_member_role(&state.db, team_id, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a team member".to_string()))?;

    if !requester_role.can_invite() {
        return Err((
            StatusCode::FORBIDDEN,
            "Only admins and owners can invite members".to_string(),
        ));
    }

    let member_type = match body.member_type.as_deref() {
        Some("agent") => MemberType::Agent,
        _ => MemberType::User,
    };

    let role = match body.role.as_deref() {
        Some("admin") => TeamRole::Admin,
        Some("viewer") => TeamRole::Viewer,
        _ => TeamRole::Member,
    };

    teams::add_team_member(
        &state.db,
        team_id,
        member_type,
        &body.member_id,
        role,
        &principal.user_id(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Notify user-type members they were added
    if member_type == MemberType::User {
        let team_name: String = sqlx::query("SELECT name FROM teams WHERE id = $1")
            .bind(team_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .and_then(|r| r.try_get("name").ok())
            .unwrap_or_default();

        let inviter_name: String = sqlx::query("SELECT display_name FROM users WHERE user_id = $1")
            .bind(&principal.user_id())
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .and_then(|r| {
                r.try_get::<Option<String>, _>("display_name")
                    .ok()
                    .flatten()
            })
            .unwrap_or_else(|| principal.user_id());

        crate::create_notification(
            &state.db,
            &body.member_id,
            "workspace_invite",
            &format!("You were added to {}", team_name),
            Some(&format!(
                "{} added you to the workspace '{}'",
                inviter_name, team_name
            )),
        )
        .await;
    }

    Ok((StatusCode::CREATED, Json(json!({ "status": "added" }))))
}

pub async fn list_members_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(team_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Verify membership
    let role = teams::get_member_role(&state.db, team_id, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if role.is_none() && !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Not a team member".to_string()));
    }

    let members = teams::get_team_members(&state.db, team_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "members": members })))
}

pub async fn remove_member_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((team_id, member_id)): Path<(uuid::Uuid, String)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let requester_role = teams::get_member_role(&state.db, team_id, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a team member".to_string()))?;

    // Members can remove themselves; admins/owners can remove others
    if member_id != principal.user_id() && !requester_role.can_admin() {
        return Err((
            StatusCode::FORBIDDEN,
            "Only admins can remove other members".to_string(),
        ));
    }

    teams::remove_team_member(&state.db, team_id, &member_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "status": "removed" })))
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoleRequest {
    role: String,
}

pub async fn update_member_role_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((team_id, member_id)): Path<(uuid::Uuid, String)>,
    Json(body): Json<UpdateRoleRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let requester_role = teams::get_member_role(&state.db, team_id, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a team member".to_string()))?;

    if !requester_role.can_admin() {
        return Err((
            StatusCode::FORBIDDEN,
            "Only admins and owners can change roles".to_string(),
        ));
    }

    let new_role = TeamRole::from_str(&body.role);

    teams::update_member_role(&state.db, team_id, &member_id, new_role)
        .await
        .map_err(|e| match e {
            fermi_auth::AuthError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            other => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
        })?;

    Ok(Json(json!({ "status": "updated" })))
}

// ─── Object sharing ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ShareObjectRequest {
    object_type: String,
    object_id: String,
    share_type: String,
    share_target: String,
    permission: Option<String>,
}

pub async fn share_object_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<ShareObjectRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    let object_type = ObjectType::from_str(&body.object_type)
        .ok_or((StatusCode::BAD_REQUEST, "Invalid object_type".to_string()))?;

    let share_type = match body.share_type.as_str() {
        "team" => ShareType::Team,
        "user" => ShareType::User,
        _ => return Err((StatusCode::BAD_REQUEST, "Invalid share_type".to_string())),
    };

    let permission = match body.permission.as_deref() {
        Some("edit") => Permission::Edit,
        Some("admin") => Permission::Admin,
        _ => Permission::View,
    };

    let share = teams::share_object(
        &state.db,
        object_type,
        &body.object_id,
        share_type,
        &body.share_target,
        permission,
        &principal.user_id(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((StatusCode::CREATED, Json(json!(share))))
}

pub async fn revoke_share_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(share_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    teams::revoke_share(&state.db, share_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "status": "revoked" })))
}
