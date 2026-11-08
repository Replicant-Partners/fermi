//! Credit holds: reserve before work, settle after (SPEC 36 R1.3 / LLD G04,
//! #48; owner decision 1(b) + 2(b)).
//!
//! Before this, an execution was paid for after the fact: a `balance <= 0`
//! pre-check, then `charge_execution_with_royalty` turned a failed charge
//! into `(0, 0)` (a free run) and `charge_gas` returned 402 after the work
//! was done [V: src/handlers/execution.rs:106, :784-805, :810 @ 59d76b00].
//!
//! Now:
//! * [`open`] reserves an estimate in one transaction. It refuses (402)
//!   before any LLM call when `balance - Σ open holds` is short, or when the
//!   wallet owes an unpaid charge it cannot repay.
//! * [`settle`] charges the actual fee and gas, pays the owner royalty, and
//!   closes the hold, all in one transaction. A charge above the wallet's
//!   balance is charged up to the balance and the rest recorded as `unpaid`
//!   on the closure: recorded, never silently forgiven.
//! * [`release`] closes an unused hold. A [`Hold`] dropped without
//!   settle or release is released by its `Drop`; `episode_reaper` releases
//!   holds past `expires_at`.
//!
//! Append-only: holds, closures and repayments are inserted, never updated
//! (migration 246). Settlement runs in its own transaction straight after
//! the episode is stored; making it the episode's transaction is Phase 3
//! ledger work (declared gap, #48).

use sqlx::{PgPool, Row};
use uuid::Uuid;

use fermi_auth::credits::{charge_tx, deposit_tx, ensure_wallet_tx};

/// Longer than `episode_reaper`'s default `EPISODE_MAX_RUNTIME` (30 min), so a
/// run the reaper has not yet failed still holds its reservation.
pub const HOLD_TTL_SECS: i64 = 40 * 60;

/// Upper bound on an execution estimate. Above the largest execution fee on
/// the dev ledger over 90 days (205 credits, 2026-11-08).
pub const EXECUTION_HOLD_CAP: i32 = 250;

/// Tool-loop iterations: `MAX_ITERATIONS` in
/// src/agent_backend/tool_executor.rs:27.
const TOOL_LOOP_ITERATIONS: i32 = 5;

/// SPEC 36 R1.3 estimate rule: the fee for `max_tokens × (iterations + 1)`
/// plus its gas, capped at [`EXECUTION_HOLD_CAP`]. A lower bound in practice
/// (input tokens are not counted), which is why settle charges actual cost
/// beyond the hold, up to the balance.
pub fn execution_estimate(fees: &crate::gas::GasFees, max_tokens: u32) -> i32 {
    let tokens = (max_tokens as i64 * (TOOL_LOOP_ITERATIONS as i64 + 1)).min(i32::MAX as i64);
    let (fee, gas) = fees.execution_fee(tokens as i32);
    (fee + gas).clamp(1, EXECUTION_HOLD_CAP)
}

#[derive(Debug)]
pub enum HoldError {
    /// `available = balance - Σ open holds` is below the estimate.
    Insufficient {
        available: i32,
        needed: i32,
    },
    /// The wallet owes an unpaid charge larger than what it has available.
    InDebt {
        owed: i32,
        available: i32,
    },
    /// The hold was already settled or released.
    AlreadyClosed,
    Db(String),
}

impl std::fmt::Display for HoldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HoldError::Insufficient { available, needed } => write!(
                f,
                "Insufficient credits: this run reserves {needed}, {available} available."
            ),
            HoldError::InDebt { owed, available } => write!(
                f,
                "An earlier run left {owed} credits unpaid and {available} are available. \
                 Top up to continue."
            ),
            HoldError::AlreadyClosed => write!(f, "credit hold already closed"),
            HoldError::Db(e) => write!(f, "credit hold: {e}"),
        }
    }
}

impl std::error::Error for HoldError {}

impl HoldError {
    pub fn is_payment_required(&self) -> bool {
        matches!(
            self,
            HoldError::Insufficient { .. } | HoldError::InDebt { .. }
        )
    }
}

fn db<E: std::fmt::Display>(e: E) -> HoldError {
    HoldError::Db(e.to_string())
}

/// An open reservation. Settle or release it; a dropped one is released.
#[must_use = "an open hold reserves credits: settle or release it"]
#[derive(Debug)]
pub struct Hold {
    pub hold_id: Uuid,
    pub wallet_id: Uuid,
    pub amount: i32,
    armed: Option<PgPool>,
}

