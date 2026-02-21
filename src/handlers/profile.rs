//! Profile and notification handlers.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Html,
    Json,
};
use fermi_auth::{get_or_create_wallet, AuthPrincipal};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::AppState;
// ─── Profile page ──────────────────────────────────────────────────

pub async fn profile_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/profile.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/profile.html: {}", e);
            format!("<h1>Profile</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

// ─── Profile API ───────────────────────────────────────────────────

pub async fn get_profile_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Fetch user record — try with bio column, fall back without if column doesn't exist yet
    let user_row = match sqlx::query(
        "SELECT user_id, email, display_name, avatar_url, role, auth_provider,
                github_username, ethereum_address, ens_name, bio, created_at
         FROM users WHERE user_id = $1",
    )
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(row) => row,
        Err(_) => {
            // bio column may not exist yet — retry without it
            sqlx::query(
                "SELECT user_id, email, display_name, avatar_url, role, auth_provider,
                        github_username, ethereum_address, ens_name, created_at
                 FROM users WHERE user_id = $1",
            )
            .bind(&user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        }
    }
    .ok_or((StatusCode::NOT_FOUND, "User not found".to_string()))?;

    // Wallet
    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Agent count + execution stats
    let stats_row = sqlx::query(
        "SELECT COUNT(*) as agent_count,
                COALESCE(SUM(total_executions), 0) as total_executions
         FROM agents WHERE user_id = $1",
    )
    .bind(&user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Public agents
    let public_agents = sqlx::query(
        "SELECT agent_name, display_alias, agent_type, description, total_executions
         FROM agents WHERE user_id = $1 AND visibility = 'public'
         ORDER BY total_executions DESC LIMIT 20",
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let agents: Vec<Value> = public_agents
        .iter()
        .map(|r| {
            json!({
                "agent_name": r.try_get::<String, _>("agent_name").unwrap_or_default(),
                "display_alias": r.try_get::<Option<String>, _>("display_alias").unwrap_or(None),
                "agent_type": r.try_get::<String, _>("agent_type").unwrap_or_default(),
                "description": r.try_get::<Option<String>, _>("description").unwrap_or(None),
                "total_executions": r.try_get::<i32, _>("total_executions").unwrap_or(0),
            })
        })
        .collect();

    let auth_provider: Option<String> = user_row.try_get("auth_provider").unwrap_or(None);
    let github_username: Option<String> = user_row.try_get("github_username").unwrap_or(None);
    let ethereum_address: Option<String> = user_row.try_get("ethereum_address").unwrap_or(None);

    Ok(Json(json!({
        "user_id": user_row.try_get::<String, _>("user_id").unwrap_or_default(),
        "email": user_row.try_get::<Option<String>, _>("email").unwrap_or(None),
        "display_name": user_row.try_get::<Option<String>, _>("display_name").unwrap_or(None),
        "avatar_url": user_row.try_get::<Option<String>, _>("avatar_url").unwrap_or(None),
        "role": user_row.try_get::<String, _>("role").unwrap_or_else(|_| "user".to_string()),
        "auth_provider": &auth_provider,
        "github_username": &github_username,
        "ethereum_address": &ethereum_address,
        "ens_name": user_row.try_get::<Option<String>, _>("ens_name").unwrap_or(None),
        "bio": user_row.try_get::<Option<String>, _>("bio").unwrap_or(None),
        "created_at": user_row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok(),
        "wallet": {
            "balance": wallet.balance,
            "total_deposited": wallet.total_deposited,
            "total_spent": wallet.total_spent,
        },
        "stats": {
            "agents_created": stats_row.try_get::<i64, _>("agent_count").unwrap_or(0),
            "total_executions": stats_row.try_get::<i64, _>("total_executions").unwrap_or(0),
            "credits_spent": wallet.total_spent,
        },
        "public_agents": agents,
        "connected_accounts": {
            "google": auth_provider.as_deref() == Some("google"),
            "github": github_username.is_some(),
            "ethereum": ethereum_address.is_some(),
        },
    })))
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    display_name: Option<String>,
    bio: Option<String>,
    avatar_url: Option<String>,
}

pub async fn update_profile_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<UpdateProfileRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    if let Some(ref name) = req.display_name {
        sqlx::query("UPDATE users SET display_name = $1 WHERE user_id = $2")
            .bind(name)
            .bind(&user_id)
            .execute(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    if let Some(ref bio) = req.bio {
        // Silently ignore if bio column doesn't exist yet (migration 029)
        let _ = sqlx::query("UPDATE users SET bio = $1 WHERE user_id = $2")
            .bind(bio)
            .bind(&user_id)
            .execute(&state.db)
            .await;
    }

    if let Some(ref avatar) = req.avatar_url {
        let url = avatar.trim();
        if !url.is_empty() && !url.starts_with("https://") {
            return Err((StatusCode::BAD_REQUEST, "Avatar URL must use HTTPS".into()));
        }
        let val: Option<&str> = if url.is_empty() { None } else { Some(url) };
        sqlx::query("UPDATE users SET avatar_url = $1 WHERE user_id = $2")
            .bind(val)
            .bind(&user_id)
            .execute(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    Ok(Json(json!({ "message": "Profile updated" })))
}

// ─── Notifications ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NotificationsParams {
    unread: Option<bool>,
    limit: Option<i64>,
}

pub async fn list_notifications_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(params): Query<NotificationsParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let limit = params.limit.unwrap_or(20).min(100);

    let rows = if params.unread.unwrap_or(false) {
        sqlx::query(
            "SELECT id, type, title, message, read, metadata, created_at FROM notifications
             WHERE user_id = $1 AND read = FALSE ORDER BY created_at DESC LIMIT $2",
        )
        .bind(&user_id)
        .bind(limit)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query(
            "SELECT id, type, title, message, read, metadata, created_at FROM notifications
             WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(&user_id)
        .bind(limit)
        .fetch_all(&state.db)
        .await
    };

    // If notifications table doesn't exist yet, return empty
    let rows = match rows {
        Ok(r) => r,
        Err(_) => vec![],
    };

    let unread_count: i64 = sqlx::query(
        "SELECT COUNT(*) as cnt FROM notifications WHERE user_id = $1 AND read = FALSE",
    )
    .bind(&user_id)
    .fetch_one(&state.db)
    .await
    .ok()
    .and_then(|r| r.try_get("cnt").ok())
    .unwrap_or(0);

    let notifications: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.try_get::<uuid::Uuid, _>("id").unwrap_or_default(),
                "type": r.try_get::<String, _>("type").unwrap_or_default(),
                "title": r.try_get::<String, _>("title").unwrap_or_default(),
                "message": r.try_get::<Option<String>, _>("message").unwrap_or(None),
                "read": r.try_get::<bool, _>("read").unwrap_or(false),
                "metadata": r.try_get::<Option<serde_json::Value>, _>("metadata").unwrap_or(None),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok(),
            })
        })
        .collect();

    Ok(Json(json!({
        "notifications": notifications,
        "unread_count": unread_count,
    })))
}

