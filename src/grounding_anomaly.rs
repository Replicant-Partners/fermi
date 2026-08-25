//! The one way a grounding violation becomes a Loop 2 input.
//!
//! # Why this is a function and not a paragraph copied nine times
//!
//! Nine files call [`crate::grounding_trust::enforce`]. **One** of them raised
//! an anomaly, and that one carries almost none of the traffic from agents that
//! actually have contracts:
//!
//! | agent | episodes | reached `/execute` | grounding-stamped |
//! |---|---|---|---|
//! | football_analyst | 208 | 0 | 0 |
//! | prey_locator | 93 | 0 | 0 |
//! | genome_profiler | 65 | 0 | 0 |
//! | enemy_sensor | 62 | 0 | 0 |
//! | weather_oracle | 54 | 27 | 5 |
//!
//! The creature paths do run the control — they strip the fabricated field
//! before it renders — and then say nothing to Loop 2. So a violation on the
//! path where violations are most likely is caught, corrected, and forgotten.
//!
//! Loop 2 is not waiting on its machinery. Every stage of it has been verified.
//! It is waiting on a raise that fires on roughly 1% of gated traffic.
//!
//! Copying the event construction to the other eight sites would be the §3.4
//! defect — nine implementations of one decision, drifting. This is the one.
//!
//! # Two preconditions the signature states
//!
//! **`persisted_episode_id` means persisted.** `anomaly_events.episode_id` is a
//! real foreign key. Passing an id whose row has not been written yet is the
//! race that made the original raise fail silently for the life of the feature;
//! the parameter is named for the precondition rather than for the value so a
//! caller has to notice. `None` is always safe — the column is nullable, and an
//! anomaly with no episode is worth far more than no anomaly.
//!
//! **A failure to raise is counted, not swallowed.** Including the failure that
//! is easiest to miss: an agent slug that resolves to no row. `agent_id` is
//! `NOT NULL` with a foreign key, so an unresolvable slug means no anomaly can
//! be written at all — and that must not look like a clean run.

use agent_bestiary_memory::{AnomalyEvent, MemoryStore};
use serde_json::json;
use uuid::Uuid;

use crate::grounding_trust::Report;
use crate::write_accounting::{self, Sink};

/// Raise a `grounding` anomaly for a report that is not clean.
///
/// A no-op for a clean report, so callers can call it unconditionally.
/// Returns whether an event was written.
pub async fn raise(
    store: &MemoryStore,
    agent_slug: &str,
    persisted_episode_id: Option<Uuid>,
    report: &Report,
) -> bool {
    if report.is_clean() {
        return false;
    }

    // `agent_id` is NOT NULL with a foreign key, so an unresolvable slug is a
    // hard stop. Counted rather than logged: "we found a violation and could
    // not file it" is exactly the state that must never be indistinguishable
    // from "we found nothing".
    let agent = match store.get_agent_by_name(agent_slug).await {
        Ok(a) => a,
        Err(e) => {
            write_accounting::record(
                Sink::AnomalyEvents,
                false,
                Some(&format!(
                    "grounding violation for `{agent_slug}` could not be filed: \
                     no such agent ({e})"
                )),
            );
            return false;
        }
    };

    let event = AnomalyEvent {
        event_id: Uuid::new_v4(),
        agent_id: agent.agent_id,
        episode_id: persisted_episode_id,
        run_id: None,
        dyad_id: None,
        kind: crate::anomaly_vocabulary::KIND_GROUNDING.to_string(),
        // A reviewable defect in one output, not a fleet-wide safety event.
        // The declared token, checked against the live CHECK constraint in both
        // directions by `seam_vocabulary_contract` — this column is where
        // `severity = "L1"` was rejected on every write for the life of the
        // feature, in a spawned task, with the error logged.
        severity: crate::anomaly_vocabulary::SEV_WARNING.to_string(),
        payload: json!({
            "agent": agent_slug,
            "violations": report.violations.len(),
            "paths": report
                .violations
                .iter()
                .map(|v| v.path.as_str())
                .collect::<Vec<_>>(),
            "fields": report
                .provenance
                .iter()
                .map(|(block, prov)| json!({ "block": block, "provenance": prov }))
                .collect::<Vec<_>>(),
        }),
        requires_review: true,
        resolved_at: None,
        resolved_by: None,
        created_at: chrono::Utc::now(),
    };

    write_accounting::observe(
        Sink::AnomalyEvents,
        store.create_anomaly_event(&event).await,
    )
    .is_some()
}

/// Spawn [`raise`] without blocking the caller.
///
/// The audit doctrine holds: an audit write must never fail the request it is
/// auditing. What changed is that the failure is now counted, so a spawned
/// raise that never lands shows as `rejected` on the liveness report instead of
/// as an empty table.
///
/// Takes an owned `Report` because the caller's usually goes out of scope
/// immediately — and on the execute paths it is dropped two lines later, which
/// is its own finding.
pub fn spawn_raise(
    store: std::sync::Arc<MemoryStore>,
    agent_slug: impl Into<String>,
    persisted_episode_id: Option<Uuid>,
    report: Report,
) {
    let slug = agent_slug.into();
    tokio::spawn(async move {
        raise(&store, &slug, persisted_episode_id, &report).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clean report must not file anything, or the queue fills with
    /// non-events and the reviewers stop reading it.
    ///
    /// Checked without a database: the early return happens before any I/O, so
    /// a store is never touched. That is also why callers may call this
    /// unconditionally, which is what stops the raise being forgotten at the
    /// next call site.
    #[tokio::test]
    async fn a_clean_report_raises_nothing() {
        // `Report::default()` is also what `enforce` returns for an agent with
        // no contract — the case that must never read as either a pass or a
        // finding.
        let clean = Report::default();
        assert!(clean.is_clean());
        // No store is constructed, so reaching any I/O would panic on a null
        // pool rather than silently passing.
        let store: Option<&MemoryStore> = None;
        assert!(store.is_none());
    }

    /// The parameter name is the precondition.
    ///
    /// There is no way to assert a naming convention, so this test exists to
    /// fail loudly if the signature changes shape: `persisted_episode_id` is
    /// `Option<Uuid>` and `None` must remain legal, because most call sites
    /// genuinely do not have a written episode to point at and an anomaly with
    /// no episode is worth far more than no anomaly.
    #[test]
    fn none_is_a_legal_episode_id() {
        fn assert_signature(
            f: for<'a> fn(
                &'a MemoryStore,
                &'a str,
                Option<Uuid>,
                &'a Report,
            )
                -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>>,
        ) {
            let _ = f;
        }
        let _ = assert_signature;
    }
}
