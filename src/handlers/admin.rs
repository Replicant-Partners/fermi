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
            "model_id": state.embedder.model_id(),
            "model_version": state.embedder.model_version(),
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

/// POST /api/admin/workspaces/:workspace_id/grant
/// Grant credits directly to a workspace wallet without deducting from any
/// user wallet. Used by admins to unblock 402s on external-developer
/// workspaces (e.g. efrain) or to top-up kask SimOps workspaces.
pub async fn admin_grant_workspace_credits_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(body): Json<GrantCreditsRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    let credits = body.credits.max(1).min(10000);
    let reason = body.reason.unwrap_or_else(|| "Admin grant".to_string());

    // Validate workspace exists and get its name for the response
    let ws_row = sqlx::query("SELECT name, workspace_budget FROM teams WHERE id = $1::uuid")
        .bind(&workspace_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Workspace {} not found", workspace_id),
            )
        })?;

    let ws_name: String = ws_row.try_get("name").unwrap_or_default();

    // Grant to workspace wallet (no user deduction)
    let wallet = get_or_create_wallet(&state.db, "workspace", &workspace_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    credit_grant(&state.db, wallet.wallet_id, credits, &reason)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Also bump teams.workspace_budget so the workspace header shows the right number
    sqlx::query("UPDATE teams SET workspace_budget = workspace_budget + $1 WHERE id = $2::uuid")
        .bind(credits)
        .bind(&workspace_id)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "status": "granted",
        "workspace_id": workspace_id,
        "workspace_name": ws_name,
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

// ─── Apps Management (admin view of all third-party apps) ─────────────────

/// GET /api/admin/apps — all apps, all visibility levels, all owners,
/// with owner display name joined and archived rows included by default
/// (so the admin can review private/unlisted third-party apps that don't
/// surface in the public `/api/apps` list).
pub async fn admin_list_apps_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(params): Query<AdminSearchParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    let base_sql = r#"SELECT a.id, a.slug, a.name, a.tagline, a.owner_user_id,
                             a.homepage_url, a.icon_url, a.composition_slug,
                             a.schema_slug, a.workspace_template, a.visibility,
                             a.published_at, a.archived_at, a.description,
                             a.created_at, a.updated_at,
                             COALESCE(u.display_name, u.email, a.owner_user_id) as owner_display_name,
                             (SELECT COUNT(*) FROM teams t WHERE t.origin = a.slug) as workspace_count
                        FROM apps a
                        LEFT JOIN users u ON u.user_id = a.owner_user_id"#;

    let rows = if let Some(ref search) = params.search {
        let q = format!("%{}%", search);
        sqlx::query(&format!(
            "{} WHERE a.slug ILIKE $1 OR a.name ILIKE $1 OR a.owner_user_id ILIKE $1 \
             ORDER BY a.created_at DESC LIMIT 500",
            base_sql
        ))
        .bind(&q)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query(&format!(
            "{} ORDER BY a.created_at DESC LIMIT 500",
            base_sql
        ))
        .fetch_all(&state.db)
        .await
    }
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let apps: Vec<Value> = rows
        .iter()
        .map(|r| {
            let tmpl: Value = r
                .try_get::<Value, _>("workspace_template")
                .unwrap_or(json!({}));
            let auto_hire_count = tmpl
                .get("auto_hire")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            json!({
                "id": r.try_get::<uuid::Uuid, _>("id").ok(),
                "slug": r.try_get::<String, _>("slug").unwrap_or_default(),
                "name": r.try_get::<String, _>("name").unwrap_or_default(),
                "tagline": r.try_get::<Option<String>, _>("tagline").ok().flatten(),
                "owner_user_id": r.try_get::<String, _>("owner_user_id").unwrap_or_default(),
                "owner_display_name": r.try_get::<Option<String>, _>("owner_display_name").ok().flatten(),
                "visibility": r.try_get::<String, _>("visibility").unwrap_or_else(|_| "private".into()),
                "published_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("published_at").ok().flatten().map(|t| t.to_rfc3339()),
                "archived_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("archived_at").ok().flatten().map(|t| t.to_rfc3339()),
                "description": r.try_get::<Option<String>, _>("description").ok().flatten(),
                "homepage_url": r.try_get::<Option<String>, _>("homepage_url").ok().flatten(),
                "composition_slug": r.try_get::<Option<String>, _>("composition_slug").ok().flatten(),
                "schema_slug": r.try_get::<Option<String>, _>("schema_slug").ok().flatten(),
                "agent_count": auto_hire_count,
                "workspace_count": r.try_get::<i64, _>("workspace_count").unwrap_or(0),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "apps": apps,
        "total": apps.len(),
    })))
}

