//! LLD G09 / SPEC 36 R4.1 (#68): `tool_verified` is earned by the run record.
//!
//! On the old code a sourced block was stamped `tool_verified` when it had
//! content, whatever the run did [V: `src/grounding_trust.rs:5496`,
//! `:5279`, `:5184` @ 8000415d]. The grounding service's own docs said the
//! opposite, that `tool_verified` means "a tool ABW ran returned this"
//! [V: `src/handlers/ground.rs:17` @ 8000415d]. Now enforcement stamps a
//! populated sourced block `model_inference`, and only
//! `grounding_trust::join_tool_calls` can raise it to `tool_verified`, when
//! every claimed value occurs in a tool output of the same run.

use crate::deployed::base_url;
use crate::support;
use fermi::grounding_trust::{ToolOutput, PROV_INFERRED, PROV_TOOL};

/// A genome_profiler document with a populated sourced block.
const RAW: &str = r#"{
  "taxonomy": {"kingdom":"Animalia","order":"Lepidoptera","family":"Nymphalidae","species":"Apatura iris"},
  "summary": "Apatura iris is a nymphalid."
}"#;

fn taxonomy_stamp(graded: &fermi::episode_boundary::Graded) -> Option<String> {
    graded
        .enforced
        .as_ref()
        .and_then(|d| d.get("taxonomy_provenance"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn grade(calls: &[ToolOutput<'_>]) -> fermi::episode_boundary::Graded {
    fermi::episode_boundary::Pulse::after_the_fact(uuid::Uuid::nil(), "G09 claim fixture").grade(
        "genome_profiler",
        None,
        Some(RAW),
        calls,
    )
}

/// Done-when, in process: no tool call, no `tool_verified`; a tool output
/// that contains every claimed value, `tool_verified`; one that disagrees on a
/// value, `model_inference`. The first assertion fails on the old code.
#[test]
fn tool_verified_requires_the_values_in_a_tool_output() {
    let none = grade(&[]);
    assert_eq!(taxonomy_stamp(&none).as_deref(), Some(PROV_INFERRED));
    assert!(
        none.report.provenance.iter().all(|(_, v)| *v != PROV_TOOL),
        "no block of a run with no tool calls is tool_verified: {:?}",
        none.report.provenance
    );
    assert!(
        none.fields
            .iter()
            .filter(|f| f.block == "taxonomy")
            .all(|f| f.provenance == PROV_INFERRED),
        "graded fields read the joined verdict"
    );

    let gbif = r#"{"kingdom":"Animalia","order":"Lepidoptera","family":"Nymphalidae","canonicalName":"Apatura iris"}"#;
    let joined = grade(&[ToolOutput {
        tool: "gbif_species_search",
        output: gbif,
    }]);
    assert_eq!(taxonomy_stamp(&joined).as_deref(), Some(PROV_TOOL));

    let other = r#"{"kingdom":"Animalia","order":"Lepidoptera","family":"Pieridae","canonicalName":"Apatura iris"}"#;
    let wrong = grade(&[ToolOutput {
        tool: "gbif_species_search",
        output: other,
    }]);
    assert_eq!(taxonomy_stamp(&wrong).as_deref(), Some(PROV_INFERRED));
    let b = wrong
        .report
        .tool_join
        .iter()
        .find(|b| b.block == "taxonomy")
        .expect("taxonomy was joined");
    assert_eq!(b.unmatched, vec!["taxonomy.family".to_string()]);
}

/// Every production enforcement site either joins or is listed here with the
/// reason it has no run record. A new site that does neither fails.
///
/// "Joins" means a `join_tool_calls(` call within the next 25 **code** lines
/// (comments stripped) whose run record is not `&[]` and not a hand-built
/// `ToolOutput { .. }` literal, either of which would satisfy the scan without
/// a record. `grounding_trust.rs` is skipped: it defines enforcement and the
/// join, and its internal calls are the implementation, not sites.
#[test]
fn every_enforcement_site_joins_or_says_why_not() {
    let root = env!("CARGO_MANIFEST_DIR");
    let call = regex::Regex::new(
        r"grounding_trust::(enforce|enforce_from_output_contract)\(|hud_contract::enforce\(",
    )
    .unwrap();
    // An enforcement function imported by name would evade `call`.
    let import = regex::Regex::new(
        r"use [^;]*grounding_trust::\{?[^;]*\b(enforce|enforce_from_output_contract)\b",
    )
    .unwrap();
    let fake_record = regex::Regex::new(r"join_tool_calls\([^;]*(&\[\]|ToolOutput \{)").unwrap();
    // (file, unjoined sites, why there is no run record)
    let unjoined: &[(&str, usize, &str)] = &[
        (
            "src/handlers/creatures/agent_modules.rs",
            1,
            "cached genome profile re-read: the cache row keeps no run record (#149)",
        ),
        (
            "src/handlers/workspace/lens_actions.rs",
            2,
            "documents built from ruleset YAML by platform code; no agent ran (#148)",
        ),
    ];
    let mut found: Vec<(String, usize)> = Vec::new();
    for entry in walkdir(std::path::Path::new(root).join("src")) {
        let rel = entry
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .trim_start_matches('/')
            .to_string();
        if rel == "src/grounding_trust.rs" {
            continue;
        }
        let text = std::fs::read_to_string(&entry).unwrap();
        let body = text.split("#[cfg(test)]").next().unwrap_or("");
        let code: Vec<&str> = body
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .filter(|l| !l.trim().is_empty())
            .collect();
        let joined_code = code.join("\n");
        assert!(
            !import.is_match(&joined_code),
            "{rel}: imports an enforcement function by name, which this scan cannot follow"
        );
        assert!(
            !fake_record.is_match(&joined_code),
            "{rel}: join_tool_calls with an empty or hand-built run record"
        );
        let mut n = 0;
        for (i, l) in code.iter().enumerate() {
            if !call.is_match(l) || l.contains("fn enforce") {
                continue;
            }
            let window = code[i..code.len().min(i + 25)].join("\n");
            if !window.contains("join_tool_calls(") {
                n += 1;
            }
        }
        if n > 0 {
            found.push((rel, n));
        }
    }
    for (file, n) in &found {
        let listed = unjoined.iter().find(|(f, _, _)| f == file);
        assert_eq!(
            listed.map(|(_, k, _)| *k),
            Some(*n),
            "{file}: {n} enforcement site(s) without join_tool_calls. Join them to the \
             run's tool calls, or list them here with the reason there is no run record."
        );
    }
    for (file, k, why) in unjoined {
        assert!(
            found.iter().any(|(f, n)| f == file && n == k),
            "{file} is listed with {k} unjoined site(s) ({why}) but the scan found a \
             different number: update the list"
        );
    }
}

fn walkdir(dir: std::path::PathBuf) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walkdir(p));
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
    out
}

/// Live: the grounding service grades a sourced block submitted with no tool
/// call through the run as `model_inference`. On the old deployment the same
/// submission came back `tool_verified`.
#[tokio::test]
#[ignore = "needs a deployment: ABW_TEST_BASE_URL + ABW_TEST_DATABASE_URL, run with --ignored"]
async fn deployed_ground_output_without_tool_calls_is_not_tool_verified() {
    let base = base_url().expect("ABW_TEST_BASE_URL must be set");
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, move |pool, c| async move {
        let mut cc = c.lock().await;
        let owner = support::make_user(&pool, &mut cc).await;
        support::make_wallet(&pool, &mut cc, "user", &owner, 5).await;
        let contract = serde_json::json!({
            "grounding": {
                "taxonomy": { "status": "sourced", "tool": "gbif_species_search",
                              "response_field": "species" }
            }
        });
        let agent: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO agents (agent_name, agent_type, version, tier, executor_type, model,
                                 temperature, author, status, visibility, user_id,
                                 output_contract)
             VALUES ($1, 'research', '1.0.0', 'community', 'llm', 'spec36-model', 0.5,
                     'spec36', 'draft', 'private', $2, $3)
             RETURNING agent_id",
        )
        .bind(support::tag("g09_agent"))
        .bind(&owner)
        .bind(&contract)
        .fetch_one(&pool)
        .await
        .expect("agent");
        cc.agents.push(agent);
        let (key, info) = fermi_auth::api_keys::create_api_key(
            &pool,
            &owner,
            "spec36 claims",
            &["ground:*".to_string()],
        )
        .await
        .expect("api key");
        cc.api_keys.push(info.key_id);
        drop(cc);

        let http = reqwest::Client::new();
        let opened: serde_json::Value = http
            .post(format!("{base}/v1/ground/runs"))
            .bearer_auth(&key)
            .json(&serde_json::json!({ "agent": agent.to_string(), "query": "spec36 g09 probe" }))
            .send()
            .await
            .expect("open")
            .json()
            .await
            .expect("open json");
        let run_id = opened["run_id"]
            .as_str()
            .unwrap_or_else(|| panic!("open: {opened}"));
        let token = opened["run_token"].as_str().expect("run token");
        assert_eq!(opened["contract"], "declared", "open: {opened}");

        let r = http
            .post(format!("{base}/v1/ground/runs/{run_id}/output"))
            .bearer_auth(token)
            .json(&serde_json::json!({ "document": {
                "taxonomy": { "family": "Nymphalidae", "species": "Apatura iris" }
            }}))
            .send()
            .await
            .expect("submit");
        let status = r.status().as_u16();
        let body: serde_json::Value = r.json().await.unwrap_or_default();

        let eps: Vec<uuid::Uuid> =
            sqlx::query_scalar("SELECT episode_id FROM episodes WHERE agent_id = $1")
                .bind(agent)
                .fetch_all(&pool)
                .await
                .unwrap();
        c.lock().await.episodes.extend(eps.iter().copied());

        assert_eq!(status, 200, "submit: {body}");
        assert_eq!(
            body["document"]["taxonomy_provenance"], PROV_INFERRED,
            "no tool was called through the run: {body}"
        );
        assert!(
            body["grounding"]["provenance"]
                .as_array()
                .is_some_and(|p| p.iter().all(|e| e[1] != PROV_TOOL)),
            "the report agrees with the document: {body}"
        );
    })
    .await;
}
