//! Can the anomaly detectors fire at all?
//!
//! `anomaly_events` holds **0 rows against 1,411 scanned timeline entries**. Its
//! liveness contract is `Conditional` — reported, never asserted — because a
//! detector that finds nothing may simply be right, and asserting on the row
//! count would be asserting that anomalies must exist.
//!
//! That reasoning is correct and it leaves the zero **unfalsifiable**, which is
//! the same standing as `provenance_floor_coverage` had when it tested for the
//! presence of a call and not its argument. The contract's own remediation says
//! so:
//!
//! > Do not chase the row count. Write a firing probe per detector — feed it
//! > input it must flag and assert an event lands — the way the taxonomy
//! > cross-check proves it can go red before its zero is believed.
//!
//! This is that probe. It does not assert that anomalies exist. It asserts that
//! **if one occurred, it would be recorded** — which is the only part a test can
//! own, and the part that was false.
//!
//! # It was false
//!
//! Loop 2's seed (`3e6c9e08`) raises a `grounding` anomaly on a grounding
//! violation. It wrote `severity = "L1"`; the column's CHECK is
//! `('info','warning','critical')`. Every insert was rejected by the database,
//! in a `tokio::spawn`, with the error `tracing::warn!`ed — so the request
//! succeeded, the table stayed at zero, and the handover recorded the remedy as
//! "watch `anomaly_events` after the next traffic".
//!
//! The pure detector functions have had unit tests since they were written and
//! all of them pass. They stop one call short of the database, and the whole
//! defect lived in that step.
//!
//! # Read-only, in the sense that matters
//!
//! Every write here happens inside a transaction that is rolled back, including
//! the ones expected to fail. Nothing is left behind on either path.

use fermi::anomaly_vocabulary::{
    is_actionable_flag, is_bookkeeping_flag, BOOKKEEPING_FLAG_PREFIXES, KINDS, SEVERITIES,
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

/// Try one `(kind, severity)` against the real table. Always rolled back.
async fn insert_is_accepted(pool: &PgPool, kind: &str, severity: &str) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let attempt = sqlx::query(
        "INSERT INTO anomaly_events (agent_id, kind, severity, payload, requires_review) \
         SELECT agent_id, $1, $2, '{\"probe\":true}'::jsonb, false FROM agents LIMIT 1",
    )
    .bind(kind)
    .bind(severity)
    .execute(&mut *tx)
    .await;
    let _ = tx.rollback().await;

    match attempt {
        Ok(r) if r.rows_affected() == 1 => Ok(()),
        Ok(_) => Err("no row inserted — is `agents` empty?".into()),
        Err(e) => Err(e.to_string()),
    }
}

/// Every kind and severity the platform can construct must land.
///
/// The direction that was broken. A vocabulary the schema has not caught up
/// with produces a writer that cannot write, and because the write is spawned
/// and best-effort, an empty sink is the only symptom.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn every_declared_kind_and_severity_is_accepted_by_the_table() {
    let pool = pool().await;
    let mut rejected: Vec<String> = Vec::new();

    for kind in KINDS {
        for severity in SEVERITIES {
            if let Err(e) = insert_is_accepted(&pool, kind, severity).await {
                rejected.push(format!("{kind}/{severity}: {e}"));
            }
        }
    }

    assert!(
        rejected.is_empty(),
        "\n{} declared (kind, severity) pair(s) the database will not accept:\n  {}\n\n\
         A writer using one of these produces no row and no error a person sees: \
         the insert is spawned and its failure is logged. This is how Loop 2's \
         seed came to be planted, believed, and inert.\n",
        rejected.len(),
        rejected.join("\n  ")
    );
}

/// The severity that was actually being written must still be refused.
///
/// A positive control for the test above. If the fix had been to widen the
/// CHECK rather than to use the platform's vocabulary, the previous test would
/// pass and the second severity scheme would survive; this one says which
/// resolution was chosen and holds it.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn the_invented_severity_is_still_rejected() {
    let pool = pool().await;
    let got = insert_is_accepted(&pool, "grounding", "L1").await;
    assert!(
        got.is_err(),
        "`L1` is accepted, so the CHECK was widened instead of the writer being \
         corrected. There are now two severity schemes for one column, and the \
         one that gets believed is whichever is nearest the reader."
    );
    assert!(
        !SEVERITIES.contains(&"L1"),
        "`L1` is declared in the Rust vocabulary but rejected by the table: \
         the two have swapped places rather than agreeing."
    );
}

