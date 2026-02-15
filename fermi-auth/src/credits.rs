//! Credit ledger and wallet service.
//!
//! Integer-based credit system with append-only ledger.
//! Every mutation creates a ledger entry. Balance is denormalized on wallets.
//!
//! Balance split: `granted_balance` (non-transferable) + `purchased_balance` (transferable).
//! Grants (signup, admin) go to granted. Deposits/revenue go to purchased.
//! Charges spend granted first. Transfers require purchased balance.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::AuthError;

/// A user or workspace wallet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub wallet_id: Uuid,
    pub owner_type: String,
    pub owner_id: String,
    pub balance: i32,
    pub granted_balance: i32,
    pub purchased_balance: i32,
    pub total_deposited: i32,
    pub total_spent: i32,
    pub created_at: DateTime<Utc>,
}

/// A single ledger transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditTransaction {
    pub tx_id: Uuid,
    pub wallet_id: Uuid,
    pub amount: i32,
    pub balance_after: i32,
    pub tx_type: String,
    pub description: Option<String>,
    pub related_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

const WALLET_SELECT_COLS: &str =
    "wallet_id, owner_type, owner_id, balance, granted_balance, purchased_balance, total_deposited, total_spent, created_at";

fn row_to_wallet(row: &sqlx::postgres::PgRow) -> Wallet {
    Wallet {
        wallet_id: row.try_get("wallet_id").unwrap(),
        owner_type: row.try_get("owner_type").unwrap(),
        owner_id: row.try_get("owner_id").unwrap(),
        balance: row.try_get("balance").unwrap(),
        granted_balance: row.try_get("granted_balance").unwrap(),
        purchased_balance: row.try_get("purchased_balance").unwrap(),
        total_deposited: row.try_get("total_deposited").unwrap(),
        total_spent: row.try_get("total_spent").unwrap(),
        created_at: row.try_get("created_at").unwrap(),
    }
}

/// Get or create a wallet for the given owner
pub async fn get_or_create_wallet(
    pool: &PgPool,
    owner_type: &str,
    owner_id: &str,
) -> Result<Wallet, AuthError> {
    // Try to get existing wallet
    let existing = sqlx::query(&format!(
        "SELECT {} FROM wallets WHERE owner_id = $1",
        WALLET_SELECT_COLS
    ))
    .bind(owner_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::Internal(format!("DB error: {}", e)))?;

    if let Some(row) = existing {
        return Ok(row_to_wallet(&row));
    }

    // Create new wallet
    let row = sqlx::query(&format!(
        "INSERT INTO wallets (owner_type, owner_id)
         VALUES ($1, $2)
         RETURNING {}",
        WALLET_SELECT_COLS
    ))
    .bind(owner_type)
    .bind(owner_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AuthError::Internal(format!("Failed to create wallet: {}", e)))?;

    let wallet = row_to_wallet(&row);

    // Auto-grant onboarding credits for new user wallets
    if owner_type == "user" {
        let onboarding_amount = std::env::var("ONBOARDING_GRANT")
            .ok()
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(100);
        let _ = grant(
            pool,
            wallet.wallet_id,
            onboarding_amount,
            "Welcome onboarding grant",
        )
        .await;
        // Re-fetch to reflect updated balance
        let refreshed = sqlx::query(&format!(
            "SELECT {} FROM wallets WHERE wallet_id = $1",
            WALLET_SELECT_COLS
        ))
        .bind(wallet.wallet_id)
        .fetch_one(pool)
        .await
        .map_err(|e| AuthError::Internal(format!("DB error: {}", e)))?;
        return Ok(row_to_wallet(&refreshed));
    }

    Ok(wallet)
}

/// Deposit credits into a wallet (always succeeds).
/// Credits go to purchased_balance (transferable — earned/bought money).
/// IMPORTANT: No BEGIN/COMMIT - PgBouncer transaction mode handles this
pub async fn deposit(
    pool: &PgPool,
    wallet_id: Uuid,
    amount: i32,
    description: &str,
) -> Result<CreditTransaction, AuthError> {
    if amount <= 0 {
        return Err(AuthError::InvalidInput(
            "Deposit amount must be positive".into(),
        ));
    }

    // Atomic update — credits go to purchased_balance
    let wallet_row = sqlx::query(
        "UPDATE wallets
         SET balance = balance + $1,
             purchased_balance = purchased_balance + $1,
             total_deposited = total_deposited + $1
         WHERE wallet_id = $2
         RETURNING balance",
    )
    .bind(amount)
    .bind(wallet_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AuthError::Internal(format!("Wallet update failed: {}", e)))?;

    let new_balance: i32 = wallet_row
        .try_get("balance")
        .map_err(|e| AuthError::Internal(format!("Failed to get balance: {}", e)))?;

    // Create ledger entry
    let ledger_row = sqlx::query(
        "INSERT INTO credit_ledger (wallet_id, amount, balance_after, tx_type, description)
         VALUES ($1, $2, $3, 'deposit', $4)
         RETURNING tx_id, wallet_id, amount, balance_after, tx_type, description, related_id, created_at",
    )
    .bind(wallet_id)
    .bind(amount)
    .bind(new_balance)
    .bind(description)
    .fetch_one(pool)
    .await
    .map_err(|e| AuthError::Internal(format!("Ledger insert failed: {}", e)))?;

    Ok(row_to_transaction(&ledger_row))
}

