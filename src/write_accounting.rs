//! Was the write **attempted**?
//!
//! The rung beneath liveness, and the one that makes the other five findable.
//!
//! # Why this exists
//!
//! This codebase has a deliberate and correct doctrine: an audit or
//! observability write must never fail the request that occasioned it. So the
//! writes are spawned, or `let _ =`'d, and their errors go to `tracing::warn!`.
//!
//! The doctrine has a missing half. **Nowhere do those failures accumulate.**
//! An audit of the loop and gate system found 30 such sites across 15 of the 22
//! feedback-loop sinks, and no counter, dead-letter table, retry queue or
//! metrics crate anywhere in the repository. Every failure path in the system
//! terminates at a log line nobody reads.
//!
//! That is not merely untidy. It is the mechanism by which the other defects
//! stay hidden, and it has now been demonstrated twice on one statement:
//!
//! | | defect | why it was invisible |
//! |---|---|---|
//! | 1 | Loop 2's grounding anomaly wrote `severity = "L1"` against a CHECK of `('info','warning','critical')` | spawned; error logged |
//! | 2 | the same INSERT references an `episode_id` whose row is not written until 97 lines later, and `anomaly_events.episode_id` is a foreign key | spawned; error logged |
//!
//! The first was found and fixed. The fix did not make the write succeed,
//! because the second was underneath it, and nothing said so. Fixing instances
//! one at a time has twice produced a new instance of the same class in the same
//! code. So the remedy is not another instance fix — it is to make the class
//! observable.
//!
//! # What liveness cannot see, and this can
//!
//! [`crate::liveness_trust`] compares a sink's row count against an opportunity
//! count, which turns an ambiguous `count(*) = 0` into `Ok` / `Silent` / `Inert`.
//! That is the right question and it is asked hours later, from outside. It
//! cannot distinguish:
//!
//! * the writer never ran, from
//! * the writer ran 340 times and the database refused every row.
//!
//! Both are `Silent`. They have completely different remedies — one is a
//! missing scheduler, the other is a rejected statement — and the difference is
//! only visible at the moment of the write. This module stands at that moment
//! and counts.
//!
//! # Why it is in memory, and why that is not a compromise
//!
//! A failure ledger that is itself a fallible database write has exactly the
//! property it exists to detect. When the database refuses the anomaly it will
//! also refuse the record of the refusal, and the ledger will be most silent
//! precisely when it is most needed.
//!
//! So the primary record is a set of atomic counters. They cannot fail, they
//! cannot recurse, and they need no migration. Their weakness is that a restart
//! clears them — which matters for a trend and not at all for the question this
//! module is built to answer, because *"this path has been attempted 340 times
//! today and has never once succeeded"* is a complete diagnosis on its own.
//!
//! Durability comes later and from the other direction: the liveness sweeper
//! already runs hourly and can flush a snapshot. A failed flush is itself
//! counted here, in memory, where it cannot be lost.
//!
//! # The forcing function
//!
//! Per `verification_for_agent_ecologies.md` §4.1, a check with nothing waiting
//! on it is indistinguishable from one that has stopped. This module has two
//! things waiting on it:
//!
//! 1. [`crate::liveness_trust`] reads it, so a `Silent` sink now reports
//!    *whether anybody tried* — and a `Silent` sink with **no accounting at
//!    all** is itself a finding, because it means the writer is not
//!    instrumented and its failures are still going nowhere.
//! 2. `tests/write_accounting_coverage.rs` fences the swallow patterns, so a
//!    new `let _ = <write>` on a declared sink fails the build.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// A sink written by at least one non-fatal path.
///
/// An enum rather than a string so a typo is a compile error. The discriminant
/// is the index into the counter arrays; `sinks_are_indexed_by_discriminant`
/// holds that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Sink {
    AnomalyEvents = 0,
    AgentTimelineEntries = 1,
    EvalSignals = 2,
    EvalRuns = 3,
    DyadState = 4,
    AgentObservabilityState = 5,
    Episodes = 6,
    SemanticRules = 7,
    ConsolidationJobs = 8,
    ForecastAgentClaims = 9,
    ForecastAttributions = 10,
    ProcessProjectionCommits = 11,
    ProcessSpacetime = 12,
    CoherenceEvaluations = 13,
    SchemaMigrations = 14,
    GateDecisions = 15,
    AssertionVerifications = 16,
    WorkspaceIntentions = 17,
}

