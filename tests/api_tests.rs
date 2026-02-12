//! Integration tests for Agent Bestiary platform.
//!
//! Run with: `cargo test --test api_tests -- --test-threads=1`
//! Requires DATABASE_URL environment variable.
//!
//! Tests cover:
//! - Credit system (wallets, grants, charges, deposits, ledger)
//! - Notifications CRUD
//! - Rate limiter logic
//! - Gas fee calculations

use fermi_auth::{
    credit_charge, credit_deposit, credit_get_balance, credit_get_transactions, credit_grant,
    get_or_create_wallet,
};
use sqlx::postgres::PgConnectOptions;
use sqlx::{PgPool, Row};
use std::str::FromStr;
use uuid::Uuid;

/// Create a test database pool (Neon-compatible: no prepared statement cache)
async fn test_pool() -> PgPool {
    // Try loading .env if present
    let _ = std::fs::read_to_string(".env").map(|contents| {
        for line in contents.lines() {
            if let Some((key, val)) = line.split_once('=') {
                let key = key.trim();
                let val = val.trim().trim_matches('"');
                if !key.is_empty() && !key.starts_with('#') && std::env::var(key).is_err() {
                    std::env::set_var(key, val);
                }
            }
        }
    });
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
    let opts = PgConnectOptions::from_str(&url)
        .expect("Invalid DATABASE_URL")
        .statement_cache_capacity(0);
    sqlx::pool::PoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect_with(opts)
        .await
        .expect("Failed to connect to test database")
}

/// Generate a unique test user ID to avoid collisions
fn test_user_id() -> String {
    format!("test_user_{}", Uuid::new_v4().to_string()[..8].to_string())
}

/// Cleanup: delete wallet and ledger entries for a test user
async fn cleanup_wallet(pool: &PgPool, owner_id: &str) {
    // Delete ledger entries first (FK constraint)
    let _ = sqlx::query(
        "DELETE FROM credit_ledger WHERE wallet_id IN (SELECT wallet_id FROM wallets WHERE owner_id = $1)",
    )
    .bind(owner_id)
    .execute(pool)
    .await;

    let _ = sqlx::query("DELETE FROM wallets WHERE owner_id = $1")
        .bind(owner_id)
        .execute(pool)
        .await;
}

/// Cleanup: delete notifications for a test user
async fn cleanup_notifications(pool: &PgPool, user_id: &str) {
    let _ = sqlx::query("DELETE FROM notifications WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await;
}

// ═══════════════════════════════════════════════════════════════════
// Credit System Tests
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_wallet_creation() {
    let pool = test_pool().await;
    let user_id = test_user_id();

    let wallet = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    assert_eq!(wallet.owner_type, "user");
    assert_eq!(wallet.owner_id, user_id);
    assert_eq!(wallet.balance, 0);
    assert_eq!(wallet.total_deposited, 0);
    assert_eq!(wallet.total_spent, 0);

    // Second call should return same wallet (idempotent)
    let wallet2 = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    assert_eq!(wallet.wallet_id, wallet2.wallet_id);

    cleanup_wallet(&pool, &user_id).await;
}

#[tokio::test]
async fn test_credit_grant() {
    let pool = test_pool().await;
    let user_id = test_user_id();

    let wallet = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    let tx = credit_grant(&pool, wallet.wallet_id, 100, "Test onboarding grant")
        .await
        .unwrap();

    assert_eq!(tx.amount, 100);
    assert_eq!(tx.balance_after, 100);
    assert_eq!(tx.tx_type, "grant");

    // Verify wallet balance updated
    let wallet_after = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    assert_eq!(wallet_after.balance, 100);
    assert_eq!(wallet_after.total_deposited, 100);

    cleanup_wallet(&pool, &user_id).await;
}

#[tokio::test]
async fn test_credit_deposit() {
    let pool = test_pool().await;
    let user_id = test_user_id();

    let wallet = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    let tx = credit_deposit(&pool, wallet.wallet_id, 500, "Stripe purchase")
        .await
        .unwrap();

    assert_eq!(tx.amount, 500);
    assert_eq!(tx.balance_after, 500);
    assert_eq!(tx.tx_type, "deposit");

    // Second deposit stacks
    let tx2 = credit_deposit(&pool, wallet.wallet_id, 200, "Another purchase")
        .await
        .unwrap();
    assert_eq!(tx2.balance_after, 700);

    let wallet_after = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    assert_eq!(wallet_after.balance, 700);
    assert_eq!(wallet_after.total_deposited, 700);

    cleanup_wallet(&pool, &user_id).await;
}

#[tokio::test]
async fn test_credit_charge_success() {
    let pool = test_pool().await;
    let user_id = test_user_id();

    let wallet = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    credit_grant(&pool, wallet.wallet_id, 100, "Setup")
        .await
        .unwrap();

    let tx = credit_charge(
        &pool,
        wallet.wallet_id,
        30,
        "execution_fee",
        "Agent run",
        None,
    )
    .await
    .unwrap();

    assert_eq!(tx.amount, -30); // Negative = debit
    assert_eq!(tx.balance_after, 70);
    assert_eq!(tx.tx_type, "execution_fee");

    let wallet_after = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    assert_eq!(wallet_after.balance, 70);
    assert_eq!(wallet_after.total_spent, 30);

    cleanup_wallet(&pool, &user_id).await;
}

#[tokio::test]
async fn test_credit_charge_insufficient_balance() {
    let pool = test_pool().await;
    let user_id = test_user_id();

    let wallet = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    credit_grant(&pool, wallet.wallet_id, 10, "Small grant")
        .await
        .unwrap();

    // Try to charge more than balance
    let result = credit_charge(
        &pool,
        wallet.wallet_id,
        50,
        "execution_fee",
        "Too expensive",
        None,
    )
    .await;
    assert!(result.is_err(), "Should fail with insufficient balance");

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Insufficient balance"),
        "Error should mention insufficient balance, got: {}",
        err_msg
    );

    // Balance should be unchanged
    let wallet_after = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    assert_eq!(
        wallet_after.balance, 10,
        "Balance should be unchanged after failed charge"
    );

    cleanup_wallet(&pool, &user_id).await;
}

