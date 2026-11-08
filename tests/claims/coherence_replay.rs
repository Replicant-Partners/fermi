//! LLD G10: the second reviewer replays the first gate outcome.
//!
//! On the old code the consensus handler re-ran the coherence gate with an
//! empty `WorldModel` [V: `src/handlers/observatory.rs:751` @ 6052df93,
//! `agent-bestiary/coherence-gate/src/gate.rs:190-192`], so the outcome stored
//! with every agent-wide correction was `undetermined` whatever the first
//! review had found, and the first Γ was lost. Now the first outcome is
//! persisted on the request and replayed.

use crate::deployed::base_url;
use crate::support;

/// The peripheral-belief world of `agent-bestiary/coherence-gate/src/tests.rs`
/// (`base_nodes`, target `e3`), which the gate approves at the production
/// threshold.
const OBSERVATIONS: [&str; 4] = [
    "observed: GBIF returned Antaxius beieri, family Tettigoniidae",
    "observed: occurrence records cluster in alpine meadow",
    "observed: no genome assembly exists for this species",
    "observed: a single unrelated sighting near a car park",
];

/// The consensus handler never runs the gate: it replays the stored outcome.
/// And the gate crate has no entry point that settles without a world model,
/// so a re-run against nothing cannot be reintroduced by a one-line edit.
#[test]
fn consensus_replays_and_never_reruns_the_gate() {
    let root = env!("CARGO_MANIFEST_DIR");
    let src = std::fs::read_to_string(format!("{root}/src/handlers/observatory.rs")).unwrap();
    let start = src
        .find("pub async fn confirm_two_reviewer_handler")
        .expect("consensus handler");
    let end = src[start..]
        .find("\n}\n")
        .map(|e| start + e)
        .expect("end of handler");
    let body: String = src[start..end]
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !body.contains("check_against(") && !body.contains(".check("),
        "the consensus handler re-runs the coherence gate"
    );
    assert!(
        body.contains("first_gate_outcome"),
        "the consensus handler must replay the first review's outcome"
    );

    let gate = std::fs::read_to_string(format!("{root}/agent-bestiary/coherence-gate/src/gate.rs"))
        .unwrap();
    assert!(
        !gate.contains("pub fn check(&self"),
        "CoherenceGate::check settles against an empty WorldModel; it must not exist"
    );
}

