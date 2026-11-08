//! Credit ledger and wallet service.
//!
//! Integer-based credit system.
//!
//! Every wallet mutation is a `*_tx` function in this file (`debit_tx` :328,
//! `credit_tx` :428, `transfer_tx` :486) that writes the wallet row and
//! its `credit_ledger` row (`insert_ledger_tx` :536) on one connection inside
//! one transaction; the pool-taking functions (`charge`, `deposit`, `grant`, …)
//! only open and commit that transaction via `in_tx` :170 [V]. `wallets.balance` is
//! canonical and `credit_ledger.balance_after` is a copy: nothing recomputes
//! balance from the ledger, and the ledger has no immutability trigger
//! [V: migrations/012_credit_ledger.sql]. Both become ledger-derived in
//! directive Phase 3 (SPEC 36 R1.2).
//!
//! Balance split: `granted_balance` (non-transferable) + `purchased_balance`
//! (transferable). Grants go to granted; deposits and revenue go to purchased.
//! Charges spend granted first. `transfer_tx` carries the split across, so a
//! transfer never converts granted credits into transferable ones.

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
/// statements into one [V: fermi-auth/src/credits.rs:227-295 @ a3cea516 issued
/// the wallet UPDATE and the ledger INSERT on `pool` with no `begin()`].
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
    debit_tx(
        conn,
        wallet_id,
        amount,
        tx_type,
        description,
        related_id,
        transferable_only,
    )
    .await
    .map(|(t, _)| t)
}

/// [`charge_tx`], also returning how much of `amount` came from
/// `granted_balance` — so [`transfer_tx`] can credit that part as granted.
async fn debit_tx(
    conn: &mut sqlx::PgConnection,
    wallet_id: Uuid,
    amount: i32,
    tx_type: &str,
    description: &str,
    related_id: Option<&str>,
    transferable_only: bool,
) -> Result<(CreditTransaction, i32), AuthError> {
    if amount <= 0 {
        return Err(AuthError::InvalidInput(
            "Charge amount must be positive".into(),
        ));
    }
    // One statement for both modes, so the debit rule exists once. SET
    // expressions read the pre-update row and RETURNING reads the new one, so
    // the granted portion spent is `granted_before - granted_after`, with
    // `granted_before` read under the row lock taken just below.
    let sql = "UPDATE wallets
         SET balance = balance - $1,
             total_spent = total_spent + $1,
             granted_balance = granted_balance
                 - CASE WHEN $3 THEN 0 ELSE LEAST(granted_balance, $1) END,
             purchased_balance = purchased_balance
                 - CASE WHEN $3 THEN $1 ELSE $1 - LEAST(granted_balance, $1) END
         WHERE wallet_id = $2
           AND CASE WHEN $3 THEN purchased_balance >= $1 ELSE balance >= $1 END
         RETURNING balance, granted_balance";
    let before_granted: Option<i32> =
        sqlx::query_scalar("SELECT granted_balance FROM wallets WHERE wallet_id = $1 FOR UPDATE")
            .bind(wallet_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| AuthError::Internal(format!("Wallet lock failed: {}", e)))?;
    let Some(before_granted) = before_granted else {
        return Err(AuthError::NotFound(format!("wallet {wallet_id}")));
    };
    let row = sqlx::query(sql)
        .bind(amount)
        .bind(wallet_id)
        .bind(transferable_only)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AuthError::Internal(format!("Wallet update failed: {}", e)))?;
    let Some(row) = row else {
        let have: i32 = sqlx::query_scalar(if transferable_only {
            "SELECT purchased_balance FROM wallets WHERE wallet_id = $1"
        } else {
            "SELECT balance FROM wallets WHERE wallet_id = $1"
        })
        .bind(wallet_id)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| AuthError::Internal(format!("Failed to check balance: {}", e)))?;
        return Err(AuthError::InsufficientCredits { have, need: amount });
    };
    let new_balance: i32 = row.try_get("balance").map_err(decode)?;
    let after_granted: i32 = row.try_get("granted_balance").map_err(decode)?;
    let t = insert_ledger_tx(
        conn,
        wallet_id,
        -amount,
        new_balance,
        tx_type,
        description,
        related_id,
    )
    .await?;
    Ok((t, before_granted - after_granted))
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
    credit_tx(conn, wallet_id, amount, 0, tx_type, description, related_id).await
}

