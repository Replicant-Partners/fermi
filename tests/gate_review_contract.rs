//! Can a gate review actually be recorded, and does the rule that matters hold?
//!
//! # Why a firing probe and not a row count
//!
//! `gate_decision_reviews` holds 0 rows, and it will hold 0 rows for a while: it
//! is a queue nobody has worked yet. Asserting on that count would assert that
//! reviews must exist, which is the error `anomaly_firing_probe` was written to
//! avoid on `anomaly_events` — and `anomaly_events` is the cautionary case,
//! because its zero was **not** an absence of anomalies. Loop 2's seed wrote
//! `severity = "L1"` against a CHECK of `('info','warning','critical')`, every
//! insert was refused by the database inside a `tokio::spawn`, the error was
//! `warn!`ed, and the handover recorded the remedy as "watch the table after the
//! next traffic". The table was not empty because nothing happened. It was empty
//! because everything that happened was rejected.
//!
//! So this asserts nothing about how many reviews exist. It asserts that **if
//! one were filed it would land**, and that the one constraint carrying a
//! judgement — `overturned` requires a rationale — refuses what it claims to.
//!
//! # The specific thing that would otherwise be believed
//!
//! `gate_review::classify_write_error` maps constraint *names* to HTTP statuses,
//! and those names were never written down anywhere: migration 216 declares its
//! CHECKs **inline on the column**, so Postgres chooses the name. The mapping is
//! four string literals guessing at a naming convention. Every unit test of that
//! function passes with the guess wrong, because the unit test feeds it the same
//! literal the implementation contains — a closed loop, and precisely the shape
//! the write-accounting scan had when it was satisfied by a declaration rather
//! than a call site.
//!
//! Only the database can settle it. If a name is wrong, a reviewer filing an
//! `overturned` with no rationale receives a 500 with a Postgres string in it at
//! the moment they were told their finding had been filed, and nothing anywhere
//! would say so.
//!
//! # Read-only, in the sense that matters
//!
//! Every write here is inside a transaction that is rolled back, including the
//! ones expected to fail. Nothing is left behind on either path.

use fermi::gate_review::{self, Refusal};
use fermi::seam_vocabulary::{ActorKind, GateReviewVerdict};
use sqlx::{PgPool, Row};

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect")
}

/// A decision to hang a review on, minted inside the caller's transaction.
///
/// Minted rather than found. `gate_decisions` may legitimately be empty — five of
/// the seven gates never write to it and the two that do had no ledger at all
/// until migration 214 — and a probe that skipped on an empty table would report
/// "nothing to check" in exactly the state where nothing has ever been checked.
async fn seed_decision(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO gate_decisions (gate, decision, reason, subject) \
         VALUES ('coherence', 'refused', 'probe', 'probe') RETURNING id",
    )
    .fetch_one(&mut **tx)
    .await
    .expect("seed a gate decision")
}

/// Attempt one review through the real INSERT. Always rolled back.
///
/// Returns the [`Refusal`] the handler would have produced, so this probe
/// exercises the *translation* and not just the constraint. Testing the
/// constraint alone would leave the four constraint-name literals in
/// `classify_write_error` unverified, and those are the part that cannot be
/// checked from inside Rust.
async fn attempt(
    pool: &PgPool,
    verdict: GateReviewVerdict,
    rationale: Option<&str>,
) -> Result<(), Refusal> {
    let mut tx = pool.begin().await.expect("begin");
    let decision_id = seed_decision(&mut tx).await;

    let outcome = sqlx::query(gate_review::REVIEW_INSERT_SQL)
        .bind(decision_id)
        .bind("coherence")
        .bind(verdict)
        .bind(rationale)
        .bind("probe@fermi")
        .bind(ActorKind::Tool)
        .bind(None::<serde_json::Value>)
        .fetch_one(&mut *tx)
        .await;

    let result = match outcome {
        Ok(row) => {
            assert!(
                row.try_get::<uuid::Uuid, _>("review_id").is_ok(),
                "the insert succeeded and returned no review_id, so the handler \
                 has nothing to hand back to the reviewer"
            );
            Ok(())
        }
        Err(e) => {
            let constraint = e.as_database_error().and_then(|d| d.constraint());
            Err(gate_review::classify_write_error(
                constraint,
                &e.to_string(),
            ))
        }
    };

    let _ = tx.rollback().await;
    result
}

