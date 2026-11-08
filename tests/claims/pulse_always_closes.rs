//! SPEC 36 R0.5 / LLD G07 (#41): every opened pulse is closed.
//!
//! `Pulse::open` reserves an episode as `running`. Before this change only
//! `close` ever finished it, so an executor error, an early `?` or a panic
//! left the row `running` for good [V: 8 such rows on the dev DB on
//! 2026-11-07; `rabble_workspace.rs` alone had a hand-written cleanup].
//!
//! The deployed tests force an executor error with an agent whose
//! `llm_provider` is unknown: `MultiModelExecutor::execute` refuses it before
//! any network call (`src/agent_backend/multi_model_executor.rs:270-283`), so
//! the tests spend nothing.

use std::time::Duration;

use agent_bestiary_memory::MemoryStore;
use fermi::episode_boundary::Pulse;

use crate::deployed::base_url;
use crate::support;

const BOGUS_PROVIDER: &str = "spec36_no_such_provider";

async fn make_agent(pool: &sqlx::PgPool, owner: Option<&str>, provider: &str) -> uuid::Uuid {
    sqlx::query_scalar(
        "INSERT INTO agents (agent_name, agent_type, version, tier, executor_type, model,
                             temperature, author, status, visibility, user_id, llm_provider)
         VALUES ($1, 'research', '1.0.0', 'community', 'llm', 'spec36-model', 0.5, 'spec36',
                 'draft', 'private', $2, $3)
         RETURNING agent_id",
    )
    .bind(support::tag("pulse_agent"))
    .bind(owner)
    .bind(provider)
    .fetch_one(pool)
    .await
    .expect("insert agent")
}

async fn status_of(pool: &sqlx::PgPool, id: uuid::Uuid) -> (String, Option<String>) {
    sqlx::query_as("SELECT execution_status, error_details FROM episodes WHERE episode_id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("episode row")
}

/// Poll until `f` holds or `limit` passes (the drop guard and the workspace
/// path both finish in a spawned task).
async fn eventually<F, Fut>(limit: Duration, mut f: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    while start.elapsed() < limit {
        if f().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    false
}

#[tokio::test]
async fn a_dropped_pulse_fails_its_row_and_abandon_records_the_reason() {
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, |pool, created| async move {
        let agent = make_agent(&pool, None, "anthropic").await;
        created.lock().await.agents.push(agent);
        let store = MemoryStore::from_pool(pool.clone());

        // Dropped without close: the early-return / panic case.
        let dropped = Pulse::open(&store, agent, "spec36 pulse dropped").await;
        let dropped_id = dropped.episode_id;
        created.lock().await.episodes.push(dropped_id);
        assert_eq!(status_of(&pool, dropped_id).await.0, "running");
        drop(dropped);
        let p = pool.clone();
        assert!(
            eventually(Duration::from_secs(10), || {
                let p = p.clone();
                async move { status_of(&p, dropped_id).await.0 == "failure" }
            })
            .await,
            "a dropped armed pulse must fail its row"
        );
        let (_, why) = status_of(&pool, dropped_id).await;
        assert!(why.unwrap_or_default().starts_with("abandoned_unclosed:"));

        // Explicit abandon, awaited.
        let ab = Pulse::open(&store, agent, "spec36 pulse abandoned").await;
        let ab_id = ab.episode_id;
        created.lock().await.episodes.push(ab_id);
        ab.abandon("executor_error: spec36 forced").await;
        assert_eq!(
            status_of(&pool, ab_id).await,
            (
                "failure".into(),
                Some("executor_error: spec36 forced".into())
            )
        );

        // Handed off: still running, owned elsewhere.
        let h = Pulse::open(&store, agent, "spec36 pulse handed off").await;
        let h_id = h.hand_off();
        created.lock().await.episodes.push(h_id);
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(status_of(&pool, h_id).await.0, "running");

        // Abandon never overwrites a finished row.
        let done = uuid::Uuid::new_v4();
        created.lock().await.episodes.push(done);
        sqlx::query(
            "INSERT INTO episodes (episode_id, agent_id, timestamp_ref, query, context,
                                   execution_status, execution_time_ms)
             VALUES ($1, $2, now(), 'spec36 finished', '{}'::jsonb, 'success', 5)",
        )
        .bind(done)
        .bind(agent)
        .execute(&pool)
        .await
        .expect("finished row");
        let changed = store
            .abandon_running(&[done], "abandoned_unclosed: test")
            .await
            .unwrap();
        assert!(changed.is_empty());
        assert_eq!(status_of(&pool, done).await.0, "success");
    })
    .await;
}