#[derive(Debug, Deserialize)]
pub struct AdminAppVisibilityRequest {
    pub visibility: String, // "private" | "unlisted" | "public"
}

/// PUT /api/admin/apps/:slug/visibility — admin can bump a third-party
/// app's visibility (e.g. mark it public on the owner's behalf during
/// support, or hide a problematic app). Owner still owns the app.
pub async fn admin_set_app_visibility_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(slug): Path<String>,
    Json(body): Json<AdminAppVisibilityRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    if !matches!(body.visibility.as_str(), "private" | "unlisted" | "public") {
        return Err((
            StatusCode::BAD_REQUEST,
            "visibility must be 'private', 'unlisted', or 'public'".into(),
        ));
    }

    let updated = sqlx::query(
        "UPDATE apps
            SET visibility = $1,
                published_at = CASE
                    WHEN $1 = 'public' AND published_at IS NULL THEN NOW()
                    ELSE published_at
                END
          WHERE slug = $2",
    )
    .bind(&body.visibility)
    .bind(&slug)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if updated.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, format!("App '{}' not found", slug)));
    }

    Ok(Json(json!({
        "status": "updated",
        "slug": slug,
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
             c.created_at, COALESCE(cc.presence, 'active') AS presence,
             u.display_name as owner_name,
             af.flight_pattern as active_flight_pattern, af.swarm_id as active_swarm_id
             FROM creatures c
             LEFT JOIN users u ON u.user_id = c.owner_id
             LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
             LEFT JOIN LATERAL (
               SELECT flight_pattern, swarm_id FROM creature_flights
               WHERE creature_id = c.creature_id AND ended_at IS NULL
               ORDER BY started_at DESC LIMIT 1
             ) af ON true
             WHERE c.owner_id IS NOT NULL
               AND (c.specimen_name ILIKE $1 OR c.scientific_name ILIKE $1 OR c.owner_id ILIKE $1)
             ORDER BY c.created_at DESC LIMIT $2 OFFSET $3"
                .to_string(),
            vec![format!("%{}%", search)],
        )
    } else {
        (
            "SELECT c.creature_id, c.owner_id, c.specimen_name, c.scientific_name,
             c.species_group, c.status, c.flagged, c.flag_reason, c.total_flights,
             c.created_at, COALESCE(cc.presence, 'active') AS presence,
             u.display_name as owner_name,
             af.flight_pattern as active_flight_pattern, af.swarm_id as active_swarm_id
             FROM creatures c
             LEFT JOIN users u ON u.user_id = c.owner_id
             LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
             LEFT JOIN LATERAL (
               SELECT flight_pattern, swarm_id FROM creature_flights
               WHERE creature_id = c.creature_id AND ended_at IS NULL
               ORDER BY started_at DESC LIMIT 1
             ) af ON true
             WHERE c.owner_id IS NOT NULL
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
                "presence": row.try_get::<Option<String>, _>("presence").unwrap_or(None),
                "active_flight_pattern": row.try_get::<Option<String>, _>("active_flight_pattern").unwrap_or(None),
                "active_swarm_id": row.try_get::<Option<uuid::Uuid>, _>("active_swarm_id").unwrap_or(None),
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

// ─── Agent ownership audit ──────────────────────────────────────────
//
// GET /api/admin/agent-ownership-audit
//
// Admin-only. Returns every agent grouped by ownership status to help
// diagnose ownership drift (the main symptom that surfaces after a
// buggy backfill: agents nulled or assigned to the wrong user).
//
// Buckets:
//   - mine:    agents currently owned by the caller (admin)
//   - others:  agents owned by other users (community agents in normal flow)
//   - orphan:  agents with user_id IS NULL (system-owned, no manager)
//
// Per-row data: agent_name, tier, current owner user_id + display_name,
// visibility, status, created_at. Sorted by tier then name within each
// bucket. No pagination — this is a diagnostic surface, not a hot path.

pub async fn admin_agent_ownership_audit_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;
    let caller_id = principal.user_id();

    let rows = sqlx::query(
        "SELECT a.agent_name, a.tier, a.user_id, a.visibility, a.status,
                a.created_at,
                COALESCE(u.display_name, u.email, u.user_id) AS owner_display
           FROM agents a
           LEFT JOIN users u ON u.user_id = a.user_id
           WHERE a.agent_name NOT LIKE 'test_agent_%'
           ORDER BY a.tier, a.agent_name",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut mine: Vec<Value> = Vec::new();
    let mut others: Vec<Value> = Vec::new();
    let mut orphan: Vec<Value> = Vec::new();

    for r in &rows {
        let owner_id: Option<String> = r.try_get("user_id").ok().flatten();
        let row_json = json!({
            "agent_name": r.try_get::<String, _>("agent_name").unwrap_or_default(),
            "tier": r.try_get::<String, _>("tier").unwrap_or_default(),
            "owner_id": owner_id.clone(),
            "owner_display": r.try_get::<Option<String>, _>("owner_display").ok().flatten(),
            "visibility": r.try_get::<String, _>("visibility").unwrap_or_default(),
            "status": r.try_get::<String, _>("status").unwrap_or_default(),
            "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                .ok().map(|t| t.to_rfc3339()),
        });

        match owner_id {
            None => orphan.push(row_json),
            Some(ref id) if id == &caller_id => mine.push(row_json),
            Some(_) => others.push(row_json),
        }
    }

    // Tier counts within each bucket — fast spot-check for over-corrections.
    let count_by_tier = |bucket: &[Value]| -> Value {
        let mut counts: std::collections::BTreeMap<String, i32> = std::collections::BTreeMap::new();
        for row in bucket {
            let tier = row
                .get("tier")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            *counts.entry(tier).or_insert(0) += 1;
        }
        json!(counts)
    };

    Ok(Json(json!({
        "caller_user_id": caller_id,
        "summary": {
            "mine": {
                "count": mine.len(),
                "by_tier": count_by_tier(&mine),
            },
            "others": {
                "count": others.len(),
                "by_tier": count_by_tier(&others),
            },
            "orphan": {
                "count": orphan.len(),
                "by_tier": count_by_tier(&orphan),
            },
        },
        "mine": mine,
        "others": others,
        "orphan": orphan,
    })))
}

// ─── Agent ownership reassignment ───────────────────────────────────
//
// POST /api/admin/agent-ownership-reassign
// Body: { agent_names: ["foo", "bar"], new_owner_user_id: "<uuid>" | null }
//
// Admin-only. Reassign one or more agents to a different user (or NULL
// to make them system-owned). Use after the audit to fix specific
// drifters without needing a one-off SQL session.
//
// Returns a per-agent {agent_name, status: updated | not_found}.

#[derive(serde::Deserialize)]
pub struct ReassignRequest {
    pub agent_names: Vec<String>,
    /// `None` means "set user_id to NULL" (system-owned).
    #[serde(default)]
    pub new_owner_user_id: Option<String>,
}

pub async fn admin_agent_ownership_reassign_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<ReassignRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    if req.agent_names.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "agent_names is empty".into()));
    }

    // If a new owner is specified, verify the user exists — otherwise
    // a typo would silently orphan the agents.
    if let Some(ref uid) = req.new_owner_user_id {
        let exists: Option<String> =
            sqlx::query_scalar("SELECT user_id FROM users WHERE user_id = $1")
                .bind(uid)
                .fetch_optional(&state.db)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if exists.is_none() {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("new_owner_user_id {} not found in users", uid),
            ));
        }
    }

    let mut results: Vec<Value> = Vec::new();
    for name in &req.agent_names {
        let rows = sqlx::query(
            "UPDATE agents SET user_id = $2 WHERE agent_name = $1 RETURNING agent_name",
        )
        .bind(name)
        .bind(&req.new_owner_user_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        results.push(json!({
            "agent_name": name,
            "status": if rows.is_empty() { "not_found" } else { "updated" },
        }));
    }

    Ok(Json(json!({
        "new_owner_user_id": req.new_owner_user_id,
        "results": results,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// Spec 23 demo cleanup — POST /api/admin/wipe-fermi-forecasts
// ═══════════════════════════════════════════════════════════════════
//
// One-shot administrative wipe of every workspace spawned by the
// Fermi Forecast App (origin = 'fermi_forecast') plus every cascading
// row across BayesOps tables, forecast tables, and workspace tables.
//
// **Destructive.** Requires an exact confirmation token in the body
// and admin auth. Supports a dry-run mode that returns row counts
// without writing.
//
// This is needed because the existing forecast portfolio was spawned
// against a speculative team-to-group mapping that doesn't match the
// real WC 2026 draw, and the platform has no in-place archive
// operation that would let us reuse the workspaces. Cleanup +
// fresh respawn gets us to a known-good base state.
//
// Designed to be:
//   - **All-or-nothing**: the entire DB wipe happens in one
//     transaction. If anything fails midway, nothing is deleted.
//   - **Idempotent on repos**: filesystem repo cleanup logs per-slug
//     failures but doesn't abort the response. Re-running on a
//     partially-cleaned state is safe.
//   - **Auditable**: returns per-table counts of what was removed
//     so the operator has a paper trail.

#[derive(Deserialize)]
pub struct WipeFermiForecastsRequest {
    /// Must equal the literal string "WIPE_ALL_FERMI_FORECASTS".
    /// Typo guard — defends against fat-finger curl calls.
    pub confirm: String,
    /// When true, return what would be deleted without actually deleting.
    #[serde(default)]
    pub dry_run: bool,
}

const WIPE_CONFIRMATION: &str = "WIPE_ALL_FERMI_FORECASTS";

pub async fn admin_wipe_fermi_forecasts_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<WipeFermiForecastsRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    if req.confirm != WIPE_CONFIRMATION {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "confirm field must equal exactly '{}' (got '{}')",
                WIPE_CONFIRMATION, req.confirm
            ),
        ));
    }

    // ── 1. Enumerate target workspaces + forecasts + slugs ──────────
    //
    // We hold these IDs separately because we'll need:
    //   (a) workspace_ids to filter workspace_* tables
    //   (b) forecast_ids  to filter forecast_* tables
    //   (c) slugs         to clean up filesystem repos after commit
    //
    // Collected before any DELETE runs so a DB failure mid-transaction
    // doesn't leave us with the IDs but no way to know what to clean up
    // on disk.

    let target_workspaces: Vec<(uuid::Uuid, String)> =
        sqlx::query("SELECT id, slug FROM teams WHERE origin = 'fermi_forecast'")
            .fetch_all(&state.db)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("enumerate workspaces: {}", e),
                )
            })?
            .into_iter()
            .map(|row| {
                let id: uuid::Uuid = row.get("id");
                let slug: String = row.get("slug");
                (id, slug)
            })
            .collect();

    let workspace_ids: Vec<uuid::Uuid> = target_workspaces.iter().map(|(id, _)| *id).collect();
    let slugs: Vec<String> = target_workspaces
        .iter()
        .map(|(_, slug)| slug.clone())
        .collect();

    let target_forecast_ids: Vec<String> = if workspace_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query("SELECT id FROM fermi_forecasts WHERE workspace_id = ANY($1)")
            .bind(&workspace_ids)
            .fetch_all(&state.db)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("enumerate forecasts: {}", e),
                )
            })?
            .into_iter()
            .map(|row| row.get::<String, _>("id"))
            .collect()
    };

    let n_workspaces = workspace_ids.len();
    let n_forecasts = target_forecast_ids.len();

    // ── 2. Count what would be deleted, per table ──────────────────
    //
    // Cheap. Always runs (dry-run mode short-circuits before the
    // delete pass; non-dry-run uses these counts for the response).
    let counts = count_targets(&state.db, &workspace_ids, &target_forecast_ids)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("count phase: {}", e),
            )
        })?;

    if req.dry_run {
        return Ok(Json(json!({
            "dry_run": true,
            "would_delete": counts,
            "workspaces": n_workspaces,
            "forecasts": n_forecasts,
            "slugs_to_clean": slugs.len(),
        })));
    }

    // ── 3. Cascade delete inside a transaction ─────────────────────
    let mut tx = state.db.begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("begin tx: {}", e),
        )
    })?;

    macro_rules! delete_by_ws {
        ($sql:expr, $label:literal) => {
            sqlx::query($sql)
                .bind(&workspace_ids)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("delete {}: {}", $label, e),
                    )
                })?
        };
    }

    macro_rules! delete_by_fc {
        ($sql:expr, $label:literal) => {
            sqlx::query($sql)
                .bind(&target_forecast_ids)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("delete {}: {}", $label, e),
                    )
                })?
        };
    }

    if !workspace_ids.is_empty() {
        // BayesOps ledger (R-1) — pending_fits references snapshots so go first
        delete_by_ws!(
            "DELETE FROM bayesops_pending_fits WHERE workspace_id = ANY($1)",
            "bayesops_pending_fits"
        );
        delete_by_ws!(
            "DELETE FROM bayesops_posterior_snapshots WHERE workspace_id = ANY($1)",
            "bayesops_posterior_snapshots"
        );

        // Workspace runtime state
        delete_by_ws!(
            "DELETE FROM workspace_messages WHERE workspace_id = ANY($1)",
            "workspace_messages"
        );
        delete_by_ws!(
            "DELETE FROM workspace_outputs WHERE workspace_id = ANY($1)",
            "workspace_outputs"
        );
        delete_by_ws!(
            "DELETE FROM workspace_agents WHERE workspace_id = ANY($1)",
            "workspace_agents"
        );
        delete_by_ws!(
            "DELETE FROM workspace_dependencies
             WHERE upstream_id = ANY($1) OR downstream_id = ANY($1)",
            "workspace_dependencies"
        );
    }

    if !target_forecast_ids.is_empty() {
        // Forecast benchmark / spacetime infrastructure (migrations 140, 094)
        delete_by_fc!(
            "DELETE FROM forecast_spacetime WHERE forecast_id = ANY($1)",
            "forecast_spacetime"
        );
        delete_by_fc!(
            "DELETE FROM forecast_commitments WHERE forecast_id = ANY($1)",
            "forecast_commitments"
        );
        // forecast_splits may not exist in all envs
        let _ = sqlx::query("DELETE FROM forecast_splits WHERE forecast_id = ANY($1)")
            .bind(&target_forecast_ids)
            .execute(&mut *tx)
            .await; // ignore — best-effort
        delete_by_fc!(
            "DELETE FROM fermi_forecast_updates WHERE forecast_id = ANY($1)",
            "fermi_forecast_updates"
        );
        delete_by_fc!(
            "DELETE FROM fermi_market_observations WHERE forecast_id = ANY($1)",
            "fermi_market_observations"
        );
        delete_by_fc!(
            "DELETE FROM fermi_portfolio_forecasts WHERE forecast_id = ANY($1)",
            "fermi_portfolio_forecasts"
        );
        delete_by_fc!(
            "DELETE FROM fermi_forecast_schedules WHERE forecast_id = ANY($1)",
            "fermi_forecast_schedules"
        );

        // The forecasts themselves
        delete_by_fc!(
            "DELETE FROM fermi_forecasts WHERE id = ANY($1)",
            "fermi_forecasts"
        );
    }

    if !workspace_ids.is_empty() {
        // Workspace shells last (FK targets above)
        delete_by_ws!(
            "DELETE FROM team_members WHERE team_id = ANY($1)",
            "team_members"
        );
        delete_by_ws!("DELETE FROM teams WHERE id = ANY($1)", "teams");
    }

    tx.commit().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("commit tx: {}", e),
        )
    })?;

    // ── 4. Repo cleanup (after DB commit) ─────────────────────────
    //
    // Per-slug. Log failures, continue. Failures here don't roll back
    // the DB — the workspaces are already gone from the database; an
    // orphaned repo is a disk-space issue, not a correctness issue.
    let mut repos_deleted = 0usize;
    let mut repo_failures: Vec<Value> = Vec::new();
    for slug in &slugs {
        match state.workspace_git.delete_workspace_repo(slug) {
            Ok(true) => repos_deleted += 1,
            Ok(false) => {} // repo didn't exist — fine
            Err(e) => {
                repo_failures.push(json!({
                    "slug": slug,
                    "error": e.to_string(),
                }));
            }
        }
    }

    Ok(Json(json!({
        "dry_run": false,
        "deleted": counts,
        "workspaces": n_workspaces,
        "forecasts": n_forecasts,
        "repos_deleted": repos_deleted,
        "repo_failures": repo_failures,
    })))
}

