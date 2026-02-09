//! Gas fee module — every A2A transaction has a cost.
//!
//! Two-layer economic model:
//!   Layer 1 (Credits): Platform gas — users buy credits, spend on actions. The product.
//!   Layer 2 (Crypto):  Agent-to-owner royalties — % tx fee on token transfers. Future.
//!
//! Gas fees fund the platform. Configurable via env vars with sensible defaults.

use fermi_auth::credit_charge;
use sqlx::PgPool;
use uuid::Uuid;

/// Layer 1: Credit gas fee schedule (the product)
#[derive(Debug, Clone)]
pub struct GasFees {
    pub message_send: i32,
    pub agent_hire: i32,
    pub agent_add: i32,
    pub execution_min: i32,
    pub execution_gas_pct: f64,
    pub consolidation_cycle: i32,
    pub file_write: i32,
    pub avatar_generate: i32,
    pub embedding_import: i32,
    pub fork_base: i32,
    pub publish_fee: i32,
    /// Layer 2: Platform transaction fee on crypto token transfers (agent→owner royalties).
    /// Expressed as a fraction (e.g. 0.025 = 2.5%). Applied to every token payout.
    /// Not yet wired — requires SIWE wallet connection + settlement layer.
    pub crypto_tx_fee_pct: f64,
}

impl GasFees {
    pub fn from_env() -> Self {
        Self {
            message_send: env_or("GAS_MESSAGE_SEND", 1),
            agent_hire: env_or("GAS_AGENT_HIRE", 5),
            agent_add: env_or("GAS_AGENT_ADD", 2),
            execution_min: env_or("GAS_EXECUTION_MIN", 1),
            execution_gas_pct: std::env::var("GAS_EXECUTION_PCT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.10),
            consolidation_cycle: env_or("GAS_CONSOLIDATION", 3),
            file_write: env_or("GAS_FILE_WRITE", 1),
            avatar_generate: env_or("GAS_AVATAR_GENERATE", 3),
            embedding_import: env_or("GAS_EMBEDDING_IMPORT", 5),
            fork_base: env_or("GAS_FORK_BASE", 2),
            publish_fee: env_or("GAS_PUBLISH_FEE", 1),
            crypto_tx_fee_pct: std::env::var("CRYPTO_TX_FEE_PCT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.025),
        }
    }

    /// Calculate execution fee: 1 credit per 1000 tokens (min execution_min) + gas surcharge
    pub fn execution_fee(&self, tokens: i32) -> (i32, i32) {
        let base = std::cmp::max(self.execution_min, tokens / 1000);
        let gas = std::cmp::max(1, (base as f64 * self.execution_gas_pct) as i32);
        (base, gas)
    }
}

impl Default for GasFees {
    fn default() -> Self {
        Self {
            message_send: 1,
            agent_hire: 5,
            agent_add: 2,
            execution_min: 1,
            execution_gas_pct: 0.10,
            consolidation_cycle: 3,
            file_write: 1,
            avatar_generate: 3,
            embedding_import: 5,
            fork_base: 2,
            publish_fee: 1,
            crypto_tx_fee_pct: 0.025, // 2.5% on token transfers
        }
    }
}

fn env_or(key: &str, default: i32) -> i32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Low balance threshold (credits)
pub const LOW_BALANCE_THRESHOLD: i32 = 10;

/// Charge a flat gas fee. Returns amount charged.
pub async fn charge_gas(
    pool: &PgPool,
    wallet_id: Uuid,
    amount: i32,
    tx_type: &str,
    description: &str,
    related_id: Option<&str>,
) -> std::result::Result<i32, (axum::http::StatusCode, String)> {
    credit_charge(pool, wallet_id, amount, tx_type, description, related_id)
        .await
        .map(|_tx| amount)
        .map_err(|e| {
            (
                axum::http::StatusCode::PAYMENT_REQUIRED,
                format!("Gas fee failed: {}", e),
            )
        })
}

/// Check wallet balance and return true if low
pub async fn check_low_balance(pool: &PgPool, wallet_id: Uuid) -> bool {
    sqlx::query("SELECT balance FROM wallets WHERE wallet_id = $1")
        .bind(wallet_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|row| sqlx::Row::try_get::<i32, _>(&row, "balance").ok())
        .map(|b| b < LOW_BALANCE_THRESHOLD)
        .unwrap_or(false)
}
