use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::str::FromStr;

/// Pool for the claim tests, or `None` (test skipped) when
/// `ABW_TEST_DATABASE_URL` is unset. Statement cache off: the dev database is
/// behind a transaction-mode pooler, as in production.
pub async fn pool() -> Option<PgPool> {
    let url = match std::env::var("ABW_TEST_DATABASE_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => {
            eprintln!("SKIP: ABW_TEST_DATABASE_URL not set");
            return None;
        }
    };
    let opts = PgConnectOptions::from_str(&url)
        .expect("ABW_TEST_DATABASE_URL is not a valid Postgres URL")
        .statement_cache_capacity(0);
    Some(
        PgPoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await
            .expect("connect to ABW_TEST_DATABASE_URL"),
    )
}

/// A unique id for rows this test run creates, so cleanup is exact.
pub fn tag(prefix: &str) -> String {
    format!(
        "spec36_{prefix}_{}",
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    )
}

/// Create a throwaway user row (dev DB enforces `teams_owner_id_fk`,
/// `agents_user_id_fk`). Returns the `user_id` text key.
pub async fn make_user(pool: &PgPool, user_id: &str) {
    sqlx::query(
        "INSERT INTO users (email, password_hash, password_salt, user_id, auth_provider, display_name)
         VALUES ($1, 'x', 'x', $2, 'legacy', 'spec36 test user')",
    )
    .bind(format!("{user_id}@spec36.invalid"))
    .bind(user_id)
    .execute(pool)
    .await
    .expect("create test user");
}

pub async fn drop_user(pool: &PgPool, user_id: &str) {
    sqlx::query("DELETE FROM users WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("delete test user");
}

/// Ensure the platform sponsor wallet (migration 244) holds at least `need`,
/// topping up with an explicit, ledgered grant. Returns its wallet id.
pub async fn sponsor_wallet(pool: &PgPool, need: i32) -> uuid::Uuid {
    let owner = fermi::apps::provision::SPONSOR_WALLET_OWNER;
    let (w, bal): (uuid::Uuid, i32) = sqlx::query_as(
        "SELECT wallet_id, balance FROM wallets WHERE owner_id = $1",
    )
    .bind(owner)
    .fetch_one(pool)
    .await
    .expect("platform sponsor wallet missing — is migration 244 applied?");
    if bal < need {
        fermi_auth::credit_grant(pool, w, need - bal, "spec36 claims: sponsor top-up for tests")
            .await
            .expect("top up sponsor");
    }
    w
}
