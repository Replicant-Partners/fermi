//! Agent wallet handlers — view earnings, collect, allocate, auto-collect.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use fermi_auth::{credit_charge, credit_deposit_typed, get_or_create_wallet, AuthPrincipal};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;

/// Verify the caller owns this agent (or is admin).
fn require_owner_or_admin(
    owner_id: &Option<String>,
    user_id: &str,
    is_admin: bool,
) -> Result<(), (StatusCode, String)> {
    if is_admin {
        return Ok(());
    }
    match owner_id {
        Some(oid) if oid == user_id => Ok(()),
        _ => Err((
            StatusCode::FORBIDDEN,
            "You do not own this agent".to_string(),
        )),
    }
}

/// GET /api/agents/:id/wallet — agent wallet summary
pub async fn get_agent_wallet_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let agent_uuid = Uuid::parse_str(&agent_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid agent ID".to_string()))?;

    // Look up agent to check ownership
    let agent_row =
        sqlx::query("SELECT user_id, agent_name, auto_collect_pct FROM agents WHERE agent_id = $1")
            .bind(agent_uuid)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("DB error: {}", e),
                )
            })?
            .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    let owner_id: Option<String> = agent_row.try_get("user_id").unwrap_or(None);
    let agent_name: String = agent_row.try_get("agent_name").unwrap_or_default();
    let auto_collect_pct: i32 = agent_row.try_get("auto_collect_pct").unwrap_or(0);

    require_owner_or_admin(&owner_id, &principal.user_id(), principal.can_admin())?;

    // Get or create agent wallet
    let wallet = get_or_create_wallet(&state.db, "agent", &agent_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    // Total earned (from agent_episode_payouts)
    let total_earned: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount), 0) FROM agent_episode_payouts WHERE agent_id = $1",
    )
    .bind(agent_uuid)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    // Total collected (agent_collect_out debits)
    let total_collected: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(ABS(amount)), 0) FROM credit_ledger WHERE wallet_id = $1 AND tx_type = 'agent_collect_out'",
    )
    .bind(wallet.wallet_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    // Total allocated (agent_allocate_* debits)
    let total_allocated: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(ABS(amount)), 0) FROM credit_ledger WHERE wallet_id = $1 AND tx_type LIKE 'agent_allocate_%'",
    )
    .bind(wallet.wallet_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    Ok(Json(json!({
        "wallet_id": wallet.wallet_id,
        "agent_id": agent_id,
        "agent_name": agent_name,
        "balance": wallet.balance,
        "total_earned": total_earned,
        "total_collected": total_collected,
        "total_allocated": total_allocated,
        "auto_collect_pct": auto_collect_pct,
    })))
}

#[derive(Deserialize)]
pub struct EarningsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

/// GET /api/agents/:id/earnings — payout history
pub async fn get_agent_earnings_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(params): Query<EarningsQuery>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let agent_uuid = Uuid::parse_str(&agent_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid agent ID".to_string()))?;

    let owner_id: Option<String> =
        sqlx::query_scalar("SELECT user_id FROM agents WHERE agent_id = $1")
            .bind(agent_uuid)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("DB error: {}", e),
                )
            })?
            .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    require_owner_or_admin(&owner_id, &principal.user_id(), principal.can_admin())?;

    let limit = params.limit.min(200).max(1);
    let offset = params.offset.max(0);

    let rows = sqlx::query(
        r#"SELECT p.payout_id, p.episode_id, p.amount, p.workspace_id,
                  p.contribution_tier, p.created_at
           FROM agent_episode_payouts p
           WHERE p.agent_id = $1
           ORDER BY p.created_at DESC
           LIMIT $2 OFFSET $3"#,
    )
    .bind(agent_uuid)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Query error: {}", e),
        )
    })?;

    let earnings: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "payout_id": r.try_get::<Uuid, _>("payout_id").unwrap_or_default(),
                "episode_id": r.try_get::<Uuid, _>("episode_id").unwrap_or_default(),
                "amount": r.try_get::<i32, _>("amount").unwrap_or(0),
                "workspace_id": r.try_get::<Option<Uuid>, _>("workspace_id").unwrap_or(None),
                "contribution_tier": r.try_get::<Option<String>, _>("contribution_tier").unwrap_or(None),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                    .map(|t| t.to_rfc3339()).unwrap_or_default(),
            })
        })
        .collect();

    // Total count for pagination
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM agent_episode_payouts WHERE agent_id = $1")
            .bind(agent_uuid)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

    Ok(Json(json!({
        "earnings": earnings,
        "total": total,
    })))
}

#[derive(Deserialize)]
pub struct CollectBody {
    pub amount: serde_json::Value, // number or "all"
}