/// A review can be recorded at all.
///
/// The `anomaly_events` question, asked before the table is believed: is the
/// zero an empty queue or a rejected write?
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn a_review_can_actually_be_written() {
    let pool = pool().await;
    for verdict in GateReviewVerdict::ALL {
        // Every variant gets a rationale here, so this test is about the write
        // path and the vocabulary only. The rationale RULE is the next test.
        let outcome = attempt(&pool, *verdict, Some("probe rationale")).await;
        assert!(
            outcome.is_ok(),
            "`{verdict}` cannot be written to gate_decision_reviews: {outcome:?}. \
             Every review of that kind would be refused by the database, and the \
             table would read as a queue nobody has worked."
        );
    }
    println!(
        "  {} verdict(s) round-trip through the real INSERT.",
        GateReviewVerdict::ALL.len()
    );
}

/// `overturned` with no rationale is refused, **and the refusal is translated.**
///
/// Two findings in one assertion and they are deliberately not split, because
/// separately each is satisfiable while the feature is broken:
///
///   * the constraint exists and refuses — true, and the reviewer still gets a
///     500 if the name in `classify_write_error` is wrong;
///   * `classify_write_error` maps the name — true against a literal it supplies
///     itself, which is a closed loop and proves nothing.
///
/// Only the pair says the reviewer gets a 400 explaining what to do.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn an_uncited_overturn_is_refused_and_the_refusal_reaches_the_reviewer() {
    let pool = pool().await;

    let bare = attempt(&pool, GateReviewVerdict::Overturned, None).await;
    assert_eq!(
        bare,
        Err(Refusal::RationaleRequired),
        "an `overturned` review with no rationale was not translated into \
         `RationaleRequired`. If it was accepted, migration 216's \
         `gate_decision_reviews_rationale_check` is not applied and an uncited \
         overturn — a complaint recorded as a finding — is now writable. If it \
         was refused as `Rejected`, the constraint fired and \
         `gate_review::classify_write_error` does not know its name, so the \
         reviewer receives a 500 with a Postgres string in it at the moment they \
         were told their finding had been filed."
    );
    assert!(
        gate_review::is_client_error(&Refusal::RationaleRequired),
        "a missing rationale answered 500 tells the reviewer the platform is \
         broken, and they stop reviewing"
    );

    // Whitespace is not a rationale. `length(trim(...)) > 0` is the half of the
    // constraint that a NOT NULL would have missed, and a single space is what a
    // form with a required field gets typed into it.
    let blank = attempt(&pool, GateReviewVerdict::Overturned, Some("   ")).await;
    assert_eq!(
        blank,
        Err(Refusal::RationaleRequired),
        "a whitespace-only rationale was accepted, so the citation requirement \
         is satisfiable by pressing the space bar"
    );

    // And the same verdict with a real rationale goes through, so the test above
    // is not passing because `overturned` is unwritable for some other reason.
    let cited = attempt(
        &pool,
        GateReviewVerdict::Overturned,
        Some("The Γ arithmetic was wrong; the correction was sound."),
    )
    .await;
    assert!(
        cited.is_ok(),
        "a cited overturn was refused too, so the check above proves nothing \
         about the rationale rule: {cited:?}"
    );
}

/// `upheld` needs no rationale, and that asymmetry is the design.
///
/// Asserted rather than left implicit. Someone tightening the constraint to
/// require a rationale for every verdict would be making the routine
/// confirmation as expensive as the finding, and the consequence is not more
/// rigour — it is that nobody reviews the routine decisions, and then the
/// *denominator* is unknown. "3 overturned" and "3 overturned of 400 reviewed"
/// are different findings and only the second is actionable.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn the_cheap_confirmation_stays_cheap() {
    let pool = pool().await;
    for verdict in [GateReviewVerdict::Upheld, GateReviewVerdict::Unclear] {
        let outcome = attempt(&pool, verdict, None).await;
        assert!(
            outcome.is_ok(),
            "`{verdict}` now requires a rationale: {outcome:?}. That makes the \
             cheap confirmation as costly as the finding, so the routine \
             decisions go unreviewed and the overturn count loses its \
             denominator. See migration 216."
        );
    }
}