pub async fn mark_notification_read_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let _ = sqlx::query("UPDATE notifications SET read = TRUE WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(&principal.user_id())
        .execute(&state.db)
        .await;

    Ok(Json(json!({ "status": "read" })))
}

pub async fn mark_all_notifications_read_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let result =
        sqlx::query("UPDATE notifications SET read = TRUE WHERE user_id = $1 AND read = FALSE")
            .bind(&principal.user_id())
            .execute(&state.db)
            .await;

    let count = result.map(|r| r.rows_affected()).unwrap_or(0);

    Ok(Json(json!({
        "status": "all_read",
        "count": count,
    })))
}

// ─── User Secrets (Connections) ────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateSecretRequest {
    pub secret_name: String,
    pub value: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    pub label: Option<String>,
    pub description: Option<String>,
}

fn default_scope() -> String {
    "*".to_string()
}

pub async fn create_secret_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreateSecretRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let encryptor = state.secret_encryptor.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Secrets not configured".to_string(),
    ))?;

    let secret_id = fermi_auth::store_secret(
        &state.db,
        encryptor,
        &principal.user_id(),
        &req.secret_name,
        &req.value,
        &req.scope,
        req.label.as_deref(),
        req.description.as_deref(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "secret_id": secret_id,
        "secret_name": req.secret_name,
        "message": "Secret stored"
    })))
}

pub async fn list_secrets_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let secrets = fermi_auth::list_secrets(&state.db, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let list: Vec<Value> = secrets
        .iter()
        .map(|s| {
            json!({
                "secret_id": s.secret_id,
                "secret_name": s.secret_name,
                "scope": s.scope,
                "label": s.label,
                "description": s.description,
                "created_at": s.created_at,
                "updated_at": s.updated_at,
            })
        })
        .collect();

    Ok(Json(json!({ "secrets": list })))
}

pub async fn delete_secret_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(secret_name): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    fermi_auth::delete_secret(&state.db, &principal.user_id(), &secret_name)
        .await
        .map_err(|e| match e {
            fermi_auth::AuthError::SecretNotFound(_) => {
                (StatusCode::NOT_FOUND, "Secret not found".to_string())
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        })?;

    Ok(Json(json!({ "message": "Secret deleted" })))
}

#[derive(Debug, Deserialize)]
pub struct AuditLogParams {
    pub limit: Option<i64>,
}

pub async fn secret_audit_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(params): Query<AuditLogParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let limit = params.limit.unwrap_or(50).min(200);

    let entries = fermi_auth::get_secret_audit_log(&state.db, &principal.user_id(), limit)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let list: Vec<Value> = entries
        .iter()
        .map(|e| {
            json!({
                "log_id": e.log_id,
                "secret_name": e.secret_name,
                "agent_name": e.agent_name,
                "workspace_id": e.workspace_id,
                "action": e.action,
                "tool_name": e.tool_name,
                "created_at": e.created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "entries": list })))
}
