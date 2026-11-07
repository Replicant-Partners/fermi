//! G03 on the live routes: a user-supplied URL pointing at an internal address
//! is refused at registration. On a3cea516..1d3cf4d3 both were accepted.

use crate::deployed::{base_url, caller};
use crate::support;

#[tokio::test]
#[ignore = "needs a deployment: ABW_TEST_BASE_URL + ABW_TEST_DATABASE_URL, run with --ignored"]
async fn deployed_internal_urls_are_refused() {
    let base = base_url().expect("ABW_TEST_BASE_URL must be set");
    let Some(pool) = support::pool().await else { return };
    support::with_cleanup(&pool, move |pool, c| async move {
        let (owner, _key) = caller(&pool, &mut *c.lock().await, 0).await;
        // An A2A key and a finished task the caller owns, so registration
        // reaches the URL check rather than the ownership 404.
        let (a2a_key, info) = fermi_auth::api_keys::create_api_key(
            &pool,
            &owner,
            "spec36 claims a2a",
            &["a2a:invoke:*".to_string(), "write".to_string()],
        )
        .await
        .unwrap();
        c.lock().await.api_keys.push(info.key_id);
        let slug = support::tag("a2a_agent");
        let agent: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO agents (agent_name, agent_type, version, tier, executor_type, model, temperature,
                                 author, status, visibility)
             VALUES ($1, 'research', '1.0.0', 'curated', 'llm', 'test', 0.5, 'spec36', 'published', 'public')
             RETURNING agent_id",
        )
        .bind(&slug)
        .fetch_one(&pool)
        .await
        .unwrap();
        c.lock().await.agents.push(agent);
        let task: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO episodes (agent_id, timestamp_ref, query, context, execution_status,
                                   execution_time_ms, source_ref)
             VALUES ($1, now(), 'spec36 ssrf probe', '{}'::jsonb, 'success', 1, $2) RETURNING episode_id",
        )
        .bind(agent)
        .bind(serde_json::json!({"kind": "a2a_handler", "agent_slug": slug, "caller_user_id": owner}))
        .fetch_one(&pool)
        .await
        .unwrap();
        c.lock().await.episodes.push(task);

        let http = reqwest::Client::new();
        let mut accepted = Vec::new();
        for target in ["http://169.254.169.254/latest/meta-data/", "https://127.0.0.1:5432/", "https://10.0.0.1/"] {
            let r = http
                .post(format!("{base}/a2a/{slug}/tasks/{task}/pushNotificationConfigs"))
                .bearer_auth(&a2a_key)
                .json(&serde_json::json!({"url": target}))
                .send()
                .await
                .unwrap();
            let s = r.status().as_u16();
            if (200..300).contains(&s) {
                accepted.push(format!("push webhook {target}: HTTP {s}"));
            }
        }
        // Push subscription endpoint.
        let r = http
            .post(format!("{base}/api/push/subscribe"))
            .bearer_auth(&a2a_key)
            .json(&serde_json::json!({"endpoint": "http://169.254.169.254/", "p256dh": "x", "auth": "x"}))
            .send()
            .await
            .unwrap();
        let s = r.status().as_u16();
        if (200..300).contains(&s) {
            accepted.push(format!("push subscribe 169.254.169.254: HTTP {s}"));
        }
        // Clean up anything the old server stored.
        sqlx::query("DELETE FROM a2a_push_configs WHERE task_id = $1")
            .bind(task)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM push_subscriptions WHERE user_id = $1")
            .bind(&owner)
            .execute(&pool)
            .await
            .unwrap();

        assert!(accepted.is_empty(), "G03: internal URLs accepted:\n{}", accepted.join("\n"));
    })
    .await;
}