/// Deposit credits with a custom tx_type (for agent_collect_in, execution_royalty, etc.)
/// Credits go to purchased_balance (transferable — earned revenue).
pub async fn deposit_typed(
    pool: &PgPool,
    wallet_id: Uuid,
    amount: i32,
    tx_type: &str,
    description: &str,
) -> Result<CreditTransaction, AuthError> {
    if amount <= 0 {
        return Err(AuthError::InvalidInput(
            "Deposit amount must be positive".into(),
        ));
    }

    let wallet_row = sqlx::query(
        "UPDATE wallets
         SET balance = balance + $1,
             purchased_balance = purchased_balance + $1,
             total_deposited = total_deposited + $1
         WHERE wallet_id = $2
         RETURNING balance",
    )
    .bind(amount)
    .bind(wallet_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AuthError::Internal(format!("Wallet update failed: {}", e)))?;

    let new_balance: i32 = wallet_row
        .try_get("balance")
        .map_err(|e| AuthError::Internal(format!("Failed to get balance: {}", e)))?;

    let ledger_row = sqlx::query(
        "INSERT INTO credit_ledger (wallet_id, amount, balance_after, tx_type, description)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING tx_id, wallet_id, amount, balance_after, tx_type, description, related_id, created_at",
    )
    .bind(wallet_id)
    .bind(amount)
    .bind(new_balance)
    .bind(tx_type)
    .bind(description)
    .fetch_one(pool)
    .await
    .map_err(|e| AuthError::Internal(format!("Ledger insert failed: {}", e)))?;

    Ok(row_to_transaction(&ledger_row))
}

/// Charge credits from a wallet (fails if insufficient balance).
/// Spends granted_balance first, then purchased_balance.
/// IMPORTANT: No BEGIN/COMMIT - PgBouncer transaction mode handles this
pub async fn charge(
    pool: &PgPool,
    wallet_id: Uuid,
    amount: i32,
    tx_type: &str,
    description: &str,
    related_id: Option<&str>,
) -> Result<CreditTransaction, AuthError> {
    if amount <= 0 {
        return Err(AuthError::InvalidInput(
            "Charge amount must be positive".into(),
        ));
    }

    // Atomic update with balance check — spend granted first, then purchased.
    // LEAST(granted_balance, $1) takes as much as possible from granted,
    // remainder ($1 - that amount) comes from purchased.
    let wallet_row = sqlx::query(
        "UPDATE wallets
         SET balance = balance - $1,
             total_spent = total_spent + $1,
             granted_balance = granted_balance - LEAST(granted_balance, $1),
             purchased_balance = purchased_balance - ($1 - LEAST(granted_balance, $1))
         WHERE wallet_id = $2 AND balance >= $1
         RETURNING balance",
    )
    .bind(amount)
    .bind(wallet_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::Internal(format!("Wallet update failed: {}", e)))?;

    let new_balance: i32 = match wallet_row {
        Some(row) => row
            .try_get("balance")
            .map_err(|e| AuthError::Internal(format!("Failed to get balance: {}", e)))?,
        None => {
            // Either wallet doesn't exist or insufficient balance
            let current_balance: Option<i32> =
                sqlx::query_scalar("SELECT balance FROM wallets WHERE wallet_id = $1")
                    .bind(wallet_id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| AuthError::Internal(format!("Failed to check balance: {}", e)))?;

            return Err(AuthError::InvalidInput(format!(
                "Insufficient balance: have {}, need {}",
                current_balance.unwrap_or(0),
                amount
            )));
        }
    };

    // Create ledger entry (negative amount for debit)
    let ledger_row = sqlx::query(
        "INSERT INTO credit_ledger (wallet_id, amount, balance_after, tx_type, description, related_id)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING tx_id, wallet_id, amount, balance_after, tx_type, description, related_id, created_at",
    )
    .bind(wallet_id)
    .bind(-amount)
    .bind(new_balance)
    .bind(tx_type)
    .bind(description)
    .bind(related_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AuthError::Internal(format!("Ledger insert failed: {}", e)))?;

    Ok(row_to_transaction(&ledger_row))
}

