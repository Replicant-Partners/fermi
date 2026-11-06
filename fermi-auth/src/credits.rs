//! Credit ledger and wallet service.
//!
//! Integer-based credit system.
//!
//! Every wallet mutation is a `*_tx` function that writes the wallet row and its
//! `credit_ledger` row on one connection inside one transaction [V: `charge_tx`,
//! `credit_tx`, `insert_ledger_tx` below]; the pool-taking functions only open
//! and commit that transaction. `wallets.balance` is canonical and the ledger's
//! `balance_after` is a copy — nothing yet recomputes balance from the ledger,
//! and the ledger is append-only by convention only (no trigger) [V: migration
//! 012]. Both become ledger-derived in directive Phase 3 (SPEC 36 R1.2).
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

fn decode(e: sqlx::Error) -> AuthError {
    AuthError::Internal(format!("row decode failed: {}", e))
}

fn row_to_wallet(row: &sqlx::postgres::PgRow) -> Result<Wallet, AuthError> {
    Ok(Wallet {
        wallet_id: row.try_get("wallet_id").map_err(decode)?,
        owner_type: row.try_get("owner_type").map_err(decode)?,
        owner_id: row.try_get("owner_id").map_err(decode)?,
        balance: row.try_get("balance").map_err(decode)?,
        granted_balance: row.try_get("granted_balance").map_err(decode)?,
        purchased_balance: row.try_get("purchased_balance").map_err(decode)?,
        total_deposited: row.try_get("total_deposited").map_err(decode)?,
        total_spent: row.try_get("total_spent").map_err(decode)?,
        created_at: row.try_get("created_at").map_err(decode)?,
    })
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
        return row_to_wallet(&row);
    }

    // Create the wallet and, for users, its onboarding grant in one
    // transaction: a new user wallet without its grant was the previous
    // failure mode (`let _ = grant(..)` swallowed it).
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AuthError::Internal(format!("begin failed: {}", e)))?;
    let row = sqlx::query(&format!(
        "INSERT INTO wallets (owner_type, owner_id)
         VALUES ($1, $2)
         ON CONFLICT (owner_id) DO NOTHING
         RETURNING {}",
        WALLET_SELECT_COLS
    ))
    .bind(owner_type)
    .bind(owner_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AuthError::Internal(format!("Failed to create wallet: {}", e)))?;

    let Some(row) = row else {
        // Lost a creation race: the winner already granted. Read theirs.
        drop(tx);
        let row = sqlx::query(&format!(
            "SELECT {} FROM wallets WHERE owner_id = $1",
            WALLET_SELECT_COLS
        ))
        .bind(owner_id)
        .fetch_one(pool)
        .await
        .map_err(|e| AuthError::Internal(format!("DB error: {}", e)))?;
        return row_to_wallet(&row);
    };
    let wallet = row_to_wallet(&row)?;

    if owner_type == "user" {
        let onboarding_amount = std::env::var("ONBOARDING_GRANT")
            .ok()
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(100);
        if onboarding_amount > 0 {
            grant_tx(
                &mut tx,
                wallet.wallet_id,
                onboarding_amount,
                "Welcome onboarding grant",
            )
            .await?;
        }
    }
    let row = sqlx::query(&format!(
        "SELECT {} FROM wallets WHERE wallet_id = $1",
        WALLET_SELECT_COLS
    ))
    .bind(wallet.wallet_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AuthError::Internal(format!("DB error: {}", e)))?;
    let wallet = row_to_wallet(&row)?;
    tx.commit()
        .await
        .map_err(|e| AuthError::Internal(format!("commit failed: {}", e)))?;
    Ok(wallet)
}