/// Credit `amount` to `wallet_id`'s granted (non-transferable) balance.
pub async fn grant_tx(
    conn: &mut sqlx::PgConnection,
    wallet_id: Uuid,
    amount: i32,
    description: &str,
) -> Result<CreditTransaction, AuthError> {
    if amount <= 0 {
        return Err(AuthError::InvalidInput(
            "Grant amount must be positive".into(),
        ));
    }
    credit_tx(conn, wallet_id, amount, amount, "grant", description, None).await
}

/// Credit `amount`, of which `granted_part` goes to `granted_balance` and the
/// rest to `purchased_balance`. The single credit statement in this crate.
async fn credit_tx(
    conn: &mut sqlx::PgConnection,
    wallet_id: Uuid,
    amount: i32,
    granted_part: i32,
    tx_type: &str,
    description: &str,
    related_id: Option<&str>,
) -> Result<CreditTransaction, AuthError> {
    if amount <= 0 {
        return Err(AuthError::InvalidInput(
            "Deposit amount must be positive".into(),
        ));
    }
    if !(0..=amount).contains(&granted_part) {
        return Err(AuthError::Internal(format!(
            "granted_part {granted_part} outside 0..={amount}"
        )));
    }
    let row = sqlx::query(
        "UPDATE wallets
         SET balance = balance + $1,
             granted_balance = granted_balance + $3,
             purchased_balance = purchased_balance + ($1 - $3),
             total_deposited = total_deposited + $1
         WHERE wallet_id = $2
         RETURNING balance",
    )
    .bind(amount)
    .bind(wallet_id)
    .bind(granted_part)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| AuthError::Internal(format!("Wallet update failed: {}", e)))?
    .ok_or_else(|| AuthError::NotFound(format!("wallet {wallet_id}")))?;
    let new_balance: i32 = row.try_get("balance").map_err(decode)?;
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
/// Both wallet rows are locked first (`ORDER BY wallet_id`, so two concurrent
/// transfers in opposite directions take the locks in the same order and
/// cannot deadlock). Writes a `transfer_out` and a `transfer_in` ledger row
/// sharing `related_id`; the pair sums to zero.
///
/// The split is preserved: whatever part of the debit came from the source's
/// `granted_balance` is credited to the target's `granted_balance`, so a
/// transfer can never turn non-transferable credits into transferable ones.
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
    // SPEC 36 R1.3: a transfer may not spend credits an open execution hold
    // reserves, and a wallet that owes an unpaid execution charge cannot move
    // credits out (`credit_hold`, migration 246).
    sqlx::query("SELECT 1 FROM wallets WHERE wallet_id = ANY($1) ORDER BY wallet_id FOR UPDATE")
        .bind(vec![from, to])
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AuthError::Internal(format!("Wallet lock failed: {}", e)))?;
    let row = sqlx::query(
        "SELECT w.balance,
            COALESCE((SELECT sum(h.amount) FROM credit_holds h
                       WHERE h.wallet_id = w.wallet_id
                         AND NOT EXISTS (SELECT 1 FROM credit_hold_closures c
                                          WHERE c.hold_id = h.hold_id)), 0)::int AS held,
            (COALESCE((SELECT sum(c.unpaid) FROM credit_hold_closures c
                         JOIN credit_holds h USING (hold_id) WHERE h.wallet_id = w.wallet_id), 0)
             - COALESCE((SELECT sum(r.amount) FROM credit_debt_repayments r
                          WHERE r.wallet_id = w.wallet_id), 0))::int AS owed
           FROM wallets w WHERE w.wallet_id = $1",
    )
    .bind(from)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| AuthError::Internal(format!("Hold check failed: {}", e)))?;
    if let Some(row) = row {
        let balance: i32 = row.try_get("balance").map_err(decode)?;
        let held: i32 = row.try_get("held").map_err(decode)?;
        let owed: i32 = row.try_get("owed").map_err(decode)?;
        if owed > 0 {
            return Err(AuthError::InvalidInput(format!(
                "This wallet owes {owed} credits from an earlier run; settle it before transferring."
            )));
        }
        if balance - held < amount {
            return Err(AuthError::InsufficientCredits {
                have: balance - held,
                need: amount,
            });
        }
    }
    transfer_typed_tx(
        conn,
        from,
        to,
        amount,
        ("transfer_out", "transfer_in"),
        description,
        related_id,
        transferable_only,
    )
    .await
}

