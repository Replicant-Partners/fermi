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

use std::collections::HashMap;

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
            "total_flights": sqlx::query("SELECT COUNT(*) as cnt FROM creature_flights")
                .fetch_one(&state.db).await
                .map(|r| r.try_get::<i64, _>("cnt").unwrap_or(0)).unwrap_or(0),
            "total_contacts": sqlx::query("SELECT COUNT(*) as cnt FROM contacts")
                .fetch_one(&state.db).await
                .map(|r| r.try_get::<i64, _>("cnt").unwrap_or(0)).unwrap_or(0),
            "total_devices": sqlx::query("SELECT COUNT(*) as cnt FROM creature_devices")
                .fetch_one(&state.db).await
                .map(|r| r.try_get::<i64, _>("cnt").unwrap_or(0)).unwrap_or(0),
        },
        "gas_economics": build_gas_economics(&state.db).await,
    })))
}

/// Build gas economics report: execution vs platform_read breakdown,
/// top-read agents, read-to-execute ratios, hourly volume.
/// This is the system optimization signal — shows where value flows.
async fn build_gas_economics(pool: &sqlx::PgPool) -> Value {
    // Total by tx_type (execution_fee vs platform_read vs gas_fee etc)
    let tx_breakdown: Vec<Value> = sqlx::query(
        "SELECT tx_type, COUNT(*) as count, COALESCE(SUM(amount), 0) as total_credits
         FROM credit_ledger
         WHERE tx_type IN ('execution_fee', 'platform_read', 'gas_fee', 'execution_royalty',
                           'creature_mint', 'rabble_chat', 'creature_art', 'creature_flight')
         GROUP BY tx_type
         ORDER BY total_credits DESC",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .iter()
    .map(|r| {
        json!({
            "tx_type": r.try_get::<String, _>("tx_type").unwrap_or_default(),
            "count": r.try_get::<i64, _>("count").unwrap_or(0),
            "total_credits": r.try_get::<i64, _>("total_credits").unwrap_or(0),
        })
    })
    .collect();

    // Top agents by platform_read demand (which agent data do users pay to view?)
    let top_read_agents: Vec<Value> = sqlx::query(
        "SELECT cl.related_id as agent_ref, COUNT(*) as reads,
                COALESCE(a.agent_name, cl.related_id) as agent_name
         FROM credit_ledger cl
         LEFT JOIN agents a ON a.agent_id::text = cl.related_id
         WHERE cl.tx_type = 'platform_read' AND cl.related_id IS NOT NULL
         GROUP BY cl.related_id, a.agent_name
         ORDER BY reads DESC
         LIMIT 20",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .iter()
    .map(|r| {
        json!({
            "agent_name": r.try_get::<String, _>("agent_name").unwrap_or_default(),
            "reads": r.try_get::<i64, _>("reads").unwrap_or(0),
        })
    })
    .collect();

    // Read-to-execute ratio per agent (high = durable value, low = ephemeral)
    let read_exec_ratio: Vec<Value> = sqlx::query(
        "SELECT COALESCE(a.agent_name, cl.related_id) as agent_name,
                SUM(CASE WHEN cl.tx_type = 'platform_read' THEN 1 ELSE 0 END) as reads,
                SUM(CASE WHEN cl.tx_type = 'execution_fee' THEN 1 ELSE 0 END) as executions
         FROM credit_ledger cl
         LEFT JOIN agents a ON a.agent_id::text = cl.related_id
         WHERE cl.tx_type IN ('platform_read', 'execution_fee')
           AND cl.related_id IS NOT NULL
         GROUP BY COALESCE(a.agent_name, cl.related_id)
         HAVING SUM(CASE WHEN cl.tx_type = 'execution_fee' THEN 1 ELSE 0 END) > 0
         ORDER BY SUM(CASE WHEN cl.tx_type = 'platform_read' THEN 1 ELSE 0 END)::float /
                  NULLIF(SUM(CASE WHEN cl.tx_type = 'execution_fee' THEN 1 ELSE 0 END), 0) DESC
         LIMIT 20",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .iter()
    .map(|r| {
        let reads = r.try_get::<i64, _>("reads").unwrap_or(0);
        let execs = r.try_get::<i64, _>("executions").unwrap_or(0);
        let ratio = if execs > 0 {
            reads as f64 / execs as f64
        } else {
            0.0
        };
        json!({
            "agent_name": r.try_get::<String, _>("agent_name").unwrap_or_default(),
            "reads": reads,
            "executions": execs,
            "read_to_execute_ratio": (ratio * 100.0).round() / 100.0,
        })
    })
    .collect();

    // 24h hourly read volume (capacity planning)
    let hourly_reads: Vec<Value> = sqlx::query(
        "SELECT date_trunc('hour', created_at) as hour, COUNT(*) as reads
         FROM credit_ledger
         WHERE tx_type = 'platform_read' AND created_at > NOW() - INTERVAL '24 hours'
         GROUP BY hour
         ORDER BY hour ASC",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .iter()
    .map(|r| {
        json!({
            "hour": r.try_get::<chrono::DateTime<chrono::Utc>, _>("hour").ok(),
            "reads": r.try_get::<i64, _>("reads").unwrap_or(0),
        })
    })
    .collect();

    // Platform read totals
    let read_totals = sqlx::query(
        "SELECT COUNT(*) as total_reads,
                COALESCE(SUM(amount), 0) as total_revenue,
                COUNT(DISTINCT wallet_id) as unique_readers
         FROM credit_ledger
         WHERE tx_type = 'platform_read'",
    )
    .fetch_one(pool)
    .await;

    let (total_reads, total_revenue, unique_readers) = match read_totals {
        Ok(r) => (
            r.try_get::<i64, _>("total_reads").unwrap_or(0),
            r.try_get::<i64, _>("total_revenue").unwrap_or(0),
            r.try_get::<i64, _>("unique_readers").unwrap_or(0),
        ),
        Err(_) => (0, 0, 0),
    };

    // Agent royalty totals (how much agents have earned)
    let agent_earnings = sqlx::query(
        "SELECT COUNT(*) as total_payouts,
                COALESCE(SUM(amount), 0) as total_earned
         FROM agent_episode_payouts",
    )
    .fetch_one(pool)
    .await;

    let (total_payouts, total_earned) = match agent_earnings {
        Ok(r) => (
            r.try_get::<i64, _>("total_payouts").unwrap_or(0),
            r.try_get::<i64, _>("total_earned").unwrap_or(0),
        ),
        Err(_) => (0, 0),
    };

    json!({
        "platform_reads": {
            "total_reads": total_reads,
            "total_revenue_credits": total_revenue,
            "unique_readers": unique_readers,
            "hourly_volume_24h": hourly_reads,
            "top_read_agents": top_read_agents,
        },
        "agent_economics": {
            "total_royalty_payouts": total_payouts,
            "total_agent_earnings_credits": total_earned,
            "read_to_execute_ratios": read_exec_ratio,
        },
        "tx_breakdown": tx_breakdown,
    })
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
            "SELECT u.user_id, u.email, u.display_name, u.role, u.auth_provider, u.created_at,
                    COALESCE(w.balance, 0) as balance,
                    COALESCE(ac.cnt, 0) as agent_count
             FROM users u
             LEFT JOIN wallets w ON w.owner_type = 'user' AND w.owner_id = u.user_id
             LEFT JOIN (SELECT user_id, COUNT(*) as cnt FROM agents GROUP BY user_id) ac ON ac.user_id = u.user_id
             WHERE u.user_id ILIKE $1 OR u.email ILIKE $1 OR u.display_name ILIKE $1
             ORDER BY u.created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(&q)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query(
            "SELECT u.user_id, u.email, u.display_name, u.role, u.auth_provider, u.created_at,
                    COALESCE(w.balance, 0) as balance,
                    COALESCE(ac.cnt, 0) as agent_count
             FROM users u
             LEFT JOIN wallets w ON w.owner_type = 'user' AND w.owner_id = u.user_id
             LEFT JOIN (SELECT user_id, COUNT(*) as cnt FROM agents GROUP BY user_id) ac ON ac.user_id = u.user_id
             ORDER BY u.created_at DESC LIMIT $1 OFFSET $2",
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
            json!({
                "user_id": r.try_get::<String, _>("user_id").unwrap_or_default(),
                "email": r.try_get::<String, _>("email").unwrap_or_default(),
                "display_name": r.try_get::<Option<String>, _>("display_name").unwrap_or(None),
                "role": r.try_get::<String, _>("role").unwrap_or_default(),
                "auth_provider": r.try_get::<String, _>("auth_provider").unwrap_or_default(),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok(),
                "balance": r.try_get::<i32, _>("balance").unwrap_or(0),
                "agent_count": r.try_get::<i64, _>("agent_count").unwrap_or(0),
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

    // Batch-load owner display names
    let owner_ids: Vec<String> = filtered
        .iter()
        .filter_map(|a| a.owner_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let owner_names: HashMap<String, String> = if !owner_ids.is_empty() {
        sqlx::query(
            "SELECT user_id, COALESCE(display_name, email, user_id) as name FROM users WHERE user_id = ANY($1)",
        )
        .bind(&owner_ids)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .iter()
        .map(|r| {
            (
                r.get::<String, _>("user_id"),
                r.get::<String, _>("name"),
            )
        })
        .collect()
    } else {
        HashMap::new()
    };

    let agents_json: Vec<Value> = filtered
        .iter()
        .map(|a| {
            let owner_name = a
                .owner_id
                .as_deref()
                .and_then(|oid| owner_names.get(oid))
                .cloned();
            json!({
                "id": a.agent_name,
                "agent_id": a.agent_id,
                "agent_name": a.agent_name,
                "display_alias": a.display_alias,
                "owner_id": a.owner_id,
                "owner_display_name": owner_name,
                "visibility": a.visibility,
                "status": a.status,
                "execution_count": a.total_executions,
                "total_executions": a.total_executions,
                "tier": a.tier,
                "model": a.model,
                "description": a.description,
                "llm_provider": a.llm_provider,
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

// ─── Rabble admin handlers ─────────────────────────────────────────

/// GET /api/admin/creatures — list all creatures (paginated, searchable)
pub async fn admin_list_creatures_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(params): Query<AdminSearchParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = (params.page.unwrap_or(1).max(1) - 1) * limit;

    let (sql, binds) = if let Some(ref search) = params.search {
        (
            "SELECT c.creature_id, c.owner_id, c.specimen_name, c.scientific_name,
             c.species_group, c.status, c.flagged, c.flag_reason, c.total_flights,
             c.created_at, u.display_name as owner_name
             FROM creatures c LEFT JOIN users u ON u.user_id = c.owner_id
             WHERE c.specimen_name ILIKE $1 OR c.scientific_name ILIKE $1 OR c.owner_id ILIKE $1
             ORDER BY c.created_at DESC LIMIT $2 OFFSET $3"
                .to_string(),
            vec![format!("%{}%", search)],
        )
    } else {
        (
            "SELECT c.creature_id, c.owner_id, c.specimen_name, c.scientific_name,
             c.species_group, c.status, c.flagged, c.flag_reason, c.total_flights,
             c.created_at, u.display_name as owner_name
             FROM creatures c LEFT JOIN users u ON u.user_id = c.owner_id
             ORDER BY c.created_at DESC LIMIT $1 OFFSET $2"
                .to_string(),
            vec![],
        )
    };

    let rows = if binds.is_empty() {
        sqlx::query(&sql)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await
    } else {
        sqlx::query(&sql)
            .bind(&binds[0])
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await
    }
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let creatures: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "creature_id": row.get::<uuid::Uuid, _>("creature_id"),
                "owner_id": row.get::<String, _>("owner_id"),
                "owner_name": row.get::<Option<String>, _>("owner_name"),
                "specimen_name": row.get::<Option<String>, _>("specimen_name"),
                "scientific_name": row.get::<String, _>("scientific_name"),
                "species_group": row.get::<String, _>("species_group"),
                "status": row.try_get::<String, _>("status").unwrap_or_else(|_| "active".to_string()),
                "flagged": row.try_get::<bool, _>("flagged").unwrap_or(false),
                "flag_reason": row.get::<Option<String>, _>("flag_reason"),
                "total_flights": row.get::<i32, _>("total_flights"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({ "creatures": creatures })))
}

#[derive(Debug, Deserialize)]
pub struct FlagCreatureRequest {
    pub flagged: bool,
    pub reason: Option<String>,
}

/// PUT /api/admin/creatures/:id/flag — flag/unflag a creature
pub async fn admin_flag_creature_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<uuid::Uuid>,
    Json(req): Json<FlagCreatureRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    sqlx::query(
        "UPDATE creatures SET flagged = $1, flag_reason = $2, updated_at = NOW() WHERE creature_id = $3",
    )
    .bind(req.flagged)
    .bind(&req.reason)
    .bind(creature_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "creature_id": creature_id,
        "flagged": req.flagged,
    })))
}

/// GET /api/admin/swarms — list all swarms (paginated)
pub async fn admin_list_swarms_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(params): Query<AdminSearchParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = (params.page.unwrap_or(1).max(1) - 1) * limit;

    let rows = sqlx::query(
        "SELECT s.swarm_id, s.creator_id, s.name, s.location_name, s.status,
         s.participant_count, s.creature_count, s.visibility, s.starts_at, s.ends_at,
         s.created_at, u.display_name as creator_name
         FROM swarm_events s LEFT JOIN users u ON u.user_id = s.creator_id
         ORDER BY s.created_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let swarms: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "swarm_id": row.get::<uuid::Uuid, _>("swarm_id"),
                "creator_id": row.get::<String, _>("creator_id"),
                "creator_name": row.get::<Option<String>, _>("creator_name"),
                "name": row.get::<String, _>("name"),
                "location_name": row.get::<Option<String>, _>("location_name"),
                "status": row.get::<String, _>("status"),
                "participant_count": row.get::<i32, _>("participant_count"),
                "creature_count": row.get::<i32, _>("creature_count"),
                "visibility": row.try_get::<String, _>("visibility").unwrap_or_else(|_| "public".to_string()),
                "starts_at": row.get::<chrono::DateTime<chrono::Utc>, _>("starts_at").to_rfc3339(),
                "ends_at": row.get::<chrono::DateTime<chrono::Utc>, _>("ends_at").to_rfc3339(),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({ "swarms": swarms })))
}

#[derive(Debug, Deserialize)]
pub struct UpdateSwarmStatusRequest {
    pub status: String,
}

/// PUT /api/admin/swarms/:id/status — update swarm status
pub async fn admin_update_swarm_status_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(swarm_id): Path<uuid::Uuid>,
    Json(req): Json<UpdateSwarmStatusRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    if !["scheduled", "active", "completed", "cancelled"].contains(&req.status.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Status must be scheduled, active, completed, or cancelled".to_string(),
        ));
    }

    sqlx::query("UPDATE swarm_events SET status = $1 WHERE swarm_id = $2")
        .bind(&req.status)
        .bind(swarm_id)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "swarm_id": swarm_id,
        "status": req.status,
    })))
}