/// The three read queries run, bind what they claim, and shape what they return.
///
/// A SQL string in a `const` is checked by nothing. `LATEST_REVIEWS_SQL` in
/// particular takes **two** binds — the second is
/// `GateReviewVerdict::Overturned`, in the `ORDER BY`, so the priority ordering
/// and the CHECK cannot disagree about the spelling — and a query whose bind
/// count is wrong fails at runtime on a surface that reads it once per page load.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn every_declared_query_runs_against_the_real_schema() {
    let pool = pool().await;

    let counts: Vec<(String, i64)> = sqlx::query_as(gate_review::STANDING_SQL)
        .bind("coherence")
        .fetch_all(&pool)
        .await
        .expect("STANDING_SQL runs");
    // And the fold accepts every token actually on file. A verdict the column
    // holds and no variant spells is half of `severity = 'L1'`, and the rows are
    // the only place it appears.
    let tally = gate_review::tally_from_counts(&counts)
        .expect("a verdict on file that no GateReviewVerdict variant spells");

    let latest = sqlx::query(gate_review::LATEST_REVIEWS_SQL)
        .bind("coherence")
        .bind(GateReviewVerdict::Overturned)
        .fetch_all(&pool)
        .await
        .expect("LATEST_REVIEWS_SQL runs with both binds");

    let lookup = sqlx::query(gate_review::DECISION_LOOKUP_SQL)
        .bind(0_i64)
        .fetch_optional(&pool)
        .await
        .expect("DECISION_LOOKUP_SQL runs");
    assert!(
        lookup.is_none(),
        "gate_decisions has a row with id 0, which BIGSERIAL does not produce — \
         the lookup is matching on something other than the id"
    );

    println!(
        "  coherence: {} decision(s) reviewed, {} overturned; {} current \
         verdict(s) on file.",
        tally.reviewed(),
        tally.overturned,
        latest.len()
    );
}

/// The reviewable gates are the ones whose decisions exist, live.
///
/// `gate_api::a_review_door_only_exists_where_the_decisions_do` asserts this
/// against `gate_trust::GATES` at compile time. This asks the database the same
/// question a different way: does the table a review points at actually accept
/// the gates the doors are declared for? A door on a gate that
/// `gate_decision_reviews_gate_check` rejects is a button whose every press is a
/// 500 — and the two CHECKs over one vocabulary (214's and 216's) are exactly
/// where that drift would live.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn every_declared_door_names_a_gate_the_review_table_accepts() {
    let pool = pool().await;
    for door in fermi::gate_api::GATE_DOORS {
        if !door.path.contains("/decisions/") {
            continue;
        }
        let mut tx = pool.begin().await.expect("begin");
        let accepted = sqlx::query(
            "INSERT INTO gate_decision_reviews \
               (decision_id, gate, verdict, rationale, actor, actor_kind) \
             SELECT id, $1, $2, 'probe', 'probe@fermi', $3 \
               FROM gate_decisions LIMIT 1",
        )
        .bind(door.subject)
        .bind(GateReviewVerdict::Upheld)
        .bind(ActorKind::Tool)
        .execute(&mut *tx)
        .await;
        let _ = tx.rollback().await;

        if let Err(e) = accepted {
            let constraint = e.as_database_error().and_then(|d| d.constraint());
            // A zero-row source is not a failure: `gate_decisions` may be empty,
            // and `INSERT ... SELECT` over nothing writes nothing.
            assert!(
                constraint != Some("gate_decision_reviews_gate_check"),
                "`{}` has a review door and `gate_decision_reviews` refuses the \
                 gate name. Every press of that button is a 500. Migration 214's \
                 CHECK and 216's are two constraints over one vocabulary — \
                 `gate_trust::GATE_IDS` — and widening one without the other is \
                 the drift `seam_vocabulary` registers both of them to catch.",
                door.subject
            );
        }
    }
}
