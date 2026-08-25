//! One specimen, three tabs.
//!
//! # What this replaces
//!
//! `templates/agent_detail.html` is 6,426 lines and eight tabs — Overview,
//! Activity, Knowledge, Economics, Field Notes, plus owner-only Eval,
//! Intelligence and Manage. An inventory of it found **thirteen metrics
//! rendered in more than one place**, several under different names for the
//! same number:
//!
//! | number | called |
//! |---|---|
//! | executions | "Runs" on the card, "Total" in Performance Statistics |
//! | ontology relationships | "facts" in the cognition panel, "Relationships" in Knowledge |
//! | tools | an identical name→description grid in Overview *and* Field Notes |
//! | Brier | twice on the same tab, from two endpoints |
//!
//! Field Notes is ~100% duplication of the header and Overview.
//!
//! # The duplication was caused by the fetching
//!
//! Those pages compose from a dozen endpoints, so the same quantity arrives
//! under whatever name its producer chose, and a reader cannot tell whether two
//! numbers that disagree are two measurements or one measurement twice. Worse,
//! Performance Statistics mixes **measured** `execution_stats` with
//! **hand-authored** `agent_card.json` values and renders both as `0.0%`, so a
//! measured zero and an absent source are indistinguishable.
//!
//! So this composes one payload server-side. **One producer per number means
//! one name per number**, and where a value cannot be measured it is absent
//! rather than zero.
//!
//! # Three tabs
//!
//! | tab | question | was |
//! |---|---|---|
//! | Profile | what is it? | Overview + Field Notes + the static half of Knowledge |
//! | Record | what has it done? | Activity + Economics + eval history + ontology counts |
//! | Health | is it working? | a link to the Observatory, and now the scoped absence readings |
//!
//! Editing is a **mode**, not a tab: Manage, Intelligence and eval authoring
//! belong in a drawer, because they are a different activity from reading.
//!
//! # Health is where the substrate surfaces
//!
//! [`crate::panel_absence::resolve_for_subject`] has existed since the scoped
//! probes landed and has had no UI. This is it: for this agent, what can the
//! platform say, and what can it not. An empty panel here carries the contract
//! that produced the emptiness and the opportunity count behind it.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use sqlx::Row;

use crate::AppState;