/// Charge credits from purchased_balance only (fails if insufficient purchased balance).
/// Used for transfers and workspace funding — granted credits cannot be transferred.
/// IMPORTANT: No BEGIN/COMMIT - PgBouncer transaction mode handles this
pub async fn charge_purchased_only(
    pool: &PgPool,
    wallet_id: Uuid,
    amount: i32,
    tx_type: &str,
    description: &str,
    related_id: Option<&str>,
) -> Result<CreditTransaction, AuthError> {
    if amount <= 0 {
        return Err(AuthError::InvalidInput(
            "Charge amount must be positive".into(),
        ));
    }

    // Atomic update — only deducts from purchased_balance
    let wallet_row = sqlx::query(
        "UPDATE wallets
         SET balance = balance - $1,
             total_spent = total_spent + $1,
             purchased_balance = purchased_balance - $1
         WHERE wallet_id = $2 AND purchased_balance >= $1
         RETURNING balance",
    )
    .bind(amount)
    .bind(wallet_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::Internal(format!("Wallet update failed: {}", e)))?;

    let new_balance: i32 = match wallet_row {
        Some(row) => row
            .try_get("balance")
            .map_err(|e| AuthError::Internal(format!("Failed to get balance: {}", e)))?,
        None => {
            // Check what they actually have
            let purchased: Option<i32> =
                sqlx::query_scalar("SELECT purchased_balance FROM wallets WHERE wallet_id = $1")
                    .bind(wallet_id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| AuthError::Internal(format!("Failed to check balance: {}", e)))?;

            return Err(AuthError::InvalidInput(format!(
                "Insufficient transferable balance: have {}, need {}. Granted credits cannot be transferred.",
                purchased.unwrap_or(0),
                amount
            )));
        }
    };

    // Create ledger entry (negative amount for debit)
    let ledger_row = sqlx::query(
        "INSERT INTO credit_ledger (wallet_id, amount, balance_after, tx_type, description, related_id)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING tx_id, wallet_id, amount, balance_after, tx_type, description, related_id, created_at",
    )
    .bind(wallet_id)
    .bind(-amount)
    .bind(new_balance)
    .bind(tx_type)
    .bind(description)
    .bind(related_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AuthError::Internal(format!("Ledger insert failed: {}", e)))?;

    Ok(row_to_transaction(&ledger_row))
}

/// Grant free credits (e.g., new user onboarding).
/// Credits go to granted_balance (non-transferable).
/// IMPORTANT: No BEGIN/COMMIT - PgBouncer transaction mode handles this
pub async fn grant(
    pool: &PgPool,
    wallet_id: Uuid,
    amount: i32,
    description: &str,
) -> Result<CreditTransaction, AuthError> {
    if amount <= 0 {
        return Err(AuthError::InvalidInput(
            "Grant amount must be positive".into(),
        ));
    }

    // Atomic update — credits go to granted_balance (non-transferable)
    let wallet_row = sqlx::query(
        "UPDATE wallets
         SET balance = balance + $1,
             granted_balance = granted_balance + $1,
             total_deposited = total_deposited + $1
         WHERE wallet_id = $2
         RETURNING balance",
    )
    .bind(amount)
    .bind(wallet_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AuthError::Internal(format!("Wallet update failed: {}", e)))?;

    let new_balance: i32 = wallet_row
        .try_get("balance")
        .map_err(|e| AuthError::Internal(format!("Failed to get balance: {}", e)))?;

    let ledger_row = sqlx::query(
        "INSERT INTO credit_ledger (wallet_id, amount, balance_after, tx_type, description)
         VALUES ($1, $2, $3, 'grant', $4)
         RETURNING tx_id, wallet_id, amount, balance_after, tx_type, description, related_id, created_at",
    )
    .bind(wallet_id)
    .bind(amount)
    .bind(new_balance)
    .bind(description)
    .fetch_one(pool)
    .await
    .map_err(|e| AuthError::Internal(format!("Ledger insert failed: {}", e)))?;

    Ok(row_to_transaction(&ledger_row))
}

/// Get wallet balance
pub async fn get_balance(pool: &PgPool, wallet_id: Uuid) -> Result<i32, AuthError> {
    let row = sqlx::query("SELECT balance FROM wallets WHERE wallet_id = $1")
        .bind(wallet_id)
        .fetch_one(pool)
        .await
        .map_err(|e| AuthError::Internal(format!("Wallet not found: {}", e)))?;

    Ok(row.try_get("balance").unwrap())
}

/// Get recent transactions for a wallet
pub async fn get_transactions(
    pool: &PgPool,
    wallet_id: Uuid,
    limit: i64,
) -> Result<Vec<CreditTransaction>, AuthError> {
    let rows = sqlx::query(
        "SELECT tx_id, wallet_id, amount, balance_after, tx_type, description, related_id, created_at
         FROM credit_ledger
         WHERE wallet_id = $1
         ORDER BY created_at DESC
         LIMIT $2",
    )
    .bind(wallet_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| AuthError::Internal(format!("Query failed: {}", e)))?;

    Ok(rows.iter().map(row_to_transaction).collect())
}

fn row_to_transaction(row: &sqlx::postgres::PgRow) -> CreditTransaction {
    CreditTransaction {
        tx_id: row.try_get("tx_id").unwrap(),
        wallet_id: row.try_get("wallet_id").unwrap(),
        amount: row.try_get("amount").unwrap(),
        balance_after: row.try_get("balance_after").unwrap(),
        tx_type: row.try_get("tx_type").unwrap(),
        description: row.try_get("description").unwrap(),
        related_id: row.try_get("related_id").unwrap(),
        created_at: row.try_get("created_at").unwrap(),
    }
}