/// Begin a transaction, run `f` on it, commit. The pool-taking entry points
/// below are thin wrappers so that every wallet mutation exists exactly once —
/// in the `_tx` functions — and is always paired with its ledger row in one
/// transaction (SPEC 36 R1.1, directive rule 10).
///
/// This replaces the claim "No BEGIN/COMMIT - PgBouncer transaction mode
/// handles this", which was false: a transaction-mode pooler pins a server
/// connection for an *explicit* transaction; it does not merge two autocommit
/// statements into one [V: the previous `charge` issued UPDATE then INSERT on
/// `pool` with no `begin()`].
async fn in_tx<T, F>(pool: &PgPool, f: F) -> Result<T, AuthError>
where
    F: for<'c> FnOnce(
        &'c mut sqlx::PgConnection,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<T, AuthError>> + Send + 'c>,
    >,
{
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AuthError::Internal(format!("begin failed: {}", e)))?;
    let out = f(&mut tx).await?;
    tx.commit()
        .await
        .map_err(|e| AuthError::Internal(format!("commit failed: {}", e)))?;
    Ok(out)
}

/// Deposit credits into a wallet. Credits go to purchased_balance.
pub async fn deposit(
    pool: &PgPool,
    wallet_id: Uuid,
    amount: i32,
    description: &str,
) -> Result<CreditTransaction, AuthError> {
    let description = description.to_string();
    in_tx(pool, move |c| {
        Box::pin(
            async move { deposit_tx(c, wallet_id, amount, "deposit", &description, None).await },
        )
    })
    .await
}

/// Deposit credits with a custom tx_type (agent_collect_in, execution_royalty, …).
pub async fn deposit_typed(
    pool: &PgPool,
    wallet_id: Uuid,
    amount: i32,
    tx_type: &str,
    description: &str,
) -> Result<CreditTransaction, AuthError> {
    let (tx_type, description) = (tx_type.to_string(), description.to_string());
    in_tx(pool, move |c| {
        Box::pin(
            async move { deposit_tx(c, wallet_id, amount, &tx_type, &description, None).await },
        )
    })
    .await
}

/// Charge credits (fails with `InsufficientCredits` if balance is short).
/// Spends granted_balance first, then purchased_balance.
pub async fn charge(
    pool: &PgPool,
    wallet_id: Uuid,
    amount: i32,
    tx_type: &str,
    description: &str,
    related_id: Option<&str>,
) -> Result<CreditTransaction, AuthError> {
    let (tx_type, description) = (tx_type.to_string(), description.to_string());
    let related_id = related_id.map(str::to_string);
    in_tx(pool, move |c| {
        Box::pin(async move {
            charge_tx(
                c,
                wallet_id,
                amount,
                &tx_type,
                &description,
                related_id.as_deref(),
                false,
            )
            .await
        })
    })
    .await
}

/// Charge from purchased_balance only. Granted credits cannot be transferred.
pub async fn charge_purchased_only(
    pool: &PgPool,
    wallet_id: Uuid,
    amount: i32,
    tx_type: &str,
    description: &str,
    related_id: Option<&str>,
) -> Result<CreditTransaction, AuthError> {
    let (tx_type, description) = (tx_type.to_string(), description.to_string());
    let related_id = related_id.map(str::to_string);
    in_tx(pool, move |c| {
        Box::pin(async move {
            charge_tx(
                c,
                wallet_id,
                amount,
                &tx_type,
                &description,
                related_id.as_deref(),
                true,
            )
            .await
        })
    })
    .await
}

/// Grant free credits (e.g. onboarding). Credits go to granted_balance.
pub async fn grant(
    pool: &PgPool,
    wallet_id: Uuid,
    amount: i32,
    description: &str,
) -> Result<CreditTransaction, AuthError> {
    let description = description.to_string();
    in_tx(pool, move |c| {
        Box::pin(async move { grant_tx(c, wallet_id, amount, &description).await })
    })
    .await
}

// ─── Transactional primitives (SPEC 36 R1.1) ──────────────────────────
//
// Each takes a connection that is already inside a transaction, so the wallet
// UPDATE and the ledger INSERT commit together or not at all — and so a caller
// can put several of them (and its own writes) in one unit. The pool-taking
// functions above predate this and are being migrated onto these.

