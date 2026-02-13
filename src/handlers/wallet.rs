//! Wallet and credit transaction handlers.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use fermi_auth::{
    credit_charge, credit_deposit, credit_get_balance, credit_get_transactions,
    get_or_create_wallet, AuthPrincipal,
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

// ─── Credit Transfer ───────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TransferRequest {
    pub recipient_id: String,
    pub amount: i32,
    pub note: Option<String>,
}

/// POST /api/wallet/transfer — send credits to another user.
/// Charges sender (amount + 1cr flat + 2.5% gas), deposits amount to recipient.
pub async fn transfer_credits_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<TransferRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let sender_id = principal.user_id();

    if body.amount <= 0 {
        return Err((StatusCode::BAD_REQUEST, "Amount must be positive".into()));
    }
    if body.amount > 10000 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Maximum transfer is 10,000 credits".into(),
        ));
    }
    if body.recipient_id == sender_id {
        return Err((
            StatusCode::BAD_REQUEST,
            "Cannot transfer to yourself".into(),
        ));
    }

    // Verify recipient exists
    let recipient_exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE user_id = $1)")
            .bind(&body.recipient_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !recipient_exists {
        return Err((StatusCode::NOT_FOUND, "Recipient not found".into()));
    }

    // Calculate gas fee (1cr flat + 2.5%)
    let gas = 1 + std::cmp::max(0, (body.amount as f64 * 0.025) as i32);
    let total_charge = body.amount + gas;

    let note = body.note.as_deref().unwrap_or("Credit transfer");

    // Get sender wallet
    let sender_wallet = get_or_create_wallet(&state.db, "user", &sender_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    // Charge sender (amount + gas)
    credit_charge(
        &state.db,
        sender_wallet.wallet_id,
        total_charge,
        "transfer_out",
        &format!(
            "Transfer to {}: {} ({}cr + {}cr gas)",
            body.recipient_id, note, body.amount, gas
        ),
        Some(&body.recipient_id),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::PAYMENT_REQUIRED,
            format!("Insufficient balance: {}", e),
        )
    })?;

    // Deposit to recipient
    let recipient_wallet = get_or_create_wallet(&state.db, "user", &body.recipient_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Recipient wallet error: {}", e),
            )
        })?;

    credit_deposit(
        &state.db,
        recipient_wallet.wallet_id,
        body.amount,
        &format!("Transfer from {}: {}", sender_id, note),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Deposit failed: {}", e),
        )
    })?;

    // Create notification for recipient
    let _ = sqlx::query(
        "INSERT INTO notifications (id, user_id, notification_type, title, body, created_at)
         VALUES ($1, $2, 'credit_transfer', $3, $4, NOW())",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(&body.recipient_id)
    .bind(format!("Received {} credits", body.amount))
    .bind(format!(
        "You received {} credits from a friend: {}",
        body.amount, note
    ))
    .execute(&state.db)
    .await;

    // Return updated sender balance
    let new_balance = credit_get_balance(&state.db, sender_wallet.wallet_id)
        .await
        .unwrap_or(0);

    Ok(Json(json!({
        "status": "transferred",
        "amount": body.amount,
        "gas_fee": gas,
        "total_charged": total_charge,
        "new_balance": new_balance,
    })))
}