/// What a sink is, who writes it non-fatally, and why that is allowed.
#[derive(Debug, Clone, Copy)]
pub struct SinkSpec {
    pub sink: Sink,
    /// The table, as a reader would name it. Checked to exist by the live tier.
    pub table: &'static str,
    /// The non-fatal writer(s). Named so a failure count points at a file.
    pub writer: &'static str,
    /// **Why this write is allowed to be swallowed at all.**
    ///
    /// Required, and the most important field. "Non-fatal" is a decision with a
    /// cost, and a site that cannot state what it is buying should propagate the
    /// error instead. Several sites in this codebase swallow a failure for no
    /// reason beyond the shape of the surrounding code.
    pub why_nonfatal: &'static str,
}

/// Every sink with a non-fatal writer, from the audit of 2026-08-22.
///
/// Rule for adding one: **if a write can fail without the caller finding out,
/// it belongs here.** The list is expected to grow as instrumentation reaches
/// further; it should shrink only when a site starts propagating its error.
pub const SINKS: &[SinkSpec] = &[
    SinkSpec {
        sink: Sink::AnomalyEvents,
        table: "anomaly_events",
        writer: "handlers::execution (grounding violation), \
                 observability::AnomalyDetector::persist",
        why_nonfatal: "An audit write must never fail the request it is auditing. \
                       This is the site where that principle hid two consecutive \
                       silent rejections, so it is also the reason this module \
                       exists.",
    },
    SinkSpec {
        sink: Sink::AgentTimelineEntries,
        table: "agent_timeline_entries",
        writer: "handlers::live_observability::record_live_observation",
        why_nonfatal: "Scoring a completed turn must not delay or fail the user's \
                       response; the entry is written after the reply is sent.",
    },
    SinkSpec {
        sink: Sink::EvalSignals,
        table: "eval_signals",
        writer: "handlers::eval::run_eval_cases, handlers::consolidation, \
                 handlers::forecasts::record_forecast_calibration_signals",
        why_nonfatal: "A signal is a measurement of a run, not part of it. \
                       Failing the run because its scorecard could not be filed \
                       would lose the run as well as the score.",
    },
    SinkSpec {
        sink: Sink::EvalRuns,
        table: "eval_runs",
        writer: "handlers::eval (completion UPDATE)",
        why_nonfatal: "The run already happened; the terminal UPDATE only records \
                       that it finished. NOTE: a swallowed failure here leaves the \
                       row `running` for ever, which no contract currently detects.",
    },
    SinkSpec {
        sink: Sink::DyadState,
        table: "dyad_state",
        writer: "api_server::spawn_dyad_observation",
        why_nonfatal: "Relationship tracking is observational and must not sit on \
                       the reply path.",
    },
    SinkSpec {
        sink: Sink::AgentObservabilityState,
        table: "agent_observability_state",
        writer: "observability::ObservabilityWorker::scan_agent (checkpoint)",
        why_nonfatal: "The sweeper is a background pass with no caller waiting. \
                       NOTE: a swallowed failure stalls the checkpoint, so the \
                       same agent is re-selected and re-fails indefinitely.",
    },
    SinkSpec {
        sink: Sink::Episodes,
        table: "episodes",
        writer: "workspace::messages, rabble_workspace, observations, \
                 swarm_telemetry, consolidation (narrator), workflows::fork",
        why_nonfatal: "On the @-mention and auto-analyst paths the agent has \
                       already replied; the episode is the record, not the work. \
                       NOTE: three of these sites then write a timeline entry \
                       whose foreign key points at the episode that failed.",
    },
    SinkSpec {
        sink: Sink::SemanticRules,
        table: "semantic_rules",
        writer: "agent_backend::kg_context::record_rule_retrievals, \
                 workflows::fork",
        why_nonfatal: "Retrieval credit is spawned off the hot path so prompt \
                       assembly is not delayed by a bookkeeping UPDATE.",
    },
    SinkSpec {
        sink: Sink::ConsolidationJobs,
        table: "consolidation_jobs",
        writer: "handlers::consolidation (sweeper and API cycle)",
        why_nonfatal: "The API returns 202 before the cycle runs. NOTE: the \
                       terminal `failed` UPDATE is itself swallowed, so a failed \
                       cycle stays `running` and the Loop 1 cadence contract, \
                       which counts completions in a window, cannot see it.",
    },
    SinkSpec {
        sink: Sink::ForecastAgentClaims,
        table: "forecast_agent_claims",
        writer: "workspace::agent_params_hook::apply_agent_multipliers",
        why_nonfatal: "Spawned so the claim write cannot fail the agent run. Its \
                       own comment records that a lost claim is unrecoverable: \
                       claims cannot be reconstructed after the fact.",
    },
    SinkSpec {
        sink: Sink::ForecastAttributions,
        table: "forecast_attributions",
        writer: "handlers::attribution::spawn_attribution",
        why_nonfatal: "Attribution runs after a forecast resolves and must not \
                       fail the resolution.",
    },
    SinkSpec {
        sink: Sink::ProcessProjectionCommits,
        table: "process_projection_commits",
        writer: "projection_commit::commit_projection",
        why_nonfatal: "The commitment anchor must not fail the observation write \
                       that occasioned it. NOTE: the anchor cannot be honestly \
                       backfilled, so a swallowed failure permanently removes \
                       that projection from Loop 5.A.",
    },
    SinkSpec {
        sink: Sink::ProcessSpacetime,
        table: "process_spacetime",
        writer: "handlers::simops_benchmark::resolve_against_projection",
        why_nonfatal: "Resolution runs on observation ingest and must not fail it. \
                       NOTE: this site does not even log — it counts `is_ok()` \
                       and discards the error — and its two CHECK vocabularies \
                       are bare string literals.",
    },
    SinkSpec {
        sink: Sink::CoherenceEvaluations,
        table: "coherence_evaluations",
        writer: "workspace::messages (automatic every-N-messages evaluation)",
        why_nonfatal: "The evaluation is batched off the message-post path. NOTE: \
                       the automatic writer does not bind its error at all, while \
                       the on-demand twin propagates — so the trend is built from \
                       whichever of the two happens to succeed.",
    },
    SinkSpec {
        sink: Sink::SchemaMigrations,
        table: "schema_migrations",
        writer: "api_server::record_migration_attempt",
        why_nonfatal: "The ledger records boot-time migration attempts and must \
                       not be able to prevent a boot. It is the only table in the \
                       repository with failure columns of its own, and its own \
                       write failure is printed and forgotten.",
    },
    SinkSpec {
        sink: Sink::GateDecisions,
        table: "gate_decisions",
        writer: "gate_trust::spawn_gate_recorder, draining the in-memory queue",
        why_nonfatal: "A gate must not be able to fail because its audit trail \
                       cannot write — that would turn an observability outage \
                       into a refusal of service, which is the one failure mode \
                       worse than the missing record this table exists to fix. \
                       The cost is paid explicitly instead: the queue is \
                       bounded and every dropped decision is counted by \
                       `gate_trust::ledger_status`, so a recorder that cannot \
                       write is visible as a number rather than as silence. \
                       This is the rung composing in the right direction — the \
                       thing that watches the gates is watched by the thing \
                       that watches the writes.",
    },
    SinkSpec {
        sink: Sink::AssertionVerifications,
        table: "assertion_verifications",
        writer: "verification_queue::enqueue, from the execute boundary",
        why_nonfatal: "An agent must not fail to answer because the queue of \
                       things to check about its answer could not be written — \
                       that turns an observability outage into a refusal of \
                       service, the one failure mode worse than the missing \
                       record. The cost is real and specific: a lost enqueue is \
                       a claim nobody will ever check, and it is invisible \
                       precisely because a claim that was never queued looks \
                       exactly like one that needed no checking. This table has \
                       held zero rows since migration 205 and nothing could say \
                       whether that was an empty queue or a rejected write — the \
                       `severity = 'L1'` shape — so the attempts are counted \
                       here and `Enqueued` carries the three reasons a queue \
                       stays empty apart.",
    },
    SinkSpec {
        sink: Sink::WorkspaceIntentions,
        table: "workspace_intentions",
        writer: "plan_solicitation::solicit, from the coherence shelf's Stage 0 floor",
        why_nonfatal: "A coherence evaluation must not fail because one member \
                       could not be asked what it plans to do. The floor exists \
                       to raise the average, and a floor that can take the whole \
                       endpoint down with it is worse than no floor. The cost is \
                       specific and was invisible until mig-218: a lost \
                       solicitation leaves that member's row absent, the \
                       strategist falls back to inferring it, and the resulting \
                       `inferred` row looks exactly like one from a workspace \
                       where nobody was ever asked. Counted here so the gap \
                       between `loop3.plans` and `loop3.intentions` can be read \
                       as a coordination finding rather than a write failure.",
    },
];

