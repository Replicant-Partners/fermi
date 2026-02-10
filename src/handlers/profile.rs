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
         FROM agents WHERE owner_id = $1",
    )
    .bind(&user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Public agents
    let public_agents = sqlx::query(
        "SELECT agent_name, display_alias, agent_type, description, total_executions
         FROM agents WHERE owner_id = $1 AND visibility = 'public'
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
                "agent_name": r.get::<String, _>("agent_name"),
                "display_alias": r.get::<Option<String>, _>("display_alias"),
                "agent_type": r.get::<String, _>("agent_type"),
                "description": r.get::<Option<String>, _>("description"),
                "total_executions": r.get::<i32, _>("total_executions"),
            })
        })
        .collect();

    Ok(Json(json!({
        "user_id": user_row.get::<String, _>("user_id"),
        "email": user_row.get::<Option<String>, _>("email"),
        "display_name": user_row.get::<Option<String>, _>("display_name"),
        "avatar_url": user_row.get::<Option<String>, _>("avatar_url"),
        "role": user_row.get::<String, _>("role"),
        "auth_provider": user_row.get::<Option<String>, _>("auth_provider"),
        "github_username": user_row.get::<Option<String>, _>("github_username"),
        "ethereum_address": user_row.get::<Option<String>, _>("ethereum_address"),
        "ens_name": user_row.get::<Option<String>, _>("ens_name"),
        "bio": user_row.try_get::<Option<String>, _>("bio").unwrap_or(None),
        "created_at": user_row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "wallet": {
            "balance": wallet.balance,
            "total_deposited": wallet.total_deposited,
            "total_spent": wallet.total_spent,
        },
        "stats": {
            "agents_created": stats_row.get::<i64, _>("agent_count"),
            "total_executions": stats_row.get::<i64, _>("total_executions"),
            "credits_spent": wallet.total_spent,
        },
        "public_agents": agents,
        "connected_accounts": {
            "google": user_row.get::<Option<String>, _>("auth_provider").as_deref() == Some("google"),
            "github": user_row.get::<Option<String>, _>("github_username").is_some(),
            "ethereum": user_row.get::<Option<String>, _>("ethereum_address").is_some(),
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
            "SELECT id, type, title, message, read, created_at FROM notifications
             WHERE user_id = $1 AND read = FALSE ORDER BY created_at DESC LIMIT $2",
        )
        .bind(&user_id)
        .bind(limit)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query(
            "SELECT id, type, title, message, read, created_at FROM notifications
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
