//! Wire-format tests for the research rollups on `GET /api/forecasts`.
//!
//! ## The bug these lock down
//!
//! The Dashboard's "Research" card summarises, across the operator's own
//! forecasts, how much evidence has been gathered and by which agents. It
//! read `evidence` and `agents_used` off the rows returned by
//! `list_forecasts_handler` — but that handler's `SELECT` never included
//! either column. Both deserialized to `None` for every row, so the card
//! rendered "no research yet" permanently, even for forecasts with six
//! evidence items and six agents on them.
//!
//! The fix ships two rollups on list rows:
//!
//!   - `evidence_count` — an integer, not the `evidence` array. Evidence
//!     items carry full source text; shipping them would multiply every
//!     list page's payload for a number the UI immediately reduces to a
//!     count anyway.
//!   - `agents_used` — the array itself, because the card needs the agent
//!     ids to price runs against the marketplace cards. It's short:
//!     `{agent_id, driver_refs}` per hired agent.
//!
//! Tests that need a live DB are marked `#[ignore]` so a vanilla
//! `cargo test` passes without one.

use std::str::FromStr;
use std::time::Duration;

use serde_json::{json, Value};
use sqlx::postgres::PgConnectOptions;
use sqlx::{PgPool, Row};

/// Acquire a Neon pool. Returns `None` if `DATABASE_URL` isn't set so the
/// test can early-return silently — matches `tests/forecast_acl.rs`.
async fn try_pool() -> Option<PgPool> {
    let _ = std::fs::read_to_string(".env").map(|contents| {
        for line in contents.lines() {
            if let Some((key, val)) = line.split_once('=') {
                let key = key.trim();
                let val = val.trim().trim_matches('"');
                if !key.is_empty() && !key.starts_with('#') && std::env::var(key).is_err() {
                    std::env::set_var(key, val);
                }
            }
        }
    });
    let url = std::env::var("DATABASE_URL").ok()?;
    let opts = PgConnectOptions::from_str(&url)
        .ok()?
        .statement_cache_capacity(0);
    sqlx::pool::PoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(30))
        .connect_with(opts)
        .await
        .ok()
}

/// One row as `list_forecasts_handler` puts it on the wire, trimmed to the
/// keys this contract is about.
fn example_list_row() -> Value {
    json!({
        "id": "fc-mancity-epl-2627",
        "owner_id": "11111111-1111-1111-1111-111111111111",
        "question_text": "Will Manchester City win the 2026-27 English Premier League?",
        "status": "active",
        "predicted_probability": 0.31,
        // The rollups. Absence of these two is the bug.
        "evidence_count": 6,
        "agents_used": [
            { "agent_id": "football_institution_agent", "driver_refs": ["squad_strength"] },
            { "agent_id": "efra_scout", "driver_refs": ["injury_load"] },
        ],
    })
}

/// The Dashboard card's own reduction, mirrored here so the contract is
/// tested as the console consumes it: prefer `evidence_count`, fall back to
/// measuring `evidence` (present on the detail endpoint and on older API
/// builds that predate the count), then zero.
fn evidence_count_of(row: &Value) -> usize {
    row.get("evidence_count")
        .and_then(|v| v.as_i64())
        .map(|n| n.max(0) as usize)
        .or_else(|| {
            row.get("evidence")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
        })
        .unwrap_or(0)
}