/// Live: an agent-wide correction approved by the first reviewer carries that
/// same outcome (verdict and Γ) through the second reviewer's confirmation.
#[tokio::test]
#[ignore = "needs a deployment: ABW_TEST_BASE_URL + ABW_TEST_DATABASE_URL, run with --ignored"]
async fn deployed_consensus_replays_the_first_gate_outcome() {
    let base = base_url().expect("ABW_TEST_BASE_URL must be set");
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, move |pool, c| async move {
        let mut cc = c.lock().await;
        let owner = support::make_user(&pool, &mut cc).await;
        let second = support::make_user(&pool, &mut cc).await;
        let agent: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO agents (agent_name, agent_type, version, tier, executor_type, model,
                                 temperature, author, status, visibility, user_id)
             VALUES ($1, 'research', '1.0.0', 'community', 'llm', 'spec36-model', 0.5,
                     'spec36', 'draft', 'private', $2)
             RETURNING agent_id",
        )
        .bind(support::tag("g10_agent"))
        .bind(&owner)
        .fetch_one(&pool)
        .await
        .expect("agent");
        cc.agents.push(agent);
        let mut eps = Vec::new();
        for (i, text) in OBSERVATIONS.iter().enumerate() {
            let id: uuid::Uuid = sqlx::query_scalar(
                "INSERT INTO episodes (agent_id, timestamp_ref, query, context, execution_status,
                                       execution_time_ms, response_text, created_at)
                 VALUES ($1, now(), 'spec36 g10 observation', '{}'::jsonb, 'success', 1, $2,
                         now() - make_interval(secs => $3))
                 RETURNING episode_id",
            )
            .bind(agent)
            .bind(*text)
            .bind(10.0 - i as f64)
            .fetch_one(&pool)
            .await
            .expect("episode");
            eps.push(id);
        }
        cc.episodes.extend(eps.iter().copied());
        for (text, from) in [
            (
                "bush-crickets in this genus are alpine",
                vec![eps[0], eps[1]],
            ),
            (
                "identify Tettigoniidae before describing morphology",
                vec![eps[0]],
            ),
            (
                "do not assert genome size without an assembly",
                vec![eps[0], eps[2]],
            ),
        ] {
            sqlx::query(
                "INSERT INTO semantic_rules (agent_id, rule_content, confidence_score,
                                             episode_count, source_episode_cluster)
                 VALUES ($1, $2, 0.8, $3, $4)",
            )
            .bind(agent)
            .bind(text)
            .bind(from.len() as i32)
            .bind(&from)
            .execute(&pool)
            .await
            .expect("rule");
        }
        let event: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO anomaly_events (agent_id, episode_id, kind)
             VALUES ($1, $2, 'contradicted') RETURNING event_id",
        )
        .bind(agent)
        .bind(eps[3])
        .fetch_one(&pool)
        .await
        .expect("anomaly");
        let (owner_key, i1) =
            fermi_auth::api_keys::create_api_key(&pool, &owner, "spec36 claims", &["write".into()])
                .await
                .expect("owner key");
        // The second reviewer must be the owner or an admin, and not the first.
        let (admin_key, i2) = fermi_auth::api_keys::create_api_key(
            &pool,
            &second,
            "spec36 claims",
            &["admin".into()],
        )
        .await
        .expect("admin key");
        cc.api_keys.extend([i1.key_id, i2.key_id]);
        drop(cc);

        let http = reqwest::Client::new();
        let first: serde_json::Value = http
            .post(format!("{base}/api/observatory/hitl/{event}/action"))
            .bearer_auth(&owner_key)
            .json(&serde_json::json!({
                "action": "intervene",
                "scope": "agent_wide",
                "classification": "belief",
                "correction_text": "that car park sighting was a misidentification",
                "justification": "spec36 g10 probe",
            }))
            .send()
            .await
            .expect("first review")
            .json()
            .await
            .expect("first review json");
        assert_eq!(
            first["status"], "awaiting_second_reviewer",
            "first: {first}"
        );
        assert_eq!(
            first["gate"]["verdict"], "approved",
            "the fixture world must be one the gate approves: {first}"
        );
        let request_id = first["request_id"].as_str().expect("request id");

        let r = http
            .post(format!(
                "{base}/api/observatory/hitl/consensus/{request_id}"
            ))
            .bearer_auth(&admin_key)
            .json(&serde_json::json!({ "approved": true }))
            .send()
            .await
            .expect("consensus");
        let status = r.status().as_u16();
        let second_body: serde_json::Value = r.json().await.unwrap_or_default();

        let synthetic: Vec<uuid::Uuid> =
            sqlx::query_scalar("SELECT episode_id FROM episodes WHERE agent_id = $1")
                .bind(agent)
                .fetch_all(&pool)
                .await
                .unwrap();
        c.lock().await.episodes.extend(synthetic);

        assert_eq!(status, 200, "consensus: {second_body}");
        assert_eq!(
            second_body["gate"], first["gate"],
            "the confirmation must carry the first review's gate outcome, not a re-run"
        );
        let stored: serde_json::Value = sqlx::query_scalar(
            "SELECT coherence_check FROM episode_corrections WHERE correction_id = $1",
        )
        .bind(
            second_body["correction_id"]
                .as_str()
                .and_then(|s| s.parse::<uuid::Uuid>().ok())
                .expect("correction id"),
        )
        .fetch_one(&pool)
        .await
        .expect("stored correction");
        assert_eq!(stored["verdict"], "approved", "persisted outcome: {stored}");
        assert_eq!(stored["gamma"], first["gate"]["gamma"]);
    })
    .await;
}