impl Sink {
    pub fn spec(self) -> &'static SinkSpec {
        &SINKS[self as usize]
    }
    pub fn table(self) -> &'static str {
        self.spec().table
    }
}

// ── Counters ───────────────────────────────────────────────────────────
//
// Fixed-size arrays indexed by discriminant. No allocation, no locking on the
// success path, and nothing that can fail.

const N: usize = SINKS.len();

#[allow(clippy::declare_interior_mutable_const)]
const ZERO: AtomicU64 = AtomicU64::new(0);
static ATTEMPTS: [AtomicU64; N] = [ZERO; N];
static FAILURES: [AtomicU64; N] = [ZERO; N];

/// Last failure per sink. Behind a lock because it is a `String`, and only ever
/// taken on the failure path, which is by construction rare.
static LAST_ERROR: Mutex<Option<Box<[Option<LastError>; N]>>> = Mutex::new(None);

#[derive(Debug, Clone, serde::Serialize)]
pub struct LastError {
    pub at: String,
    pub message: String,
}

/// Record one attempt and its outcome.
///
/// Prefer [`observe`], which cannot be called with the wrong outcome.
pub fn record(sink: Sink, succeeded: bool, error: Option<&str>) {
    let i = sink as usize;
    ATTEMPTS[i].fetch_add(1, Ordering::Relaxed);
    if succeeded {
        return;
    }
    FAILURES[i].fetch_add(1, Ordering::Relaxed);

    if let Some(msg) = error {
        if let Ok(mut guard) = LAST_ERROR.lock() {
            let slot = guard.get_or_insert_with(|| Box::new([const { None }; N]));
            slot[i] = Some(LastError {
                at: chrono::Utc::now().to_rfc3339(),
                message: truncate(msg),
            });
        }
    }
}