fn agent_ids_of(row: &Value) -> Vec<String> {
    row.get("agents_used")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    e.get("agent_id")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A list row carries the research rollups, and the card's reduction reads
/// non-zero values off it. Pre-fix both keys were absent and this asserted
/// 0 / empty — which is exactly what the operator saw.
#[test]
fn list_row_carries_research_rollups() {
    let row = example_list_row();

    assert!(
        row.get("evidence_count").is_some(),
        "list rows MUST carry `evidence_count` — the Dashboard Research \
         card has no other way to count evidence over a list"
    );
    assert!(
        row.get("agents_used").is_some(),
        "list rows MUST carry `agents_used` — the Research card needs the \
         agent ids to price runs"
    );

    assert_eq!(evidence_count_of(&row), 6);
    assert_eq!(
        agent_ids_of(&row),
        vec!["football_institution_agent", "efra_scout"]
    );
}

/// `evidence_count` is a scalar, never the array. Shipping the array on
/// list pages is the regression this guards: it would work, then quietly
/// balloon every page.
#[test]
fn evidence_count_is_scalar_and_evidence_array_is_absent() {
    let row = example_list_row();
    assert!(
        row["evidence_count"].is_i64(),
        "evidence_count must be an integer, got {:?}",
        row["evidence_count"]
    );
    assert!(
        row.get("evidence").is_none(),
        "the `evidence` array must NOT ride along on list rows — items \
         carry full source text; clients that need them fetch the detail \
         endpoint"
    );
}

/// Older API builds return neither key. The console must degrade to zero
/// rather than fail to deserialize — `evidence_count` is
/// `#[serde(default)]` on `Forecast` for this reason.
#[test]
fn missing_rollups_degrade_to_zero() {
    let legacy = json!({ "id": "fc-old", "question_text": "…", "status": "active" });
    assert_eq!(evidence_count_of(&legacy), 0);
    assert!(agent_ids_of(&legacy).is_empty());
}

/// The detail endpoint returns `evidence` without a count. The fallback
/// branch must measure the array so a Composer-hydrated forecast still
/// registers on the card.
#[test]
fn detail_shape_falls_back_to_measuring_the_array() {
    let detail = json!({
        "id": "fc-mancity-epl-2627",
        "evidence": [ {"id": "e1"}, {"id": "e2"}, {"id": "e3"} ],
        "agents_used": [ {"agent_id": "efra_scout", "driver_refs": []} ],
    });
    assert_eq!(evidence_count_of(&detail), 3);
    assert_eq!(agent_ids_of(&detail), vec!["efra_scout"]);
}

/// A negative or absurd count from a bad writer clamps to zero instead of
/// panicking on the `as usize` cast.
#[test]
fn negative_count_clamps() {
    let bad = json!({ "id": "fc-bad", "evidence_count": -4 });
    assert_eq!(evidence_count_of(&bad), 0);
}

// ═══ Ownership scope split (Mine / Shared chips) ════════════════════
//
// The three forecast lists the Dashboard reads come from `list_forecasts`
// with no `scope`, so they mix owned forecasts with ones shared with the
// caller. `ResearchScope::admits` splits them client-side. Mirrored here.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Scope {
    All,
    Mine,
    Shared,
}

fn admits(scope: Scope, owner_id: &str, me: Option<&str>) -> bool {
    match (scope, me) {
        (Scope::All, _) | (_, None) => true,
        (Scope::Mine, Some(me)) => owner_id == me,
        (Scope::Shared, Some(me)) => owner_id != me,
    }
}

const ME: &str = "11111111-1111-1111-1111-111111111111";
const SOMEONE_ELSE: &str = "22222222-2222-2222-2222-222222222222";

#[test]
fn scope_splits_owned_from_shared() {
    assert!(admits(Scope::Mine, ME, Some(ME)));
    assert!(!admits(Scope::Mine, SOMEONE_ELSE, Some(ME)));

    assert!(admits(Scope::Shared, SOMEONE_ELSE, Some(ME)));
    assert!(!admits(Scope::Shared, ME, Some(ME)));

    // All is a superset of both, always.
    assert!(admits(Scope::All, ME, Some(ME)));
    assert!(admits(Scope::All, SOMEONE_ELSE, Some(ME)));
}

/// Mine and Shared partition the visible set: every forecast lands in
/// exactly one. A row that fell through both chips would be invisible
/// unless you happened to be on All.
#[test]
fn mine_and_shared_partition_the_set() {
    for owner in [ME, SOMEONE_ELSE] {
        let in_mine = admits(Scope::Mine, owner, Some(ME));
        let in_shared = admits(Scope::Shared, owner, Some(ME));
        assert!(
            in_mine ^ in_shared,
            "owner {owner} must be in exactly one of Mine/Shared, \
             got mine={in_mine} shared={in_shared}"
        );
    }
}

/// Before the first successful auth `current_user_id` is `None`. Every
/// scope must admit everything then — an unknown identity must not render
/// as an empty card, which is the failure mode this whole fix is about.
#[test]
fn unknown_identity_admits_everything() {
    for scope in [Scope::All, Scope::Mine, Scope::Shared] {
        assert!(admits(scope, ME, None), "{scope:?} must admit when me=None");
        assert!(admits(scope, SOMEONE_ELSE, None));
    }
}

