//! SPEC 36 R0.2 item 4 / I9 (#38): by-ID and bulk episode reads are tenant-scoped.
//!
//! Before: `/api/episodes/:id`, `/lineage`, `/api/episodes/recent` and
//! `/api/workspaces/:id/flow` were on the public router and served episode
//! content to anyone, signed in or not; `/api/agents/:id/eval/runs` served a
//! private agent's evals. Observed on the dev prototype at 4c9bac67 (the
//! deployed tests below).

use crate::deployed::{base_url, caller};
use crate::support;
use fermi::episode_access::{may_read, may_read_agent_evals, may_read_workspace, Reader};

fn reader(user: &str) -> Reader {
    Reader {
        user_id: user.to_string(),
        admin: false,
    }
}

/// The access rule, in a rolled-back transaction.
#[tokio::test]
async fn episode_access_rule() {
    let Some(pool) = support::pool().await else {
        return;
    };
    let mut tx = pool.begin().await.unwrap();
    let mk_user = |id: String| {
        sqlx::query(
            "INSERT INTO users (email, password_hash, password_salt, user_id, auth_provider, display_name)
             VALUES ($1, 'x', 'x', $2, 'legacy', 'spec36')",
        )
        .bind(format!("{id}@{}", support::TEST_EMAIL_DOMAIN))
        .bind(id)
    };
    let owner = support::tag("user");
    let human = support::tag("user");
    let member = support::tag("user");
    let stranger = support::tag("user");
    for u in [&owner, &human, &member, &stranger] {
        mk_user(u.clone()).execute(&mut *tx).await.unwrap();
    }
    let agent: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO agents (agent_name, agent_type, version, tier, executor_type, model, temperature,
                             author, status, visibility, user_id)
         VALUES ($1, 'research', '1.0.0', 'community', 'llm', 'test', 0.5, 'spec36', 'published', 'public', $2)
         RETURNING agent_id",
    )
    .bind(support::tag("agent"))
    .bind(&owner)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let private_agent: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO agents (agent_name, agent_type, version, tier, executor_type, model, temperature,
                             author, status, visibility, user_id)
         VALUES ($1, 'research', '1.0.0', 'community', 'llm', 'test', 0.5, 'spec36', 'draft', 'private', $2)
         RETURNING agent_id",
    )
    .bind(support::tag("agent"))
    .bind(&owner)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let ws = fermi_auth::teams::create_team_tx(
        &mut tx,
        "spec36 ws",
        &support::tag("ws"),
        None,
        &member,
        "spec36",
    )
    .await
    .unwrap();

    let ep = |dyad: Option<String>, ws_id: Option<uuid::Uuid>| {
        sqlx::query_scalar::<_, uuid::Uuid>(
            "INSERT INTO episodes (agent_id, timestamp_ref, query, context, execution_status,
                                   execution_time_ms, dyad_id, workspace_id)
             VALUES ($1, now(), 'spec36 private question', '{}'::jsonb, 'success', 1, $2, $3)
             RETURNING episode_id",
        )
        .bind(agent)
        .bind(dyad)
        .bind(ws_id)
    };
    let talked = ep(Some(format!("dyad:{agent}:{human}")), None)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let in_ws = ep(None, Some(ws.id)).fetch_one(&mut *tx).await.unwrap();
    let bare = ep(None, None).fetch_one(&mut *tx).await.unwrap();

    // Agent owner sees everything of their agent.
    for e in [talked, in_ws, bare] {
        assert!(
            may_read(&mut tx, e, &reader(&owner)).await.unwrap(),
            "owner reads {e}"
        );
    }
    // The human in the dyad sees their conversation, not others'.
    assert!(may_read(&mut tx, talked, &reader(&human)).await.unwrap());
    assert!(!may_read(&mut tx, bare, &reader(&human)).await.unwrap());
    // A workspace member sees the workspace's episodes, not others'.
    assert!(may_read(&mut tx, in_ws, &reader(&member)).await.unwrap());
    assert!(!may_read(&mut tx, talked, &reader(&member)).await.unwrap());
    // A stranger sees none, even though the agent is public.
    for e in [talked, in_ws, bare] {
        assert!(
            !may_read(&mut tx, e, &reader(&stranger)).await.unwrap(),
            "stranger read {e}"
        );
    }
    // Admin sees all; a nonexistent id is false (404), not an error.
    let admin = Reader {
        user_id: stranger.clone(),
        admin: true,
    };
    assert!(may_read(&mut tx, bare, &admin).await.unwrap());
    assert!(!may_read(&mut tx, uuid::Uuid::new_v4(), &admin)
        .await
        .unwrap());

    // Workspace flow: members only.
    assert!(may_read_workspace(&mut tx, ws.id, &reader(&member))
        .await
        .unwrap());
    assert!(!may_read_workspace(&mut tx, ws.id, &reader(&stranger))
        .await
        .unwrap());

    // Evals: public published agent readable signed-out; private only by owner.
    assert!(may_read_agent_evals(&mut tx, agent, None).await.unwrap());
    assert!(!may_read_agent_evals(&mut tx, private_agent, None)
        .await
        .unwrap());
    assert!(
        !may_read_agent_evals(&mut tx, private_agent, Some(&reader(&stranger)))
            .await
            .unwrap()
    );
    assert!(
        may_read_agent_evals(&mut tx, private_agent, Some(&reader(&owner)))
            .await
            .unwrap()
    );
}