/// [`transfer_tx`] with explicit ledger types and **without** the hold and
/// debt check: for the royalty leg of an execution settlement
/// (`credit_hold::settle`), which spends the credits its own hold reserved.
/// The granted/purchased split is preserved as in `transfer_tx`.
#[allow(clippy::too_many_arguments)]
pub async fn transfer_typed_tx(
    conn: &mut sqlx::PgConnection,
    from: Uuid,
    to: Uuid,
    amount: i32,
    (out_type, in_type): (&str, &str),
    description: &str,
    related_id: Option<&str>,
    transferable_only: bool,
) -> Result<(CreditTransaction, CreditTransaction), AuthError> {
    if from == to {
        return Err(AuthError::InvalidInput(
            "Cannot transfer to the same wallet".into(),
        ));
    }
    let locked: Vec<Uuid> = sqlx::query_scalar(
        "SELECT wallet_id FROM wallets WHERE wallet_id = ANY($1) ORDER BY wallet_id FOR UPDATE",
    )
    .bind(vec![from, to])
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| AuthError::Internal(format!("Wallet lock failed: {}", e)))?;
    if locked.len() != 2 {
        let missing = if locked.contains(&from) { to } else { from };
        return Err(AuthError::NotFound(format!("wallet {missing}")));
    }
    let (out, granted_part) = debit_tx(
        conn,
        from,
        amount,
        out_type,
        description,
        related_id,
        transferable_only,
    )
    .await?;
    // The ledger row records the whole amount once (`transfer_in`), so the
    // pair still sums to zero; the bucket split follows the debit.
    let inn = credit_tx(
        conn,
        to,
        amount,
        granted_part,
        in_type,
        description,
        related_id,
    )
    .await?;
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
/// larger unit of work. Refuses an existing wallet of a different
/// `owner_type` (owner ids are globally unique across types).
pub async fn ensure_wallet_tx(
    conn: &mut sqlx::PgConnection,
    owner_type: &str,
    owner_id: &str,
) -> Result<Uuid, AuthError> {
    let inserted: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO wallets (owner_type, owner_id) VALUES ($1, $2)
         ON CONFLICT (owner_id) DO NOTHING
         RETURNING wallet_id",
    )
    .bind(owner_type)
    .bind(owner_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| AuthError::Internal(format!("Failed to ensure wallet: {}", e)))?;
    if let Some(w) = inserted {
        return Ok(w);
    }
    wallet_id_tx(conn, owner_type, owner_id)
        .await?
        .ok_or_else(|| {
            AuthError::Internal(format!(
                "wallet owner_id {owner_id} exists with an owner_type other than {owner_type}"
            ))
        })
}

/// The wallet for `(owner_type, owner_id)`, if any — typed, so a user id can
/// never resolve to a platform or workspace wallet.
pub async fn wallet_id_tx(
    conn: &mut sqlx::PgConnection,
    owner_type: &str,
    owner_id: &str,
) -> Result<Option<Uuid>, AuthError> {
    sqlx::query_scalar("SELECT wallet_id FROM wallets WHERE owner_type = $1 AND owner_id = $2")
        .bind(owner_type)
        .bind(owner_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AuthError::Internal(format!("wallet lookup failed: {}", e)))
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