/// Keep the whole error, minus a runaway. Postgres messages carry the failing
/// row, which is the useful part, so this is generous.
fn truncate(s: &str) -> String {
    const LIMIT: usize = 600;
    if s.len() <= LIMIT {
        return s.to_string();
    }
    let mut cut = LIMIT;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…", &s[..cut])
}

/// Count a write's outcome, log its failure, and hand the result back.
///
/// The replacement for `if let Err(e) = write.await { tracing::warn!(…) }`. It
/// is deliberately shorter than the pattern it replaces, because an
/// instrumentation API that costs the caller lines does not get adopted and
/// then the instrumentation is the thing that is missing.
///
/// Returns `Some(value)` on success and `None` on failure, so a caller that
/// wants to branch still can — and one that does not can `let _ =` it exactly
/// as before, with the difference that the failure is now counted.
pub fn observe<T, E: std::fmt::Display>(sink: Sink, result: Result<T, E>) -> Option<T> {
    match result {
        Ok(v) => {
            record(sink, true, None);
            Some(v)
        }
        Err(e) => {
            let msg = e.to_string();
            record(sink, false, Some(&msg));
            tracing::warn!(
                sink = sink.table(),
                writer = sink.spec().writer,
                error = %msg,
                "side_write_failed"
            );
            None
        }
    }
}

/// What has been attempted, and how much of it landed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SinkAccount {
    pub table: &'static str,
    pub writer: &'static str,
    pub attempts: u64,
    pub failures: u64,
    pub last_error: Option<LastError>,
}

impl SinkAccount {
    /// Attempted at least once and never succeeded.
    ///
    /// The verdict this module exists to make available. It is not a liveness
    /// verdict — liveness asks about rows, this asks about attempts — and the
    /// two together are what distinguish "nothing ran" from "everything ran and
    /// was refused".
    pub fn is_totally_rejected(&self) -> bool {
        self.attempts > 0 && self.failures == self.attempts
    }
}

/// Read the counters for one sink.
pub fn account(sink: Sink) -> SinkAccount {
    let i = sink as usize;
    let last_error = LAST_ERROR
        .lock()
        .ok()
        .and_then(|g| g.as_ref().and_then(|s| s[i].clone()));
    SinkAccount {
        table: sink.table(),
        writer: sink.spec().writer,
        attempts: ATTEMPTS[i].load(Ordering::Relaxed),
        failures: FAILURES[i].load(Ordering::Relaxed),
        last_error,
    }
}

/// Read every counter.
pub fn accounts() -> Vec<SinkAccount> {
    SINKS.iter().map(|s| account(s.sink)).collect()
}

/// The account for a table name, if that table is instrumented.
///
/// How [`crate::liveness_trust`] joins a contract to its attempt counts. Returns
/// `None` for an uninstrumented sink, and the caller must report that rather
/// than treat it as zero — an uninstrumented writer's failures are still going
/// nowhere, which is a finding and not a clean bill.
pub fn account_for_table(table: &str) -> Option<SinkAccount> {
    SINKS
        .iter()
        .find(|s| s.table == table)
        .map(|s| account(s.sink))
}