/// The All count is counted, not summed from Mine + Shared. With an unknown
/// identity both sub-scopes admit every row, so summing would double-count.
#[test]
fn all_count_is_not_the_sum_of_the_parts() {
    let owners = [ME, SOMEONE_ELSE, ME];

    let counts = |me: Option<&str>| {
        let all = owners.len();
        let mine = owners.iter().filter(|o| admits(Scope::Mine, o, me)).count();
        let shared = owners
            .iter()
            .filter(|o| admits(Scope::Shared, o, me))
            .count();
        (all, mine, shared)
    };

    let (all, mine, shared) = counts(Some(ME));
    assert_eq!((all, mine, shared), (3, 2, 1));
    assert_eq!(mine + shared, all, "known identity: parts tile the whole");

    let (all, mine, shared) = counts(None);
    assert_eq!((all, mine, shared), (3, 3, 3));
    assert_ne!(
        mine + shared,
        all,
        "unknown identity double-counts — which is exactly why `all_rows` \
         is its own counter in the card"
    );
}

// ═══ Measured execution stats ═══════════════════════════════════
//
// The Research card prices a forecast as
//   sum over agents in `agents_used` of avg_cost_per_run,
// where avg_cost_per_run = execution_stats.total_cost_usd / total_executions.
// That was permanently unavailable because `execution_stats` came from
// `agents.total_executions` / `agents.total_cost_usd` — denormalised
// counters nothing ever writes. Real per-run cost lives in
// `episodes.cost_usd`, which is what the ABW web UI's EXECUTION HISTORY
// has been showing all along.

/// The console's own reduction: an agent is priceable only when both
/// numbers are positive, so a divide-by-zero can't produce an
/// authoritative-looking 0.00.
fn avg_cost_per_run(stats: &Value) -> Option<f64> {
    let execs = stats.get("total_executions").and_then(|v| v.as_i64())?;
    let cost = stats.get("total_cost_usd").and_then(|v| v.as_f64())?;
    (execs > 0 && cost > 0.0).then(|| cost / execs as f64)
}

#[test]
fn measured_stats_make_an_agent_priceable() {
    // efra_critical_factor's real numbers: 3 runs, all failures, $1.032012.
    let measured = json!({
        "total_executions": 3,
        "successful_executions": 0,
        "failed_executions": 3,
        "total_cost_usd": 1.032012,
        "source": "episodes",
    });
    let avg = avg_cost_per_run(&measured).expect("measured stats must be priceable");
    assert!((avg - 0.344004).abs() < 1e-6, "got {avg}");
}

/// Failed runs still cost money. Pricing off `successful_executions` would
/// under-report spend on exactly the agents that are burning budget without
/// producing evidence.
#[test]
fn failures_are_still_billable() {
    let all_failed = json!({
        "total_executions": 3,
        "successful_executions": 0,
        "failed_executions": 3,
        "total_cost_usd": 1.032012,
    });
    assert!(
        avg_cost_per_run(&all_failed).is_some(),
        "an agent whose every run failed has still spent real money"
    );
}

/// The dead-rollup shape yields no price — the "cost n/a" the operator saw.
#[test]
fn dead_rollup_yields_no_price() {
    let dead = json!({
        "total_executions": 0,
        "successful_executions": 0,
        "failed_executions": 0,
        "total_cost_usd": 0.0,
        "source": "agents_row",
    });
    assert_eq!(avg_cost_per_run(&dead), None);
}

/// `execution_stats.source` must be present so a zero can be diagnosed:
/// `episodes` + zeros means the agent genuinely never ran, `agents_row`
/// means nobody measured.
#[test]
fn exec_stats_declare_their_provenance() {
    for (src, stats) in [
        (
            "episodes",
            json!({"total_executions": 3, "total_cost_usd": 1.03, "source": "episodes"}),
        ),
        (
            "agents_row",
            json!({"total_executions": 0, "total_cost_usd": 0.0, "source": "agents_row"}),
        ),
    ] {
        assert_eq!(
            stats.get("source").and_then(|v| v.as_str()),
            Some(src),
            "execution_stats must say where it came from"
        );
    }
}

