//! Admin handlers — stats, user management, agent flagging, waitlist.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use fermi_auth::{credit_grant, get_or_create_wallet, AuthPrincipal};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::{create_notification, AppState};
// ─── Admin API handlers ────────────────────────────────────────────

pub fn require_admin(principal: &AuthPrincipal) -> Result<(), (StatusCode, String)> {
    if !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Admin access required".into()));
    }
    Ok(())
}

pub async fn admin_stats_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    let total_users: i64 = sqlx::query("SELECT COUNT(*) as cnt FROM users")
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .try_get("cnt")
        .unwrap_or(0);

    let total_agents: i64 =
        sqlx::query("SELECT COUNT(*) as cnt FROM agents WHERE agent_name NOT LIKE 'test_agent_%'")
            .fetch_one(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .try_get("cnt")
            .unwrap_or(0);

    let total_episodes: i64 = sqlx::query("SELECT COUNT(*) as cnt FROM episodes")
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .try_get("cnt")
        .unwrap_or(0);

    let total_credits: i64 = sqlx::query("SELECT COALESCE(SUM(balance), 0) as total FROM wallets")
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .try_get("total")
        .unwrap_or(0);

    let total_spent: i64 =
        sqlx::query("SELECT COALESCE(SUM(total_spent), 0) as total FROM wallets")
            .fetch_one(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .try_get("total")
            .unwrap_or(0);

    let recent_txs: i64 = sqlx::query(
        "SELECT COUNT(*) as cnt FROM credit_ledger WHERE created_at > NOW() - INTERVAL '24 hours'",
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .try_get("cnt")
    .unwrap_or(0);

    let episodes_with_embeddings: i64 =
        sqlx::query("SELECT COUNT(*) as cnt FROM episodes WHERE embedding IS NOT NULL")
            .fetch_one(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .try_get("cnt")
            .unwrap_or(0);

    // Rabble stats
    let total_creatures: i64 = sqlx::query("SELECT COUNT(*) as cnt FROM creatures")
        .fetch_one(&state.db)
        .await
        .map(|r| r.try_get("cnt").unwrap_or(0))
        .unwrap_or(0);

    let active_rabbles: i64 =
        sqlx::query("SELECT COUNT(*) as cnt FROM swarm_events WHERE status = 'active'")
            .fetch_one(&state.db)
            .await
            .map(|r| r.try_get("cnt").unwrap_or(0))
            .unwrap_or(0);

    let total_rabble_messages: i64 = sqlx::query("SELECT COUNT(*) as cnt FROM rabble_messages")
        .fetch_one(&state.db)
        .await
        .map(|r| r.try_get("cnt").unwrap_or(0))
        .unwrap_or(0);

    Ok(Json(json!({
        "total_users": total_users,
        "total_agents": total_agents,
        "total_episodes": total_episodes,
        "credits_in_circulation": total_credits,
        "credits_total_spent": total_spent,
        "transactions_24h": recent_txs,
        "embeddings": {
            "provider": state.embedder.provider_name(),
            "dimension": state.embedder.dimension(),
            "episodes_with_embeddings": episodes_with_embeddings,
            "episodes_without_embeddings": total_episodes - episodes_with_embeddings,
        },
        "rabble": {
            "total_creatures": total_creatures,
            "active_rabbles": active_rabbles,
            "total_rabble_messages": total_rabble_messages,
        }
    })))
}

#[derive(Debug, Deserialize)]
pub struct AdminSearchParams {
    search: Option<String>,
    page: Option<i64>,
    limit: Option<i64>,
}

pub async fn admin_list_users_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(params): Query<AdminSearchParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = (params.page.unwrap_or(1).max(1) - 1) * limit;

    let rows = if let Some(ref search) = params.search {
        let q = format!("%{}%", search);
        sqlx::query(
            "SELECT user_id, email, display_name, role, auth_provider, created_at
             FROM users WHERE user_id ILIKE $1 OR email ILIKE $1 OR display_name ILIKE $1
             ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(&q)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query(
            "SELECT user_id, email, display_name, role, auth_provider, created_at
             FROM users ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await
    }
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let users: Vec<Value> = rows
        .iter()
        .map(|r| {
            // Get wallet balance
            json!({
                "user_id": r.try_get::<String, _>("user_id").unwrap_or_default(),
                "email": r.try_get::<String, _>("email").unwrap_or_default(),
                "display_name": r.try_get::<Option<String>, _>("display_name").unwrap_or(None),
                "role": r.try_get::<String, _>("role").unwrap_or_default(),
                "auth_provider": r.try_get::<String, _>("auth_provider").unwrap_or_default(),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok(),
            })
        })
        .collect();

    Ok(Json(json!({ "users": users })))
}

#[derive(Debug, Deserialize)]
pub struct GrantCreditsRequest {
    credits: i32,
    reason: Option<String>,
}

pub async fn admin_grant_credits_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(target_user_id): Path<String>,
    Json(body): Json<GrantCreditsRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    let credits = body.credits.max(1).min(10000);
    let reason = body.reason.unwrap_or_else(|| "Admin grant".to_string());

    let wallet = get_or_create_wallet(&state.db, "user", &target_user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    credit_grant(&state.db, wallet.wallet_id, credits, &reason)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Look up display name for the response
    let display_name: Option<String> =
        sqlx::query("SELECT COALESCE(display_name, email) as name FROM users WHERE user_id = $1")
            .bind(&target_user_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .and_then(|r| r.try_get("name").ok());

    // Notify the user
    create_notification(
        &state.db,
        &target_user_id,
        "system",
        &format!("You received {} credits", credits),
        Some(&reason),
    )
    .await;

    Ok(Json(json!({
        "status": "granted",
        "user_id": target_user_id,
        "display_name": display_name,
        "credits": credits,
        "reason": reason,
    })))
}

pub async fn admin_list_agents_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(params): Query<AdminSearchParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    let agents = state
        .memory_store
        .list_agents()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut filtered: Vec<_> = agents
        .into_iter()
        .filter(|a| !a.agent_name.starts_with("test_agent_"))
        .collect();

    if let Some(ref search) = params.search {
        let q = search.to_lowercase();
        filtered.retain(|a| {
            a.agent_name.to_lowercase().contains(&q)
                || a.display_alias
                    .as_deref()
                    .map(|d| d.to_lowercase().contains(&q))
                    .unwrap_or(false)
                || a.owner_id
                    .as_deref()
                    .map(|o| o.to_lowercase().contains(&q))
                    .unwrap_or(false)
        });
    }

    let agents_json: Vec<Value> = filtered
        .iter()
        .map(|a| {
            json!({
                "agent_name": a.agent_name,
                "display_alias": a.display_alias,
                "owner_id": a.owner_id,
                "visibility": a.visibility,
                "total_executions": a.total_executions,
                "tier": a.tier,
                "model": a.model,
            })
        })
        .collect();

    Ok(Json(
        json!({ "agents": agents_json, "total": agents_json.len() }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct FlagAgentRequest {
    visibility: String, // "hidden" or "public"
}

pub async fn admin_flag_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(body): Json<FlagAgentRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    sqlx::query("UPDATE agents SET visibility = $1 WHERE agent_name = $2")
        .bind(&body.visibility)
        .bind(&agent_id)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "status": "updated",
        "agent_id": agent_id,
        "visibility": body.visibility,
    })))
}

// ─── Waitlist Management ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WaitlistParams {
    pub status: Option<String>,
    pub search: Option<String>,
}

pub async fn admin_list_waitlist_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(params): Query<WaitlistParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    let rows = if let Some(ref search) = params.search {
        let q = format!("%{}%", search);
        sqlx::query(
            "SELECT id, email, source, status, notes, created_at, invited_at
             FROM waitlist WHERE email ILIKE $1
             ORDER BY created_at DESC",
        )
        .bind(&q)
        .fetch_all(&state.db)
        .await
    } else if let Some(ref status) = params.status {
        sqlx::query(
            "SELECT id, email, source, status, notes, created_at, invited_at
             FROM waitlist WHERE status = $1
             ORDER BY created_at DESC",
        )
        .bind(status)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query(
            "SELECT id, email, source, status, notes, created_at, invited_at
             FROM waitlist ORDER BY created_at DESC",
        )
        .fetch_all(&state.db)
        .await
    }
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let entries: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.try_get::<uuid::Uuid, _>("id").ok(),
                "email": r.try_get::<String, _>("email").unwrap_or_default(),
                "source": r.try_get::<Option<String>, _>("source").unwrap_or(None),
                "status": r.try_get::<Option<String>, _>("status").unwrap_or(Some("pending".into())),
                "notes": r.try_get::<Option<String>, _>("notes").unwrap_or(None),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok(),
                "invited_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("invited_at").unwrap_or(None),
            })
        })
        .collect();

    let total = entries.len();
    let pending = entries.iter().filter(|e| e["status"] == "pending").count();
    let invited = entries.iter().filter(|e| e["status"] == "invited").count();

    Ok(Json(json!({
        "entries": entries,
        "total": total,
        "pending": pending,
        "invited": invited,
    })))
}