async fn count_targets(
    db: &sqlx::PgPool,
    workspace_ids: &[uuid::Uuid],
    forecast_ids: &[String],
) -> Result<Value, sqlx::Error> {
    let mut counts = serde_json::Map::new();

    async fn count_ws(
        db: &sqlx::PgPool,
        sql: &str,
        ids: &[uuid::Uuid],
    ) -> Result<i64, sqlx::Error> {
        if ids.is_empty() {
            return Ok(0);
        }
        let row = sqlx::query(sql).bind(ids).fetch_one(db).await?;
        Ok(row.try_get::<i64, _>("c").unwrap_or(0))
    }
    async fn count_fc(db: &sqlx::PgPool, sql: &str, ids: &[String]) -> Result<i64, sqlx::Error> {
        if ids.is_empty() {
            return Ok(0);
        }
        let row = sqlx::query(sql).bind(ids).fetch_one(db).await?;
        Ok(row.try_get::<i64, _>("c").unwrap_or(0))
    }

    counts.insert(
        "bayesops_pending_fits".into(),
        json!(
            count_ws(
                db,
                "SELECT COUNT(*) AS c FROM bayesops_pending_fits WHERE workspace_id = ANY($1)",
                workspace_ids
            )
            .await?
        ),
    );
    counts.insert("bayesops_posterior_snapshots".into(), json!(
        count_ws(db, "SELECT COUNT(*) AS c FROM bayesops_posterior_snapshots WHERE workspace_id = ANY($1)", workspace_ids).await?
    ));
    counts.insert(
        "workspace_messages".into(),
        json!(
            count_ws(
                db,
                "SELECT COUNT(*) AS c FROM workspace_messages WHERE workspace_id = ANY($1)",
                workspace_ids
            )
            .await?
        ),
    );
    counts.insert(
        "workspace_outputs".into(),
        json!(
            count_ws(
                db,
                "SELECT COUNT(*) AS c FROM workspace_outputs WHERE workspace_id = ANY($1)",
                workspace_ids
            )
            .await?
        ),
    );
    counts.insert(
        "workspace_agents".into(),
        json!(
            count_ws(
                db,
                "SELECT COUNT(*) AS c FROM workspace_agents WHERE workspace_id = ANY($1)",
                workspace_ids
            )
            .await?
        ),
    );
    counts.insert("workspace_dependencies".into(), json!(
        count_ws(db, "SELECT COUNT(*) AS c FROM workspace_dependencies WHERE upstream_id = ANY($1) OR downstream_id = ANY($1)", workspace_ids).await?
    ));
    counts.insert(
        "forecast_spacetime".into(),
        json!(
            count_fc(
                db,
                "SELECT COUNT(*) AS c FROM forecast_spacetime WHERE forecast_id = ANY($1)",
                forecast_ids
            )
            .await?
        ),
    );
    counts.insert(
        "forecast_commitments".into(),
        json!(
            count_fc(
                db,
                "SELECT COUNT(*) AS c FROM forecast_commitments WHERE forecast_id = ANY($1)",
                forecast_ids
            )
            .await?
        ),
    );
    counts.insert(
        "fermi_forecast_updates".into(),
        json!(
            count_fc(
                db,
                "SELECT COUNT(*) AS c FROM fermi_forecast_updates WHERE forecast_id = ANY($1)",
                forecast_ids
            )
            .await?
        ),
    );
    counts.insert(
        "fermi_market_observations".into(),
        json!(
            count_fc(
                db,
                "SELECT COUNT(*) AS c FROM fermi_market_observations WHERE forecast_id = ANY($1)",
                forecast_ids
            )
            .await?
        ),
    );
    counts.insert(
        "fermi_portfolio_forecasts".into(),
        json!(
            count_fc(
                db,
                "SELECT COUNT(*) AS c FROM fermi_portfolio_forecasts WHERE forecast_id = ANY($1)",
                forecast_ids
            )
            .await?
        ),
    );
    counts.insert(
        "fermi_forecast_schedules".into(),
        json!(
            count_fc(
                db,
                "SELECT COUNT(*) AS c FROM fermi_forecast_schedules WHERE forecast_id = ANY($1)",
                forecast_ids
            )
            .await?
        ),
    );
    counts.insert("fermi_forecasts".into(), json!(forecast_ids.len()));
    counts.insert(
        "team_members".into(),
        json!(
            count_ws(
                db,
                "SELECT COUNT(*) AS c FROM team_members WHERE team_id = ANY($1)",
                workspace_ids
            )
            .await?
        ),
    );
    counts.insert("teams".into(), json!(workspace_ids.len()));

    Ok(Value::Object(counts))
}

