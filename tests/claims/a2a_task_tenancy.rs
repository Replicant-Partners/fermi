//! SPEC 36 R0.2 / LLD G02 (#38): A2A task reads are tenant-scoped.
//!
//! Before: `GET /a2a/:slug/tasks/:episode_id` checked only the key's scope,
//! then returned any episode by UUID [V: src/handlers/a2a.rs:583-656 @
//! a3cea516]. The deployed test below shows it on the live route.
//!
//! The rule under test (`task_belongs_to`): the episode's `source_ref` names
//! the caller, is an A2A kind, and the episode ran the agent named in the
//! path. Anything else is 404.

use crate::deployed::{base_url, caller};
use crate::support::{self, Created};

/// Insert a finished episode for `agent`, attributed to `owner` the way the
/// A2A write paths attribute it (`source_ref.caller_user_id`).
async fn a2a_episode(
    pool: &sqlx::PgPool,
    agent: uuid::Uuid,
    owner: &str,
    kind: &str,
) -> uuid::Uuid {
    sqlx::query_scalar(
        "INSERT INTO episodes (agent_id, timestamp_ref, query, context, execution_status,
                               execution_time_ms, response_text, source_ref)
         VALUES ($1, now(), 'spec36 a2a tenancy probe', '{}'::jsonb, 'success', 1,
                 '{\"answer\":\"secret of the owner\"}', $2)
         RETURNING episode_id",
    )
    .bind(agent)
    .bind(serde_json::json!({"kind": kind, "agent_slug": "x", "caller_user_id": owner}))
    .fetch_one(pool)
    .await
    .expect("insert episode")
}

async fn public_agent(pool: &sqlx::PgPool, c: &mut Created) -> (uuid::Uuid, String) {
    let name = support::tag("a2a_agent");
    let id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO agents (agent_name, agent_type, version, tier, executor_type, model, temperature,
                             author, status, visibility)
         VALUES ($1, 'research', '1.0.0', 'curated', 'llm', 'test', 0.5, 'spec36', 'published', 'public')
         RETURNING agent_id",
    )
    .bind(&name)
    .fetch_one(pool)
    .await
    .expect("insert agent");
    c.agents.push(id);
    (id, name)
}

async fn get_task(base: &str, key: &str, slug: &str, ep: uuid::Uuid) -> (u16, String) {
    let r = reqwest::Client::new()
        .get(format!("{base}/a2a/{slug}/tasks/{ep}"))
        .bearer_auth(key)
        .send()
        .await
        .expect("request");
    let s = r.status().as_u16();
    (s, r.text().await.unwrap_or_default())
}

async fn mint_a2a_key(pool: &sqlx::PgPool, c: &mut Created) -> (String, String) {
    let user = support::make_user(pool, c).await;
    let (key, info) = fermi_auth::api_keys::create_api_key(
        pool,
        &user,
        "spec36 claims a2a",
        &["a2a:invoke:*".to_string()],
    )
    .await
    .expect("mint key");
    c.api_keys.push(info.key_id);
    (user, key)
}

/// Live route: another user's key, with full `a2a:invoke:*` scope, must not
/// read the task. Fails on a3cea516 (200 with the owner's response).
#[tokio::test]
#[ignore = "needs a deployment: ABW_TEST_BASE_URL + ABW_TEST_DATABASE_URL, run with --ignored"]
async fn deployed_a2a_task_is_invisible_to_other_users() {
    let base = base_url().expect("ABW_TEST_BASE_URL must be set for --ignored deployed tests");
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, move |pool, c| async move {
        let (agent, slug) = public_agent(&pool, &mut *c.lock().await).await;
        let (owner, owner_key) = mint_a2a_key(&pool, &mut *c.lock().await).await;
        let (_other, other_key) = mint_a2a_key(&pool, &mut *c.lock().await).await;
        let ep = a2a_episode(&pool, agent, &owner, "a2a_handler").await;

        let (other_status, other_body) = get_task(&base, &other_key, &slug, ep).await;
        let (owner_status, _) = get_task(&base, &owner_key, &slug, ep).await;
        sqlx::query("DELETE FROM episodes WHERE episode_id = $1")
            .bind(ep)
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(
            other_status,
            404,
            "G02: another user's key read the task (HTTP {other_status}): {}",
            &other_body[..other_body.len().min(200)]
        );
        assert_eq!(
            owner_status, 200,
            "the owner must still be able to poll their task"
        );
    })
    .await;
}

/// `task_belongs_to` against the DB: owner yes; other user no; right owner but
/// wrong slug no; non-A2A episode no.
#[tokio::test]
async fn task_ownership_rule() {
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, |pool, c| async move {
        let (agent, slug) = public_agent(&pool, &mut *c.lock().await).await;
        let (_other_agent, other_slug) = public_agent(&pool, &mut *c.lock().await).await;
        let owner = support::make_user(&pool, &mut *c.lock().await).await;
        let ep = a2a_episode(&pool, agent, &owner, "a2a_stream_handler").await;
        let not_a2a = a2a_episode(&pool, agent, &owner, "execute_handler").await;

        let q = |e: uuid::Uuid, s: String, u: String| {
            let pool = pool.clone();
            async move {
                fermi::a2a_task::task_belongs_to(&pool, e, &s, &u)
                    .await
                    .unwrap()
            }
        };
        let owned = q(ep, slug.clone(), owner.clone()).await;
        let other_user = q(ep, slug.clone(), "someone_else".into()).await;
        let wrong_slug = q(ep, other_slug, owner.clone()).await;
        let non_a2a = q(not_a2a, slug, owner).await;
        sqlx::query("DELETE FROM episodes WHERE episode_id = ANY($1)")
            .bind(vec![ep, not_a2a])
            .execute(&pool)
            .await
            .unwrap();

        assert!(owned, "owner must see their task");
        assert!(!other_user, "another user must not");
        assert!(
            !wrong_slug,
            "the slug in the path must be the agent that ran it"
        );
        assert!(!non_a2a, "only A2A episodes are A2A tasks");
    })
    .await;
}
