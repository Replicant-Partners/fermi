//! Can a contracted claim actually be queued for verification?
//!
//! `assertion_verifications` has held **0 rows since migration 205**. The audit's
//! conclusion was that it needed a writer rather than a schema, and
//! `src/verification_queue.rs` is that writer. This is the suite that says whether
//! the writer's row is one the table accepts.
//!
//! # Why offline tests cannot settle it
//!
//! The pure half is covered: `assertions::from_graded_field` is falsified in its
//! own module, `Enqueued::is_problem` in the registry, and the routing comes from
//! `Grounding::Sourced { tool }` rather than a second declaration. All of that can
//! be wrong in exactly one way an offline test cannot see — **the row can be
//! refused by the database** — and that is the failure mode this table's whole
//! history is made of. `anomaly_events` held 0 rows because every insert wrote
//! `severity = "L1"` against a CHECK of `('info','warning','critical')`, in a
//! spawned task, with the error logged. The unit tests of the detectors all
//! passed; they stopped one call short of the database and the entire defect lived
//! in that step.
//!
//! So this asserts nothing about how many claims exist. It asserts that **if one
//! were queued it would land**, which is the only part a test can own and the part
//! that was false for `anomaly_events` for the life of the feature.
//!
//! # Read-only, in the sense that matters
//!
//! Every write is inside a transaction that is rolled back, including the ones
//! expected to fail.

use fermi::grounding_trust as gt;
use fermi::seam_vocabulary::ActorKind;
use sqlx::{PgPool, Row};
use uuid::Uuid;

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect")
}

/// An episode to hang a verification on, minted inside the caller's transaction.
///
/// Minted rather than found, for `gate_review_contract`'s reason: a probe that
/// skipped on an empty table would report "nothing to check" in exactly the state
/// where nothing has ever been checked.
async fn seed_episode(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Option<Uuid> {
    // `timestamp_ref` and `context` are NOT NULL with no default. The first
    // version of this omitted both, so every seed failed and all three tests
    // below reported the database refusing the *verification* when what it had
    // refused was the episode -- a probe blaming the thing it was written to
    // check. Read the live definition; never infer a table's requirements from
    // the columns a writer happens to mention.
    sqlx::query_scalar(
        "INSERT INTO episodes (agent_id, timestamp_ref, query, context, \
                               response_text, execution_status, \
                               execution_time_ms, provenance) \
         SELECT agent_id, now(), 'probe', '{}'::jsonb, 'probe', 'success', 0, \
                'auto_pass' \
           FROM agents LIMIT 1 \
         RETURNING episode_id",
    )
    .fetch_optional(&mut **tx)
    .await
    .ok()
    .flatten()
}

/// Try one pending verdict through the real INSERT. Always rolled back.
async fn attempt(pool: &PgPool, verdict: &str) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let Some(episode_id) = seed_episode(&mut tx).await else {
        let _ = tx.rollback().await;
        return Err("could not seed an episode; `agents` may be empty".into());
    };

    let outcome = sqlx::query(fermi::verification_queue::ENQUEUE_SQL)
        .bind(Uuid::new_v4())
        .bind(episode_id)
        .bind(verdict)
        .bind(fermi::verification_queue::ENQUEUED_BY)
        .bind(ActorKind::Platform)
        .bind(serde_json::json!({"probe": true}))
        .fetch_one(&mut *tx)
        .await;

    let result = match outcome {
        Ok(row) => {
            if row.try_get::<Uuid, _>("verification_id").is_ok() {
                Ok(())
            } else {
                Err("the insert succeeded and returned no verification_id".into())
            }
        }
        Err(e) => Err(e.to_string()),
    };
    let _ = tx.rollback().await;
    result
}

