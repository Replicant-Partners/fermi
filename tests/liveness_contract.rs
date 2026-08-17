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

/// Every verification must point at an assertion that exists.
///
/// The price of storing assertions flat in a JSONB array. `episodes.assertions`
/// is fast to read and genuinely immutable, but Postgres cannot put a foreign
/// key on an element inside an array, so `assertion_verifications.assertion_id`
/// is a reference nothing enforces.
///
/// That is a dangling citation, and this codebase already has one: a semantic
/// rule naming three episodes with no rows behind them. Unenforced integrity is
/// acceptable only if it is *checked*, and the check has to ship with the
/// schema rather than after it — which is why this test exists in the same
/// change as migration 205 rather than in the follow-up that would never have
/// been written.
///
/// A dangling verification is worse than a missing one: it asserts that
/// something was verified while pointing at nothing, so the assertion it was
/// supposed to settle stays pending for ever while the queue reports work done.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn every_verification_points_at_an_assertion_that_exists() {
    let pool = pool().await;

    if !column_exists(&pool, "assertion_verifications", "verdict").await {
        println!("  migration 205 not deployed — nothing to check yet.");
        return;
    }

    // Left join the log against the assertion ids actually present in
    // `episodes.assertions`. A row with no match is unresolvable.
    let dangling: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint \
           FROM assertion_verifications v \
          WHERE NOT EXISTS ( \
                SELECT 1 FROM episodes e, jsonb_array_elements(e.assertions) AS a \
                 WHERE e.assertions IS NOT NULL \
                   AND (a ->> 'assertion_id')::uuid = v.assertion_id)",
    )
    .fetch_one(&pool)
    .await
    .expect("dangling probe");

    let total: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM assertion_verifications")
        .fetch_one(&pool)
        .await
        .expect("total");

    println!("  {total} verification(s), {dangling} unresolvable");

    assert_eq!(
        dangling, 0,
        "{dangling} of {total} verification(s) reference an assertion_id that \
         exists in no episode. Flat storage cannot enforce this with a foreign \
         key, which is exactly why it is checked here. A dangling verification \
         is worse than a missing one: the queue reports the work as done while \
         the assertion it should have settled stays pending for ever."
    );
}

/// A human verdict without a citation must be impossible, not merely
/// discouraged.
///
/// Enforced by CHECK in migration 205, asserted here against the live database
/// because a constraint that was declared and never applied is the failure this
/// afternoon found seventeen times over. `human_sourced` scores as high as
/// `tool_verified` precisely because someone else can follow the citation to the
/// same source — so an uncited one would be a one-click path from a guess to a
/// fact, with a person's name attached.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn an_uncited_human_verification_is_rejected_by_the_database() {
    let pool = pool().await;

    if !column_exists(&pool, "assertion_verifications", "verdict").await {
        println!("  migration 205 not deployed — constraint not yet in force.");
        return;
    }

    // Attempted inside a transaction that is always rolled back, so the probe
    // cannot leave a row behind whether it succeeds or fails.
    let mut tx = pool.begin().await.expect("begin");
    let attempt = sqlx::query(
        "INSERT INTO assertion_verifications \
             (assertion_id, episode_id, verdict, actor, actor_kind) \
         SELECT gen_random_uuid(), episode_id, 'human_sourced', 'probe', 'human' \
           FROM episodes LIMIT 1",
    )
    .execute(&mut *tx)
    .await;
    let _ = tx.rollback().await;

    assert!(
        attempt.is_err(),
        "the database accepted a `human_sourced` verification with no citation. \
         That makes the human route the cheapest way to turn a guess into a fact \
         — it scores level with a tool call, and the citation is the only thing \
         that earns the score."
    );
}
