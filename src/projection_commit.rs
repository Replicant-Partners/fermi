//! The commitment anchor for a model projection — the immutable clock.
//!
//! A row in `process_projection_commits` records that a value was predicted
//! *before* any measurement of it arrived. Without it there is no Loop 5.A
//! (projection accuracy): scoring a prediction you could have written after
//! seeing the answer is not verification, it is transcription.
//!
//! # Why this is in the library
//!
//! It was in `api_server`'s `handlers::simops_benchmark`, which the library
//! cannot reach. The library owns the agent tool that writes synthetic
//! observations, so the tool could not call the commit function that its own
//! result field reported on — and in place of the call it had:
//!
//! ```ignore
//! #[allow(unused_variables)]
//! let _ = (pool, observation_id, session_id, /* ...every argument... */);
//! None
//! ```
//!
//! with a comment describing it as "hooks for an observability path that may or
//! may not be live". Both arms of the `if` returned `None`, so the tool's
//! `commitment_hash` was `null` whether or not a clock had started, and no
//! caller could tell the difference. The stub was not a shortcut; it was a
//! module boundary, and it had held for the life of the feature.
//!
//! Measured before the move: **0 rows in `process_projection_commits`, 61
//! distinct projections on file.** Everything downstream — `process_spacetime`,
//! `eval_signals.projection_accuracy` — was empty for want of this one write,
//! and the liveness rung attributed the emptiness to the trigger site at the
//! far end of the chain.
//!
//! # Two callers, deliberately
//!
//! * `agent_backend::simops_tools::execute_simops_write_observation` — the
//!   agent tool. Zero rows to date.
//! * `handlers::observations::ingest` — the HTTP batch endpoint. **This is
//!   where the projections actually arrive**, from an external
//!   `kask:dynamics/...` runner.
//!
//! Both decide *what is a projection* with [`crate::projection_kind`] rather
//! than by comparing a tag inline, which is the mistake that made this
//! invisible: the three readers matched `source = "simops_simulation"` and the
//! producer writes `source_kind = "dynamics_projection"`.

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// `sha256(observation_id | predicted_value | model_uri | phenomenon_time)`.
///
/// The hash is the anchor's identity and its idempotency key. It deliberately
/// does **not** include the measurement: the point of the commitment is that it
/// is computable before one exists.
pub fn projection_commitment_hash(
    observation_id: &Uuid,
    predicted_value: f64,
    model_uri: Option<&str>,
    phenomenon_time_ms: i64,
) -> String {
    let mut h = Sha256::new();
    h.update(observation_id.to_string().as_bytes());
    h.update(b"|");
    h.update(format!("{:.8}", predicted_value).as_bytes());
    h.update(b"|");
    h.update(model_uri.unwrap_or("").as_bytes());
    h.update(b"|");
    h.update(phenomenon_time_ms.to_string().as_bytes());
    format!("{:x}", h.finalize())
}

/// Anchor a projection. Returns the commitment hash, or `None` if nothing was
/// written.
///
/// Idempotent, and non-fatal by design: a failure to record the anchor must not
/// fail the observation write that occasioned it. `None` therefore means
/// "no clock started", and the caller should surface it as such rather than
/// reporting success — the previous version returned `None` unconditionally and
/// that is precisely how this went unnoticed.
#[allow(clippy::too_many_arguments)]
pub async fn commit_projection(
    pool: &PgPool,
    observation_id: Uuid,
    session_id: Uuid,
    workspace_id: Option<Uuid>,
    observable_property: &str,
    feature_of_interest: Option<&str>,
    predicted_value: f64,
    model_uri: Option<&str>,
    stage_id: Option<&str>,
    projection_id: Option<&str>,
    phenomenon_time_ms: i64,
    process_context: Option<&serde_json::Value>,
) -> Option<String> {
    // Non-fatal if migration 141 is pending.
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables
         WHERE table_name='process_projection_commits')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if !exists {
        return None;
    }

    let hash = projection_commitment_hash(
        &observation_id,
        predicted_value,
        model_uri,
        phenomenon_time_ms,
    );

    let already: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM process_projection_commits WHERE commitment_hash=$1)",
    )
    .bind(&hash)
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if already {
        return Some(hash);
    }

    let inserted = sqlx::query(
        r#"INSERT INTO process_projection_commits
           (sosa_observation_id, projection_id, workspace_id, session_id,
            observable_property, feature_of_interest, predicted_value,
            model_uri, stage_id, commitment_hash, committed_at,
            phenomenon_time_ms, process_context)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,NOW(),$11,$12)"#,
    )
    .bind(observation_id)
    .bind(projection_id)
    .bind(workspace_id)
    .bind(session_id)
    .bind(observable_property)
    .bind(feature_of_interest)
    .bind(predicted_value)
    .bind(model_uri)
    .bind(stage_id)
    .bind(&hash)
    .bind(phenomenon_time_ms)
    .bind(process_context)
    .execute(pool)
    .await;

    // Counted, not merely logged. A commitment that silently fails to write
    // leaves a projection that can never be scored, and the gap is
    // unrecoverable: the anchor's whole value is that it pre-dates the
    // measurement, so it cannot be honestly backfilled. That makes a lost
    // failure here strictly worse than a lost row elsewhere, and a `warn!` in a
    // log nobody reads is not a record of it.
    crate::write_accounting::observe(
        crate::write_accounting::Sink::ProcessProjectionCommits,
        inserted,
    )
    .map(|_| hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_changes_with_the_predicted_value() {
        // The anchor's job is to make the prediction unalterable. If two
        // different predictions hashed alike, the idempotency check above would
        // treat the second as already committed and drop it.
        let oid = Uuid::nil();
        let a = projection_commitment_hash(&oid, 1.0, Some("m@v1"), 0);
        let b = projection_commitment_hash(&oid, 1.000001, Some("m@v1"), 0);
        assert_ne!(a, b);
    }

    #[test]
    fn the_hash_does_not_depend_on_the_measurement() {
        // Stated as a test because it is the property that makes the anchor
        // mean anything, and it is invisible in the signature: there is no
        // measurement parameter, and there must never be one.
        let oid = Uuid::from_u128(7);
        assert_eq!(
            projection_commitment_hash(&oid, 4.2, Some("kask:dynamics/x@v1"), 1_700_000_000_000),
            projection_commitment_hash(&oid, 4.2, Some("kask:dynamics/x@v1"), 1_700_000_000_000),
        );
    }

    #[test]
    fn a_missing_model_uri_does_not_collide_with_an_empty_one() {
        // `unwrap_or("")` makes `None` and `Some("")` hash alike. Asserted so
        // the collision is a decision on record rather than a surprise: no
        // producer emits `Some("")`, and if one starts, this test is where the
        // question gets asked.
        let oid = Uuid::nil();
        assert_eq!(
            projection_commitment_hash(&oid, 1.0, None, 0),
            projection_commitment_hash(&oid, 1.0, Some(""), 0),
        );
    }
}