#[derive(Debug, Deserialize)]
pub struct InviteRequest {
    pub emails: Vec<String>,
    pub notes: Option<String>,
}

pub async fn admin_invite_waitlist_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<InviteRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    let notes = body.notes.unwrap_or_else(|| "Invited by admin".to_string());
    let mut invited = 0;

    for email in &body.emails {
        let result = sqlx::query(
            "UPDATE waitlist SET status = 'invited', invited_at = NOW(), notes = $1
             WHERE email = $2 AND status = 'pending'",
        )
        .bind(&notes)
        .bind(email.trim().to_lowercase())
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        invited += result.rows_affected();
    }

    Ok(Json(json!({
        "status": "ok",
        "invited": invited,
        "total_requested": body.emails.len(),
    })))
}

#[derive(Debug, Deserialize)]
pub struct AddWaitlistRequest {
    pub email: String,
    pub notes: Option<String>,
}

pub async fn admin_add_waitlist_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<AddWaitlistRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    let email = body.email.trim().to_lowercase();
    if !email.contains('@') || email.len() < 5 {
        return Err((StatusCode::BAD_REQUEST, "Invalid email".into()));
    }

    sqlx::query(
        "INSERT INTO waitlist (email, source, notes) VALUES ($1, 'admin', $2)
         ON CONFLICT (email) DO UPDATE SET notes = COALESCE($2, waitlist.notes)",
    )
    .bind(&email)
    .bind(&body.notes)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "status": "ok",
        "email": email,
    })))
}

pub async fn admin_delete_waitlist_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(entry_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    sqlx::query("DELETE FROM waitlist WHERE id = $1")
        .bind(entry_id)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "status": "deleted" })))
}
