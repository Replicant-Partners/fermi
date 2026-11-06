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
        c.lock().await.episodes.push(ep);

        let (other_status, other_body) = get_task(&base, &other_key, &slug, ep).await;
        let (owner_status, _) = get_task(&base, &owner_key, &slug, ep).await;

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
/// wrong slug no; non-A2A episode no. Runs in a rolled-back transaction.
#[tokio::test]
async fn task_ownership_rule() {
    let Some(pool) = support::pool().await else {
        return;
    };
    let mut tx = pool.begin().await.unwrap();
    let agent_sql =
        "INSERT INTO agents (agent_name, agent_type, version, tier, executor_type, model,
                                         temperature, author, status, visibility)
                     VALUES ($1, 'research', '1.0.0', 'curated', 'llm', 'test', 0.5, 'spec36',
                             'published', 'public')
                     RETURNING agent_id";
    let slug = support::tag("a2a_agent");
    let other_slug = support::tag("a2a_agent");
    let agent: uuid::Uuid = sqlx::query_scalar(agent_sql)
        .bind(&slug)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    sqlx::query_scalar::<_, uuid::Uuid>(agent_sql)
        .bind(&other_slug)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let owner = support::tag("user");
    let ep_sql = "INSERT INTO episodes (agent_id, timestamp_ref, query, context, execution_status,
                                        execution_time_ms, source_ref)
                  VALUES ($1, now(), 'spec36 a2a tenancy probe', '{}'::jsonb, 'success', 1, $2)
                  RETURNING episode_id";
    let ep: uuid::Uuid = sqlx::query_scalar(ep_sql)
        .bind(agent)
        .bind(serde_json::json!({"kind": "a2a_stream_handler", "caller_user_id": owner}))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let not_a2a: uuid::Uuid = sqlx::query_scalar(ep_sql)
        .bind(agent)
        .bind(serde_json::json!({"kind": "execute_handler", "caller_user_id": owner}))
        .fetch_one(&mut *tx)
        .await
        .unwrap();

    // Fail-closed shapes: a reservation (no source_ref), and an A2A kind
    // with no caller recorded.
    let reserved: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO episodes (agent_id, timestamp_ref, query, context, execution_status, execution_time_ms)
         VALUES ($1, now(), 'spec36 reservation', '{}'::jsonb, 'running', 0) RETURNING episode_id",
    )
    .bind(agent)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let no_caller: uuid::Uuid = sqlx::query_scalar(ep_sql)
        .bind(agent)
        .bind(serde_json::json!({"kind": "a2a_handler", "agent_slug": slug}))
        .fetch_one(&mut *tx)
        .await
        .unwrap();

    use fermi::a2a_task::task_belongs_to as owns;
    assert!(
        !owns(&mut tx, reserved, &slug, &owner).await.unwrap(),
        "a reservation is not yet a task"
    );
    assert!(
        !owns(&mut tx, no_caller, &slug, &owner).await.unwrap(),
        "no recorded caller: nobody owns it"
    );
    assert!(
        owns(&mut tx, ep, &slug, &owner).await.unwrap(),
        "owner must see their task"
    );
    assert!(
        !owns(&mut tx, ep, &slug, "someone_else").await.unwrap(),
        "another user must not"
    );
    assert!(
        !owns(&mut tx, ep, &other_slug, &owner).await.unwrap(),
        "path slug must be the agent that ran it"
    );
    assert!(
        !owns(&mut tx, not_a2a, &slug, &owner).await.unwrap(),
        "only A2A episodes are A2A tasks"
    );
}

/// Each A2A handler that takes a task id from the path authorises it with
/// `task_belongs_to` before touching the task, and the two `:config_id`
/// routes filter on the slug too. A source check, because an in-process
/// router needs a full AppState; deleting any guard fails this test.
#[test]
fn every_task_id_route_is_guarded() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/handlers/a2a.rs"))
        .unwrap();
    let body = |name: &str| -> String {
        let start = src
            .find(&format!("pub async fn {name}("))
            .unwrap_or_else(|| panic!("{name} missing"));
        let rest = &src[start..];
        let end = rest[1..]
            .find("\npub async fn ")
            .map(|i| i + 1)
            .unwrap_or(rest.len());
        rest[..end].to_string()
    };
    for h in [
        "get_task_handler",
        "create_push_config_handler",
        "list_push_configs_handler",
    ] {
        assert!(
            body(h).contains("task_belongs_to(&state"),
            "{h} must call task_belongs_to"
        );
    }
    for h in ["get_push_config_handler", "delete_push_config_handler"] {
        let b = body(h);
        assert!(
            b.contains("caller_user_id = $3 AND agent_slug = $4"),
            "{h} must filter on caller and slug"
        );
    }
}