// ─── Recompose all mutex groups ────────────────────────────────────
//
// One-shot maintenance endpoint: re-runs `recompose_mutex_group` over
// every non-archived mutex group. Idempotent — the recompose reads
// `sim_probability` (raw standalones) each time and re-derives
// `predicted_probability` from scratch, so calling this repeatedly
// converges to the same state.
//
// The trigger case: after fixing a renormalisation bug in
// `recompose.rs`, existing displayed values in the DB stay wrong until
// something touches the group. This endpoint forces the sync without
// waiting for a sim run or a resolve.

#[derive(Debug, Deserialize, Default)]
pub struct RecomposeMutexGroupsQuery {
    /// If set, restrict the run to a single group_id (e.g.
    /// `wc_2026_winner`) instead of iterating every mutex group.
    pub group_id: Option<String>,
}

pub async fn admin_recompose_mutex_groups_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<RecomposeMutexGroupsQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    // Enumerate target groups. When `group_id` is given we still round-
    // trip through the DB so a bad id returns an empty result rather
    // than a fabricated "success".
    let group_ids: Vec<String> = if let Some(gid) = &q.group_id {
        sqlx::query(
            "SELECT group_id FROM public.forecast_relationship_groups
              WHERE kind = 'mutex' AND archived_at IS NULL AND group_id = $1",
        )
        .bind(gid)
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .into_iter()
        .filter_map(|r| r.try_get::<String, _>("group_id").ok())
        .collect()
    } else {
        sqlx::query(
            "SELECT group_id FROM public.forecast_relationship_groups
              WHERE kind = 'mutex' AND archived_at IS NULL
              ORDER BY group_id",
        )
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .into_iter()
        .filter_map(|r| r.try_get::<String, _>("group_id").ok())
        .collect()
    };

    // Run recompose per group. Best-effort: a single group's failure
    // shouldn't block the others — we surface per-group errors in the
    // response instead.
    let mut per_group: Vec<Value> = Vec::with_capacity(group_ids.len());
    let mut total_members: usize = 0;
    let mut groups_ok: usize = 0;
    let mut groups_err: usize = 0;

    for gid in &group_ids {
        match crate::handlers::relationships::recompose::recompose_mutex_group(gid, &state.db).await
        {
            Ok(map) => {
                let sum: f64 = map.values().sum();
                groups_ok += 1;
                total_members += map.len();
                per_group.push(json!({
                    "group_id": gid,
                    "members": map.len(),
                    "displayed_sum": sum,
                    "status": "ok",
                }));
            }
            Err((code, msg)) => {
                groups_err += 1;
                per_group.push(json!({
                    "group_id": gid,
                    "status": "error",
                    "http_status": code.as_u16(),
                    "error": msg,
                }));
            }
        }
    }

    Ok(Json(json!({
        "groups_processed": group_ids.len(),
        "groups_ok": groups_ok,
        "groups_err": groups_err,
        "members_touched": total_members,
        "per_group": per_group,
    })))
}