async fn status(url: String, key: Option<&str>) -> (u16, String) {
    let mut r = reqwest::Client::new().get(url);
    if let Some(k) = key {
        r = r.bearer_auth(k);
    }
    let resp = r.send().await.expect("request");
    let s = resp.status().as_u16();
    (s, resp.text().await.unwrap_or_default())
}

/// Live routes, signed out and as a stranger. Fails on 4c9bac67 (200s).
#[tokio::test]
#[ignore = "needs a deployment: ABW_TEST_BASE_URL + ABW_TEST_DATABASE_URL, run with --ignored"]
async fn deployed_episode_reads_are_tenant_scoped() {
    let base = base_url().expect("ABW_TEST_BASE_URL must be set");
    let Some(pool) = support::pool().await else {
        return;
    };
    // An existing episode of a private agent, and an existing workspace.
    let private_ep: uuid::Uuid = sqlx::query_scalar(
        "SELECT e.episode_id FROM episodes e JOIN agents a USING (agent_id)
          WHERE a.visibility = 'private' ORDER BY e.created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("dev DB has a private-agent episode");
    let private_agent: uuid::Uuid =
        sqlx::query_scalar("SELECT agent_id FROM episodes WHERE episode_id = $1")
            .bind(private_ep)
            .fetch_one(&pool)
            .await
            .unwrap();
    let ws: uuid::Uuid = sqlx::query_scalar(
        "SELECT t.id FROM teams t WHERE EXISTS (SELECT 1 FROM workspace_messages m WHERE m.workspace_id = t.id)
          ORDER BY t.created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    support::with_cleanup(&pool, move |pool, c| async move {
        let (_stranger, key) = caller(&pool, &mut *c.lock().await, 0).await;
        let urls = [
            format!("{base}/api/episodes/{private_ep}"),
            format!("{base}/api/episodes/{private_ep}/lineage"),
            format!("{base}/api/episodes/{private_ep}/trace"),
            format!("{base}/api/workspaces/{ws}/flow"),
            format!("{base}/api/agents/{private_agent}/eval/runs"),
        ];
        let mut leaks = Vec::new();
        for u in &urls {
            let (anon, _) = status(u.clone(), None).await;
            let (strange, _) = status(u.clone(), Some(&key)).await;
            if anon == 200 || strange == 200 {
                leaks.push(format!("{u}: signed-out {anon}, stranger {strange}"));
            }
        }
        let (anon_recent, _) = status(format!("{base}/api/episodes/recent"), None).await;
        let (_, stranger_recent) = status(format!("{base}/api/episodes/recent"), Some(&key)).await;
        if anon_recent == 200 {
            leaks.push(format!("/api/episodes/recent signed-out {anon_recent}"));
        }
        let n: usize = serde_json::from_str::<serde_json::Value>(&stranger_recent)
            .ok()
            .and_then(|v| v["episodes"].as_array().map(|a| a.len()))
            .unwrap_or(0);
        if n > 0 {
            leaks.push(format!(
                "/api/episodes/recent returned {n} episodes to a stranger"
            ));
        }
        assert!(leaks.is_empty(), "R0.2.4 leaks:\n{}", leaks.join("\n"));
    })
    .await;
}