/// Debit `amount` from `wallet_id` inside the caller's transaction.
///
/// `transferable_only` restricts the debit to `purchased_balance` (granted
/// credits are not transferable); otherwise granted is spent first.
pub async fn charge_tx(
    conn: &mut sqlx::PgConnection,
    wallet_id: Uuid,
    amount: i32,
    tx_type: &str,
    description: &str,
    related_id: Option<&str>,
    transferable_only: bool,
) -> Result<CreditTransaction, AuthError> {
    if amount <= 0 {
        return Err(AuthError::InvalidInput(
            "Charge amount must be positive".into(),
        ));
    }
    // One statement for both modes, so the debit rule exists once.
    let sql = "UPDATE wallets
         SET balance = balance - $1,
             total_spent = total_spent + $1,
             granted_balance = granted_balance
                 - CASE WHEN $3 THEN 0 ELSE LEAST(granted_balance, $1) END,
             purchased_balance = purchased_balance
                 - CASE WHEN $3 THEN $1 ELSE $1 - LEAST(granted_balance, $1) END
         WHERE wallet_id = $2
           AND CASE WHEN $3 THEN purchased_balance >= $1 ELSE balance >= $1 END
         RETURNING balance";
    let row = sqlx::query(sql)
        .bind(amount)
        .bind(wallet_id)
        .bind(transferable_only)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AuthError::Internal(format!("Wallet update failed: {}", e)))?;
    let Some(row) = row else {
        let have: Option<i32> = sqlx::query_scalar(if transferable_only {
            "SELECT purchased_balance FROM wallets WHERE wallet_id = $1"
        } else {
            "SELECT balance FROM wallets WHERE wallet_id = $1"
        })
        .bind(wallet_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AuthError::Internal(format!("Failed to check balance: {}", e)))?;
        return Err(AuthError::InsufficientCredits {
            have: have.unwrap_or(0),
            need: amount,
        });
    };
    let new_balance: i32 = row
        .try_get("balance")
        .map_err(|e| AuthError::Internal(format!("Failed to get balance: {}", e)))?;
    insert_ledger_tx(
        conn,
        wallet_id,
        -amount,
        new_balance,
        tx_type,
        description,
        related_id,
    )
    .await
}

/// Credit `amount` to `wallet_id`'s purchased balance inside the caller's transaction.
pub async fn deposit_tx(
    conn: &mut sqlx::PgConnection,
    wallet_id: Uuid,
    amount: i32,
    tx_type: &str,
    description: &str,
    related_id: Option<&str>,
) -> Result<CreditTransaction, AuthError> {
    credit_tx(
        conn,
        wallet_id,
        amount,
        false,
        tx_type,
        description,
        related_id,
    )
    .await
}

/// Credit `amount` to `wallet_id`'s granted (non-transferable) balance.
pub async fn grant_tx(
    conn: &mut sqlx::PgConnection,
    wallet_id: Uuid,
    amount: i32,
    description: &str,
) -> Result<CreditTransaction, AuthError> {
    credit_tx(conn, wallet_id, amount, true, "grant", description, None).await
}

async fn credit_tx(
    conn: &mut sqlx::PgConnection,
    wallet_id: Uuid,
    amount: i32,
    granted: bool,
    tx_type: &str,
    description: &str,
    related_id: Option<&str>,
) -> Result<CreditTransaction, AuthError> {
    if amount <= 0 {
        return Err(AuthError::InvalidInput(
            "Deposit amount must be positive".into(),
        ));
    }
    let row = sqlx::query(
        "UPDATE wallets
         SET balance = balance + $1,
             granted_balance = granted_balance + CASE WHEN $3 THEN $1 ELSE 0 END,
             purchased_balance = purchased_balance + CASE WHEN $3 THEN 0 ELSE $1 END,
             total_deposited = total_deposited + $1
         WHERE wallet_id = $2
         RETURNING balance",
    )
    .bind(amount)
    .bind(wallet_id)
    .bind(granted)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| AuthError::Internal(format!("Wallet update failed: {}", e)))?
    .ok_or_else(|| AuthError::NotFound(format!("wallet {wallet_id}")))?;
    let new_balance: i32 = row
        .try_get("balance")
        .map_err(|e| AuthError::Internal(format!("Failed to get balance: {}", e)))?;
    insert_ledger_tx(
        conn,
        wallet_id,
        amount,
        new_balance,
        tx_type,
        description,
        related_id,
    )
    .await
}

