//! Agent wallet handlers — view earnings, collect, allocate, auto-collect.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use fermi_auth::{
    credit_charge, credit_deposit_typed, get_or_create_wallet, rbac, AuthPrincipal, ObjectType,
    Visibility,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;

/// One agent, resolved the way every other agent endpoint resolves one.
///
/// # The bug this ends
///
/// Every handler in this file opened with `Uuid::parse_str(&agent_id)` and
/// returned `400 Invalid agent ID` when it failed. Every URL on the platform
/// carries the **name** — `/agent/football_analyst`, `/specimen/…` — so the
/// Manage tab's wallet panel called `/api/agents/football_analyst/wallet`,
/// got a 400, and printed **"Could not load wallet"**. An owner reading that
/// has been told their agent has no bank account; what happened is that this
/// file spoke a different dialect from the other ~40 agent routes, all of
/// which go through `resolve_agent`.
///
/// # Why the uuid still travels
///
/// Wallets are keyed by `owner_ref`, and the agent wallet's `owner_ref` is
/// the uuid **string**. Passing the path parameter straight to
/// `get_or_create_wallet` would have minted a SECOND wallet named
/// `football_analyst` the first time anyone used a name — an empty balance
/// beside a funded one, with no error. So callers get the parsed uuid back
/// and `wallet_ref()` is the only thing they may hand to the wallet layer.
async fn resolve_agent_for_wallet(
    state: &AppState,
    agent_id: &str,
) -> Result<(Uuid, Option<String>, String), (StatusCode, String)> {
    let agent = crate::resolve_agent(state, agent_id).await?;
    Ok((agent.agent_id, agent.owner_id, agent.agent_name))
}

/// The string the wallet layer keys an agent wallet by. Never the path
/// parameter — see [`resolve_agent_for_wallet`].
fn wallet_ref(agent_uuid: Uuid) -> String {
    agent_uuid.to_string()
}

/// v0.10.5: substrate RBAC. Wallet operations are Admin-only
/// (financial actions on the agent's own credit balance). No share
/// grants access.
///
/// Note: we pass `Visibility::Private` unconditionally because
/// wallet access does not depend on the agent's public/private
/// setting — an unpublished draft's wallet still needs owner-only
/// access. The visibility parameter to `rbac::require_admin_on`
/// only affects the "public grants View to everyone" branch, which
/// is irrelevant for Admin-required calls.
async fn require_admin_on_agent(
    pool: &sqlx::PgPool,
    principal: &AuthPrincipal,
    agent_uuid: Uuid,
    owner_id: &Option<String>,
) -> Result<(), (StatusCode, String)> {
    rbac::require_admin_on(
        pool,
        principal,
        ObjectType::Agent,
        &agent_uuid.to_string(),
        owner_id.as_deref().unwrap_or(""),
        Visibility::Private,
    )
    .await
    .map(|_| ())
}

/// GET /api/agents/:id/wallet — agent wallet summary
pub async fn get_agent_wallet_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let (agent_uuid, owner_id, agent_name) = resolve_agent_for_wallet(&state, &agent_id).await?;

    let auto_collect_pct: i32 =
        sqlx::query_scalar("SELECT auto_collect_pct FROM agents WHERE agent_id = $1")
            .bind(agent_uuid)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

    require_admin_on_agent(&state.db, &principal, agent_uuid, &owner_id).await?;

    // Get or create agent wallet
    let wallet = get_or_create_wallet(&state.db, "agent", &wallet_ref(agent_uuid))
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
        // The uuid, not whatever the caller happened to put in the path. A
        // response that echoes the request cannot be used to key anything.
        "agent_id": agent_uuid,
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
    let (agent_uuid, owner_id, _) = resolve_agent_for_wallet(&state, &agent_id).await?;

    require_admin_on_agent(&state.db, &principal, agent_uuid, &owner_id).await?;

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
    let (agent_uuid, owner_id, agent_name) = resolve_agent_for_wallet(&state, &agent_id).await?;
    let user_id = principal.user_id();

    require_admin_on_agent(&state.db, &principal, agent_uuid, &owner_id).await?;

    let agent_wallet = get_or_create_wallet(&state.db, "agent", &wallet_ref(agent_uuid))
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
    let (agent_uuid, owner_id, _) = resolve_agent_for_wallet(&state, &agent_id).await?;

    require_admin_on_agent(&state.db, &principal, agent_uuid, &owner_id).await?;

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

    let agent_wallet = get_or_create_wallet(&state.db, "agent", &wallet_ref(agent_uuid))
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
        // The uuid, so the ledger reference resolves whether the caller
        // addressed the agent by name or by id.
        Some(&wallet_ref(agent_uuid)),
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
    let (agent_uuid, owner_id, _) = resolve_agent_for_wallet(&state, &agent_id).await?;

    require_admin_on_agent(&state.db, &principal, agent_uuid, &owner_id).await?;

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

#[cfg(test)]
mod tests {
    /// Every route on this platform is addressed by agent NAME.
    ///
    /// `/agent/football_analyst`, `/specimen/football_analyst`,
    /// `/api/agents/football_analyst/…` — the uuid appears in no URL a human
    /// or a page ever holds. Every handler in this file nonetheless opened
    /// with `Uuid::parse_str(&agent_id)` and returned `400 Invalid agent ID`,
    /// so the Manage tab's wallet panel rendered **"Could not load wallet"**
    /// for every agent on the platform, for as long as the panel has existed.
    ///
    /// The failure is invisible to the type system and to every unit test,
    /// because the handler is correct in isolation — it is wrong only about
    /// which dialect the rest of the platform speaks. So this reads the file:
    /// the path parameter goes through `resolve_agent`, like everywhere else,
    /// or the wallet stops being reachable again.
    ///
    /// The needles are assembled rather than written, because this test's own
    /// prose names the pattern it forbids and a literal would match itself.
    #[test]
    fn the_wallet_is_addressed_the_way_every_other_agent_route_is() {
        // Code only. Both the resolver's doc comment and this test's own prose
        // quote the patterns being forbidden, which is the point of them.
        let whole = include_str!("agent_wallet.rs");
        let code: String = whole[..whole.find("#[cfg(test)]").unwrap_or(whole.len())]
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let src = code.as_str();
        let parse_path = format!("Uuid::{}(&agent_id)", "parse_str");
        let wallet_by_path = format!(
            "get_or_create_wallet(&state.db, {}, &agent_id)",
            "\"agent\""
        );
        assert!(
            !src.contains(&parse_path),
            "a wallet handler parses the path parameter as a uuid. Every URL on \
             this platform carries the agent NAME, so that is a 400 on every \
             real request and the panel renders \"Could not load wallet\". Use \
             `resolve_agent_for_wallet`, which accepts either."
        );
        assert!(
            !src.contains(&wallet_by_path),
            "an agent wallet is being looked up by the raw path parameter. \
             `owner_ref` is the uuid STRING, so a request addressed by name \
             would mint a second, empty wallet beside the funded one and report \
             a zero balance with no error. Use `wallet_ref(agent_uuid)`."
        );
    }
}