#[tokio::test]
async fn test_credit_charge_exact_balance() {
    let pool = test_pool().await;
    let user_id = test_user_id();

    let wallet = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    credit_grant(&pool, wallet.wallet_id, 50, "Exact test")
        .await
        .unwrap();

    // Charge exactly the full balance
    let tx = credit_charge(&pool, wallet.wallet_id, 50, "gas_fee", "Full spend", None)
        .await
        .unwrap();
    assert_eq!(tx.balance_after, 0);

    let wallet_after = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    assert_eq!(wallet_after.balance, 0);
    assert_eq!(wallet_after.total_spent, 50);

    cleanup_wallet(&pool, &user_id).await;
}

#[tokio::test]
async fn test_credit_invalid_amounts() {
    let pool = test_pool().await;
    let user_id = test_user_id();

    let wallet = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();

    // Zero grant should fail
    assert!(credit_grant(&pool, wallet.wallet_id, 0, "Zero")
        .await
        .is_err());

    // Negative grant should fail
    assert!(credit_grant(&pool, wallet.wallet_id, -10, "Negative")
        .await
        .is_err());

    // Zero deposit should fail
    assert!(credit_deposit(&pool, wallet.wallet_id, 0, "Zero")
        .await
        .is_err());

    // Zero charge should fail
    assert!(
        credit_charge(&pool, wallet.wallet_id, 0, "gas_fee", "Zero", None)
            .await
            .is_err()
    );

    cleanup_wallet(&pool, &user_id).await;
}

#[tokio::test]
async fn test_transaction_ledger() {
    let pool = test_pool().await;
    let user_id = test_user_id();

    let wallet = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();

    // Create a series of transactions
    credit_grant(&pool, wallet.wallet_id, 100, "Onboarding")
        .await
        .unwrap();
    credit_deposit(&pool, wallet.wallet_id, 200, "Purchase")
        .await
        .unwrap();
    credit_charge(&pool, wallet.wallet_id, 30, "execution_fee", "Run 1", None)
        .await
        .unwrap();
    credit_charge(&pool, wallet.wallet_id, 10, "gas_fee", "Gas fee", None)
        .await
        .unwrap();

    let txs = credit_get_transactions(&pool, wallet.wallet_id, 10)
        .await
        .unwrap();

    // Should have 4 transactions, newest first
    assert_eq!(txs.len(), 4, "Expected 4 transactions");

    // Most recent first
    assert_eq!(txs[0].tx_type, "gas_fee");
    assert_eq!(txs[0].amount, -10);
    assert_eq!(txs[0].balance_after, 260);

    assert_eq!(txs[1].tx_type, "execution_fee");
    assert_eq!(txs[1].amount, -30);
    assert_eq!(txs[1].balance_after, 270);

    assert_eq!(txs[2].tx_type, "deposit");
    assert_eq!(txs[2].amount, 200);
    assert_eq!(txs[2].balance_after, 300);

    assert_eq!(txs[3].tx_type, "grant");
    assert_eq!(txs[3].amount, 100);
    assert_eq!(txs[3].balance_after, 100);

    // Test balance API
    let balance = credit_get_balance(&pool, wallet.wallet_id).await.unwrap();
    assert_eq!(balance, 260);

    cleanup_wallet(&pool, &user_id).await;
}

