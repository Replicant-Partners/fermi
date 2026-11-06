//! Black-box claims against a *deployed* ABW (the Railway dev prototype).
//!
//! Runs only when both `ABW_TEST_BASE_URL` and `ABW_TEST_DATABASE_URL` are set
//! and point at the same environment. Each test mints a throwaway user + API
//! key with the server's own `create_api_key`, calls the live HTTP route, and
//! checks the database. These are the tests that show a fix is *deployed*, not
//! just merged.

use crate::support;

pub fn base_url() -> Option<String> {
    std::env::var("ABW_TEST_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
}

pub struct Caller {
    pub user_id: String,
    pub key: String,
    pub key_id: uuid::Uuid,
}

pub async fn caller(pool: &sqlx::PgPool, purchased: i32) -> Caller {
    let user_id = support::tag("http");
    support::make_user(pool, &user_id).await;
    let w: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO wallets (owner_type, owner_id) VALUES ('user', $1) RETURNING wallet_id",
    )
    .bind(&user_id)
    .fetch_one(pool)
    .await
    .unwrap();
    if purchased > 0 {
        fermi_auth::credit_deposit(pool, w, purchased, "spec36 test funding")
            .await
            .unwrap();
    }
    let (key, info) = fermi_auth::api_keys::create_api_key(
        pool,
        &user_id,
        "spec36 claims",
        &["write".to_string()],
    )
    .await
    .expect("mint API key");
    Caller {
        user_id,
        key,
        key_id: info.key_id,
    }
}

pub async fn drop_caller(pool: &sqlx::PgPool, c: &Caller) {
    sqlx::query("DELETE FROM api_keys WHERE key_id = $1")
        .bind(c.key_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM credit_ledger WHERE wallet_id IN (SELECT wallet_id FROM wallets WHERE owner_id = $1)")
        .bind(&c.user_id).execute(pool).await.unwrap();
    sqlx::query("DELETE FROM wallets WHERE owner_id = $1")
        .bind(&c.user_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM team_members WHERE member_id = $1")
        .bind(&c.user_id)
        .execute(pool)
        .await
        .unwrap();
    support::drop_user(pool, &c.user_id).await;
}

/// Delete a workspace this run created, including anything the server added.
pub async fn drop_workspace(pool: &sqlx::PgPool, ws: uuid::Uuid) {
    let w = ws.to_string();
    for q in [
        "DELETE FROM workspace_agents WHERE workspace_id = $1::uuid",
        "DELETE FROM workspace_outputs WHERE workspace_id = $1::uuid",
        "DELETE FROM workspace_messages WHERE workspace_id = $1::uuid",
        "DELETE FROM workspace_dependencies WHERE downstream_id = $1::uuid OR upstream_id = $1::uuid",
        "DELETE FROM team_members WHERE team_id = $1::uuid",
        "DELETE FROM credit_ledger WHERE wallet_id IN (SELECT wallet_id FROM wallets WHERE owner_id = $1)",
        "DELETE FROM wallets WHERE owner_id = $1",
        "DELETE FROM teams WHERE id = $1::uuid",
    ] {
        sqlx::query(q).bind(&w).execute(pool).await.unwrap();
    }
}

/// G01 on the live route: a zero-balance caller must get 402 and mint nothing.
#[tokio::test]
async fn deployed_app_spawn_does_not_mint_credits() {
    let (Some(base), Some(pool)) = (base_url(), support::pool().await) else {
        eprintln!("SKIP: needs ABW_TEST_BASE_URL and ABW_TEST_DATABASE_URL");
        return;
    };
    let c = caller(&pool, 0).await;
    let before: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(amount),0)::bigint FROM credit_ledger")
            .fetch_one(&pool)
            .await
            .unwrap();

    let resp = reqwest::Client::new()
        .post(format!("{base}/api/apps/fermi_forecast/workspaces"))
        .bearer_auth(&c.key)
        .json(&serde_json::json!({"name": "spec36 G01 deployed probe", "extra_budget": 5}))
        .send()
        .await
        .expect("request");
    let status = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    let ws: Option<uuid::Uuid> = body["workspace_id"].as_str().and_then(|s| s.parse().ok());
    let after: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(amount),0)::bigint FROM credit_ledger")
            .fetch_one(&pool)
            .await
            .unwrap();
    let ws_balance: Option<i32> = match ws {
        Some(w) => sqlx::query_scalar("SELECT balance FROM wallets WHERE owner_id = $1")
            .bind(w.to_string())
            .fetch_optional(&pool)
            .await
            .unwrap(),
        None => None,
    };

    if let Some(w) = ws {
        drop_workspace(&pool, w).await;
    }
    drop_caller(&pool, &c).await;

    assert_eq!(
        (status, ws.is_some(), after - before),
        (402, false, 0),
        "G01: zero-balance spawn returned {status}, workspace {ws:?} funded with {ws_balance:?}, \
         ledger moved by {} — credits minted with no debit",
        after - before
    );
}