/// Both pending verdicts round-trip through the real table.
///
/// The `anomaly_events` question, asked before this table's zero is believed. If
/// either token is refused, every enqueue of that kind is lost in a spawned task
/// and the queue reads as empty for the life of the feature.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn a_pending_verification_can_actually_be_written() {
    let pool = pool().await;
    for verdict in [gt::PROV_PENDING_TOOL, gt::PROV_PENDING_HUMAN] {
        let outcome = attempt(&pool, verdict).await;
        assert!(
            outcome.is_ok(),
            "`{verdict}` cannot be written to assertion_verifications: \
             {outcome:?}. Every claim routed that way would be refused by the \
             database, and the table would read as a queue nobody has worked — \
             which is exactly how `anomaly_events` held 0 rows while every \
             insert was being rejected."
        );
    }
    println!("  both pending verdicts round-trip through the real INSERT.");
}

/// The queue does not need a citation to hold a pending row.
///
/// Migration 205's CHECK requires `source_citation` for `human_sourced` only, and
/// `ENQUEUE_SQL` deliberately omits the column. Asserted because the tempting
/// "fix" for a constraint violation is to write an empty string, and that turns
/// the citation requirement — the thing that stops a one-click *verified* button
/// being a laundering UI — into decoration.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn a_pending_row_needs_no_citation_and_a_sourced_one_still_does() {
    let pool = pool().await;

    assert!(
        attempt(&pool, gt::PROV_PENDING_HUMAN).await.is_ok(),
        "a pending row was refused for want of a citation it cannot have yet"
    );

    // And the constraint that matters is still armed. `human_sourced` with no
    // citation must be refused, or the enqueue path has quietly widened the one
    // rule that makes a human verdict cost something.
    let laundered = attempt(&pool, gt::PROV_HUMAN_SOURCED).await;
    assert!(
        laundered.is_err(),
        "`human_sourced` was accepted with no `source_citation`. Migration 205's \
         citation CHECK is the reason `human_sourced` scores as high as \
         `tool_verified` in `grounding_trust::strength` — someone else can follow \
         the citation to the same source. Without it the score is unearned and a \
         one-click `verified` button becomes a laundering UI."
    );
}

/// Every verdict the queue can write is a token the column declares.
///
/// The `severity = 'L1'` check, pointed at this table. `verification_queue` writes
/// `pending_tool_check` / `pending_human_check` from
/// `grounding_trust::PROVENANCE_VALUES`, and `seam_vocabulary` registers
/// `assertion_verifications.verdict` against that same ladder — so this asserts
/// the two ends agree in the one place a declaration check cannot reach: the
/// server.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn the_pending_tier_is_inside_the_columns_vocabulary() {
    let pool = pool().await;

    // Looked up by NAME, not by matching the definition text.
    //
    // The first version searched for a constraint whose definition contained
    // "verdict" and took the first -- which is the CITATION check, because it
    // reads `verdict <> 'human_sourced'`. So the test reported that the column
    // had no vocabulary while `assertion_verifications_verdict_check` was sitting
    // right there. The name is what `seam_vocabulary::VOCABULARIES` declares and
    // it is the only thing a test should pin: a definition is Postgres's
    // rendering and a name is a choice somebody made in a migration.
    const DECLARED: &str = "assertion_verifications_verdict_check";

    let def: Option<String> = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
          WHERE conrelid = 'public.assertion_verifications'::regclass \
            AND conname = $1",
    )
    .bind(DECLARED)
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();

    let Some(d) = def else {
        panic!(
            "`{DECLARED}` does not exist, and `seam_vocabulary::VOCABULARIES` \
             declares it. Either migration 205 has not run or the constraint was \
             renamed -- and `tests/seam_vocabulary_contract.rs` is the tier that \
             owns that finding, so this failing means the two disagree."
        );
    };

    for token in [gt::PROV_PENDING_TOOL, gt::PROV_PENDING_HUMAN] {
        assert!(
            d.contains(token),
            "`{token}` is written by `verification_queue::enqueue` and \
             `{DECLARED}` does not accept it:\n  {d}\nThis is the \
             `severity = 'L1'` shape exactly: Rust holds one vocabulary, Postgres \
             holds another, nothing compares them, and the rejected write is \
             swallowed in a spawned task."
        );
    }
    println!("  {DECLARED} accepts both pending tokens.");
}
