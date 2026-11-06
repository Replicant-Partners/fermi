use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::str::FromStr;

/// Pool for the claim tests.
///
/// Reads `ABW_TEST_DATABASE_URL` only — never `DATABASE_URL` or `.env`
/// (issue #28). When unset:
/// * with `CLAIMS_REQUIRED=1` (CI) the test **panics** — a skipped claim is
///   not a passed claim;
/// * otherwise it returns `None` and the test returns early. Note `cargo test`
///   reports such a test as `ok`; the SKIP line on stderr is the only signal.
pub async fn pool() -> Option<PgPool> {
    let url = match std::env::var("ABW_TEST_DATABASE_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => {
            if std::env::var("CLAIMS_REQUIRED").as_deref() == Ok("1") {
                panic!("CLAIMS_REQUIRED=1 but ABW_TEST_DATABASE_URL is not set");
            }
            eprintln!("SKIP (reported as ok): ABW_TEST_DATABASE_URL not set");
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

/// Domain of every test user's email (RFC 2606 `.invalid`), so cleanup can
/// never match a real account.
pub const TEST_EMAIL_DOMAIN: &str = "spec36.invalid";

/// A unique id for rows this test run creates, so cleanup is exact.
pub fn tag(prefix: &str) -> String {
    format!(
        "spec36_{prefix}_{}",
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    )
}

/// Everything a test created, deleted on drop — including when an assertion
/// panics, so a failing test does not leave rows behind.
///
/// Deletion order respects foreign keys. It deletes `credit_ledger` rows only
/// for wallets this run created; when R2.3 makes the ledger immutable these
/// tests move to rolled-back transactions instead.
#[derive(Default)]
pub struct Created {
    pub users: Vec<String>,
    pub wallet_owners: Vec<String>,
    pub teams: Vec<uuid::Uuid>,
    pub agents: Vec<uuid::Uuid>,
    pub api_keys: Vec<uuid::Uuid>,
}

impl Created {
    pub async fn cleanup(&mut self, pool: &PgPool) {
        let teams: Vec<String> = self.teams.iter().map(|t| t.to_string()).collect();
        for q in [
            "DELETE FROM workspace_agents WHERE workspace_id::text = ANY($1)",
            "DELETE FROM workspace_outputs WHERE workspace_id::text = ANY($1)",
            "DELETE FROM workspace_messages WHERE workspace_id::text = ANY($1)",
            "DELETE FROM workspace_dependencies WHERE downstream_id::text = ANY($1) OR upstream_id::text = ANY($1)",
            "DELETE FROM team_members WHERE team_id::text = ANY($1)",
        ] {
            sqlx::query(q).bind(&teams).execute(pool).await.expect(q);
        }
        let mut owners = self.wallet_owners.clone();
        owners.extend(teams.iter().cloned());
        owners.extend(self.users.iter().cloned());
        // `app_sponsorships` is append-only by trigger (migration 244), and
        // cleanup does not weaken that. Rows it pins — the test sponsor
        // wallet and the sponsor's transfer_out ledger rows — are kept;
        // everything else this run created is deleted. Kept rows are all
        // `spec36_`-tagged and belong to per-test sponsor wallets, never to
        // `platform:app_sponsorship`.
        let pinned = "SELECT ledger_tx_id FROM app_sponsorships s
                        JOIN wallets w ON w.wallet_id = s.sponsor_wallet
                       WHERE w.owner_id = ANY($1)";
        sqlx::query(&format!(
            "DELETE FROM credit_ledger WHERE wallet_id IN (SELECT wallet_id FROM wallets WHERE owner_id = ANY($1))
               AND tx_id NOT IN ({pinned})"
        ))
        .bind(&owners)
        .execute(pool)
        .await
        .expect("ledger cleanup");
        sqlx::query(
            "DELETE FROM wallets w WHERE owner_id = ANY($1)
               AND NOT EXISTS (SELECT 1 FROM app_sponsorships s WHERE s.sponsor_wallet = w.wallet_id)
               AND NOT EXISTS (SELECT 1 FROM credit_ledger l WHERE l.wallet_id = w.wallet_id)",
        )
        .bind(&owners)
        .execute(pool)
        .await
        .expect("wallet cleanup");
        sqlx::query("DELETE FROM teams WHERE id::text = ANY($1)")
            .bind(&teams)
            .execute(pool)
            .await
            .expect("team cleanup");
        sqlx::query("DELETE FROM api_keys WHERE key_id = ANY($1)")
            .bind(&self.api_keys)
            .execute(pool)
            .await
            .expect("key cleanup");
        sqlx::query("DELETE FROM agents WHERE agent_id = ANY($1)")
            .bind(&self.agents)
            .execute(pool)
            .await
            .expect("agent cleanup");
        sqlx::query("DELETE FROM team_members WHERE member_id = ANY($1)")
            .bind(&self.users)
            .execute(pool)
            .await
            .expect("membership cleanup");
        sqlx::query("DELETE FROM users WHERE user_id = ANY($1) AND email LIKE $2")
            .bind(&self.users)
            .bind(format!("%@{TEST_EMAIL_DOMAIN}"))
            .execute(pool)
            .await
            .expect("user cleanup");
        *self = Created::default();
    }
}

/// Runs `body`, then cleans up what it recorded in `Created`, then re-raises
/// any panic from `body`.
pub async fn with_cleanup<F, Fut>(pool: &PgPool, body: F)
where
    F: FnOnce(PgPool, std::sync::Arc<tokio::sync::Mutex<Created>>) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let created = std::sync::Arc::new(tokio::sync::Mutex::new(Created::default()));
    let res = tokio::spawn(body(pool.clone(), created.clone())).await;
    created.lock().await.cleanup(pool).await;
    if let Err(e) = res {
        std::panic::resume_unwind(e.into_panic());
    }
}

/// Create a throwaway user row (the dev DB enforces `teams_owner_id_fk`,
/// `agents_user_id_fk`).
pub async fn make_user(pool: &PgPool, c: &mut Created) -> String {
    let user_id = tag("user");
    sqlx::query(
        "INSERT INTO users (email, password_hash, password_salt, user_id, auth_provider, display_name)
         VALUES ($1, 'x', 'x', $2, 'legacy', 'spec36 test user')",
    )
    .bind(format!("{user_id}@{TEST_EMAIL_DOMAIN}"))
    .bind(&user_id)
    .execute(pool)
    .await
    .expect("create test user");
    c.users.push(user_id.clone());
    user_id
}

/// A wallet of `owner_type` holding `purchased` transferable credits.
pub async fn make_wallet(
    pool: &PgPool,
    c: &mut Created,
    owner_type: &str,
    owner_id: &str,
    purchased: i32,
) -> uuid::Uuid {
    let w: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO wallets (owner_type, owner_id) VALUES ($1, $2) RETURNING wallet_id",
    )
    .bind(owner_type)
    .bind(owner_id)
    .fetch_one(pool)
    .await
    .expect("create wallet");
    c.wallet_owners.push(owner_id.to_string());
    if purchased > 0 {
        fermi_auth::credit_deposit(pool, w, purchased, "spec36 test funding")
            .await
            .expect("fund wallet");
    }
    w
}

/// Σ ledger over the given wallet owners — scoped, so concurrent writes by
/// the live server to unrelated wallets cannot perturb the assertion.
pub async fn ledger_sum(pool: &PgPool, owners: &[String]) -> i64 {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(l.amount),0)::bigint FROM credit_ledger l
         JOIN wallets w USING (wallet_id) WHERE w.owner_id = ANY($1)",
    )
    .bind(owners)
    .fetch_one(pool)
    .await
    .expect("ledger sum")
}

pub async fn balances(pool: &PgPool, owner_id: &str) -> (i32, i32, i32) {
    sqlx::query_as(
        "SELECT balance, granted_balance, purchased_balance FROM wallets WHERE owner_id = $1",
    )
    .bind(owner_id)
    .fetch_one(pool)
    .await
    .expect("balances")
}