/// Move `amount` from one wallet to another inside the caller's transaction.
///
/// Both wallet rows are locked in UUID order first, so two concurrent
/// transfers in opposite directions cannot deadlock. Writes a `transfer_out`
/// and a `transfer_in` ledger row sharing `related_id`; the pair sums to zero.
pub async fn transfer_tx(
    conn: &mut sqlx::PgConnection,
    from: Uuid,
    to: Uuid,
    amount: i32,
    description: &str,
    related_id: Option<&str>,
    transferable_only: bool,
) -> Result<(CreditTransaction, CreditTransaction), AuthError> {
    if from == to {
        return Err(AuthError::InvalidInput(
            "Cannot transfer to the same wallet".into(),
        ));
    }
    let (a, b) = if from < to { (from, to) } else { (to, from) };
    sqlx::query(
        "SELECT wallet_id FROM wallets WHERE wallet_id = ANY($1) ORDER BY wallet_id FOR UPDATE",
    )
    .bind(vec![a, b])
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| AuthError::Internal(format!("Wallet lock failed: {}", e)))?;
    let out = charge_tx(
        conn,
        from,
        amount,
        "transfer_out",
        description,
        related_id,
        transferable_only,
    )
    .await?;
    let inn = deposit_tx(conn, to, amount, "transfer_in", description, related_id).await?;
    Ok((out, inn))
}

async fn insert_ledger_tx(
    conn: &mut sqlx::PgConnection,
    wallet_id: Uuid,
    amount: i32,
    balance_after: i32,
    tx_type: &str,
    description: &str,
    related_id: Option<&str>,
) -> Result<CreditTransaction, AuthError> {
    let row = sqlx::query(
        "INSERT INTO credit_ledger (wallet_id, amount, balance_after, tx_type, description, related_id)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING tx_id, wallet_id, amount, balance_after, tx_type, description, related_id, created_at",
    )
    .bind(wallet_id)
    .bind(amount)
    .bind(balance_after)
    .bind(tx_type)
    .bind(description)
    .bind(related_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| AuthError::Internal(format!("Ledger insert failed: {}", e)))?;
    row_to_transaction(&row)
}

/// Get-or-create a wallet inside the caller's transaction, **without** the
/// onboarding grant. For workspace and agent wallets created as part of a
/// larger unit of work.
pub async fn ensure_wallet_tx(
    conn: &mut sqlx::PgConnection,
    owner_type: &str,
    owner_id: &str,
) -> Result<Uuid, AuthError> {
    sqlx::query_scalar(
        "INSERT INTO wallets (owner_type, owner_id) VALUES ($1, $2)
         ON CONFLICT (owner_id) DO UPDATE SET owner_id = EXCLUDED.owner_id
         RETURNING wallet_id",
    )
    .bind(owner_type)
    .bind(owner_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| AuthError::Internal(format!("Failed to ensure wallet: {}", e)))
}

/// Get wallet balance
pub async fn get_balance(pool: &PgPool, wallet_id: Uuid) -> Result<i32, AuthError> {
    let row = sqlx::query("SELECT balance FROM wallets WHERE wallet_id = $1")
        .bind(wallet_id)
        .fetch_one(pool)
        .await
        .map_err(|e| AuthError::Internal(format!("Wallet not found: {}", e)))?;

    row.try_get("balance").map_err(decode)
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

    rows.iter().map(row_to_transaction).collect()
}

fn row_to_transaction(row: &sqlx::postgres::PgRow) -> Result<CreditTransaction, AuthError> {
    Ok(CreditTransaction {
        tx_id: row.try_get("tx_id").map_err(decode)?,
        wallet_id: row.try_get("wallet_id").map_err(decode)?,
        amount: row.try_get("amount").map_err(decode)?,
        balance_after: row.try_get("balance_after").map_err(decode)?,
        tx_type: row.try_get("tx_type").map_err(decode)?,
        description: row.try_get("description").map_err(decode)?,
        related_id: row.try_get("related_id").map_err(decode)?,
        created_at: row.try_get("created_at").map_err(decode)?,
    })
}