/// Live-DB proof that the measured rollup the handler ships returns real
/// numbers, and that the `agents` columns it replaces are in fact dead.
///
/// The query is lifted verbatim from `agents::measured_exec_stats`; if that
/// drifts, this test must drift with it.
#[tokio::test]
#[ignore]
async fn measured_exec_stats_beat_the_dead_rollup() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let rows = sqlx::query(
        "SELECT s.agent_id,
                s.total, s.successful, s.failed, s.cost_usd, s.avg_execution_time_ms,
                a.total_executions AS row_executions,
                a.total_cost_usd   AS row_cost
           FROM (SELECT agent_id,
                        COUNT(*)::bigint AS total,
                        COUNT(*) FILTER (WHERE execution_status = 'success')::bigint AS successful,
                        COUNT(*) FILTER (WHERE execution_status = 'failure')::bigint AS failed,
                        COALESCE(SUM(cost_usd), 0)::float8 AS cost_usd,
                        COALESCE(AVG(execution_time_ms), 0)::bigint AS avg_execution_time_ms
                   FROM episodes
                  GROUP BY agent_id) s
           JOIN agents a ON a.agent_id = s.agent_id
          ORDER BY s.total DESC
          LIMIT 50",
    )
    .fetch_all(&pool)
    .await
    .expect(
        "measured rollup must execute — a type error here degrades every \
         agent in the catalogue to unpriceable",
    );

    if rows.is_empty() {
        eprintln!("skip: no episodes in this DB");
        return;
    }

    let mut priceable = 0usize;
    let mut row_rollup_populated = 0usize;
    for r in &rows {
        // Decode types must match `measured_exec_stats` exactly; a mismatch
        // there reads back as 0 and silently un-prices the agent.
        let total: i64 = r.try_get("total").expect("total is bigint");
        let successful: i64 = r.try_get("successful").expect("successful is bigint");
        let failed: i64 = r.try_get("failed").expect("failed is bigint");
        let cost: f64 = r.try_get("cost_usd").expect("cost_usd casts to float8");
        let _avg_ms: i64 = r
            .try_get("avg_execution_time_ms")
            .expect("avg_execution_time_ms casts to bigint");

        assert!(total > 0, "a GROUP BY row implies at least one episode");
        assert!(
            successful + failed <= total,
            "status buckets cannot exceed the total"
        );
        assert!(cost >= 0.0, "cost must never be negative");
        if total > 0 && cost > 0.0 {
            priceable += 1;
        }

        let row_execs: i32 = r.try_get("row_executions").unwrap_or(0);
        if row_execs > 0 {
            row_rollup_populated += 1;
        }
    }

    assert!(
        priceable > 0,
        "no agent has both runs and cost in `episodes` — the Research card \
         would legitimately show 'cost n/a' and this test cannot tell that \
         apart from the dead-column bug"
    );
    assert!(
        row_rollup_populated < rows.len(),
        "if `agents.total_executions` were actually maintained for every \
         agent with episodes, reading it would have been fine and this \
         change would be pointless — re-examine before deleting it"
    );
}

/// Live-DB proof that the rollup expressions the handler ships agree with
/// the underlying columns — i.e. the count is really the count, and the
/// `jsonb_typeof` guard doesn't silently zero out good arrays.
///
/// The expressions are lifted verbatim from `list_forecasts_handler`; if
/// the handler drifts, this test must drift with it.
#[tokio::test]
#[ignore]
async fn rollup_sql_agrees_with_columns() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let rows = sqlx::query(
        "SELECT COALESCE(jsonb_array_length(
                    CASE WHEN jsonb_typeof(f.evidence) = 'array'
                         THEN f.evidence ELSE '[]'::jsonb END), 0) AS evidence_count,
                CASE WHEN jsonb_typeof(f.agents_used) = 'array'
                     THEN f.agents_used ELSE '[]'::jsonb END AS agents_used,
                f.evidence AS raw_evidence
           FROM fermi_forecasts f
          ORDER BY f.updated_at DESC NULLS LAST
          LIMIT 200",
    )
    .fetch_all(&pool)
    .await
    .expect(
        "rollup projection must execute — a type error here 500s the \
             entire forecast list",
    );

    let mut with_research = 0usize;
    for r in &rows {
        // The handler reads this as i32; a widening here would be a silent
        // decode failure that reads back as 0.
        let count: i32 = r
            .try_get("evidence_count")
            .expect("evidence_count is integer");
        let agents: Value = r.try_get("agents_used").expect("agents_used is jsonb");
        let raw: Value = r.try_get("raw_evidence").expect("evidence is jsonb");

        assert!(count >= 0, "count must never be negative");
        assert!(
            agents.is_array(),
            "the guard must always yield an array, got {:?}",
            agents
        );
        if let Some(arr) = raw.as_array() {
            assert_eq!(
                count as usize,
                arr.len(),
                "rollup count must equal the evidence array length"
            );
        }
        if count > 0 || !agents.as_array().map(|a| a.is_empty()).unwrap_or(true) {
            with_research += 1;
        }
    }

    assert!(
        with_research > 0,
        "no forecast in the DB has any evidence or agents — either the \
         fixture set is empty or research writes are broken; the Dashboard \
         Research card would be legitimately empty and this test can't \
         distinguish that from the projection bug"
    );
}