/// Nothing may be accepted that Rust does not know about.
///
/// The other direction, and the one migration 200 broke: it widened the CHECK
/// for `grounding` and no `AnomalyKind` variant was ever added, so for two
/// hundred migrations the only kind the platform actually wrote was the one no
/// enum could express. A vocabulary that is a strict subset of the schema is a
/// producer waiting to be written; a schema that is a strict subset of the
/// vocabulary is the writer that cannot write.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn the_table_accepts_nothing_the_vocabulary_omits() {
    let pool = pool().await;

    let def: String = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
          WHERE conrelid = 'public.anomaly_events'::regclass \
            AND conname = 'anomaly_events_kind_check'",
    )
    .fetch_one(&pool)
    .await
    .expect("the kind CHECK must exist; without it this table accepts anything");

    // `CHECK ((kind = ANY (ARRAY['drift'::text, ...])))`
    let in_check: Vec<String> = def
        .split('\'')
        .skip(1)
        .step_by(2)
        .map(|s| s.to_string())
        .collect();
    assert!(
        !in_check.is_empty(),
        "could not read any kind out of the constraint: {def}"
    );

    let undeclared: Vec<&String> = in_check
        .iter()
        .filter(|k| !KINDS.contains(&k.as_str()))
        .collect();
    assert!(
        undeclared.is_empty(),
        "the table accepts {undeclared:?}, which `fermi::anomaly_vocabulary` does \
         not declare. Either a producer is missing, or the CHECK was widened for \
         a feature nobody finished. Migration 200 did exactly that for \
         `grounding` and it took until now to notice."
    );

    let unaccepted: Vec<&&str> = KINDS
        .iter()
        .filter(|k| !in_check.iter().any(|c| c == *k))
        .collect();
    assert!(
        unaccepted.is_empty(),
        "Rust declares {unaccepted:?}, which the table would reject"
    );
}

/// Every flag the platform has actually written is one a detector reads, or is
/// declared inert with a reason.
///
/// Measured: **262 of 1,417** timeline entries carry a flag, and every one of
/// them is `social:observed` — bookkeeping, matched by no detector, correctly.
/// So the detectors' zero is honest: nothing actionable has ever been flagged.
///
/// That is worth asserting rather than assuming, because the failure it guards
/// is silent by construction. If `WildGuardEvaluator` began emitting
/// `harmful:violence` instead of `safety:violence`, the flag would be written,
/// the scan would run, the detector would match nothing, and the row count
/// would stay at zero — indistinguishable from a safe fleet. There is no
/// downstream symptom at all.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn every_flag_written_is_one_a_detector_reads_or_a_declared_no_op() {
    let pool = pool().await;

    let rows = sqlx::query(
        "SELECT f AS flag, count(*)::bigint AS n \
           FROM agent_timeline_entries, jsonb_array_elements_text(anomaly_flags) f \
          GROUP BY 1 ORDER BY 2 DESC",
    )
    .fetch_all(&pool)
    .await
    .expect("flag census");

    println!("\n  flags written to agent_timeline_entries:");
    let mut unknown: Vec<String> = Vec::new();
    let mut actionable = 0i64;
    for r in &rows {
        let flag: String = r.get("flag");
        let n: i64 = r.get("n");
        match (is_actionable_flag(&flag), is_bookkeeping_flag(&flag)) {
            (Some(kind), _) => {
                actionable += n;
                println!("  {flag:<28} {n:>6}  -> {kind}");
            }
            (None, Some(_)) => println!("  {flag:<28} {n:>6}  -> (declared inert)"),
            (None, None) => {
                println!("  {flag:<28} {n:>6}  -> NO DETECTOR");
                unknown.push(format!("{flag} ({n} entries)"));
            }
        }
    }
    if rows.is_empty() {
        println!("  (none)");
    }
    println!("\n  {actionable} flag(s) a detector would act on.");

    assert!(
        unknown.is_empty(),
        "\n{} flag shape(s) are written and read by nothing:\n  {}\n\n\
         An unmatched flag is a detector that cannot fire, and it has no \
         symptom: the flag is written, the scan runs, the detector matches \
         nothing, and the sink stays empty exactly as it would if the fleet were \
         clean. Add a prefix to ACTIONABLE_FLAG_PREFIXES, or declare it in \
         BOOKKEEPING_FLAG_PREFIXES with a reason.\n",
        unknown.len(),
        unknown.join("\n  ")
    );

    // Not an assertion: zero actionable flags is the current, honest state.
    if actionable == 0 {
        println!(
            "  No actionable flag has ever been written, so `anomaly_events` \
             being empty is consistent with the detectors working. What the \
             tests above establish is that a flag, if one appeared, would reach \
             the table."
        );
    }
}

/// The census must be able to see something, or it proves nothing.
///
/// The trap the scans in `§5` fell into: a check over an empty set passes for
/// ever. `social:observed` is the positive control — it demonstrates that flags
/// reach `agent_timeline_entries` at all, so "no actionable flags" is a finding
/// about the producers rather than about the pipe.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn the_flag_census_has_something_to_look_at() {
    let pool = pool().await;
    let flagged: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM agent_timeline_entries \
          WHERE jsonb_array_length(anomaly_flags) > 0",
    )
    .fetch_one(&pool)
    .await
    .expect("flag count");

    assert!(
        flagged > 0,
        "not one timeline entry carries a flag, so the census above is a check \
         over an empty set and cannot fail. Before believing it, establish that \
         the scorer writes flags at all."
    );
    println!("  {flagged} entr(ies) carry at least one flag.");

    // And every declared bookkeeping prefix should still be in use; a stale one
    // is an exemption that outlived its reason.
    for (prefix, _) in BOOKKEEPING_FLAG_PREFIXES {
        let seen: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM agent_timeline_entries, \
                    jsonb_array_elements_text(anomaly_flags) f \
              WHERE f LIKE $1 || '%'",
        )
        .bind(prefix)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
        if seen == 0 {
            println!(
                "  note: `{prefix}` is declared inert and no longer appears. \
                 If its producer is gone, remove the exemption."
            );
        }
    }
}
