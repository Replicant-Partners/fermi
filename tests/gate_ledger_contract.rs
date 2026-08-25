//! Does a gate decision actually reach `gate_decisions`?
//!
//! ```text
//! cargo test --test gate_ledger_contract -- --ignored --nocapture
//! ```
//!
//! # Why this test rather than waiting for traffic
//!
//! `liveness_trust` exists because a declared write path that has never executed
//! is indistinguishable from an unused one. Migration 214 created the table and
//! `spawn_gate_recorder` drains into it — and the two Recorded gates (coherence,
//! admission) are only asked when somebody publishes an agent or intervenes on
//! an anomaly. Loop 2 has never turned, so on this deployment that may be never.
//!
//! Waiting would leave the ledger's first real use as its first test, which is
//! the position `DESIGN_gates_as_a_service.md` §1 warns about from the other
//! direction: *a gate whose first user is a paying stranger has not been
//! operated.* So the write path gets a positive control.
//!
//! # It cleans up after itself
//!
//! This runs against whatever `DATABASE_URL` points at, which is production. It
//! writes rows with a recognisable subject and deletes exactly those rows. It
//! asserts the delete count, because a self-cleaning test that silently fails to
//! clean is worse than one that leaves a mess it announced.

use fermi::gate_trust::{self, Decision, Gate};
use sqlx::PgPool;

const SUBJECT: &str = "gate_ledger_contract::positive_control";

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect")
}

#[tokio::test]
#[ignore = "needs DATABASE_URL; writes and deletes its own rows"]
async fn a_recorded_decision_reaches_the_table() {
    let pool = pool().await;

    // Clear anything a previous interrupted run left behind, so the assertion
    // below is about this run.
    sqlx::query("DELETE FROM gate_decisions WHERE subject = $1")
        .bind(SUBJECT)
        .execute(&pool)
        .await
        .expect("pre-clean");

    // Drain whatever real traffic has queued, so the batch under test is ours
    // and we are not stealing another process's decisions.
    let _ = gate_trust::flush(&pool).await;

    // A refusal and an approval, on the two Recorded gates.
    gate_trust::decided_about(
        Gate::Coherence,
        Decision::Refused,
        Some("positive control — the world model rejected it"),
        Some(SUBJECT),
    );
    gate_trust::decided_about(Gate::Admission, Decision::Approved, None, Some(SUBJECT));

    // And one Counted-tier decision, which must NOT appear.
    gate_trust::decided_about(
        Gate::RateLimit,
        Decision::Refused,
        Some("positive control — counted tier"),
        Some(SUBJECT),
    );

    let pending = gate_trust::ledger_status().pending;
    assert!(
        pending >= 2,
        "two Recorded decisions were made and {pending} are queued"
    );

    let landed = gate_trust::flush(&pool).await;
    assert!(
        landed >= 2,
        "flush reported {landed} rows landed, expected >= 2"
    );

    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT gate, decision, reason FROM gate_decisions
          WHERE subject = $1 ORDER BY gate",
    )
    .bind(SUBJECT)
    .fetch_all(&pool)
    .await
    .expect("read back");

    println!("\n  gate_decisions rows written by this test:");
    for (g, d, r) in &rows {
        println!("    {g:<12} {d:<12} {}", r.as_deref().unwrap_or("—"));
    }
    println!();

    let gates: Vec<&str> = rows.iter().map(|(g, _, _)| g.as_str()).collect();
    assert_eq!(
        gates,
        vec!["admission", "coherence"],
        "the Recorded gates landed and the Counted one did not"
    );

    let admission = &rows[0];
    assert_eq!(admission.1, "approved");
    assert_eq!(
        admission.2, None,
        "an approval carries no reason: one per pass would make the table noise"
    );

    let coherence = &rows[1];
    assert_eq!(coherence.1, "refused");
    assert!(
        coherence.2.as_deref().unwrap_or("").contains("world model"),
        "a refusal must carry why"
    );

    assert_eq!(
        gate_trust::ledger_status().dropped,
        0,
        "nothing may be dropped on a queue this small"
    );

    // Clean up, and assert the cleanup.
    let deleted = sqlx::query("DELETE FROM gate_decisions WHERE subject = $1")
        .bind(SUBJECT)
        .execute(&pool)
        .await
        .expect("clean")
        .rows_affected();
    assert_eq!(
        deleted as usize,
        rows.len(),
        "the test left rows behind in a production table"
    );
}

/// The vocabulary the column accepts must be the vocabulary Rust writes.
///
/// `seam_vocabulary` asserts this against the CHECK constraint. This asserts it
/// the third way, which is the one with no substitute: by writing every token
/// and seeing whether the database takes it.
#[tokio::test]
#[ignore = "needs DATABASE_URL; writes and deletes its own rows"]
async fn every_declared_decision_token_is_accepted() {
    let pool = pool().await;
    let subject = "gate_ledger_contract::vocabulary";

    sqlx::query("DELETE FROM gate_decisions WHERE subject = $1")
        .bind(subject)
        .execute(&pool)
        .await
        .expect("pre-clean");

    let mut rejected = Vec::new();
    for token in gate_trust::DECISIONS {
        let r = sqlx::query(
            "INSERT INTO gate_decisions (gate, decision, subject) VALUES ('coherence', $1, $2)",
        )
        .bind(token)
        .bind(subject)
        .execute(&pool)
        .await;
        if let Err(e) = r {
            rejected.push(format!("{token}: {e}"));
        }
    }

    let deleted = sqlx::query("DELETE FROM gate_decisions WHERE subject = $1")
        .bind(subject)
        .execute(&pool)
        .await
        .expect("clean")
        .rows_affected();

    assert!(
        rejected.is_empty(),
        "the column rejects {} token(s) that `gate_trust::DECISIONS` declares. This \
         is the `severity = 'L1'` shape: Rust holds one vocabulary, the CHECK holds \
         another, and the write is swallowed.\n  {}",
        rejected.len(),
        rejected.join("\n  ")
    );
    assert_eq!(deleted as usize, gate_trust::DECISIONS.len());
}

/// Every gate id must be writable, or the gate's decisions vanish.
#[tokio::test]
#[ignore = "needs DATABASE_URL; writes and deletes its own rows"]
async fn every_declared_gate_id_is_accepted() {
    let pool = pool().await;
    let subject = "gate_ledger_contract::gates";

    sqlx::query("DELETE FROM gate_decisions WHERE subject = $1")
        .bind(subject)
        .execute(&pool)
        .await
        .expect("pre-clean");

    let mut rejected = Vec::new();
    for id in gate_trust::GATE_IDS {
        let r = sqlx::query(
            "INSERT INTO gate_decisions (gate, decision, subject) VALUES ($1, 'refused', $2)",
        )
        .bind(id)
        .bind(subject)
        .execute(&pool)
        .await;
        if let Err(e) = r {
            rejected.push(format!("{id}: {e}"));
        }
    }

    let deleted = sqlx::query("DELETE FROM gate_decisions WHERE subject = $1")
        .bind(subject)
        .execute(&pool)
        .await
        .expect("clean")
        .rows_affected();

    assert!(
        rejected.is_empty(),
        "adding a gate to `gate_trust::GATES` without widening the CHECK makes \
         every decision by that gate unwritable, in a batch insert whose error is \
         swallowed by design:\n  {}",
        rejected.join("\n  ")
    );
    assert_eq!(deleted as usize, gate_trust::GATE_IDS.len());
}