/// The reaper's selection: stale `running` rows are found; fresh ones and
/// reservations an open grounding run owns are not. Only this test's rows are
/// abandoned, so it never touches unrelated dev data.
#[tokio::test]
async fn the_reaper_selects_stale_rows_and_spares_open_ground_runs() {
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, |pool, created| async move {
        let owner = support::make_user(&pool, &mut *created.lock().await).await;
        let agent = make_agent(&pool, Some(&owner), "anthropic").await;
        created.lock().await.agents.push(agent);
        let mut ids = Vec::new();
        for age in ["2 hours", "2 hours", "1 minute"] {
            let id = uuid::Uuid::new_v4();
            sqlx::query(&format!(
                "INSERT INTO episodes (episode_id, agent_id, timestamp_ref, query, context,
                                       execution_status, execution_time_ms)
                 VALUES ($1, $2, now() - interval '{age}', 'spec36 reaper', '{{}}'::jsonb,
                         'running', 0)"
            ))
            .bind(id)
            .bind(agent)
            .execute(&pool)
            .await
            .expect("running row");
            created.lock().await.episodes.push(id);
            ids.push(id);
        }
        let (stale, grounded, fresh) = (ids[0], ids[1], ids[2]);
        sqlx::query(
            "INSERT INTO ground_runs (run_id, agent_id, user_id, token_hash, allowed_tools,
                                      query, expires_at)
             VALUES ($1, $2, $3, 'spec36', '{}', 'spec36', now() + interval '1 hour')",
        )
        .bind(grounded)
        .bind(agent)
        .bind(&owner)
        .execute(&pool)
        .await
        .expect("open ground run");

        let store = MemoryStore::from_pool(pool.clone());
        let found = store
            .stale_running_episodes(fermi::episode_reaper::DEFAULT_MAX_RUNTIME)
            .await
            .expect("select");
        assert!(found.contains(&stale), "a 2 h old running row is stale");
        assert!(
            !found.contains(&grounded),
            "an open ground run owns its row"
        );
        assert!(!found.contains(&fresh), "a 1 min old row is not stale");
        let changed = store
            .abandon_running(&[stale], "abandoned_timeout: spec36")
            .await
            .expect("abandon");
        assert_eq!(changed, vec![stale]);

        sqlx::query("DELETE FROM ground_runs WHERE run_id = $1")
            .bind(grounded)
            .execute(&pool)
            .await
            .expect("ground run cleanup");
    })
    .await;
}

/// Source check: every executor call that follows a `Pulse::open` in a
/// handler ends the pulse on its error arm (R0.5.1: "Executor errors MUST
/// call `abandon`"). Paths without an explicit arm are still covered by the
/// drop guard; this pins the explicit ones so they cannot regress to `?`.
#[test]
fn executor_errors_abandon_their_pulse() {
    let root = env!("CARGO_MANIFEST_DIR");
    for (file, n) in [
        ("src/handlers/execution.rs", 1),
        ("src/handlers/execution_stream.rs", 1),
        ("src/handlers/a2a.rs", 2),
        ("src/handlers/eval.rs", 1),
        ("src/handlers/workspace/messages.rs", 1),
        ("src/handlers/workspace/coherence.rs", 1),
        ("src/handlers/rabble_workspace.rs", 1),
        ("src/agent_backend/tools/domains/platform.rs", 2),
    ] {
        let src = std::fs::read_to_string(format!("{root}/{file}")).unwrap();
        let got = src.matches(".abandon(&format!(\"executor_error: ").count();
        assert_eq!(got, n, "{file}: executor-error abandons");
    }
}

async fn funded_owner_with_key(pool: &sqlx::PgPool, c: &mut support::Created) -> (String, String) {
    let owner = support::make_user(pool, c).await;
    support::make_wallet(pool, c, "user", &owner, 50).await;
    let (key, info) =
        fermi_auth::api_keys::create_api_key(pool, &owner, "spec36 claims", &["write".to_string()])
            .await
            .expect("api key");
    c.api_keys.push(info.key_id);
    (owner, key)
}

async fn running_rows(pool: &sqlx::PgPool, agent: uuid::Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM episodes WHERE agent_id = $1 AND execution_status = 'running'",
    )
    .bind(agent)
    .fetch_one(pool)
    .await
    .expect("count")
}

async fn track_episodes(pool: &sqlx::PgPool, c: &mut support::Created, agent: uuid::Uuid) -> i64 {
    let ids: Vec<uuid::Uuid> =
        sqlx::query_scalar("SELECT episode_id FROM episodes WHERE agent_id = $1")
            .bind(agent)
            .fetch_all(pool)
            .await
            .expect("ids");
    let n = ids.len() as i64;
    c.episodes.extend(ids);
    n
}