/// Reset every counter. **Tests only.**
#[cfg(test)]
fn reset() {
    for i in 0..N {
        ATTEMPTS[i].store(0, Ordering::Relaxed);
        FAILURES[i].store(0, Ordering::Relaxed);
    }
    if let Ok(mut g) = LAST_ERROR.lock() {
        *g = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The counters are process-global and `cargo test` runs in threads, so a
    /// test that resets them races every other test that reads them.
    ///
    /// Serialised rather than sharded across distinct sinks, because the
    /// sharding would be an invariant nobody could see: the next test added
    /// would reuse a sink and the failure would be intermittent. A flaky check
    /// is a check that gets deleted, and the deletion looks like cleanup.
    static SERIAL: Mutex<()> = Mutex::new(());

    /// Take the lock and clear the counters. Poisoning is ignored: a panicking
    /// test has already failed and must not cascade into the others.
    fn exclusive() -> std::sync::MutexGuard<'static, ()> {
        let g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        g
    }

    /// The counter arrays are indexed by discriminant, so the table must be in
    /// discriminant order. Getting this wrong would attribute every failure to
    /// the wrong sink — and it would still count, so the total would look right.
    #[test]
    fn sinks_are_indexed_by_discriminant() {
        for (i, spec) in SINKS.iter().enumerate() {
            assert_eq!(
                spec.sink as usize, i,
                "{} is declared at index {i} but its discriminant is {}",
                spec.table, spec.sink as usize
            );
        }
    }

    #[test]
    fn no_two_sinks_name_the_same_table() {
        let mut seen = std::collections::HashSet::new();
        for s in SINKS {
            assert!(seen.insert(s.table), "{} is declared twice", s.table);
        }
    }

    /// Swallowing a failure is a decision, and a decision that cannot be
    /// explained should not be made. This is the same discipline as
    /// `every_contract_names_a_writer_and_a_consequence` one rung up.
    #[test]
    fn every_sink_says_why_it_is_allowed_to_swallow() {
        for s in SINKS {
            assert!(
                s.why_nonfatal.len() > 60,
                "{}: a swallowed write with no stated reason should propagate \
                 its error instead",
                s.table
            );
            assert!(
                s.writer.contains("::"),
                "{}: `writer` must point at code, got `{}`",
                s.table,
                s.writer
            );
        }
    }

    #[test]
    fn a_sink_nobody_has_touched_is_not_a_rejected_one() {
        let _g = exclusive();
        let a = account(Sink::AnomalyEvents);
        assert_eq!(a.attempts, 0);
        // The distinction the whole module turns on: zero attempts is silence,
        // not rejection, and must not be reported as a failure.
        assert!(!a.is_totally_rejected());
    }

    #[test]
    fn a_path_that_always_fails_is_reported_as_such() {
        let _g = exclusive();
        for _ in 0..3 {
            let r: Result<(), String> = Err("violates check constraint".into());
            assert!(observe(Sink::AnomalyEvents, r).is_none());
        }
        let a = account(Sink::AnomalyEvents);
        assert_eq!((a.attempts, a.failures), (3, 3));
        assert!(a.is_totally_rejected());
        assert!(a.last_error.unwrap().message.contains("check constraint"));
    }

    #[test]
    fn one_success_means_the_path_is_not_totally_rejected() {
        let _g = exclusive();
        let _ = observe(Sink::EvalSignals, Err::<(), _>("boom"));
        let _ = observe(Sink::EvalSignals, Ok::<_, String>(()));
        let a = account(Sink::EvalSignals);
        assert_eq!((a.attempts, a.failures), (2, 1));
        // Liveness is binary about rows for the same reason: once it has worked
        // once, the path exists and the rate is a different question.
        assert!(!a.is_totally_rejected());
    }

    #[test]
    fn observe_hands_the_value_back() {
        let _g = exclusive();
        let got = observe(Sink::Episodes, Ok::<_, String>(42));
        assert_eq!(got, Some(42));
    }

    #[test]
    fn counters_do_not_bleed_between_sinks() {
        let _g = exclusive();
        let _ = observe(Sink::DyadState, Err::<(), _>("x"));
        assert_eq!(account(Sink::DyadState).failures, 1);
        assert_eq!(account(Sink::SemanticRules).failures, 0);
    }

    #[test]
    fn an_uninstrumented_table_is_none_and_not_zero() {
        // `hitl_actions` propagates its errors, so it has no account. The caller
        // must be able to tell that apart from an instrumented sink at zero,
        // because the first is fine and the second is a path nobody has tried.
        assert!(account_for_table("hitl_actions").is_none());
        assert!(account_for_table("anomaly_events").is_some());
    }

    #[test]
    fn a_long_error_is_truncated_on_a_char_boundary() {
        let s = "é".repeat(1000);
        let t = truncate(&s);
        assert!(t.len() <= 604);
        assert!(t.ends_with('…'));
    }
}