/// `GET /api/specimen/:agent_name`
pub async fn specimen_handler(
    State(state): State<AppState>,
    Path(agent_name): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db = &state.db;

    // ── Profile: what it is ──────────────────────────────────────────────
    let row = sqlx::query(
        "SELECT a.agent_id, a.agent_name,
                COALESCE(a.display_alias, a.agent_name) AS label,
                a.description, a.agent_type, a.tier, a.min_tier,
                a.llm_provider, a.model, a.executor_type, a.temperature,
                a.status, a.visibility, a.tags, a.accepts, a.produces,
                a.taxonomy, a.fork_count, a.forked_from, a.persona_version,
                a.system_prompt, a.sample_queries, a.mcp_tools,
                a.dreaming_budget_credits, a.dreaming_credits_used,
                (a.output_contract IS NOT NULL)             AS declares_contract,
                (a.output_contract -> 'schema' IS NOT NULL) AS typed,
                om.source        AS provenance,
                ev.current_level AS level,
                ev.peak_level    AS peak_level
           FROM agents a
           LEFT JOIN orchestra_members om
                  ON om.agent_id = a.agent_id AND om.orchestra_name = 'fermi'
           LEFT JOIN agent_evolution ev ON ev.agent_id = a.agent_id
          WHERE a.agent_name = $1",
    )
    .bind(&agent_name)
    .fetch_optional(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("specimen: {e}")))?
    .ok_or((StatusCode::NOT_FOUND, format!("no specimen `{agent_name}`")))?;

    let agent_id: uuid::Uuid = row.get("agent_id");

    // ── Record: what it has done ─────────────────────────────────────────
    //
    // Every count from one place, so no two names can disagree. `episodes` is
    // the measured source; the `agents.total_executions` rollup is deliberately
    // NOT read — `rollup_trust` exists because that column was added with the
    // table and never wired, and 3 of 743 rows carry a non-zero value while the
    // episode log recorded every run faithfully.
    let rec = sqlx::query(
        "SELECT (SELECT count(*) FROM episodes WHERE agent_id = $1)                       AS runs,
                (SELECT count(*) FROM episodes
                  WHERE agent_id = $1 AND execution_status = 'success')                   AS succeeded,
                (SELECT count(*) FROM episodes
                  WHERE agent_id = $1 AND execution_status <> 'success')                  AS failed,
                (SELECT sum(cost_usd) FROM episodes WHERE agent_id = $1)                  AS cost_usd,
                (SELECT max(created_at) FROM episodes WHERE agent_id = $1)                AS last_run,
                (SELECT count(*) FROM entities WHERE agent_id = $1)                       AS entities,
                (SELECT count(*) FROM facts WHERE agent_id = $1)                          AS facts,
                (SELECT count(*) FROM semantic_rules WHERE agent_id = $1)                 AS rules,
                (SELECT count(*) FROM semantic_rules
                  WHERE agent_id = $1 AND application_count > 0)                          AS rules_retrieved,
                (SELECT count(*) FROM consolidation_jobs
                  WHERE agent_id = $1 AND status = 'completed')                           AS dream_cycles,
                (SELECT max(created_at) FROM consolidation_jobs
                  WHERE agent_id = $1 AND status = 'completed')                           AS last_dreamt,
                (SELECT count(*) FROM eval_runs WHERE agent_id = $1)                      AS eval_runs,
                (SELECT max(created_at) FROM eval_runs WHERE agent_id = $1)               AS last_eval",
    )
    .bind(agent_id)
    .fetch_one(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("record: {e}")))?;

    let runs: i64 = rec.try_get("runs").unwrap_or(0);
    let cost: Option<f64> = rec.try_get("cost_usd").ok().flatten();

    // ── Recent episodes ──────────────────────────────────────────────────
    let episodes = sqlx::query(
        "SELECT query, execution_status, error_details, cost_usd, created_at
           FROM episodes WHERE agent_id = $1
          ORDER BY created_at DESC LIMIT 15",
    )
    .bind(agent_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    // ── Health: what the platform can and cannot say about THIS agent ────
    let observation = fermi::native_evaluators::Observation {
        writes: fermi::write_accounting::accounts(),
        gates: fermi::gate_trust::accounts(),
        loops: fermi::loop_model::evaluate(db).await,
        liveness: fermi::liveness_trust::latest(),
        gate_ledger: Some(fermi::gate_trust::ledger_status()),
    };

    let mut health = Vec::new();
    for p in fermi::panel_absence::PANELS {
        if p.scope != fermi::panel_absence::Scope::Agent {
            continue;
        }
        let a = fermi::panel_absence::resolve_for_subject(db, p, agent_id, &observation).await;
        let s = fermi::panel_contract::stamp_absence(p, &a, fermi::panel_contract::Density::Scan);
        health.push(json!({
            "panel": p.id,
            "shows": p.shows,
            "reading": s.reading,
            "marker": s.marker,
            "marker_word": s.marker_word,
            "token": s.token,
            "detail": a.detail,
            "answered_by": a.answered_by,
            "remediation": a.remediation,
        }));
    }

    let taxonomy: Option<Value> = row.try_get("taxonomy").ok().flatten();
    let succeeded: i64 = rec.try_get("succeeded").unwrap_or(0);

    Ok(Json(json!({
        "profile": {
            "agent_name": row.get::<String, _>("agent_name"),
            "label": row.get::<String, _>("label"),
            "description": row.try_get::<Option<String>, _>("description").ok().flatten(),
            "agent_type": row.try_get::<Option<String>, _>("agent_type").ok().flatten(),
            "taxonomy": taxonomy,
            "tier": row.try_get::<Option<String>, _>("tier").ok().flatten(),
            "min_tier": row.try_get::<Option<String>, _>("min_tier").ok().flatten(),
            "status": row.try_get::<Option<String>, _>("status").ok().flatten(),
            "visibility": row.try_get::<Option<String>, _>("visibility").ok().flatten(),
            "tags": row.try_get::<Option<Vec<String>>, _>("tags").ok().flatten().unwrap_or_default(),
            "accepts": row.try_get::<Option<Vec<String>>, _>("accepts").ok().flatten().unwrap_or_default(),
            "produces": row.try_get::<Option<Vec<String>>, _>("produces").ok().flatten().unwrap_or_default(),
            "declares_contract": row.try_get::<Option<bool>, _>("declares_contract").ok().flatten().unwrap_or(false),
            "typed": row.try_get::<Option<bool>, _>("typed").ok().flatten().unwrap_or(false),
            "provenance": row.try_get::<Option<String>, _>("provenance").ok().flatten(),
            "level": row.try_get::<Option<i32>, _>("level").ok().flatten(),
            "peak_level": row.try_get::<Option<i32>, _>("peak_level").ok().flatten(),
            "forked_from": row.try_get::<Option<String>, _>("forked_from").ok().flatten(),
            "fork_count": row.try_get::<Option<i32>, _>("fork_count").ok().flatten().unwrap_or(0),
            "sample_queries": row.try_get::<Option<Vec<String>>, _>("sample_queries").ok().flatten().unwrap_or_default(),
            "substrate": {
                "provider": row.try_get::<Option<String>, _>("llm_provider").ok().flatten(),
                "model": row.try_get::<Option<String>, _>("model").ok().flatten(),
                "executor": row.try_get::<Option<String>, _>("executor_type").ok().flatten(),
                "temperature": row.try_get::<Option<f64>, _>("temperature").ok().flatten(),
                "persona_version": row.try_get::<Option<i32>, _>("persona_version").ok().flatten(),
            },
        },
        "record": {
            "runs": runs,
            "succeeded": succeeded,
            "failed": rec.try_get::<Option<i64>, _>("failed").ok().flatten().unwrap_or(0),
            // Absent rather than zero when there is nothing to divide. A success
            // rate of 0% and "never run" are different facts and the old page
            // rendered both as 0.0%.
            "success_rate": if runs > 0 { json!(succeeded as f64 / runs as f64) } else { Value::Null },
            "cost_usd": cost,
            "cost_per_run": match (cost, runs) { (Some(c), r) if r > 0 => json!(c / r as f64), _ => Value::Null },
            "last_run": rec.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_run").ok().flatten().map(|t| t.to_rfc3339()),
            // One name each. The old page called `facts` "Relationships" in one
            // panel and "facts" in another, for the same number.
            "entities": rec.try_get::<Option<i64>, _>("entities").ok().flatten().unwrap_or(0),
            "facts": rec.try_get::<Option<i64>, _>("facts").ok().flatten().unwrap_or(0),
            "rules": rec.try_get::<Option<i64>, _>("rules").ok().flatten().unwrap_or(0),
            "rules_retrieved": rec.try_get::<Option<i64>, _>("rules_retrieved").ok().flatten().unwrap_or(0),
            "dream_cycles": rec.try_get::<Option<i64>, _>("dream_cycles").ok().flatten().unwrap_or(0),
            "last_dreamt": rec.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_dreamt").ok().flatten().map(|t| t.to_rfc3339()),
            "dream_budget": row.try_get::<Option<i32>, _>("dreaming_budget_credits").ok().flatten().unwrap_or(0),
            "dream_used": row.try_get::<Option<i32>, _>("dreaming_credits_used").ok().flatten().unwrap_or(0),
            "eval_runs": rec.try_get::<Option<i64>, _>("eval_runs").ok().flatten().unwrap_or(0),
            "last_eval": rec.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_eval").ok().flatten().map(|t| t.to_rfc3339()),
            "episodes": episodes.iter().map(|e| json!({
                "query": e.try_get::<Option<String>, _>("query").ok().flatten(),
                "status": e.try_get::<Option<String>, _>("execution_status").ok().flatten(),
                "error": e.try_get::<Option<String>, _>("error_details").ok().flatten(),
                "cost_usd": e.try_get::<Option<f64>, _>("cost_usd").ok().flatten(),
                "at": e.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at").ok().flatten().map(|t| t.to_rfc3339()),
            })).collect::<Vec<_>>(),
        },
        "health": health,
    })))
}