/// POST /api/agents/:id/collect — transfer from agent wallet to owner wallet
pub async fn collect_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    principal: AuthPrincipal,
    Json(body): Json<CollectBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let agent_uuid = Uuid::parse_str(&agent_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid agent ID".to_string()))?;

    let agent_row = sqlx::query("SELECT user_id, agent_name FROM agents WHERE agent_id = $1")
        .bind(agent_uuid)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("DB error: {}", e),
            )
        })?
        .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    let owner_id: Option<String> = agent_row.try_get("user_id").unwrap_or(None);
    let agent_name: String = agent_row.try_get("agent_name").unwrap_or_default();
    let user_id = principal.user_id();

    require_owner_or_admin(&owner_id, &user_id, principal.can_admin())?;

    let agent_wallet = get_or_create_wallet(&state.db, "agent", &agent_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    // Resolve amount
    let amount: i32 = match &body.amount {
        serde_json::Value::String(s) if s == "all" => agent_wallet.balance,
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(|v| v as i32)
            .ok_or((StatusCode::BAD_REQUEST, "Invalid amount".to_string()))?,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "amount must be a number or \"all\"".to_string(),
            ))
        }
    };

    if amount <= 0 {
        return Err((StatusCode::BAD_REQUEST, "Nothing to collect".to_string()));
    }

    if amount > agent_wallet.balance {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Insufficient agent balance: have {}, requested {}",
                agent_wallet.balance, amount
            ),
        ));
    }

    // Debit agent wallet
    credit_charge(
        &state.db,
        agent_wallet.wallet_id,
        amount,
        "agent_collect_out",
        &format!("Collected by owner"),
        None,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Charge failed: {}", e),
        )
    })?;

    // Credit owner wallet
    let actual_owner = owner_id.unwrap_or_else(|| user_id.clone());
    let owner_wallet = get_or_create_wallet(&state.db, "user", &actual_owner)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    credit_deposit_typed(
        &state.db,
        owner_wallet.wallet_id,
        amount,
        "agent_collect_in",
        &format!("Collected from {}", agent_name),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Deposit failed: {}", e),
        )
    })?;

    // Fetch updated balances
    let new_agent_balance: i32 =
        sqlx::query_scalar("SELECT balance FROM wallets WHERE wallet_id = $1")
            .bind(agent_wallet.wallet_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

    let new_owner_balance: i32 =
        sqlx::query_scalar("SELECT balance FROM wallets WHERE wallet_id = $1")
            .bind(owner_wallet.wallet_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

    Ok(Json(json!({
        "collected": amount,
        "agent_balance": new_agent_balance,
        "owner_balance": new_owner_balance,
    })))
}

#[derive(Deserialize)]
pub struct AllocateBody {
    pub service: String,
    pub amount: i32,
}

/// POST /api/agents/:id/allocate — spend agent credits on a service
pub async fn allocate_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    principal: AuthPrincipal,
    Json(body): Json<AllocateBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let agent_uuid = Uuid::parse_str(&agent_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid agent ID".to_string()))?;

    let owner_id: Option<String> =
        sqlx::query_scalar("SELECT user_id FROM agents WHERE agent_id = $1")
            .bind(agent_uuid)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("DB error: {}", e),
                )
            })?
            .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    require_owner_or_admin(&owner_id, &principal.user_id(), principal.can_admin())?;

    let (tx_type, budget_column) = match body.service.as_str() {
        "dream_cycle" => ("agent_allocate_dream", Some("dreaming_budget_credits")),
        "education" => ("agent_allocate_education", Some("education_budget_credits")),
        "coherence_eval" => ("agent_allocate_coherence", None),
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "service must be dream_cycle, education, or coherence_eval".to_string(),
            ))
        }
    };

    if body.amount <= 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Amount must be positive".to_string(),
        ));
    }

    let agent_wallet = get_or_create_wallet(&state.db, "agent", &agent_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    if body.amount > agent_wallet.balance {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Insufficient agent balance: have {}, requested {}",
                agent_wallet.balance, body.amount
            ),
        ));
    }

    // Debit agent wallet
    credit_charge(
        &state.db,
        agent_wallet.wallet_id,
        body.amount,
        tx_type,
        &format!("Allocate to {}", body.service),
        Some(&agent_id),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Charge failed: {}", e),
        )
    })?;

    // Update agent budget if applicable
    if let Some(col) = budget_column {
        let query = format!(
            "UPDATE agents SET {} = {} + $1 WHERE agent_id = $2",
            col, col
        );
        let _ = sqlx::query(&query)
            .bind(body.amount)
            .bind(agent_uuid)
            .execute(&state.db)
            .await;
    }

    let new_balance: i32 = sqlx::query_scalar("SELECT balance FROM wallets WHERE wallet_id = $1")
        .bind(agent_wallet.wallet_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    Ok(Json(json!({
        "allocated": body.amount,
        "service": body.service,
        "agent_balance": new_balance,
    })))
}

#[derive(Deserialize)]
pub struct AutoCollectBody {
    pub pct: i32,
}

/// PUT /api/agents/:id/auto-collect — set auto-collect percentage
pub async fn set_auto_collect_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    principal: AuthPrincipal,
    Json(body): Json<AutoCollectBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let agent_uuid = Uuid::parse_str(&agent_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid agent ID".to_string()))?;

    let owner_id: Option<String> =
        sqlx::query_scalar("SELECT user_id FROM agents WHERE agent_id = $1")
            .bind(agent_uuid)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("DB error: {}", e),
                )
            })?
            .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    require_owner_or_admin(&owner_id, &principal.user_id(), principal.can_admin())?;

    if body.pct < 0 || body.pct > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            "pct must be between 0 and 100".to_string(),
        ));
    }

    sqlx::query("UPDATE agents SET auto_collect_pct = $1 WHERE agent_id = $2")
        .bind(body.pct)
        .bind(agent_uuid)
        .execute(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Update failed: {}", e),
            )
        })?;

    Ok(Json(json!({
        "auto_collect_pct": body.pct,
    })))
}
