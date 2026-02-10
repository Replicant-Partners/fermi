//! Wallet and credit transaction handlers.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use fermi_auth::{
    credit_get_balance, credit_get_transactions, get_or_create_wallet, AuthPrincipal,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::AppState;
// ─── Wallet / Credits handlers ─────────────────────────────────────

pub async fn get_wallet_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    Ok(Json(json!({
        "wallet_id": wallet.wallet_id,
        "balance": wallet.balance,
        "total_deposited": wallet.total_deposited,
        "total_spent": wallet.total_spent,
        "created_at": wallet.created_at,
    })))
}

#[derive(Deserialize)]
pub struct TransactionsQuery {
    #[serde(default = "default_tx_limit")]
    limit: i64,
}

pub fn default_tx_limit() -> i64 {
    50
}

pub async fn get_transactions_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(params): Query<TransactionsQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    let txs = credit_get_transactions(&state.db, wallet.wallet_id, params.limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Transaction query error: {}", e),
            )
        })?;

    let tx_list: Vec<Value> = txs
        .iter()
        .map(|t| {
            json!({
                "tx_id": t.tx_id,
                "amount": t.amount,
                "balance_after": t.balance_after,
                "tx_type": t.tx_type,
                "description": t.description,
                "related_id": t.related_id,
                "created_at": t.created_at,
            })
        })
        .collect();

    Ok(Json(json!({
        "wallet_id": wallet.wallet_id,
        "balance": wallet.balance,
        "transactions": tx_list,
    })))
}
