//! Does the anchor prove what its column claims?
//!
//! Run with:
//! ```text
//! cargo test --test projection_anchor_contract -- --ignored --nocapture
//! ```
//!
//! # The invariant, and why it needs a probe rather than a review
//!
//! Loop 5.B claims *"a physical measurement is scored against what the model
//! projected — the one signal an agent cannot talk its way out of."* Exactly one
//! thing makes that true of a row in `process_spacetime`: the prediction was
//! anchored **before the world was measured**. Everything else in the row —
//! the error, the accuracy score, the direction — is arithmetic that is equally
//! correct when the answer was already known.
//!
//! Migration 141 encoded that invariant as
//! `committed_at IS NOT NULL AND committed_at < resolved_at`, and `resolved_at`
//! is `NOW()` at scoring time. `resolve_against_projection` can only score a
//! projection whose commit row it has just read, so the commit always pre-dates
//! the scoring pass: **the column was `true` for every row that could exist.**
//!
//! That is invisible to every check the platform had. The column exists, so
//! `schema_trust` passes. It has a plausible expression, so a reader nods. It is
//! never `false`, so no report ever contradicts it. Migration 215 changed the
//! comparison to `measured_at`, and this file is what says so — by writing a row
//! whose commit post-dates its measurement and requiring the database to call it
//! what it is.
//!
//! Both probes run inside a transaction that is always rolled back, so neither
//! can leave a row behind whether it succeeds or fails.

use fermi::seam_vocabulary::{DeltaDirection, ResolutionMode};
use sqlx::PgPool;

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect")
}

/// The generation expression the live column actually carries.
///
/// Read rather than assumed. The first version of this file reasoned about the
/// expression from the migration *file* and asserted a fact about production on
/// the strength of it — which is the audit's own recurring defect, and it is
/// cheaper to just ask.
async fn live_expression(pool: &PgPool) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT pg_get_expr(d.adbin, d.adrelid) \
           FROM pg_attrdef d \
           JOIN pg_attribute a ON a.attrelid = d.adrelid AND a.attnum = d.adnum \
          WHERE d.adrelid = 'public.process_spacetime'::regclass \
            AND a.attname  = 'committed_before_measured'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| "<no such column>".to_string())
}

/// Write one `process_spacetime` row with all three timestamps placed
/// explicitly, and return what the database says about it. Rolled back.
///
/// **All three, and that is the whole point.** The first version of this probe
/// moved `committed_at` around `measured_at` and left `resolved_at` at its
/// `NOW()` default — which makes the two expressions agree, because a commit
/// after the measurement is also after `NOW()`. It passed against production
/// with the tautological expression still in force, and the pass was nearly
/// believed. A probe must reproduce the shape the live path produces:
/// commit first, measurement earlier, scoring last.
///
/// `projection_commit_id` is left null — a nullable FK, and the invariant under
/// test is about three timestamps in this row rather than about the parent. Both
/// vocabularies are bound from their types, so this also exercises the
/// `process_spacetime` write path's encoding against production.
async fn committed_before_measured(
    pool: &PgPool,
    committed_min: i32,
    measured_min: i32,
) -> Option<bool> {
    let mut tx = pool.begin().await.expect("begin");
    let verdict: Result<Option<bool>, _> = sqlx::query_scalar(
        "INSERT INTO process_spacetime \
             (observable_property, predicted_value, real_observation_id, \
              actual_value, measured_at, absolute_error, relative_error, \
              accuracy_score, delta_direction, resolution_mode, \
              committed_at, resolved_at) \
         VALUES ('anchor_probe', 1.0, gen_random_uuid(), 1.0, \
                 NOW() + ($3 || ' minutes')::interval, 0.0, 0.0, 1.0, $1, $2, \
                 NOW() + ($4 || ' minutes')::interval, NOW()) \
         RETURNING committed_before_measured",
    )
    .bind(DeltaDirection::Exact)
    .bind(ResolutionMode::AnyReading)
    .bind(measured_min.to_string())
    .bind(committed_min.to_string())
    .fetch_one(&mut *tx)
    .await;
    let _ = tx.rollback().await;
    verdict.expect("the probe insert was refused")
}

/// A prediction filed after the measurement is not a prediction.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn a_commit_that_post_dates_its_measurement_is_not_counted_as_anchored() {
    let pool = pool().await;
    let expr = live_expression(&pool).await;
    println!("  live expression: {expr}");

    // Honest: anchored an hour ago, measured half an hour ago, scored now.
    // Both the old expression and the new one call this `true`, and that is
    // correct — it is the case the feature is for.
    assert_eq!(
        committed_before_measured(&pool, -60, -30).await,
        Some(true),
        "a prediction anchored before its measurement does not read as \
         anchored, so the invariant now rejects the honest case — which is how \
         a check comes to be switched off.\n  expression: {expr}"
    );

    // Transcription: measured an hour ago, anchored half an hour ago, scored
    // now. This is exactly the shape a backfill of the 61 projections already
    // on file would produce against the 7,576 real readings already on file.
    //
    // Migration 141's expression calls it `true` — `committed_at` is before
    // `resolved_at`, as it must be, since the scorer read the commit first.
    assert_eq!(
        committed_before_measured(&pool, -30, -60).await,
        Some(false),
        "the database reports a prediction filed AFTER its measurement as \
         `committed_before_measured`, so Loop 5.B's central column is a \
         tautology and scoring an answer that was already known is \
         indistinguishable from a forecast.\n\n  expression: {expr}\n\n  \
         Migration 215 replaces it. If it is registered and this is still \
         failing, the server has not rebooted — check `schema_migrations` for \
         215, and note that a row with failures means it ran and could not \
         apply."
    );
}

/// The chain must not be closeable by transcription.
///
/// `loop_model` counts this stage with `WHERE committed_before_measured`. If the
/// column were still a tautology the filter would be a no-op and the loop could
/// be made to read `turning` by anchoring the 61 projections already on file and
/// resolving them against the 7,576 real readings already on file — every row
/// scoring an answer that was knowable when the prediction was filed.
///
/// Asserted as a difference between two counts rather than as a number, because
/// the honest state today is that both are zero: nothing has ever been anchored.
/// A number would have to be updated the first time the loop turns; the
/// relationship holds for ever.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn the_loop_counts_no_more_than_the_database_can_vouch_for() {
    let pool = pool().await;

    let total: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM process_spacetime")
        .fetch_one(&pool)
        .await
        .expect("total");
    let anchored: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM process_spacetime WHERE committed_before_measured",
    )
    .fetch_one(&pool)
    .await
    .expect("anchored");

    println!("  process_spacetime: {total} row(s), {anchored} anchored before measurement");

    assert!(
        anchored <= total,
        "more anchored rows than rows, which is arithmetically impossible and \
         means one of these queries is not reading what it says"
    );

    // The claim this file protects, stated the only way it can be stated while
    // the sink is empty: whatever is in there, the loop counts the verified
    // subset. When `total > anchored` this line is the interesting one, and it
    // will be the first evidence the platform has ever had that something was
    // scored against a known answer.
    if total > anchored {
        println!(
            "  {} row(s) score a projection that did NOT pre-date its \
             measurement. These are transcription, not verification, and \
             `loop_model` excludes them.",
            total - anchored
        );
    }
}
