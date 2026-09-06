//! `gate_decisions.episode_id` has no foreign key. This is the check instead.
//!
//! # Why the reference is unenforced
//!
//! `assertion_verifications.episode_id` **is** a real foreign key. This one is
//! deliberately not, and the difference is the writer.
//!
//! `gate_trust::spawn_gate_recorder` drains its queue with a single
//! `INSERT ... SELECT FROM UNNEST(...)`, so **one bad reference rejects the whole
//! batch.** An episode write that failed for its own reasons would take every
//! unrelated gate decision in that flush down with it — a gate's audit trail lost
//! because something else did not land, which is exactly the coupling a ledger
//! exists to avoid.
//!
//! A decision is also enqueued *before* its episode row exists: the gate fires
//! mid-request and the recorder flushes on a timer. That is the same race that
//! made Loop 2's original raise fail silently for the life of the feature, when
//! it referenced an episode id whose row had not been written yet.
//!
//! So the reference is checked rather than enforced, and the precedent is
//! `assertion_verifications.assertion_id` — not a foreign key either, because its
//! target lives inside a JSONB array. Different reason, same remedy: **an
//! unresolvable reference is a finding, not a rejected write.**
//!
//! # What this suite asserts, and what it only reports
//!
//! Asserted: a decision that names an episode names one that **exists**. That is
//! the property the missing foreign key would have guaranteed, and it is the only
//! thing here that cannot be a matter of degree.
//!
//! Reported: how many decisions carry an episode at all. That number is a
//! function of which gates are `Recorded` and how much traffic there has been, and
//! a threshold on it would be a target.

use sqlx::{PgPool, Row};

/// Is the column this suite is about actually deployed?
///
/// Read from `information_schema` rather than inferred from a failed query. A
/// raw `column "episode_id" does not exist` from the driver is a true fact about
/// a query presented as a failure of the thing under test — the reader cannot
/// tell a pending deploy from a broken one, and this repository has the scar:
/// `seam_vocabulary_contract` distinguishes *no table* from *table without the
/// constraint* from *migration ran and could not apply*, because assuming
/// "pending" is how migration 212 sat unregistered while everything downstream
/// bound to a column production did not have.
///
/// Two states are separated here. **Absent** means migration 220 has not run —
/// pending a deploy, and this suite has nothing to check yet. **Present** means
/// it has, and every assertion below applies.
async fn episode_id_is_deployed(pool: &PgPool) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM information_schema.columns \
          WHERE table_schema = 'public' AND table_name = 'gate_decisions' \
            AND column_name = 'episode_id'",
    )
    .fetch_one(pool)
    .await
    .map(|n| n > 0)
    .unwrap_or(false)
}

/// Report the pending state and skip, or return `true` to proceed.
async fn ready(pool: &PgPool, what: &str) -> bool {
    if episode_id_is_deployed(pool).await {
        return true;
    }
    println!(
        "\n  SKIPPED: {what}\n\n  `gate_decisions.episode_id` does not exist, so \
         migration 220 has not run. Check `schema_migrations` before believing \
         that: no row means never attempted (pending a deploy, or never \
         registered in `run_migrations` — which is what happened to 212); a row \
         with failures means it ran and could not apply.\n\n  This is not a \
         pass. Nothing about the reference has been checked.\n"
    );
    false
}

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect")
}

/// Every episode named by a gate decision must exist.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn no_gate_decision_points_at_an_episode_that_is_not_there() {
    let pool = pool().await;
    if !ready(
        &pool,
        "no_gate_decision_points_at_an_episode_that_is_not_there",
    )
    .await
    {
        return;
    }

    let dangling: Vec<(String, uuid::Uuid)> = sqlx::query(
        "SELECT g.gate::text AS gate, g.episode_id \
           FROM gate_decisions g \
          WHERE g.episode_id IS NOT NULL \
            AND NOT EXISTS (SELECT 1 FROM episodes e WHERE e.episode_id = g.episode_id) \
          LIMIT 50",
    )
    .fetch_all(&pool)
    .await
    .expect("read gate_decisions")
    .iter()
    .filter_map(|r| {
        Some((
            r.try_get::<String, _>("gate").ok()?,
            r.try_get::<uuid::Uuid, _>("episode_id").ok()?,
        ))
    })
    .collect();

    assert!(
        dangling.is_empty(),
        "\n  {} gate decision(s) name an episode that does not exist:\n    {}\n\n\
         The reference is unenforced because the recorder inserts a batch in one \
         statement and a foreign key would let one bad row reject the rest. That \
         trade is only sound while this check holds. A dangling reference means \
         either the episode write failed after its gate decided — in which case \
         the trace will render checkpoints for an artifact nobody can open — or an id \
         is being minted in two places.\n",
        dangling.len(),
        dangling
            .iter()
            .map(|(g, e)| format!("{g}: {e}"))
            .collect::<Vec<_>>()
            .join("\n    ")
    );
}

/// What the ledger actually holds, per gate.
///
/// Reported, never asserted. `grounding` was promoted to `Recorded` by migration
/// 221 and this is where its arrival becomes visible — but a count is a fact about
/// traffic since the last deploy, and the counters are process-local, so a
/// threshold here would fire on a quiet afternoon.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn report_which_gates_have_recorded_anything() {
    let pool = pool().await;
    if !ready(&pool, "report_which_gates_have_recorded_anything").await {
        return;
    }

    let rows = sqlx::query(
        "SELECT gate::text AS gate, decision::text AS decision, \
                count(*)::bigint AS n, \
                count(episode_id)::bigint AS with_episode \
           FROM gate_decisions \
          GROUP BY 1, 2 ORDER BY 1, 2",
    )
    .fetch_all(&pool)
    .await
    .expect("read gate_decisions");

    if rows.is_empty() {
        println!(
            "\n  `gate_decisions` is empty. Two `Recorded` gates existed before \
             migration 221 and neither fires on the execute path: `coherence` \
             gates an AgentWide correction, `admission` gates publish. \
             `grounding` is the first per-episode gate to record, so this table \
             fills from the next execute onward.\n"
        );
        return;
    }

    println!("\n  gate_decisions, by gate and verdict:");
    for r in &rows {
        let gate: String = r.try_get("gate").unwrap_or_default();
        let decision: String = r.try_get("decision").unwrap_or_default();
        let n: i64 = r.try_get("n").unwrap_or(0);
        let with: i64 = r.try_get("with_episode").unwrap_or(0);
        println!("    {gate:<16} {decision:<14} {n:>6}   {with:>6} name an episode");
    }
    println!(
        "\n  A NULL `episode_id` is correct and final for `credit` and \
         `rate_limit`: they decide whether to run at all, before any artifact \
         exists and possibly instead of one. It is a gap only for a gate that \
         fires after the output.\n"
    );
}
