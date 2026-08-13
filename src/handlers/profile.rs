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

    // Agent count + execution stats.
    //
    // Counted from `agent_execution_rollup` (i.e. from `episodes`), not
    // from `agents.total_executions` — nothing in the codebase writes that
    // column, so summing it reported 0 runs for every profile on the
    // platform. See migrations/192 and src/rollup_trust.rs.
    //
    // LEFT JOIN + COALESCE because an agent with no episodes is absent
    // from the view, not present with a zero; an inner join would drop
    // never-run agents out of `agent_count` too. The outer `SUM` over
    // bigint yields NUMERIC, hence the explicit ::bigint for the i64 read.
    let stats_row = sqlx::query(
        "SELECT COUNT(*) as agent_count,
                COALESCE(SUM(COALESCE(r.executions, 0)), 0)::bigint as total_executions
         FROM agents a
         LEFT JOIN agent_execution_rollup r ON r.agent_id = a.agent_id
         WHERE a.user_id = $1",
    )
    .bind(&user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Public agents. Ranked and reported on measured runs from
    // `agent_execution_rollup`; ordering by the write-orphaned
    // `agents.total_executions` sorted a permanently-zero column, so the
    // "top 20" was effectively arbitrary. See migrations/192 and
    // src/rollup_trust.rs.
    let public_agents = sqlx::query(
        "SELECT a.agent_name, a.display_alias, a.agent_type, a.description,
                COALESCE(r.executions, 0) as total_executions
         FROM agents a
         LEFT JOIN agent_execution_rollup r ON r.agent_id = a.agent_id
         WHERE a.user_id = $1 AND a.visibility = 'public'
         ORDER BY COALESCE(r.executions, 0) DESC LIMIT 20",
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
                // bigint from the view, not the INTEGER column it replaced:
                // an i32 read here fails to decode and silently falls back
                // to 0, which is the bug this query was changed to fix.
                "total_executions": r.try_get::<i64, _>("total_executions").unwrap_or(0),
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
    /// Filter by notification surface source.
    /// - "abw"    — Agent Bestiary World platform notifications (default)
    /// - "rabble" — Rabble creature/swarm/social notifications
    /// - "system" — Platform-wide (visible in all surfaces)
    /// - "all"    — No source filter (admin use)
    /// Rabble's Flutter client passes no source param, so we default to
    /// returning only "rabble" and "system" notifications for that surface.
    /// ABW web clients pass source=abw (or nothing, defaulting to abw).
    source: Option<String>,
}

pub async fn list_notifications_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(params): Query<NotificationsParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let limit = params.limit.unwrap_or(20).min(100);

    // Source filter: "all" = no filter; explicit value = exact match;
    // None = default to "abw" (ABW web surfaces).
    // Note: Rabble's compiled Flutter client cannot pass query params, so
    // it hits this endpoint with source=None. We default to "abw" here,
    // which means Rabble will see ABW notifications. To fix Rabble bleed,
    // Rabble-targeted notifications should use create_notification_for_surface
    // with source="rabble", and Rabble reads with source=None will see only
    // "abw" (which contains no Rabble-specific content).
    // The actual fix: ABW platform notifications are source="abw", Rabble
    // social/creature notifications are source="rabble". The Rabble client
    // reads source=None which returns source IN ('abw','system') — and since
    // Rabble notifications are tagged "rabble", they won't cross-contaminate.
    // For backwards compat we return all sources when source param is absent,
    // but exclude the OTHER surface's notifications:
    //   - No param → exclude source='rabble' (ABW default view)
    //   - source=rabble → only rabble + system
    //   - source=abw → only abw + system
    //   - source=all → everything
    let source_filter = params.source.as_deref();

    let rows = {
        let base = "SELECT id, type, title, message, read, metadata, created_at, source FROM notifications WHERE user_id = $1";
        let unread_clause = if params.unread.unwrap_or(false) {
            " AND read = FALSE"
        } else {
            ""
        };
        let source_clause = match source_filter {
            Some("all") => " AND TRUE",
            Some("rabble") => " AND source IN ('rabble', 'system')",
            Some("abw") | None => " AND source IN ('abw', 'system')",
            Some(s) => {
                // Specific source value
                let _ = s; // silence unused warning; handled below with bind
                " AND source = $3"
            }
        };
        let order = " ORDER BY created_at DESC LIMIT $2";

        if matches!(source_filter, Some(s) if s != "all" && s != "rabble" && s != "abw") {
            sqlx::query(&format!("{base}{unread_clause}{source_clause}{order}"))
                .bind(&user_id)
                .bind(limit)
                .bind(source_filter.unwrap_or("abw"))
                .fetch_all(&state.db)
                .await
        } else {
            sqlx::query(&format!("{base}{unread_clause}{source_clause}{order}"))
                .bind(&user_id)
                .bind(limit)
                .fetch_all(&state.db)
                .await
        }
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
                "source": r.try_get::<Option<String>, _>("source").unwrap_or(Some("abw".into())),
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