#[tokio::test]
async fn test_workspace_wallet_isolation() {
    let pool = test_pool().await;
    let user_id = test_user_id();
    let workspace_id = format!("test_ws_{}", Uuid::new_v4().to_string()[..8].to_string());

    // Create user wallet and workspace wallet
    let user_wallet = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    let ws_wallet = get_or_create_wallet(&pool, "workspace", &workspace_id)
        .await
        .unwrap();

    assert_ne!(
        user_wallet.wallet_id, ws_wallet.wallet_id,
        "Different wallets"
    );

    // Grant to user, deposit to workspace
    credit_grant(&pool, user_wallet.wallet_id, 100, "User grant")
        .await
        .unwrap();
    credit_deposit(&pool, ws_wallet.wallet_id, 50, "WS fund")
        .await
        .unwrap();

    // Charge from workspace shouldn't affect user
    credit_charge(&pool, ws_wallet.wallet_id, 10, "gas_fee", "WS gas", None)
        .await
        .unwrap();

    let user_after = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    let ws_after = get_or_create_wallet(&pool, "workspace", &workspace_id)
        .await
        .unwrap();

    assert_eq!(user_after.balance, 100, "User balance unchanged");
    assert_eq!(ws_after.balance, 40, "Workspace charged correctly");

    cleanup_wallet(&pool, &user_id).await;
    cleanup_wallet(&pool, &workspace_id).await;
}