impl Drop for Hold {
    fn drop(&mut self) {
        let Some(pool) = self.armed.take() else {
            return;
        };
        let id = self.hold_id;
        match tokio::runtime::Handle::try_current() {
            Ok(rt) => {
                rt.spawn(async move {
                    if let Err(e) =
                        release_id(&pool, id, "dropped: the run ended without settling").await
                    {
                        tracing::error!(hold = %id, error = %e,
                            "could not release hold; episode_reaper releases it at expiry");
                    }
                });
            }
            Err(_) => tracing::error!(hold = %id,
                "hold dropped outside a runtime; episode_reaper releases it at expiry"),
        }
    }
}

/// Reserve `amount` on `wallet_id`. One transaction, with the wallet row
/// locked, so concurrent opens on one wallet cannot both see the same
/// available balance.
pub async fn open(
    pool: &PgPool,
    wallet_id: Uuid,
    amount: i32,
    purpose: &str,
) -> Result<Hold, HoldError> {
    let amount = amount.max(1);
    let mut tx = pool.begin().await.map_err(db)?;
    let balance: i32 =
        sqlx::query_scalar("SELECT balance FROM wallets WHERE wallet_id = $1 FOR UPDATE")
            .bind(wallet_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
    let row = sqlx::query(
        "SELECT
            COALESCE((SELECT sum(h.amount) FROM credit_holds h
                       WHERE h.wallet_id = $1
                         AND NOT EXISTS (SELECT 1 FROM credit_hold_closures c
                                          WHERE c.hold_id = h.hold_id)), 0)::int AS held,
            (COALESCE((SELECT sum(c.unpaid) FROM credit_hold_closures c
                         JOIN credit_holds h USING (hold_id) WHERE h.wallet_id = $1), 0)
             - COALESCE((SELECT sum(r.amount) FROM credit_debt_repayments r
                          WHERE r.wallet_id = $1), 0))::int AS owed",
    )
    .bind(wallet_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(db)?;
    let held: i32 = row.try_get("held").map_err(db)?;
    let owed: i32 = row.try_get("owed").map_err(db)?;
    let mut available = balance - held;

    if owed > 0 {
        if available < owed {
            return Err(HoldError::InDebt { owed, available });
        }
        // The unpaid part of an earlier execution fee, charged now.
        charge_tx(
            &mut tx,
            wallet_id,
            owed,
            "execution_fee",
            "Unpaid execution fee from an earlier run",
            None,
            false,
        )
        .await
        .map_err(db)?;
        sqlx::query("INSERT INTO credit_debt_repayments (wallet_id, amount) VALUES ($1, $2)")
            .bind(wallet_id)
            .bind(owed)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        available -= owed;
    }
    if available < amount {
        return Err(HoldError::Insufficient {
            available,
            needed: amount,
        });
    }
    let hold_id: Uuid = sqlx::query_scalar(
        "INSERT INTO credit_holds (wallet_id, amount, purpose, expires_at)
         VALUES ($1, $2, $3, now() + make_interval(secs => $4))
         RETURNING hold_id",
    )
    .bind(wallet_id)
    .bind(amount)
    .bind(purpose)
    .bind(HOLD_TTL_SECS as f64)
    .fetch_one(&mut *tx)
    .await
    .map_err(db)?;
    tx.commit().await.map_err(db)?;
    Ok(Hold {
        hold_id,
        wallet_id,
        amount,
        armed: Some(pool.clone()),
    })
}

/// What a finished execution costs.
#[derive(Debug, Clone)]
pub struct Settlement {
    pub execution_fee: i32,
    pub gas_fee: i32,
    /// `(owner user id, royalty)`, from [`crate::gas::royalty_for`].
    pub royalty: Option<(String, i32)>,
    pub description: String,
    /// The episode, stored as the ledger rows' `related_id`.
    pub related_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settled {
    pub charged: i32,
    pub unpaid: i32,
    pub royalty_paid: i32,
}

/// Charge the actual cost and close the hold, in one transaction.
///
/// On error nothing was charged and the hold stays **open** (not released):
/// releasing it would make a failed settlement a free run. It remains
/// reserved and is visible as an open hold until `episode_reaper` releases
/// it at `expires_at`.
pub async fn settle(pool: &PgPool, mut hold: Hold, s: &Settlement) -> Result<Settled, HoldError> {
    let r = settle_inner(pool, &hold, s).await;
    hold.armed = None;
    if let Err(e) = &r {
        tracing::error!(hold = %hold.hold_id, error = %e, "settlement failed; hold left open");
    }
    r
}

async fn settle_inner(pool: &PgPool, hold: &Hold, s: &Settlement) -> Result<Settled, HoldError> {
    let mut tx = pool.begin().await.map_err(db)?;
    // Lock the hold, then refuse a second close.
    sqlx::query("SELECT 1 FROM credit_holds WHERE hold_id = $1 FOR UPDATE")
        .bind(hold.hold_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
    let closed: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM credit_hold_closures WHERE hold_id = $1)")
            .bind(hold.hold_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
    if closed {
        return Err(HoldError::AlreadyClosed);
    }
    let balance: i32 =
        sqlx::query_scalar("SELECT balance FROM wallets WHERE wallet_id = $1 FOR UPDATE")
            .bind(hold.wallet_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;

    let exec = s.execution_fee.max(0);
    let gas = s.gas_fee.max(0);
    let payable = (exec + gas).min(balance.max(0));
    let exec_paid = exec.min(payable);
    let gas_paid = payable - exec_paid;
    let related = s.related_id.as_deref();
    if exec_paid > 0 {
        charge_tx(
            &mut tx,
            hold.wallet_id,
            exec_paid,
            "execution_fee",
            &s.description,
            related,
            false,
        )
        .await
        .map_err(db)?;
    }
    if gas_paid > 0 {
        charge_tx(
            &mut tx,
            hold.wallet_id,
            gas_paid,
            "gas_fee",
            &format!("Gas: {}", s.description),
            related,
            false,
        )
        .await
        .map_err(db)?;
    }
    // The owner's share comes out of what the caller actually paid.
    let mut royalty_paid = 0;
    if let Some((owner, r)) = &s.royalty {
        let r = (*r).min(exec_paid);
        if r > 0 {
            let owner_wallet = ensure_wallet_tx(&mut tx, "user", owner).await.map_err(db)?;
            deposit_tx(
                &mut tx,
                owner_wallet,
                r,
                "agent_royalty_in",
                &format!("Royalty: {}", s.description),
                related,
            )
            .await
            .map_err(db)?;
            royalty_paid = r;
        }
    }
    let unpaid = exec + gas - payable;
    sqlx::query(
        "INSERT INTO credit_hold_closures (hold_id, kind, charged, unpaid, reason)
         VALUES ($1, 'settle', $2, $3, $4)",
    )
    .bind(hold.hold_id)
    .bind(payable)
    .bind(unpaid)
    .bind(related)
    .execute(&mut *tx)
    .await
    .map_err(db)?;
    tx.commit().await.map_err(db)?;
    if unpaid > 0 {
        tracing::warn!(hold = %hold.hold_id, unpaid, "execution cost exceeded the balance; recorded unpaid");
    }
    Ok(Settled {
        charged: payable,
        unpaid,
        royalty_paid,
    })
}

/// Close an unused hold. Charges nothing.
pub async fn release(pool: &PgPool, mut hold: Hold, reason: &str) -> Result<(), HoldError> {
    hold.armed = None;
    release_id(pool, hold.hold_id, reason).await.map(|_| ())
}

/// `true` when this call closed it; `false` when it was already closed.
async fn release_id(pool: &PgPool, hold_id: Uuid, reason: &str) -> Result<bool, HoldError> {
    let r = sqlx::query(
        "INSERT INTO credit_hold_closures (hold_id, kind, reason) VALUES ($1, 'release', $2)
         ON CONFLICT (hold_id) DO NOTHING",
    )
    .bind(hold_id)
    .bind(reason)
    .execute(pool)
    .await
    .map_err(db)?;
    Ok(r.rows_affected() == 1)
}

/// Release every open hold past `expires_at`. Called by `episode_reaper`.
pub async fn release_expired(pool: &PgPool) -> Result<u64, HoldError> {
    let r = sqlx::query(
        "INSERT INTO credit_hold_closures (hold_id, kind, reason)
         SELECT h.hold_id, 'release', 'expired: released by episode_reaper'
           FROM credit_holds h
          WHERE h.expires_at < now()
            AND NOT EXISTS (SELECT 1 FROM credit_hold_closures c WHERE c.hold_id = h.hold_id)
         ON CONFLICT (hold_id) DO NOTHING",
    )
    .execute(pool)
    .await
    .map_err(db)?;
    Ok(r.rows_affected())
}

#[cfg(test)]
mod tests {
    #[test]
    fn estimate_follows_the_rule_and_is_capped() {
        let fees = crate::gas::GasFees::default();
        // 4096 × 6 = 24 576 tokens → fee 24, gas max(1, 2) = 2.
        assert_eq!(super::execution_estimate(&fees, 4096), 26);
        assert_eq!(
            super::execution_estimate(&fees, u32::MAX),
            super::EXECUTION_HOLD_CAP
        );
        assert!(super::execution_estimate(&fees, 0) >= 1);
    }
}
