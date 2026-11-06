//! Post-deploy smoke for G01 / R0.1: the sponsored path works end to end.
//!
//! A fresh user with zero credits spawns a seeded App. Expected on a deployment
//! running #132 with a funded sponsor wallet: 201, `template_paid_by =
//! "sponsor"`, caller charged nothing, sponsor debited exactly the template
//! budget, workspace credited the same, and one `app_sponsorships` row.
//!
//! This spends real sponsor credits (one template budget) and leaves the
//! append-only sponsorship row, so it is `#[ignore]` and run deliberately.

use crate::deployed::{base_url, caller};
use crate::support;

#[tokio::test]
#[ignore = "spends one sponsored spawn on a deployment; run deliberately with --ignored"]
async fn deployed_sponsored_spawn_charges_the_sponsor_not_the_caller() {
    let base = base_url().expect("ABW_TEST_BASE_URL must be set");
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, move |pool, c| async move {
        let (user, key) = caller(&pool, &mut *c.lock().await, 0).await;
        let sponsor = || async {
            sqlx::query_scalar::<_, i32>(
                "SELECT balance FROM wallets WHERE owner_type = 'platform' AND owner_id = 'platform:app_sponsorship'",
            )
            .fetch_one(&pool)
            .await
            .unwrap()
        };
        let template: i32 = sqlx::query_scalar(
            "SELECT (workspace_template->>'initial_budget')::int FROM apps WHERE slug = 'fermi_forecast'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let before = sponsor().await;

        let resp = reqwest::Client::new()
            .post(format!("{base}/api/apps/fermi_forecast/workspaces"))
            .bearer_auth(&key)
            .json(&serde_json::json!({"name": "spec36 sponsored smoke"}))
            .send()
            .await
            .unwrap();
        let status = resp.status().as_u16();
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        let ws: Option<uuid::Uuid> = body["workspace_id"].as_str().and_then(|s| s.parse().ok());
        if let Some(w) = ws {
            c.lock().await.teams.push(w);
        }
        let after = sponsor().await;
        let ws_balance: Option<i32> = match ws {
            Some(w) => sqlx::query_scalar("SELECT balance FROM wallets WHERE owner_id = $1")
                .bind(w.to_string())
                .fetch_optional(&pool)
                .await
                .unwrap(),
            None => None,
        };
        let caller_balance: i32 = sqlx::query_scalar("SELECT balance FROM wallets WHERE owner_id = $1")
            .bind(&user)
            .fetch_one(&pool)
            .await
            .unwrap();
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_sponsorships WHERE beneficiary_id = $1")
            .bind(&user)
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(status, 201, "spawn failed: {body}");
        assert_eq!(body["provisioned"]["template_paid_by"], "sponsor", "{body}");
        assert_eq!(caller_balance, 0, "caller was charged");
        assert_eq!(before - after, template, "sponsor debited exactly the template budget");
        assert_eq!(ws_balance, Some(template), "workspace credited exactly the template budget");
        assert_eq!(rows, 1, "one sponsorship recorded for the caller");
    })
    .await;
}