// ═══════════════════════════════════════════════════════════════════
// Notification Tests
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_notifications_crud() {
    let pool = test_pool().await;
    let user_id = test_user_id();

    // Create notifications
    for i in 0..3 {
        sqlx::query(
            "INSERT INTO notifications (user_id, type, title, message) VALUES ($1, $2, $3, $4)",
        )
        .bind(&user_id)
        .bind("system")
        .bind(format!("Test notification {}", i))
        .bind(format!("Message body {}", i))
        .execute(&pool)
        .await
        .unwrap();
    }

    // List unread
    let rows = sqlx::query(
        "SELECT id, title, read FROM notifications WHERE user_id = $1 AND read = FALSE ORDER BY created_at DESC",
    )
    .bind(&user_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 3, "All 3 notifications should be unread");

    // Mark one as read
    let first_id: Uuid = rows[0].get("id");
    sqlx::query("UPDATE notifications SET read = TRUE WHERE id = $1")
        .bind(first_id)
        .execute(&pool)
        .await
        .unwrap();

    let unread = sqlx::query(
        "SELECT COUNT(*) as cnt FROM notifications WHERE user_id = $1 AND read = FALSE",
    )
    .bind(&user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let count: i64 = unread.get("cnt");
    assert_eq!(count, 2, "Should have 2 unread after marking one read");

    // Mark all as read
    sqlx::query("UPDATE notifications SET read = TRUE WHERE user_id = $1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();

    let unread2 = sqlx::query(
        "SELECT COUNT(*) as cnt FROM notifications WHERE user_id = $1 AND read = FALSE",
    )
    .bind(&user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let count2: i64 = unread2.get("cnt");
    assert_eq!(count2, 0, "All should be read");

    cleanup_notifications(&pool, &user_id).await;
}

// ═══════════════════════════════════════════════════════════════════
// Rate Limiter Unit Tests (in-process, no DB needed)
// ═══════════════════════════════════════════════════════════════════

// The rate limiter is a DashMap-based sliding window.
// Since it's private to api_server.rs, we test the pattern here with a minimal replica.

use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;

struct TestRateLimiter {
    windows: Arc<DashMap<String, Vec<Instant>>>,
    max_requests: u32,
    window_secs: u64,
}

impl TestRateLimiter {
    fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            windows: Arc::new(DashMap::new()),
            max_requests,
            window_secs,
        }
    }

    fn check(&self, key: &str) -> Result<u32, u64> {
        let now = Instant::now();
        let window_duration = std::time::Duration::from_secs(self.window_secs);
        let mut entry = self.windows.entry(key.to_string()).or_insert_with(Vec::new);
        entry.retain(|t| now.duration_since(*t) < window_duration);
        if entry.len() >= self.max_requests as usize {
            let oldest = entry.first().unwrap();
            let retry_after = window_duration
                .checked_sub(now.duration_since(*oldest))
                .unwrap_or_default()
                .as_secs()
                + 1;
            return Err(retry_after);
        }
        entry.push(now);
        Ok(self.max_requests - entry.len() as u32)
    }
}

#[test]
fn test_rate_limiter_allows_under_limit() {
    let rl = TestRateLimiter::new(5, 60);
    for i in 0..5 {
        let result = rl.check("user1");
        assert!(result.is_ok(), "Request {} should be allowed", i);
    }
}

#[test]
fn test_rate_limiter_blocks_over_limit() {
    let rl = TestRateLimiter::new(3, 60);
    assert!(rl.check("user1").is_ok());
    assert!(rl.check("user1").is_ok());
    assert!(rl.check("user1").is_ok());

    let result = rl.check("user1");
    assert!(result.is_err(), "4th request should be blocked");
}

#[test]
fn test_rate_limiter_isolates_keys() {
    let rl = TestRateLimiter::new(2, 60);
    assert!(rl.check("user1").is_ok());
    assert!(rl.check("user1").is_ok());
    assert!(rl.check("user1").is_err()); // user1 blocked

    // user2 should still be allowed
    assert!(rl.check("user2").is_ok());
    assert!(rl.check("user2").is_ok());
}

#[test]
fn test_rate_limiter_remaining_count() {
    let rl = TestRateLimiter::new(5, 60);
    assert_eq!(rl.check("user1").unwrap(), 4); // 5 - 1 = 4 remaining
    assert_eq!(rl.check("user1").unwrap(), 3);
    assert_eq!(rl.check("user1").unwrap(), 2);
    assert_eq!(rl.check("user1").unwrap(), 1);
    assert_eq!(rl.check("user1").unwrap(), 0);
    assert!(rl.check("user1").is_err());
}

// ═══════════════════════════════════════════════════════════════════
// Gas Fee Calculation Tests
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_execution_fee_calculation() {
    // Formula: max(1, tokens / 1000) + 10% gas
    let tokens = 5000;
    let execution_fee = std::cmp::max(1, tokens / 1000); // 5
    let gas_fee = std::cmp::max(1, (execution_fee as f64 * 0.10).ceil() as i32); // 1
    assert_eq!(execution_fee, 5);
    assert_eq!(gas_fee, 1);
    assert_eq!(execution_fee + gas_fee, 6); // Total cost for 5K tokens
}

#[test]
fn test_execution_fee_minimum() {
    // Even for tiny requests, min 1 credit execution + 1 credit gas
    let tokens = 100; // Under 1000
    let execution_fee = std::cmp::max(1, tokens / 1000); // 1 (min)
    let gas_fee = std::cmp::max(1, (execution_fee as f64 * 0.10).ceil() as i32); // 1 (min)
    assert_eq!(execution_fee, 1);
    assert_eq!(gas_fee, 1);
}

#[test]
fn test_execution_fee_large() {
    let tokens = 100_000;
    let execution_fee = std::cmp::max(1, tokens / 1000); // 100
    let gas_fee = std::cmp::max(1, (execution_fee as f64 * 0.10).ceil() as i32); // 10
    assert_eq!(execution_fee, 100);
    assert_eq!(gas_fee, 10);
    assert_eq!(execution_fee + gas_fee, 110);
}

// ═══════════════════════════════════════════════════════════════════
// Sprint R: GasFees struct tests (unit, no DB)
// ═══════════════════════════════════════════════════════════════════

use fermi::gas::GasFees;

#[test]
fn test_gas_fees_default_values() {
    let fees = GasFees::default();
    assert_eq!(fees.message_send, 1);
    assert_eq!(fees.agent_hire, 5);
    assert_eq!(fees.agent_add, 2);
    assert_eq!(fees.execution_min, 1);
    assert_eq!(fees.platform_read, 1);
    assert_eq!(fees.creature_mint, 3);
    assert_eq!(fees.creature_art, 5);
    assert_eq!(fees.rabble_chat, 1);
    assert_eq!(fees.marketplace_listing_fee, 3);
    assert_eq!((fees.execution_gas_pct * 100.0) as i32, 10); // 10%
    assert_eq!((fees.marketplace_platform_pct * 100.0) as i32, 15); // 15%
    assert_eq!((fees.crypto_tx_fee_pct * 1000.0) as i32, 25); // 2.5%
}

#[test]
fn test_gas_fees_execution_fee_method() {
    let fees = GasFees::default();

    // Small request — minimums kick in
    let (base, gas) = fees.execution_fee(100);
    assert_eq!(base, 1, "Min 1 credit for <1000 tokens");
    assert_eq!(gas, 1, "Min 1 credit gas");

    // Medium request
    let (base, gas) = fees.execution_fee(5000);
    assert_eq!(base, 5);
    assert_eq!(gas, 1, "10% of 5 rounds to 0, but min is 1");

    // Large request
    let (base, gas) = fees.execution_fee(50_000);
    assert_eq!(base, 50);
    assert_eq!(gas, 5, "10% of 50 = 5");

    // Very large request
    let (base, gas) = fees.execution_fee(100_000);
    assert_eq!(base, 100);
    assert_eq!(gas, 10, "10% of 100 = 10");
}

#[test]
fn test_gas_fees_upload_fee_method() {
    let fees = GasFees::default();

    // Tiny file — min 1 MB
    let fee = fees.upload_fee(1024); // 1 KB
    assert_eq!(fee, 1 + 1, "file_write(1) + 1MB(1)");

    // 1 MB exactly
    let fee = fees.upload_fee(1024 * 1024);
    assert_eq!(fee, 1 + 1, "file_write(1) + 1MB(1)");

    // 1.5 MB — ceil to 2 MB
    let fee = fees.upload_fee(1024 * 1024 + 512 * 1024);
    assert_eq!(fee, 1 + 2, "file_write(1) + ceil(1.5)=2MB(2)");

    // 5 MB
    let fee = fees.upload_fee(5 * 1024 * 1024);
    assert_eq!(fee, 1 + 5, "file_write(1) + 5MB(5)");
}

#[test]
fn test_gas_fees_execution_fee_zero_tokens() {
    let fees = GasFees::default();
    let (base, gas) = fees.execution_fee(0);
    assert_eq!(base, 1, "Min execution fee even for 0 tokens");
    assert_eq!(gas, 1, "Min gas fee");
}

// ═══════════════════════════════════════════════════════════════════
// Sprint R: charge_and_distribute integration tests
// ═══════════════════════════════════════════════════════════════════

use fermi::gas::charge_and_distribute;

#[tokio::test]
async fn test_charge_and_distribute_basic() {
    let pool = test_pool().await;
    let user_id = test_user_id();
    let agent1_id = Uuid::new_v4();
    let agent2_id = Uuid::new_v4();

    // Setup: create user wallet with funds
    let user_wallet = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    credit_grant(&pool, user_wallet.wallet_id, 100, "Test funds")
        .await
        .unwrap();

    // Create test agents (unique names to avoid collision across runs)
    let suffix = Uuid::new_v4().to_string()[..8].to_string();
    for (agent_id, prefix) in &[(agent1_id, "test_dist_a1"), (agent2_id, "test_dist_a2")] {
        sqlx::query(
            "INSERT INTO agents (agent_id, agent_name, agent_type, version, tier, executor_type, model, temperature, author)
             VALUES ($1, $2, 'test', '1.0.0', 'system', 'llm', 'test-model', 0.3, 'test')"
        )
        .bind(agent_id)
        .bind(format!("{}_{}", prefix, suffix))
        .execute(&pool)
        .await
        .unwrap();
    }

    // Distribute 10 credits among 2 agents
    let agent_ids = vec![agent1_id, agent2_id];
    let charged = charge_and_distribute(
        &pool,
        user_wallet.wallet_id,
        10,
        "execution_fee",
        "Test distribution",
        &agent_ids,
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(charged, 10, "Should charge full amount");

    // User balance reduced
    let user_after = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    assert_eq!(user_after.balance, 90, "User should have 90 remaining");

    // Platform keeps 10% = 1 credit, agents get 9 total
    // 9 / 2 = 4 each, remainder 1 goes to last agent
    let a1_wallet = get_or_create_wallet(&pool, "agent", &agent1_id.to_string())
        .await
        .unwrap();
    let a2_wallet = get_or_create_wallet(&pool, "agent", &agent2_id.to_string())
        .await
        .unwrap();

    assert_eq!(a1_wallet.balance, 4, "Agent 1 gets 4 credits");
    assert_eq!(a2_wallet.balance, 5, "Agent 2 gets 4 + 1 remainder = 5");

    // Verify payout records
    let payouts = sqlx::query(
        "SELECT agent_id, amount FROM agent_episode_payouts WHERE agent_id IN ($1, $2) ORDER BY amount"
    )
    .bind(agent1_id)
    .bind(agent2_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(payouts.len(), 2, "Should have 2 payout records");

    // Cleanup
    cleanup_wallet(&pool, &user_id).await;
    cleanup_wallet(&pool, &agent1_id.to_string()).await;
    cleanup_wallet(&pool, &agent2_id.to_string()).await;
    let _ = sqlx::query("DELETE FROM agent_episode_payouts WHERE agent_id IN ($1, $2)")
        .bind(agent1_id)
        .bind(agent2_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE agent_id IN ($1, $2)")
        .bind(agent1_id)
        .bind(agent2_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn test_charge_and_distribute_no_agents() {
    let pool = test_pool().await;
    let user_id = test_user_id();

    let user_wallet = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    credit_grant(&pool, user_wallet.wallet_id, 50, "Test funds")
        .await
        .unwrap();

    // Distribute with empty agent list — should still charge user
    let charged = charge_and_distribute(
        &pool,
        user_wallet.wallet_id,
        5,
        "platform_read",
        "Infrastructure read",
        &[], // no agents
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(charged, 5);
    let user_after = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    assert_eq!(user_after.balance, 45, "User charged even with no agents");

    cleanup_wallet(&pool, &user_id).await;
}

#[tokio::test]
async fn test_charge_and_distribute_insufficient_funds() {
    let pool = test_pool().await;
    let user_id = test_user_id();

    let user_wallet = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    credit_grant(&pool, user_wallet.wallet_id, 3, "Small funds")
        .await
        .unwrap();

    let result = charge_and_distribute(
        &pool,
        user_wallet.wallet_id,
        10, // more than balance
        "execution_fee",
        "Too expensive",
        &[Uuid::new_v4()],
        None,
        None,
    )
    .await;

    assert!(result.is_err(), "Should fail with insufficient funds");
    let (status, _msg) = result.unwrap_err();
    assert_eq!(status, axum::http::StatusCode::PAYMENT_REQUIRED);

    // Balance unchanged
    let user_after = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    assert_eq!(
        user_after.balance, 3,
        "Balance unchanged after failed charge"
    );

    cleanup_wallet(&pool, &user_id).await;
}

#[tokio::test]
async fn test_charge_and_distribute_single_agent() {
    let pool = test_pool().await;
    let user_id = test_user_id();
    let agent_id = Uuid::new_v4();

    let user_wallet = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    credit_grant(&pool, user_wallet.wallet_id, 100, "Test funds")
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO agents (agent_id, agent_name, agent_type, version, tier, executor_type, model, temperature, author)
         VALUES ($1, $2, 'test', '1.0.0', 'system', 'llm', 'test-model', 0.3, 'test')"
    )
    .bind(agent_id)
    .bind(format!("test_dist_single_{}", Uuid::new_v4().to_string()[..8].to_string()))
    .execute(&pool)
    .await
    .unwrap();

    // 20 credits to 1 agent: platform gets 2, agent gets 18
    let charged = charge_and_distribute(
        &pool,
        user_wallet.wallet_id,
        20,
        "execution_fee",
        "Single agent test",
        &[agent_id],
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(charged, 20);

    let agent_wallet = get_or_create_wallet(&pool, "agent", &agent_id.to_string())
        .await
        .unwrap();
    assert_eq!(agent_wallet.balance, 18, "Agent gets 90% of 20 = 18");

    // Cleanup
    cleanup_wallet(&pool, &user_id).await;
    cleanup_wallet(&pool, &agent_id.to_string()).await;
    let _ = sqlx::query("DELETE FROM agent_episode_payouts WHERE agent_id = $1")
        .bind(agent_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE agent_id = $1")
        .bind(agent_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn test_charge_and_distribute_four_agents() {
    let pool = test_pool().await;
    let user_id = test_user_id();

    let user_wallet = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    credit_grant(&pool, user_wallet.wallet_id, 100, "Test funds")
        .await
        .unwrap();

    // Create 4 agents (like rabble workspace pattern, unique names per run)
    let agent_ids: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();
    let suffix = Uuid::new_v4().to_string()[..8].to_string();
    for (i, agent_id) in agent_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO agents (agent_id, agent_name, agent_type, version, tier, executor_type, model, temperature, author)
             VALUES ($1, $2, 'test', '1.0.0', 'system', 'llm', 'test-model', 0.3, 'test')"
        )
        .bind(agent_id)
        .bind(format!("test_dist_4a_{}_{}", i, suffix))
        .execute(&pool)
        .await
        .unwrap();
    }

    // 8 credits / 4 agents: platform 1, agent pool 7, per_agent 1, remainder 3 to last
    let charged = charge_and_distribute(
        &pool,
        user_wallet.wallet_id,
        8,
        "creature_mint",
        "Mint distribution",
        &agent_ids,
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(charged, 8);

    // Platform keeps max(1, 8*0.10) = 1, agent pool = 7
    // 7 / 4 = 1 per agent, remainder 3 goes to last agent
    for (i, agent_id) in agent_ids.iter().enumerate() {
        let w = get_or_create_wallet(&pool, "agent", &agent_id.to_string())
            .await
            .unwrap();
        if i == 3 {
            assert_eq!(w.balance, 4, "Last agent gets 1 + 3 remainder = 4");
        } else {
            assert_eq!(w.balance, 1, "Agent {} gets 1 credit", i);
        }
    }

    // Cleanup
    cleanup_wallet(&pool, &user_id).await;
    for agent_id in &agent_ids {
        cleanup_wallet(&pool, &agent_id.to_string()).await;
        let _ = sqlx::query("DELETE FROM agent_episode_payouts WHERE agent_id = $1")
            .bind(agent_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM agents WHERE agent_id = $1")
            .bind(agent_id)
            .execute(&pool)
            .await;
    }
}

// ═══════════════════════════════════════════════════════════════════
// Sprint R: Platform read gas (charge_gas wrapper)
// ═══════════════════════════════════════════════════════════════════

use fermi::gas::charge_gas;

#[tokio::test]
async fn test_platform_read_charge() {
    let pool = test_pool().await;
    let user_id = test_user_id();

    let wallet = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    credit_grant(&pool, wallet.wallet_id, 10, "Test funds")
        .await
        .unwrap();

    let fees = GasFees::default();

    // Charge platform_read
    let charged = charge_gas(
        &pool,
        wallet.wallet_id,
        fees.platform_read,
        "platform_read",
        "Flock visualization data",
        Some("test-swarm-id"),
    )
    .await
    .unwrap();

    assert_eq!(charged, 1, "Platform read costs 1 credit");

    let wallet_after = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    assert_eq!(wallet_after.balance, 9);

    // Verify ledger entry
    let txs = credit_get_transactions(&pool, wallet.wallet_id, 5)
        .await
        .unwrap();
    let read_tx = txs.iter().find(|t| t.tx_type == "platform_read");
    assert!(read_tx.is_some(), "Should have platform_read tx in ledger");
    assert_eq!(read_tx.unwrap().amount, -1);

    cleanup_wallet(&pool, &user_id).await;
}

#[tokio::test]
async fn test_platform_read_charge_insufficient() {
    let pool = test_pool().await;
    let user_id = test_user_id();

    let wallet = get_or_create_wallet(&pool, "user", &user_id).await.unwrap();
    // No credits granted — wallet has 0 balance

    let result = charge_gas(
        &pool,
        wallet.wallet_id,
        1,
        "platform_read",
        "Should fail",
        None,
    )
    .await;

    assert!(result.is_err(), "Should fail with 0 balance");
    let (status, _) = result.unwrap_err();
    assert_eq!(status, axum::http::StatusCode::PAYMENT_REQUIRED);

    cleanup_wallet(&pool, &user_id).await;
}
