//! Black-box claims against a *deployed* ABW (the Railway dev prototype).
//!
//! Runs only when `ABW_TEST_BASE_URL` and `ABW_TEST_DATABASE_URL` are both set
//! and point at the same environment. Not run in CI (CI has no deployment);
//! these are the evidence that a fix is *deployed*, and the only tests here
//! that can be pointed at old code.
//!
//! Observed 2026-11-06 against a3cea516 (before the fix): the G01 test below
//! FAILED with `201, workspace funded with 505, ledger moved by 505`.

use crate::support::{self, Created};

pub fn base_url() -> Option<String> {
    std::env::var("ABW_TEST_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
}

/// A throwaway user with a `purchased`-credit wallet and an API key minted by
/// the server's own `create_api_key`.
pub async fn caller(pool: &sqlx::PgPool, c: &mut Created, purchased: i32) -> (String, String) {
    let user = support::make_user(pool, c).await;
    support::make_wallet(pool, c, "user", &user, purchased).await;
    let (key, info) =
        fermi_auth::api_keys::create_api_key(pool, &user, "spec36 claims", &["write".to_string()])
            .await
            .expect("mint API key");
    c.api_keys.push(info.key_id);
    (user, key)
}

/// G01 on the live route: a zero-balance caller asking for `extra_budget`
/// must get 402 and nothing may be created or moved.
///
/// Deliberately exercises the caller-pays leg only, so it never spends the
/// shared `platform:app_sponsorship` wallet (asserted unchanged).
#[tokio::test]
#[ignore = "needs a deployment: ABW_TEST_BASE_URL + ABW_TEST_DATABASE_URL, run with --ignored"]
async fn deployed_app_spawn_does_not_mint_credits() {
    let base = base_url().expect("ABW_TEST_BASE_URL must be set for --ignored deployed tests");
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, move |pool, c| async move {
        let (user, key) = caller(&pool, &mut *c.lock().await, 0).await;
        let sponsor_before: Option<i32> = sqlx::query_scalar(
            "SELECT balance FROM wallets WHERE owner_type = 'platform' AND owner_id = 'platform:app_sponsorship'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        let ledger_before = support::ledger_sum(&pool, &[user.clone()]).await;

        let resp = reqwest::Client::new()
            .post(format!("{base}/api/apps/fermi_forecast/workspaces"))
            .bearer_auth(&key)
            .json(&serde_json::json!({"name": "spec36 G01 deployed probe", "extra_budget": 5}))
            .send()
            .await
            .expect("request");
        let status = resp.status().as_u16();
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        let ws: Option<uuid::Uuid> = body["workspace_id"].as_str().and_then(|s| s.parse().ok());
        if let Some(w) = ws {
            c.lock().await.teams.push(w);
        }
        let ws_balance: Option<i32> = match ws {
            Some(w) => sqlx::query_scalar("SELECT balance FROM wallets WHERE owner_id = $1")
                .bind(w.to_string()).fetch_optional(&pool).await.unwrap(),
            None => None,
        };
        let sponsor_after: Option<i32> = sqlx::query_scalar(
            "SELECT balance FROM wallets WHERE owner_type = 'platform' AND owner_id = 'platform:app_sponsorship'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        let mut owners = vec![user.clone()];
        if let Some(w) = ws {
            owners.push(w.to_string());
        }
        let ledger_moved = support::ledger_sum(&pool, &owners).await - ledger_before;

        assert_eq!(
            (status, ws.is_some()),
            (402, false),
            "G01: zero-balance spawn returned {status}, workspace {ws:?} funded with {ws_balance:?} \
             — credits minted with no debit"
        );
        assert_eq!(sponsor_before, sponsor_after, "a refused spawn must not touch the sponsor wallet");
        assert_eq!(ledger_moved, 0, "G01: ledger over caller + workspace moved by {ledger_moved}");
    })
    .await;
}
