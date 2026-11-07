//! The other half of R0.2.4: the rule must not over-block. An agent's owner
//! reads their own agent's episode and trace through the live routes.

use crate::deployed::base_url;
use crate::support;

#[tokio::test]
#[ignore = "needs a deployment: ABW_TEST_BASE_URL + ABW_TEST_DATABASE_URL, run with --ignored"]
async fn deployed_owner_reads_own_episode() {
    let base = base_url().expect("ABW_TEST_BASE_URL must be set");
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, move |pool, c| async move {
        // A throwaway owner, agent and finished episode.
        let owner = support::make_user(&pool, &mut *c.lock().await).await;
        let agent: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO agents (agent_name, agent_type, version, tier, executor_type, model, temperature,
                                 author, status, visibility, user_id)
             VALUES ($1, 'research', '1.0.0', 'community', 'llm', 'test', 0.5, 'spec36', 'draft', 'private', $2)
             RETURNING agent_id",
        )
        .bind(support::tag("agent"))
        .bind(&owner)
        .fetch_one(&pool)
        .await
        .unwrap();
        c.lock().await.agents.push(agent);
        let ep: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO episodes (agent_id, timestamp_ref, query, context, execution_status,
                                   execution_time_ms, response_text)
             VALUES ($1, now(), 'spec36 owner probe', '{}'::jsonb, 'success', 1, 'ok') RETURNING episode_id",
        )
        .bind(agent)
        .fetch_one(&pool)
        .await
        .unwrap();
        c.lock().await.episodes.push(ep);
        let (key, info) =
            fermi_auth::api_keys::create_api_key(&pool, &owner, "spec36 claims", &["write".to_string()])
                .await
                .unwrap();
        c.lock().await.api_keys.push(info.key_id);

        let http = reqwest::Client::new();
        for path in [format!("/api/episodes/{ep}"), format!("/api/episodes/{ep}/trace")] {
            let s = http
                .get(format!("{base}{path}"))
                .bearer_auth(&key)
                .send()
                .await
                .unwrap()
                .status()
                .as_u16();
            assert_eq!(s, 200, "owner must read {path}");
        }
    })
    .await;
}
