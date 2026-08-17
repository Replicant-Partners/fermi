//! Live tier for [`fermi::liveness_trust`]: does the write path ever run?
//!
//! Read-only. Every query is a bare SELECT and the offline tier asserts that at
//! the unit level.
//!
//! Run with:
//! ```text
//! cargo test --test liveness_contract -- --ignored --nocapture
//! ```
//!
//! # What a failure means
//!
//! * **SILENT** — opportunities exist and the sink is empty. The signal that
//!   feature depends on does not exist. Either the writer is broken or it is
//!   not deployed; the consequence is identical, which is why the status does
//!   not guess.
//! * **INERT** — no opportunities yet. Not a pass. Counted separately so a
//!   suite of checks that has proven nothing cannot present itself as green.
//! * **UNRUNNABLE** — a query errored. Never a pass: an unrunnable check
//!   reports healthy for ever, which is the `fermi_leaderboard` matview failure
//!   that went unnoticed for eight releases.

use fermi::liveness_trust::{
    classify, known_silent, Expectation, LivenessContract, Status, LIVENESS_CONTRACTS,
};
use sqlx::{PgPool, Row};

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect")
}

async fn column_exists(pool: &PgPool, table: &str, column: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM information_schema.columns \
          WHERE table_name = $1 AND column_name = $2",
    )
    .bind(table)
    .bind(column)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
        > 0
}

/// Run one contract. Returns the status and the two counts for the report.
async fn evaluate(pool: &PgPool, c: &LivenessContract) -> (Status, i64, i64) {
    if let Some((table, column)) = c.requires {
        if !column_exists(pool, table, column).await {
            return (Status::NotDeployed, -1, -1);
        }
    }

    let read = |sql: &'static str, col: &'static str| async move {
        sqlx::query(sql)
            .fetch_one(pool)
            .await
            .ok()
            .and_then(|r| r.try_get::<i64, _>(col).ok())
    };

    let writes = read(c.sink_sql, "writes").await;
    let opportunities = read(c.opportunity_sql, "opportunities").await;

    match (writes, opportunities) {
        (Some(w), Some(o)) => (classify(w, o), w, o),
        _ => (Status::Unrunnable, -1, -1),
    }
}

#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn every_declared_write_path_has_run_at_least_once() {
    let pool = pool().await;

    let mut silent: Vec<String> = Vec::new();
    let mut unrunnable: Vec<String> = Vec::new();
    let mut inert = 0usize;
    let mut ok = 0usize;
    let mut excused: Vec<&str> = Vec::new();

    println!(
        "\n  {:<38} {:>8} {:>8}  {}",
        "sink", "writes", "opps", "status"
    );
    println!("  {}", "-".repeat(72));

    for c in LIVENESS_CONTRACTS {
        let (status, w, o) = evaluate(&pool, c).await;
        let label = match status {
            Status::Ok => "OK",
            Status::Silent => "SILENT",
            Status::Inert => "INERT",
            Status::NotDeployed => "NOT DEPLOYED",
            Status::Unrunnable => "UNRUNNABLE",
        };
        println!("  {:<38} {:>8} {:>8}  {}", c.sink, w, o, label);

        match status {
            Status::Ok => ok += 1,
            Status::Inert | Status::NotDeployed => inert += 1,
            Status::Unrunnable => unrunnable.push(format!(
                "{}: a query could not run. Not a pass — an unrunnable check \
                 reports healthy for ever.",
                c.sink
            )),
            Status::Silent => {
                // Conditional writers are reported, never asserted: an anomaly
                // detector that finds nothing may simply be correct, and
                // asserting on its row count would be asserting that anomalies
                // must exist.
                if c.expectation == Expectation::Conditional {
                    println!(
                        "        conditional — reported, not asserted. {}",
                        c.remediation
                    );
                } else if let Some(why) = known_silent(c.sink) {
                    excused.push(c.sink);
                    println!("        known-silent: {why}");
                } else {
                    silent.push(format!(
                        "{}\n         writer: {}\n         lost:   {}\n         next:   {}",
                        c.sink, c.writer, c.why, c.remediation
                    ));
                }
            }
        }
    }

    println!(
        "\n  {ok} live, {inert} inert, {} excused, {} silent, {} unrunnable",
        excused.len(),
        silent.len(),
        unrunnable.len()
    );

    // An entry that is no longer silent must be removed, or the list rots into
    // a set of permanent excuses nobody re-reads. Same discipline as the
    // grounding cross-check exemptions.
    for (sink, _) in fermi::liveness_trust::KNOWN_SILENT {
        assert!(
            excused.contains(sink),
            "`{sink}` is listed in KNOWN_SILENT but is no longer silent. Remove \
             the entry — the list may only shrink, and a stale exemption is a \
             standing permission that was never re-examined."
        );
    }

    assert!(
        unrunnable.is_empty(),
        "\n{} contract(s) could not be evaluated:\n  {}\n",
        unrunnable.len(),
        unrunnable.join("\n  ")
    );

    assert!(
        silent.is_empty(),
        "\n{} declared write path(s) have never run:\n  {}\n\n\
         Each of these is a feature whose data does not exist. The code is \
         present and usually looks correct — that is the point of this tier.\n",
        silent.len(),
        silent.join("\n  ")
    );

    // A suite in which nothing has had the chance to fire is not a passing
    // suite, and must not be able to look like one.
    assert!(
        ok > 0,
        "every contract is inert or excused: {inert} inert, {} excused. Nothing \
         has been demonstrated to work, and this assertion is the only thing \
         standing between that state and a green tick.",
        excused.len()
    );
}

