//! Gas fee module — every A2A transaction has a cost.
//!
//! Gas fees fund the platform. Configurable via env vars with sensible defaults.

use fermi_auth::credit_charge;
use sqlx::PgPool;
use uuid::Uuid;

/// Gas fee schedule
#[derive(Debug, Clone)]
pub struct GasFees {
    pub message_send: i32,
    pub agent_hire: i32,
    pub agent_add: i32,
    pub execution_min: i32,
    pub execution_gas_pct: f64,
    pub consolidation_cycle: i32,
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
        }
    }
}

fn env_or(key: &str, default: i32) -> i32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

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