/// Live, `/api/agents/:id/execute`: an executor error leaves no `running` row.
#[tokio::test]
#[ignore = "needs a deployment: ABW_TEST_BASE_URL + ABW_TEST_DATABASE_URL, run with --ignored"]
async fn deployed_execute_error_leaves_no_running_row() {
    let base = base_url().expect("ABW_TEST_BASE_URL must be set");
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, move |pool, c| async move {
        let (owner, key) = funded_owner_with_key(&pool, &mut *c.lock().await).await;
        let agent = make_agent(&pool, Some(&owner), BOGUS_PROVIDER).await;
        c.lock().await.agents.push(agent);

        let r = reqwest::Client::new()
            .post(format!("{base}/api/agents/{agent}/execute"))
            .bearer_auth(&key)
            .json(&serde_json::json!({ "query": "spec36 pulse probe" }))
            .send()
            .await
            .expect("request");
        let status = r.status().as_u16();
        let body = r.text().await.unwrap_or_default();
        assert!(
            status >= 500,
            "the forced executor error surfaces: {status} {body}"
        );

        let p = pool.clone();
        let closed = eventually(Duration::from_secs(15), || {
            let p = p.clone();
            async move { running_rows(&p, agent).await == 0 }
        })
        .await;
        let total = track_episodes(&pool, &mut *c.lock().await, agent).await;
        assert!(total >= 1, "the run reserved an episode");
        assert!(closed, "no running row may remain after an executor error");
    })
    .await;
}

/// Live, workspace `@mention`: the background execution's executor error
/// leaves no `running` row.
#[tokio::test]
#[ignore = "needs a deployment: ABW_TEST_BASE_URL + ABW_TEST_DATABASE_URL, run with --ignored"]
async fn deployed_workspace_message_error_leaves_no_running_row() {
    let base = base_url().expect("ABW_TEST_BASE_URL must be set");
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, move |pool, c| async move {
        let (owner, key) = funded_owner_with_key(&pool, &mut *c.lock().await).await;
        let agent_name = support::tag("pulse_ws_agent");
        let agent: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO agents (agent_name, agent_type, version, tier, executor_type, model,
                                 temperature, author, status, visibility, user_id, llm_provider)
             VALUES ($1, 'research', '1.0.0', 'community', 'llm', 'spec36-model', 0.5,
                     'spec36', 'draft', 'private', $2, $3)
             RETURNING agent_id",
        )
        .bind(&agent_name)
        .bind(&owner)
        .bind(BOGUS_PROVIDER)
        .fetch_one(&pool)
        .await
        .expect("agent");
        c.lock().await.agents.push(agent);
        let team = fermi_auth::teams::create_team(
            &pool,
            "spec36 pulse ws",
            &support::tag("pulse-ws").replace('_', "-"),
            None,
            &owner,
            "fermi_forecast",
        )
        .await
        .expect("team");
        c.lock().await.teams.push(team.id);
        support::make_wallet(
            &pool,
            &mut *c.lock().await,
            "workspace",
            &team.id.to_string(),
            50,
        )
        .await;
        sqlx::query(
            "INSERT INTO workspace_agents (workspace_id, agent_id, added_by) VALUES ($1, $2, $3)",
        )
        .bind(team.id)
        .bind(agent)
        .bind(&owner)
        .execute(&pool)
        .await
        .expect("add agent to workspace");

        let r = reqwest::Client::new()
            .post(format!("{base}/api/workspaces/{}/messages", team.id))
            .bearer_auth(&key)
            .json(&serde_json::json!({ "content": format!("@{agent_name} spec36 pulse probe") }))
            .send()
            .await
            .expect("request");
        let status = r.status().as_u16();
        let body = r.text().await.unwrap_or_default();
        assert_eq!(status, 200, "message accepted: {body}");

        // The run is spawned: wait for it to reserve, then to end.
        let p = pool.clone();
        let reserved = eventually(Duration::from_secs(30), || {
            let p = p.clone();
            async move {
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM episodes WHERE agent_id = $1")
                    .bind(agent)
                    .fetch_one(&p)
                    .await
                    .unwrap_or(0)
                    > 0
            }
        })
        .await;
        let p = pool.clone();
        let closed = reserved
            && eventually(Duration::from_secs(30), || {
                let p = p.clone();
                async move { running_rows(&p, agent).await == 0 }
            })
            .await;
        track_episodes(&pool, &mut *c.lock().await, agent).await;
        assert!(reserved, "the @mention reserved an episode");
        assert!(closed, "no running row may remain after an executor error");
    })
    .await;
}
