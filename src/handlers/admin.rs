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
    /// Filter users by the app they signed up through. `Some("fermi_console")`
    /// shows just that cohort. `Some("")` shows direct ABW signups (NULL
    /// signup_app_slug). `None` shows everyone.
    signup_app: Option<String>,
}

pub async fn admin_list_users_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(params): Query<AdminSearchParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = (params.page.unwrap_or(1).max(1) - 1) * limit;

    // Optional `?signup_app=<slug>` filters to a specific cohort. Empty
    // string is treated as "only show users who came in via ABW direct
    // (signup_app_slug IS NULL)", which is what a bare `signup_app=`
    // resolves to on the query-string.
    let signup_app_filter = params.signup_app.clone();

    let base_select = "u.user_id, u.email, u.display_name, u.role, u.auth_provider, \
                       u.signup_app_slug, u.created_at, \
                       COALESCE(w.balance, 0) as balance, \
                       COALESCE(ac.cnt, 0) as agent_count \
                       FROM users u \
                       LEFT JOIN wallets w ON w.owner_type = 'user' AND w.owner_id = u.user_id \
                       LEFT JOIN (SELECT user_id, COUNT(*) as cnt FROM agents GROUP BY user_id) ac \
                         ON ac.user_id = u.user_id";

    let rows = match (params.search.as_ref(), signup_app_filter.as_ref()) {
        (Some(search), Some(app)) if !app.is_empty() => {
            let q = format!("%{}%", search);
            sqlx::query(&format!(
                "SELECT {} WHERE (u.user_id ILIKE $1 OR u.email ILIKE $1 OR u.display_name ILIKE $1) \
                 AND u.signup_app_slug = $2 \
                 ORDER BY u.created_at DESC LIMIT $3 OFFSET $4",
                base_select
            ))
            .bind(&q)
            .bind(app)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await
        }
        (Some(search), Some(_empty_app)) => {
            // signup_app was provided but empty — filter for NULL slugs.
            let q = format!("%{}%", search);
            sqlx::query(&format!(
                "SELECT {} WHERE (u.user_id ILIKE $1 OR u.email ILIKE $1 OR u.display_name ILIKE $1) \
                 AND u.signup_app_slug IS NULL \
                 ORDER BY u.created_at DESC LIMIT $2 OFFSET $3",
                base_select
            ))
            .bind(&q)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await
        }
        (Some(search), None) => {
            let q = format!("%{}%", search);
            sqlx::query(&format!(
                "SELECT {} WHERE u.user_id ILIKE $1 OR u.email ILIKE $1 OR u.display_name ILIKE $1 \
                 ORDER BY u.created_at DESC LIMIT $2 OFFSET $3",
                base_select
            ))
            .bind(&q)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await
        }
        (None, Some(app)) if !app.is_empty() => {
            sqlx::query(&format!(
                "SELECT {} WHERE u.signup_app_slug = $1 \
                 ORDER BY u.created_at DESC LIMIT $2 OFFSET $3",
                base_select
            ))
            .bind(app)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await
        }
        (None, Some(_empty_app)) => {
            sqlx::query(&format!(
                "SELECT {} WHERE u.signup_app_slug IS NULL \
                 ORDER BY u.created_at DESC LIMIT $1 OFFSET $2",
                base_select
            ))
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await
        }
        (None, None) => {
            sqlx::query(&format!(
                "SELECT {} ORDER BY u.created_at DESC LIMIT $1 OFFSET $2",
                base_select
            ))
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await
        }
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
                "signup_app_slug": r.try_get::<Option<String>, _>("signup_app_slug").unwrap_or(None),
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
        .filter(|a| !crate::handlers::is_test_cruft(&a.agent_name))
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

    // Run counts measured from `episodes`. The `agents.total_executions`
    // column this used to read is never written by any code path, so the
    // admin agent list reported 0 executions for every agent on the
    // platform — including ones with hundreds of real runs. See
    // migrations/192 and src/rollup_trust.rs.
    let agent_uuids: Vec<uuid::Uuid> = filtered.iter().map(|a| a.agent_id).collect();
    let exec_stats = fermi::agent_economics::measured_exec_stats(&state.db, &agent_uuids).await;

    let agents_json: Vec<Value> = filtered
        .iter()
        .map(|a| {
            let owner_name = a
                .owner_id
                .as_deref()
                .and_then(|oid| owner_names.get(oid))
                .cloned();
            let m = exec_stats.get(&a.agent_id).copied().unwrap_or_default();
            json!({
                "id": a.agent_name,
                "agent_id": a.agent_id,
                "agent_name": a.agent_name,
                "display_alias": a.display_alias,
                "owner_id": a.owner_id,
                "owner_display_name": owner_name,
                "visibility": a.visibility,
                "status": a.status,
                "execution_count": m.executions,
                "total_executions": m.executions,
                "total_cost_usd": m.cost_usd,
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

// ─── Schema health diagnostic ──────────────────────────────
//
// v0.11.0: the manifest + check logic lived here through v0.10.x.
// It's now factored into `crate::schema_trust` so the boot-time
// invocation in main() and this endpoint share one source of truth.
// The response body shape is preserved for backwards compatibility
// with existing dashboards that poll this endpoint.
//
// The three legacy `SCHEMA_TABLES / SCHEMA_FUNCTIONS / SCHEMA_COLUMNS`
// constants below are kept as re-exports for any downstream code that
// referenced them directly — marked `#[deprecated]` so the compiler
// nudges callers to migrate to `crate::schema_trust::*`.

// SCHEMA_TABLES / SCHEMA_FUNCTIONS / SCHEMA_COLUMNS now live in
// `crate::schema_trust`. The manifest expanded 5x in v0.11.0 to cover
// every column the code actually depends on — not just the ones
// `ensure_critical_schema` handles.

use crate::schema_trust;

pub async fn admin_schema_health_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    // v0.11.0: delegate to the shared trust-contract module so the
    // boot-time check and this endpoint always agree on "healthy".
    let verdict = schema_trust::verify(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(verdict.to_health_json()))
}

// ─── Liveness: does the write path ever actually run? ──────────
//
// The standing clock's read surface. Until this existed the only way to learn
// whether a declared write path had ever executed was to run a shell script by
// hand; nothing scheduled it and CI did not run it either. A rung with no
// endpoint and no schedule is indistinguishable from a rung that passes, which
// is the exact defect `liveness_trust` was written to catch.
//
// Two things this deliberately does NOT do:
//
//   * It does not run the sweep on request. The sweep touches `episodes`, and
//     an endpoint that runs it is an endpoint that can be used to load the
//     database. It reports what the sweeper last found.
//   * It does not report `never_run` as healthy. `status` is `never_run` until
//     the first sweep completes — absence is not a verdict.
pub async fn admin_liveness_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    // Read at request time, not from the sweep snapshot.
    //
    // The counters are in-process and free to read, and they move continuously
    // while the sweep is hourly. Serving an hour-old attempt count beside a
    // live one would answer "is anything being refused right now" with a number
    // from before the deploy that broke it — and this is the one surface where
    // that question is asked.
    let attempts = fermi::write_accounting::accounts();
    let refused: Vec<_> = attempts
        .iter()
        .filter(|a| a.is_totally_rejected())
        .collect();

    // What every refusal point decided since boot. Read live for the same
    // reason as the write counters, and reported alongside them because the two
    // answer halves of one question: `gates` is what the system declined to do,
    // `write_accounting` is what it tried to do and could not.
    let gates = fermi::gate_trust::accounts();
    let gates_refusing_everything = fermi::gate_trust::refusing_everything();

    // Where each feedback loop stops, and why. The one query on this endpoint,
    // and it is worth it: everything else here reports a rung, and this reports
    // the chains those rungs are rungs of. A stage-by-stage view makes ten
    // findings out of two — only the first empty link in a chain is actionable,
    // and the ones below it are empty because of it.
    let loops = fermi::loop_model::evaluate(&state.db).await;

    // The native evaluators read the same instant everything else on this
    // endpoint reports, and turn it into verdicts with remedies. They are the
    // reason this endpoint is worth reading rather than parsing: `gates` is a
    // table of counters, `native` is what those counters mean.
    // One snapshot, read by everything below it, so the evaluators and the
    // panel resolver cannot disagree about what instant they are describing.
    let observation = fermi::native_evaluators::Observation {
        writes: attempts.clone(),
        gates: gates.clone(),
        loops: loops.clone(),
        liveness: fermi::liveness_trust::latest(),
        // Which of the counters above can say more than "since boot".
        gate_ledger: Some(fermi::gate_trust::ledger_status()),
    };
    let native = fermi::native_evaluators::run(&observation);

    // Why each UI surface that can be blank is blank, routed to the contract
    // that knows. Served here rather than from the page handlers because the
    // answer is a property of the platform, not of the request: a panel that
    // renders its own guess at emptiness is the defect this resolves.
    // What the platform lets a person do, and what stands in front of it. Two
    // findings rather than the whole table: a write nobody has justified
    // leaving ungated, and a gate whose verdict is computed and thrown away.
    // The second is the audit's §3 as a live query — on the surface a caller
    // sees, a discarded verdict and an absent gate are the same thing.
    let commands_ungoverned = fermi::command_registry::ungoverned_writes();
    let commands_discarded: Vec<_> = fermi::command_registry::gates_computed_and_discarded()
        .into_iter()
        .map(|(cmd, gate)| format!("{cmd}: {gate} runs and is discarded"))
        .collect();

    let panels = fermi::panel_absence::resolve_all(&observation);
    let panels_unexplained: Vec<_> = panels
        .iter()
        .filter(|a| a.reading == fermi::panel_absence::Reading::Unknown)
        .map(|a| a.panel)
        .collect();
    let stalled_in_code: Vec<_> = loops
        .iter()
        .filter(|l| {
            matches!(
                l.reason,
                Some("no_trigger") | Some("writes_refused") | Some("gate_refuses_everything")
            )
        })
        .map(|l| {
            format!(
                "{}.{}: {}",
                l.id,
                l.stops_at.unwrap_or("?"),
                l.reason.unwrap_or("?")
            )
        })
        .collect();

    // Loops carrying a stage whose count query did not run. Named beside
    // `loops_stalled_in_code` for the same reason `gates_refusing_everything`
    // is named beside `gates`: the reading that matters must not require a
    // consumer to scan the array for it. These are neither turning nor stalled,
    // and folding them into either column is how an observer failure comes to
    // present as a healthy system.
    let loops_unread: Vec<_> = loops
        .iter()
        .filter(|l| !l.measured())
        .map(|l| format!("{}.{}: probe_failed", l.id, l.stops_at.unwrap_or("?")))
        .collect();

    let Some(report) = crate::liveness_trust::latest() else {
        return Ok(Json(serde_json::json!({
            "status": "never_run",
            "detail": "No sweep has completed since boot. This is not a pass: an inert \
                       check and a passing check are indistinguishable from outside.",
            "contracts_declared": crate::liveness_trust::LIVENESS_CONTRACTS.len(),
            // Available immediately, and worth having before the first sweep:
            // the counters need no database and start at boot.
            "write_accounting": attempts,
            "refused": refused,
            "gates": gates,
            "gates_refusing_everything": gates_refusing_everything,
            "loops": loops,
            "loops_stalled_in_code": stalled_in_code,
            "loops_unread": loops_unread,
            "native": native,
            "panels": panels,
            "panels_unexplained": panels_unexplained,
            "commands_ungoverned": commands_ungoverned,
            "commands_gate_discarded": commands_discarded,
            "native": native,
        })));
    };

    let status = if !gates_refusing_everything.is_empty() {
        // A gate that has been asked and has approved nothing. Ranked with the
        // refused writes and above `degraded`, because it is the signature of
        // the longest-lived defect this system has had: a control that rejects
        // everything for reasons unrelated to its input looks, from every other
        // surface, exactly like a strict control working well.
        //
        // The inverse — a gate that has never refused anything — is reported in
        // `gates[].reading` and deliberately does NOT change the status. A gate
        // legitimately refuses nothing when nothing warranted refusal, and
        // asserting otherwise would assert that violations must exist.
        "gate_refusing_everything"
    } else if !refused.is_empty() {
        // Ranked above `degraded` and above the positive-control check, because
        // it is the only one of the three that admits no benign reading. A
        // silent sink may be unused; a refused write is a statement the
        // database will not accept.
        "writes_refused"
    } else if report.is_healthy() {
        "healthy"
    } else if !report.has_positive_control() {
        // 0 live cannot distinguish "every path is broken" from "the sweep is
        // broken", so it gets its own name rather than being folded into
        // `degraded`.
        "no_positive_control"
    } else {
        "degraded"
    };

    Ok(Json(serde_json::json!({
        "status": status,
        "report": report,
        "write_accounting": attempts,
        "refused": refused,
        "gates": gates,
        "gates_refusing_everything": gates_refusing_everything,
        "loops": loops,
        "loops_stalled_in_code": stalled_in_code,
        "loops_unread": loops_unread,
        "native": native,
        "panels": panels,
        "panels_unexplained": panels_unexplained,
        "commands_ungoverned": commands_ungoverned,
        "commands_gate_discarded": commands_discarded,
        "native": native,
    })))
}

// v0.11.0: legacy inline check body removed — the shared module in
// crate::schema_trust is the single source of truth. If you need the
// pre-v0.11.0 body for reference, see commit history for admin.rs
// prior to the v0.11.0 tag.

#[cfg(any())]
async fn admin_schema_health_handler_legacy_removed(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;
    let pool = &state.db;

    // ── Tables ─────────────────────────────────
    let present_tables: std::collections::HashSet<String> = sqlx::query(
        "SELECT table_name FROM information_schema.tables
          WHERE table_schema = 'public'
            AND table_name = ANY($1)",
    )
    .bind(SCHEMA_TABLES)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .into_iter()
    .filter_map(|r| r.try_get::<String, _>("table_name").ok())
    .collect();

    let mut tables_missing: u32 = 0;
    let tables: Vec<Value> = SCHEMA_TABLES
        .iter()
        .map(|name| {
            let present = present_tables.contains(*name);
            if !present {
                tables_missing += 1;
            }
            json!({ "name": name, "present": present })
        })
        .collect();

    // ── Functions ──────────────────────────────────────────────
    // pg_proc joined with pg_namespace: proname is unique per
    // (namespace, arg-type list). Compare argument types via
    // pg_get_function_arguments so 0-arg fns match with empty string.
    let present_functions: Vec<(String, String)> = sqlx::query(
        "SELECT p.proname,
                pg_get_function_identity_arguments(p.oid) AS args
           FROM pg_catalog.pg_proc p
           JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
          WHERE n.nspname = 'public'
            AND p.proname = ANY($1)",
    )
    .bind(SCHEMA_FUNCTIONS.iter().map(|(n, _)| *n).collect::<Vec<_>>())
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .into_iter()
    .filter_map(|r| {
        Some((
            r.try_get::<String, _>("proname").ok()?,
            r.try_get::<String, _>("args").ok()?,
        ))
    })
    .collect();

    let normalise = |s: &str| s.replace(' ', "").to_lowercase();
    let mut functions_missing: u32 = 0;
    let functions: Vec<Value> = SCHEMA_FUNCTIONS
        .iter()
        .map(|(name, sig)| {
            let want = normalise(sig);
            // Collect every signature we saw for this name so a mismatch
            // is easy to diagnose ("expected real, boolean but the DB has
            // predicted real, actual boolean"). Without this the probe
            // only tells us present:false and we can't tell if the
            // function is truly absent or just has a signature drift.
            let found_sigs: Vec<String> = present_functions
                .iter()
                .filter(|(n, _)| n == name)
                .map(|(_, s)| s.clone())
                .collect();
            let present = found_sigs.iter().any(|s| normalise(s) == want);
            if !present {
                functions_missing += 1;
            }
            json!({
                "name": name,
                "signature": sig,
                "present": present,
                "found_signatures": found_sigs,
            })
        })
        .collect();

    // ── Columns ────────────────────────────────────────────────
    let tables_for_cols: Vec<String> = SCHEMA_COLUMNS.iter().map(|(t, _)| t.to_string()).collect();
    let cols_for_cols: Vec<String> = SCHEMA_COLUMNS.iter().map(|(_, c)| c.to_string()).collect();
    let present_columns: std::collections::HashSet<(String, String)> = sqlx::query(
        "SELECT table_name, column_name FROM information_schema.columns
          WHERE table_schema = 'public'
            AND table_name = ANY($1)
            AND column_name = ANY($2)",
    )
    .bind(&tables_for_cols)
    .bind(&cols_for_cols)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .into_iter()
    .filter_map(|r| {
        Some((
            r.try_get::<String, _>("table_name").ok()?,
            r.try_get::<String, _>("column_name").ok()?,
        ))
    })
    .collect();

    let mut columns_missing: u32 = 0;
    let columns: Vec<Value> = SCHEMA_COLUMNS
        .iter()
        .map(|(table, column)| {
            let key = (table.to_string(), column.to_string());
            let present = present_columns.contains(&key);
            if !present {
                columns_missing += 1;
            }
            json!({ "table": table, "column": column, "present": present })
        })
        .collect();

    let total_missing = tables_missing + functions_missing + columns_missing;
    let status = if total_missing == 0 {
        "healthy"
    } else {
        "degraded"
    };

    Ok(Json(json!({
        "status": status,
        "checked_at": chrono::Utc::now().to_rfc3339(),
        "tables": tables,
        "functions": functions,
        "columns": columns,
        "summary": {
            "tables": {
                "total": SCHEMA_TABLES.len(),
                "missing": tables_missing,
            },
            "functions": {
                "total": SCHEMA_FUNCTIONS.len(),
                "missing": functions_missing,
            },
            "columns": {
                "total": SCHEMA_COLUMNS.len(),
                "missing": columns_missing,
            },
            "total_missing": total_missing,
        },
    })))
}

// ═══════════════════════════════════════════════════════════════════
// v0.10.20 — Legacy agent-slug audit + rename
//
// Un-routable agent names produced by the pre-2026-05-23 platform
// (before `slug::validate` locked down creation surfaces in
// commit d0f94e8). Names containing `-` or `/` are unreachable via
// /agent/<name> because axum's tree router splits on `/`. This
// endpoint audits them and, with `?apply=true`, renames them to
// slug-compliant snake_case and backfills the JSONB references
// in `fermi_forecasts.agents_used`.
//
// Design:
//   * GET /api/admin/agents/legacy-slugs         — audit only (dry-run)
//   * POST /api/admin/agents/legacy-slugs        — audit only
//   * POST /api/admin/agents/legacy-slugs?apply=true — execute rename
//
// Both use `admin_legacy_agent_slugs_handler`. Query param `apply`
// (default false) toggles between audit and mutate. The response
// shape is identical in both modes; `action_taken` differs per row.
//
// Every actual rename lands in `admin_bypass_events` with the
// old → new mapping so the trail is legible six months from now.
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct LegacySlugsQuery {
    /// When true, execute the rename in a transaction. Default false
    /// (audit-only). Only platform admins can hit this endpoint at
    /// all, so no further gate is needed on `apply`.
    #[serde(default)]
    pub apply: bool,

    /// v0.10.24: restrict the audit / apply set to agents whose
    /// current `agent_name` starts with this prefix. Used to target
    /// a specific vertical (e.g. `efra-ai/` for Mario's real work)
    /// so a bulk `--apply` doesn't sweep unrelated test fixtures.
    /// Applied at the SQL layer via `WHERE agent_name LIKE $1`
    /// (with `%` appended by the handler — not the caller).
    #[serde(default)]
    pub prefix: Option<String>,

    /// v0.10.24: cap the batch size. Even with mig-168 speeding up
    /// reads, apply still runs ~3 statements per rename in one
    /// transaction. 574 renames blew past the 60s client timeout on
    /// Ivan's first try. `--limit 50` keeps a run bounded and
    /// resumable. Applied AFTER the slug-rule filter (only counts
    /// actual legacy rows).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Conservative slug sanitiser scoped to this handler.
///
/// Mirrors the intent of `apps::workspace_fork::slugify` (which is
/// private to that module) and `crate::slug::validate` (which is
/// reject-only). Rules:
///
///   * Lowercase every ASCII letter.
///   * Keep digits and underscores.
///   * Replace every other char (including `-`, `/`, `.`, spaces)
///     with a single `_` — collapse runs so we don't emit `__`.
///   * Strip leading digits and underscores (slugs must start with
///     a letter per `slug::validate`).
///   * Strip trailing underscores.
///   * Truncate to 64 chars.
///   * Return `None` if the result would be < 3 chars (unrecoverable
///     — caller needs to rename manually).
///
/// The returned string is guaranteed to satisfy `slug::validate`
/// when `Some`. If it doesn't, that's a bug in this fn; the caller
/// re-validates defensively.
fn sanitise_legacy_agent_name(input: &str) -> Option<String> {
    let mut out = String::with_capacity(input.len());
    let mut prev_underscore = true; // treat start as if preceded by `_`
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            let lc = c.to_ascii_lowercase();
            // Slugs must start with a letter.
            if out.is_empty() && lc.is_ascii_digit() {
                continue;
            }
            out.push(lc);
            prev_underscore = false;
        } else if !prev_underscore && !out.is_empty() {
            out.push('_');
            prev_underscore = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.len() > 64 {
        out.truncate(64);
        while out.ends_with('_') {
            out.pop();
        }
    }
    if out.len() < 3 {
        return None;
    }
    // Reserved slugs (see `apps::builder::is_reserved`) should not
    // become an agent name via rename. If the sanitised form
    // collides with one, return None and let the caller flag for
    // manual review.
    const RESERVED: &[&str] = &[
        "rabble_swarm",
        "bestiary_workspace",
        "personal_workspace",
        "fermi_forecast",
        "silat_workspace",
    ];
    if RESERVED.contains(&out.as_str()) {
        return None;
    }
    Some(out)
}

pub async fn admin_legacy_agent_slugs_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<LegacySlugsQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;
    let admin_user_id = principal.user_id();

    // 1. Load every agent (optionally prefix-filtered) — then filter
    //    to legacy names client-side. `slug::validate` is the
    //    authoritative rule; anything it rejects is legacy data by
    //    definition.
    //
    //    v0.10.24: `prefix` is pushed down to SQL via LIKE so we
    //    don't pull all 500+ rows to filter 9. `%` is appended by
    //    the handler — caller supplies the literal prefix. The
    //    escape-handling here treats `_` and `%` as literal in the
    //    caller-supplied prefix (unusual on operator input; if it
    //    matters we'd need pg_escape, which sqlx doesn't expose
    //    directly; the current callers are admin CLI users so
    //    controlled input is fine).
    let rows = match q.prefix.as_deref() {
        Some(prefix) if !prefix.is_empty() => {
            let like_pattern = format!("{}%", prefix);
            sqlx::query(
                "SELECT agent_id, agent_name FROM agents \
                 WHERE agent_name LIKE $1 ORDER BY agent_name",
            )
            .bind(&like_pattern)
            .fetch_all(&state.db)
            .await
        }
        _ => {
            sqlx::query("SELECT agent_id, agent_name FROM agents ORDER BY agent_name")
                .fetch_all(&state.db)
                .await
        }
    }
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("list agents: {}", e),
        )
    })?;

    // 2. For every legacy name, propose a sanitised form and check
    //    for collisions. Two collision axes:
    //      (a) existing_names: some other agent already has this name.
    //      (b) proposal_names: two legacy names sanitise to the same
    //          target within this batch. First-come wins; the second
    //          gets flagged.
    let mut existing_names: std::collections::HashSet<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("agent_name").ok())
        .collect();

    let mut proposals: Vec<(uuid::Uuid, String, Option<String>, Option<String>)> = Vec::new();
    let mut claimed_new: std::collections::HashSet<String> = std::collections::HashSet::new();

    for row in &rows {
        let agent_id: uuid::Uuid = match row.try_get("agent_id") {
            Ok(id) => id,
            Err(_) => continue,
        };
        let old_name: String = match row.try_get("agent_name") {
            Ok(n) => n,
            Err(_) => continue,
        };
        // Skip agents that already satisfy the slug rule.
        if fermi::slug::validate(&old_name).is_ok() {
            continue;
        }
        let (proposal, collision) = match sanitise_legacy_agent_name(&old_name) {
            None => (
                None,
                Some(
                    "sanitiser produced < 3 chars or reserved slug — needs manual rename"
                        .to_string(),
                ),
            ),
            Some(new) => {
                // Re-validate defensively — sanitiser should always
                // produce a valid slug when Some.
                if let Err(msg) = fermi::slug::validate(&new) {
                    (
                        None,
                        Some(format!(
                            "sanitiser produced invalid slug `{}`: {} — needs manual rename",
                            new, msg
                        )),
                    )
                } else if existing_names.contains(&new) && new != old_name {
                    (
                        Some(new.clone()),
                        Some(format!("another agent already uses `{}`", new)),
                    )
                } else if claimed_new.contains(&new) {
                    (
                        Some(new.clone()),
                        Some(format!(
                            "a legacy-name peer in this batch sanitises to `{}` as well",
                            new
                        )),
                    )
                } else {
                    claimed_new.insert(new.clone());
                    (Some(new), None)
                }
            }
        };
        proposals.push((agent_id, old_name, proposal, collision));
    }

    // v0.10.24: apply `limit` AFTER the slug-rule filter so the cap
    // is meaningful ("first 50 legacy names") rather than "first 50
    // agents alphabetically, most of which may not be legacy at all".
    // Deterministic order because the SELECT is `ORDER BY agent_name`
    // — so `--limit N` from the same starting state always picks the
    // same N, safe to run repeatedly to chunk through the backlog.
    let total_matched_before_limit = proposals.len();
    let limited = q.limit.map(|n| n < proposals.len()).unwrap_or(false);
    if let Some(n) = q.limit {
        proposals.truncate(n);
    }

    // 3. Count `fermi_forecasts.agents_used` JSONB references per
    //    legacy name. Pure informational — renames only happen after
    //    the operator eyeballs the report. `agents_used` stores an
    //    array of `{agent_name: "..."}` objects.
    //
    //    v0.10.23: was one COUNT per legacy name (N+1). With 43
    //    legacy names and no GIN index, that's 43 sequential seq-scans
    //    of `fermi_forecasts` — blew past the abw-cli 60s client
    //    timeout. Rewritten as one query: unnest the legacy-name
    //    array once, LEFT JOIN against the JSONB containment, GROUP
    //    BY. Combined with mig-168 (GIN index on `agents_used`) the
    //    endpoint returns in milliseconds even at 10x current scale.
    let mut forecast_ref_counts: std::collections::HashMap<String, i64> =
        std::collections::HashMap::new();
    if !proposals.is_empty() {
        let legacy_names: Vec<String> = proposals
            .iter()
            .map(|(_id, old, _new, _coll)| old.clone())
            .collect();

        // One round trip, one seq-scan (or one GIN lookup per name
        // when mig-168 is present). LEFT JOIN so names with zero
        // references still appear in the result set with count 0.
        let rows = sqlx::query(
            r#"SELECT ln.name AS name, COUNT(f.id)::int8 AS refs
                 FROM unnest($1::text[]) AS ln(name)
                 LEFT JOIN fermi_forecasts f
                        ON f.agents_used @> jsonb_build_array(
                             jsonb_build_object('agent_name', ln.name))
                GROUP BY ln.name"#,
        )
        .bind(&legacy_names)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("count agents_used refs: {}", e),
            )
        })?;

        for row in rows {
            let name: String = row.try_get("name").unwrap_or_default();
            let refs: i64 = row.try_get("refs").unwrap_or(0);
            forecast_ref_counts.insert(name, refs);
        }
    }

    // 4. If applying, execute renames + JSONB backfill in ONE
    //    transaction so a partial write can't leave the DB in a
    //    torn state. We loop inside the tx per row so a mid-tx
    //    failure still rolls back everything.
    let mut applied_count: usize = 0;
    let mut skipped_collisions: usize = 0;
    let mut unrecoverable: usize = 0;

    if q.apply {
        let mut tx = state.db.begin().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("begin tx: {}", e),
            )
        })?;

        for (agent_id, old_name, proposal, collision) in &proposals {
            if collision.is_some() {
                skipped_collisions += 1;
                continue;
            }
            let new_name = match proposal {
                Some(n) => n.clone(),
                None => {
                    unrecoverable += 1;
                    continue;
                }
            };

            // Rename the agent row itself.
            sqlx::query("UPDATE agents SET agent_name = $1 WHERE agent_id = $2")
                .bind(&new_name)
                .bind(agent_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("rename agent {}: {}", old_name, e),
                    )
                })?;

            // Backfill `fermi_forecasts.agents_used` JSONB. Rewrite
            // every `{agent_name: <old>}` object to `{agent_name: <new>}`
            // in-place, preserving any sibling keys on the object.
            // Using jsonb_path array-map is the cleanest option in
            // Postgres 12+: for each element whose agent_name matches,
            // set agent_name = new. Fallback: string-substitute the
            // whole JSONB by casting to text and back, but that risks
            // false matches on any string equal to old_name; use the
            // structural path variant instead.
            //
            // Assumption: `agents_used` is either NULL or an ARRAY of
            // OBJECTS (verified against every writer in the codebase).
            // If it's ever a bare string somewhere in prod data, the
            // jsonb_path_query will simply skip it — no rewrite.
            sqlx::query(
                r#"UPDATE fermi_forecasts
                      SET agents_used = (
                          SELECT jsonb_agg(
                              CASE
                                  WHEN elem ? 'agent_name'
                                       AND elem->>'agent_name' = $1
                                  THEN jsonb_set(elem, '{agent_name}', to_jsonb($2::text))
                                  ELSE elem
                              END
                          )
                          FROM jsonb_array_elements(agents_used) AS elem
                      )
                      WHERE agents_used IS NOT NULL
                        AND jsonb_typeof(agents_used) = 'array'
                        AND agents_used @> jsonb_build_array(
                              jsonb_build_object('agent_name', $1::text))"#,
            )
            .bind(old_name)
            .bind(&new_name)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!(
                        "backfill agents_used for {} → {}: {}",
                        old_name, new_name, e
                    ),
                )
            })?;

            // Log to admin_bypass_events. Same pattern as force_publish
            // (v0.10.5). target_type='agent', target_id=UUID string,
            // action='rename_legacy_slug'. Reason string includes both
            // sides of the mapping so the audit trail is self-contained.
            let details = json!({
                "old_name": old_name,
                "new_name": new_name,
                "forecast_refs_backfilled": forecast_ref_counts.get(old_name).copied().unwrap_or(0),
                "reason": "legacy slug produced by pre-d0f94e8 creation path; unroutable at /agent/<name>",
            });
            sqlx::query(
                "INSERT INTO admin_bypass_events \
                 (admin_user_id, target_type, target_id, action, details) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(&admin_user_id)
            .bind("agent")
            .bind(agent_id.to_string())
            .bind("rename_legacy_slug")
            .bind(&details)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("log admin_bypass_events for {}: {}", old_name, e),
                )
            })?;

            // Update the in-memory sets so a subsequent proposal in
            // the same batch can't collide with a name we just claimed.
            existing_names.remove(old_name);
            existing_names.insert(new_name.clone());
            applied_count += 1;
        }

        tx.commit().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("commit tx: {}", e),
            )
        })?;
    }

    // 5. Build the response report.
    let entries: Vec<Value> = proposals
        .iter()
        .map(|(agent_id, old_name, proposal, collision)| {
            let action = if q.apply {
                if collision.is_some() {
                    "skipped:collision"
                } else if proposal.is_none() {
                    "skipped:unrecoverable"
                } else {
                    "renamed"
                }
            } else {
                "audit_only"
            };
            json!({
                "agent_id": agent_id.to_string(),
                "old_name": old_name,
                "proposed_new_name": proposal,
                "collision": collision,
                "forecast_refs": forecast_ref_counts.get(old_name).copied().unwrap_or(0),
                "action_taken": action,
            })
        })
        .collect();

    let would_rename = proposals
        .iter()
        .filter(|(_, _, p, c)| p.is_some() && c.is_none())
        .count();
    let collision_count = proposals.iter().filter(|(_, _, _, c)| c.is_some()).count();

    Ok(Json(json!({
        // Post-limit counts — what this response body actually covers.
        "total_legacy": proposals.len(),
        "would_rename": would_rename,
        "collisions": collision_count,
        "applied": if q.apply { Some(applied_count) } else { None },
        "skipped_collisions": if q.apply { Some(skipped_collisions) } else { None },
        "skipped_unrecoverable": if q.apply { Some(unrecoverable) } else { None },
        "apply": q.apply,
        "entries": entries,
        // v0.10.24: filter + batch metadata so the caller knows when
        // more legacy rows remain beyond this response. `total_matched`
        // is the full slug-rule-failing set inside the `prefix` filter
        // (or all agents if no prefix); `truncated` is true when
        // `limit` clipped the tail.
        "prefix": q.prefix,
        "limit": q.limit,
        "total_matched": total_matched_before_limit,
        "truncated": limited,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// v0.10.25 — Cleanup orphan test-fixture agents
//
// The 5 leaking test fixtures in agent-bestiary/memory/src/ have
// been inserting `test_agent_<uuid>` rows into the shared DB
// without cleanup since forever. v0.10.20's legacy-slug audit
// surfaced 565 of them. Renaming preserves garbage; the right
// remedy is DELETE, gated by strong safety criteria so we never
// touch a real agent.
//
// Endpoint: /api/admin/agents/cleanup-test-cruft
//   GET                     — dry-run
//   POST                    — dry-run
//   POST ?apply=true        — execute DELETE (cascades to episodes,
//                             semantic_rules, entities/facts,
//                             ontology_snapshots, workspace_agents,
//                             agent_versions, eval_*, dyad_*,
//                             observability_*, hitl_actions, and
//                             all mig-049 tables after mig-169).
//
// Safety criteria — a row is eligible for deletion ONLY when ALL
// of these hold:
//
//   * `agent_name LIKE '<prefix>%'` (default prefix: `test_agent_`)
//   * no rows in `episodes`          (never ran real workload)
//   * `created_at < NOW() - INTERVAL '<older_than_hours> hours'`
//                                     (default: 24h grace period)
//   * `tier NOT IN ('curated','system')`  (never touch platform agents)
//
// The never-ran gate reads `episodes` — the write-time record of every
// run — and NOT `agents.total_executions`. That column is never written
// by any code path (see migrations/192 and src/rollup_trust.rs), so it is
// zero for every row in the table, and a `total_executions = 0` predicate
// was therefore vacuous: it eliminated nothing and protected nothing. The
// guard read like defense-in-depth while contributing none.
//
// Nothing was ever wrongly deleted, because the prefix, tier and age
// gates are individually sufficient. But a safety criterion that cannot
// fail is worse than an absent one: it makes the remaining gates look
// more redundant than they are, so the next person to relax one thinks
// there are four backstops when there are three.
//
// Deliberately NOT gated on `visibility` or `status` — the leaking
// test fixtures in `agent-bestiary/memory/src/` create rows with
// `visibility = 'public'` (either explicitly in the test_agent()
// factory or via the mig-010 default), so a visibility check would
// PROTECT the exact rows we want to clean up. The remaining four
// gates (explicit prefix + zero executions + grace period + tier
// exclusion) are sufficient defense-in-depth.
//
// Every deletion is logged to admin_bypass_events with the
// `{agent_id, agent_name, tier, created_at}` snapshot as details.
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct CleanupTestCruftQuery {
    #[serde(default)]
    pub apply: bool,
    /// Name prefix identifying test cruft. Default `test_agent_`
    /// matches the fixture pattern in `agent-bestiary/memory/src/`.
    /// Change with care — too-permissive prefixes (e.g. `test`)
    /// could match legitimate agents named `testing_advisor` etc.
    #[serde(default)]
    pub prefix: Option<String>,
    /// Grace period in hours. Rows created within this window are
    /// protected — they might be an actively-running test suite
    /// that hasn't torn down yet. Default 24.
    #[serde(default)]
    pub older_than_hours: Option<i64>,
    /// Cap batch size. Cleanup runs 2 statements per row in one
    /// transaction (DELETE + audit INSERT), so this is safety
    /// against the 60s client timeout on very large batches.
    #[serde(default)]
    pub limit: Option<usize>,
}

pub async fn admin_cleanup_test_cruft_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<CleanupTestCruftQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;
    let admin_user_id = principal.user_id();

    let prefix = q
        .prefix
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("test_agent_")
        .to_string();
    let older_than_hours = q.older_than_hours.unwrap_or(24).max(0);
    let like_pattern = format!("{}%", prefix);

    // 1. Find candidate rows. Every safety predicate is baked into
    //    the SELECT so the caller can't accidentally bypass one by
    //    tweaking a query param. `INTERVAL '$1 hours'` isn't a
    //    Postgres literal we can bind directly; use `make_interval`
    //    which does accept a bound integer.
    let rows = sqlx::query(
        "SELECT a.agent_id, a.agent_name, a.tier, a.visibility, a.status, \
                a.created_at, \
                COALESCE(r.executions, 0) AS executions \
           FROM agents a \
           LEFT JOIN agent_execution_rollup r ON r.agent_id = a.agent_id \
          WHERE a.agent_name LIKE $1 \
            AND r.agent_id IS NULL \
            AND a.created_at < NOW() - make_interval(hours => $2::int) \
            AND a.tier NOT IN ('curated', 'system') \
          ORDER BY a.created_at ASC",
    )
    .bind(&like_pattern)
    .bind(older_than_hours as i32)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("list cruft candidates: {}", e),
        )
    })?;

    let total_matched = rows.len();

    // 2. Apply `limit` (cap on batch size).
    let mut candidates: Vec<(uuid::Uuid, String, String, chrono::DateTime<chrono::Utc>)> =
        Vec::with_capacity(rows.len());
    for row in &rows {
        let agent_id: uuid::Uuid = match row.try_get("agent_id") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let agent_name: String = row.try_get("agent_name").unwrap_or_default();
        let tier: String = row.try_get("tier").unwrap_or_default();
        let created_at: chrono::DateTime<chrono::Utc> = row
            .try_get("created_at")
            .unwrap_or_else(|_| chrono::Utc::now());
        candidates.push((agent_id, agent_name, tier, created_at));
    }
    let truncated = q.limit.map(|n| n < candidates.len()).unwrap_or(false);
    if let Some(n) = q.limit {
        candidates.truncate(n);
    }

    // 3. If applying, DELETE in one transaction + audit-log each.
    //    CASCADE handles episodes/entities/facts/versions/etc.
    //    After mig-169 the mig-049 tables also cascade.
    let mut deleted_count: usize = 0;
    let mut failures: Vec<Value> = Vec::new();

    if q.apply && !candidates.is_empty() {
        let mut tx = state.db.begin().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("begin tx: {}", e),
            )
        })?;

        for (agent_id, agent_name, tier, created_at) in &candidates {
            let details = json!({
                "agent_id":     agent_id.to_string(),
                "agent_name":   agent_name,
                "tier":         tier,
                "created_at":   created_at.to_rfc3339(),
                "prefix":       prefix,
                "older_than_hours": older_than_hours,
                "reason":       "orphan test-fixture row — no episodes, older than grace period",
            });

            // Log FIRST so the audit trail exists even if the
            // DELETE errors out for some unexpected reason (partial
            // rollback still preserves the trail because both are
            // in the same tx — either both land or neither).
            if let Err(e) = sqlx::query(
                "INSERT INTO admin_bypass_events \
                 (admin_user_id, target_type, target_id, action, details) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(&admin_user_id)
            .bind("agent")
            .bind(agent_id.to_string())
            .bind("delete_test_cruft")
            .bind(&details)
            .execute(&mut *tx)
            .await
            {
                failures.push(json!({
                    "agent_id": agent_id.to_string(),
                    "agent_name": agent_name,
                    "stage": "audit_insert",
                    "error": e.to_string(),
                }));
                continue;
            }

            match sqlx::query("DELETE FROM agents WHERE agent_id = $1")
                .bind(agent_id)
                .execute(&mut *tx)
                .await
            {
                Ok(_) => {
                    deleted_count += 1;
                }
                Err(e) => {
                    failures.push(json!({
                        "agent_id": agent_id.to_string(),
                        "agent_name": agent_name,
                        "stage": "delete",
                        "error": e.to_string(),
                    }));
                }
            }
        }

        // Commit even if some rows failed — the successful ones
        // and their audit rows should land. Failures are surfaced
        // in the response body so the operator can decide next step.
        tx.commit().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("commit tx: {}", e),
            )
        })?;
    }

    // 4. Build entry list for the response.
    let entries: Vec<Value> = candidates
        .iter()
        .map(|(agent_id, agent_name, tier, created_at)| {
            json!({
                "agent_id":    agent_id.to_string(),
                "agent_name":  agent_name,
                "tier":        tier,
                "created_at":  created_at.to_rfc3339(),
                "action_taken": if q.apply { "deleted" } else { "audit_only" },
            })
        })
        .collect();

    Ok(Json(json!({
        "prefix":              prefix,
        "older_than_hours":    older_than_hours,
        "limit":               q.limit,
        "apply":               q.apply,
        "total_matched":       total_matched,
        "in_this_batch":       candidates.len(),
        "truncated":           truncated,
        "deleted":             if q.apply { Some(deleted_count) } else { None },
        "failures":            failures,
        "entries":             entries,
    })))
}