/// What the platform throws away, reported rather than asserted.
///
/// The `forecast_agent_claims` contract deliberately counts only
/// workspace-bound opportunities, because the hook cannot fire without a
/// workspace and a permanently-red check is one people learn to scroll past.
/// But the discarded output is real and needs somewhere to be visible, so it
/// lives here: the agent produced a quantified judgement and the platform had
/// nowhere to put it.
///
/// This is the demand signal for the assertion layer — assertion recorded per
/// episode, claim being an assertion bound to a driver — and it is the reason
/// an agent evaluated outside a workspace can currently never accumulate a
/// track record.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn report_quantified_output_the_platform_discards() {
    let pool = pool().await;

    let rows = sqlx::query(
        "SELECT a.agent_name, \
                count(*)::bigint AS total, \
                count(*) FILTER (WHERE e.context ->> 'workspace_id' IS NULL)::bigint AS unbound, \
                count(*) FILTER (WHERE e.response_text ~ 'Suggested p50:\\s+[0-9.]+\\s*\\(p5:\\s+[0-9.]+,\\s+p95:\\s+[0-9.]+\\)')::bigint AS parseable \
           FROM episodes e JOIN agents a ON a.agent_id = e.agent_id \
          WHERE e.response_text ~ 'Suggested p50' \
          GROUP BY 1 ORDER BY 2 DESC",
    )
    .fetch_all(&pool)
    .await
    .expect("census");

    println!("\n  agents that quantified a judgement:");
    println!(
        "  {:<34} {:>7} {:>9} {:>10}",
        "agent", "lines", "unbound", "parseable"
    );
    let mut total = 0i64;
    let mut unbound = 0i64;
    let mut parseable = 0i64;
    for r in &rows {
        let name: String = r.get("agent_name");
        let t: i64 = r.get("total");
        let u: i64 = r.get("unbound");
        let p: i64 = r.get("parseable");
        total += t;
        unbound += u;
        parseable += p;
        println!("  {name:<34} {t:>7} {u:>9} {p:>10}");
    }

    println!(
        "\n  {total} quantified judgement(s); {unbound} produced outside any \
         workspace and therefore discarded; {parseable} match the regex the \
         card calls MANDATORY and machine-parsed."
    );
    if total > 0 {
        println!(
            "  format loss: {} of {total} lost to markdown emphasis around the \
             number.",
            total - parseable
        );
    }
}
