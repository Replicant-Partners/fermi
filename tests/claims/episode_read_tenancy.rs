//! SPEC 36 R0.2 item 4 / I9 (#38): by-ID and bulk episode reads are tenant-scoped.
//!
//! Before (deployed prototype, 4c9bac67) the deployed test below recorded 14
//! exposures: by-ID episode, lineage, flow, private-agent eval runs and test
//! cases, and private specimens answered signed-out callers; `/trace` answered
//! any signed-in user; `/api/episodes/recent`, `/api/stream`, agent metrics'
//! failures and the verification queue returned other users' episodes.

use crate::deployed::{base_url, caller};
use crate::support;
use fermi::episode_access::{may_read, may_read_agent_evals, may_read_workspace, Reader};

fn reader(user: &str) -> Reader {
    Reader {
        user_id: user.to_string(),
        admin: false,
    }
}

/// The access rule, every branch, in a rolled-back transaction.
#[tokio::test]
async fn episode_access_rule() {
    let Some(pool) = support::pool().await else {
        return;
    };
    let mut tx = pool.begin().await.unwrap();
    let owner = support::tag("user");
    let human = support::tag("user");
    let member = support::tag("user");
    let stranger = support::tag("user");
    for u in [&owner, &human, &member, &stranger] {
        sqlx::query(
            "INSERT INTO users (email, password_hash, password_salt, user_id, auth_provider, display_name)
             VALUES ($1, 'x', 'x', $2, 'legacy', 'spec36')",
        )
        .bind(format!("{u}@{}", support::TEST_EMAIL_DOMAIN))
        .bind(u)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    let new_agent = |status: &'static str, visibility: &'static str| {
        sqlx::query_scalar::<_, uuid::Uuid>(
            "INSERT INTO agents (agent_name, agent_type, version, tier, executor_type, model, temperature,
                                 author, status, visibility, user_id)
             VALUES ($1, 'research', '1.0.0', 'community', 'llm', 'test', 0.5, 'spec36', $2, $3, $4)
             RETURNING agent_id",
        )
        .bind(support::tag("agent"))
        .bind(status)
        .bind(visibility)
        .bind(owner.clone())
    };
    let agent = new_agent("published", "public")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let private_agent = new_agent("draft", "private")
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

    let ep = |dyad: Option<String>, ws_id: Option<uuid::Uuid>, src: Option<serde_json::Value>| {
        sqlx::query_scalar::<_, uuid::Uuid>(
            "INSERT INTO episodes (agent_id, timestamp_ref, query, context, execution_status,
                                   execution_time_ms, dyad_id, workspace_id, source_ref)
             VALUES ($1, now(), 'spec36 private question', '{}'::jsonb, 'success', 1, $2, $3, $4)
             RETURNING episode_id",
        )
        .bind(agent)
        .bind(dyad)
        .bind(ws_id)
        .bind(src)
    };
    let talked = ep(Some(format!("dyad:{agent}:{human}")), None, None)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let in_ws = ep(None, Some(ws.id), None)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let bare = ep(None, None, None).fetch_one(&mut *tx).await.unwrap();
    let a2a = ep(
        None,
        None,
        Some(serde_json::json!({"kind": "a2a_handler", "caller_user_id": human})),
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let src_ws = ep(
        None,
        None,
        Some(serde_json::json!({"kind": "x", "workspace_id": ws.id.to_string()})),
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let eval = ep(Some(format!("eval:{agent}:{human}")), None, None)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let bad_ws = ep(
        None,
        None,
        Some(serde_json::json!({"kind": "x", "workspace_id": "wild"})),
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let all = [talked, in_ws, bare, a2a, src_ws, eval, bad_ws];

    for e in all {
        assert!(
            may_read(&mut tx, e, &reader(&owner)).await.unwrap(),
            "agent owner reads {e}"
        );
        assert!(
            !may_read(&mut tx, e, &reader(&stranger)).await.unwrap(),
            "stranger read {e}"
        );
        let nobody = Reader {
            user_id: String::new(),
            admin: false,
        };
        assert!(
            !may_read(&mut tx, e, &nobody).await.unwrap(),
            "signed-out reader read {e}"
        );
    }
    assert!(
        may_read(&mut tx, talked, &reader(&human)).await.unwrap(),
        "dyad human"
    );
    assert!(
        may_read(&mut tx, eval, &reader(&human)).await.unwrap(),
        "eval dyad human"
    );
    assert!(
        may_read(&mut tx, a2a, &reader(&human)).await.unwrap(),
        "A2A caller"
    );
    assert!(
        !may_read(&mut tx, bare, &reader(&human)).await.unwrap(),
        "not the human's"
    );
    assert!(
        may_read(&mut tx, in_ws, &reader(&member)).await.unwrap(),
        "workspace member"
    );
    assert!(
        may_read(&mut tx, src_ws, &reader(&member)).await.unwrap(),
        "workspace via source_ref"
    );
    assert!(
        !may_read(&mut tx, bad_ws, &reader(&member)).await.unwrap(),
        "non-uuid workspace id: no error, no match"
    );
    assert!(
        !may_read(&mut tx, talked, &reader(&member)).await.unwrap(),
        "member: not the workspace's"
    );
    let admin = Reader {
        user_id: stranger.clone(),
        admin: true,
    };
    assert!(may_read(&mut tx, bare, &admin).await.unwrap(), "admin");
    assert!(
        !may_read(&mut tx, uuid::Uuid::new_v4(), &admin)
            .await
            .unwrap(),
        "missing id is false"
    );

    assert!(may_read_workspace(&mut tx, ws.id, &reader(&member))
        .await
        .unwrap());
    assert!(!may_read_workspace(&mut tx, ws.id, &reader(&stranger))
        .await
        .unwrap());

    assert!(
        may_read_agent_evals(&mut tx, agent, None).await.unwrap(),
        "public published agent: catalogue"
    );
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

/// Every handler that serves episode content by id or in bulk carries the
/// guard. A source check, because the handlers live in the api-server binary;
/// deleting any guard fails this test.
#[test]
fn every_episode_reading_handler_is_guarded() {
    let read = |f: &str| {
        std::fs::read_to_string(format!("{}/src/handlers/{f}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    };
    let body = |src: &str, name: &str| -> String {
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
    let checks: &[(&str, &str, &str)] = &[
        (
            "metrics.rs",
            "get_episode_detail_handler",
            "require_readable(",
        ),
        ("metrics.rs", "agent_metrics_handler", "predicate("),
        ("loops.rs", "episode_trace_handler", "require_readable("),
        ("loops.rs", "probe_field_handler", "require_readable("),
        ("loops.rs", "contradict_field_handler", "require_readable("),
        ("loops.rs", "agent_coordination_notes_handler", "predicate("),
        ("loops.rs", "verification_queue_handler", "predicate("),
        (
            "specimen.rs",
            "episode_lineage_handler",
            "require_readable(",
        ),
        ("specimen.rs", "recent_episodes_handler", "predicate("),
        ("specimen.rs", "stream_handler", "predicate("),
        ("specimen.rs", "specimen_handler", "predicate("),
        (
            "specimen.rs",
            "workspace_flow_handler",
            "may_read_workspace(",
        ),
        (
            "consolidation.rs",
            "get_agent_episodes_handler",
            "predicate(",
        ),
        ("eval.rs", "list_eval_runs_handler", "require_agent_evals("),
        (
            "eval.rs",
            "list_eval_signals_handler",
            "require_agent_evals(",
        ),
        (
            "eval.rs",
            "list_eval_test_cases_handler",
            "require_agent_evals(",
        ),
    ];
    for (file, handler, guard) in checks {
        assert!(
            body(&read(file), handler).contains(guard),
            "{file}::{handler} must call {guard}..)"
        );
    }
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

/// Live routes, signed out and as a stranger with no history.
#[tokio::test]
#[ignore = "needs a deployment: ABW_TEST_BASE_URL + ABW_TEST_DATABASE_URL, run with --ignored"]
async fn deployed_episode_reads_are_tenant_scoped() {
    let base = base_url().expect("ABW_TEST_BASE_URL must be set");
    let Some(pool) = support::pool().await else {
        return;
    };
    // A finished episode of a private agent (a `running` row 502s on old code
    // through the G05 reader panic, which proves nothing either way).
    let (private_ep, private_agent, private_name): (uuid::Uuid, uuid::Uuid, String) = sqlx::query_as(
        "SELECT e.episode_id, a.agent_id, a.agent_name FROM episodes e JOIN agents a USING (agent_id)
          WHERE a.visibility = 'private' AND e.execution_status <> 'running'
          ORDER BY e.created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("dev DB has a finished private-agent episode");
    let ws: uuid::Uuid = sqlx::query_scalar(
        "SELECT t.id FROM teams t WHERE EXISTS (SELECT 1 FROM workspace_messages m WHERE m.workspace_id = t.id)
          ORDER BY t.created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    support::with_cleanup(&pool, move |pool, c| async move {
        let (_stranger, key) = caller(&pool, &mut *c.lock().await, 0).await;
        let mut leaks = Vec::new();
        for path in [
            format!("/api/episodes/{private_ep}"),
            format!("/api/episodes/{private_ep}/lineage"),
            format!("/api/episodes/{private_ep}/trace"),
            format!("/api/workspaces/{ws}/flow"),
            format!("/api/agents/{private_agent}/eval/runs"),
            format!("/api/agents/{private_agent}/eval/test-cases"),
            format!("/api/specimen/{private_name}"),
        ] {
            let (anon, _) = status(format!("{base}{path}"), None).await;
            let (strange, _) = status(format!("{base}{path}"), Some(&key)).await;
            if anon == 200 || strange == 200 {
                leaks.push(format!("{path}: signed-out {anon}, stranger {strange}"));
            }
        }
        for (path, field) in [
            ("/api/episodes/recent".to_string(), "episodes"),
            ("/api/stream".to_string(), "exchanges"),
            (format!("/api/agents/{private_agent}/episodes"), "episodes"),
            ("/api/verification-queue".to_string(), "items"),
            (
                format!("/api/agents/{private_agent}/metrics"),
                "recent_failures",
            ),
        ] {
            let (anon, anon_body) = status(format!("{base}{path}"), None).await;
            let (_, body) = status(format!("{base}{path}"), Some(&key)).await;
            let count = |b: &str| {
                serde_json::from_str::<serde_json::Value>(b)
                    .ok()
                    .and_then(|v| v[field].as_array().map(|a| a.len()))
                    .unwrap_or(0)
            };
            if anon == 200 && count(&anon_body) > 0 {
                leaks.push(format!(
                    "{path}: signed-out got {} {field}",
                    count(&anon_body)
                ));
            }
            if count(&body) > 0 {
                leaks.push(format!("{path}: stranger got {} {field}", count(&body)));
            }
        }
        assert!(leaks.is_empty(), "R0.2.4 leaks:\n{}", leaks.join("\n"));
    })
    .await;
}
