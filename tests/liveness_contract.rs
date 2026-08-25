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
    column_exists, evaluate_one, known_silent, Expectation, Status, LIVENESS_CONTRACTS,
};
use sqlx::{PgPool, Row};

// The runner used to live here, and only here — which is why the only way to
// learn whether a write path had ever run was for a human to invoke this file.
// It now lives in `fermi::liveness_trust` so the scheduled sweeper, the admin
// endpoint and this test all execute the same arithmetic. Two implementations
// of one trust calculation eventually disagree, and the one that gets believed
// is the one nearest the writer.

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect")
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
        let (status, w, o) = evaluate_one(&pool, c).await;
        let label = status.label();
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
                // The rule lives in `is_actionable_silence`, not here. This
                // file used to apply its own copy of it, and the library
                // applied a different one — so the script said `0 silent` and
                // the library's report said `anomaly_events` was silent with no
                // excuse. §3.4: one implementation, owned by the layer that owns
                // the vocabulary.
                if c.expectation == Expectation::Conditional {
                    println!(
                        "        conditional — reported, not asserted. {}",
                        c.remediation
                    );
                    debug_assert!(!fermi::liveness_trust::is_actionable_silence(c));
                } else if let Some(why) = known_silent(c.sink) {
                    excused.push(c.sink);
                    println!("        known-silent: {why}");
                    debug_assert!(!fermi::liveness_trust::is_actionable_silence(c));
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

/// Where Loop 5.A's projection-accuracy signal path actually stops, reported
/// rather than asserted.
///
/// The chain is three links and every one of them is now honestly `INERT`,
/// which is a true statement and a quiet one. It replaces a single contract
/// that read **0 writes / 12,167 opportunities** and was loud about the wrong
/// thing: all 12,167 of those rows are the projections themselves, so the rung
/// was comparing an empty sink against a census of the predictions and calling
/// the difference missed work.
///
/// A finding that becomes three `INERT`s can be scrolled past, so the numbers
/// live here where the shape of the gap is legible. The gap is not a wiring
/// defect any more; it is that nothing has yet measured anything that was
/// projected.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn report_where_projection_calibration_stops() {
    let pool = pool().await;

    let one = |sql: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(sql)
                .fetch_one(&pool)
                .await
                .unwrap_or(-1)
        }
    };

    let projections = one(
        "SELECT count(DISTINCT extra->>'projection_id')::bigint FROM sosa_observations \
          WHERE extra ? 'projection_id'",
    )
    .await;
    let points =
        one("SELECT count(*)::bigint FROM sosa_observations WHERE extra ? 'projection_id'").await;
    let commits = one("SELECT count(*)::bigint FROM process_projection_commits").await;
    let resolved = one("SELECT count(*)::bigint FROM process_spacetime").await;
    let scored =
        one("SELECT count(*)::bigint FROM eval_signals WHERE evaluator_name ILIKE '%projection%'")
            .await;

    // The question the old 12,167 was standing in for, asked properly: does any
    // measurement exist for anything that was ever projected?
    let overlapping = one("SELECT count(*)::bigint FROM sosa_observations r \
          WHERE NOT (r.extra ? 'projection_id') \
            AND EXISTS (SELECT 1 FROM sosa_observations s \
                         WHERE s.extra ? 'projection_id' \
                           AND s.observable_property = r.observable_property)")
    .await;
    let measurements =
        one("SELECT count(*)::bigint FROM sosa_observations WHERE NOT (extra ? 'projection_id')")
            .await;

    println!("\n  Loop 5.A (projection accuracy), link by link:");
    println!(
        "  {:<44} {:>10}",
        "projections written (distinct runs)", projections
    );
    println!("  {:<44} {:>10}", "  └ trajectory points", points);
    println!("  {:<44} {:>10}", "commitments anchored", commits);
    println!(
        "  {:<44} {:>10}",
        "resolutions (measurement met model)", resolved
    );
    println!("  {:<44} {:>10}", "accuracy signals scored", scored);
    println!(
        "\n  {measurements} measurement(s) on file; {overlapping} of them share an \
         observable_property with any projection."
    );

    if resolved == 0 {
        println!(
            "\n  Loop 5.A's projection-accuracy path has never had an input. \
             Not a wiring fault: the projections cover a set of \
             `chem:`/`bio:` properties that the measurement stream almost \
             entirely does not touch. Until the two streams overlap, every \
             link below the anchor is correctly INERT and no amount of \
             triggering will produce a signal."
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
    // `verdict` and `actor_kind` are bound from their owning declarations, not
    // spelled. `assertion_verifications` has no production writer yet — Loop 4
    // stalls upstream of it — so this probe is the only place either vocabulary
    // meets the live column, and binding `ActorKind::Human` is what exercises
    // the `#[sqlx(type_name = "text")]` encoding against a real Postgres. A
    // spliced literal would have proved the constraint and nothing about the
    // type the first production writer will use.
    let mut tx = pool.begin().await.expect("begin");
    let attempt = sqlx::query(
        "INSERT INTO assertion_verifications \
             (assertion_id, episode_id, verdict, actor, actor_kind) \
         SELECT gen_random_uuid(), episode_id, $1, 'probe', $2 \
           FROM episodes LIMIT 1",
    )
    .bind(fermi::grounding_trust::PROV_HUMAN_SOURCED)
    .bind(fermi::seam_vocabulary::ActorKind::Human)
    .execute(&mut *tx)
    .await;
    let _ = tx.rollback().await;

    let err = attempt.expect_err(
        "the database accepted a `human_sourced` verification with no citation. \
         That makes the human route the cheapest way to turn a guess into a fact \
         — it scores level with a tool call, and the citation is the only thing \
         that earns the score.",
    );

    // `is_err()` alone would pass on a bind that never reached the constraint.
    // Both values are now bound rather than spliced, and a bound
    // `#[sqlx(type_name = "text")]` enum resolves its type by name at bind time
    // — so a typo in the attribute fails with `TypeNotFound`, which is also an
    // error, and this probe would have reported the constraint as enforced on
    // the strength of it. Naming the constraint is what makes the pass mean
    // what it says, and it is simultaneously the only live proof that
    // `ActorKind` encodes as text against a real Postgres.
    let msg = err.to_string();
    assert!(
        msg.contains("assertion_verifications_citation_check"),
        "the insert failed, but not on the citation constraint — so this test \
         has demonstrated nothing about it. Postgres said: {msg}"
    );
}
