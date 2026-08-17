//! Does the write path ever actually run?
//!
//! The fifth trust contract, and the one that would have caught the other four.
//!
//! # Why it exists
//!
//! Every contract before this one examines data that **exists**:
//!
//! * [`crate::schema_trust`] — does the column exist?
//! * [`crate::rollup_trust`] — is the cached value true?
//! * [`crate::grounding_trust`] — could the value have been known?
//! * [`crate::port_trust`] — is the caller sending what the agent takes?
//!
//! None of them can see a table that is empty because nothing ever wrote to
//! it. That blind spot produced five separate findings in a single afternoon:
//!
//! | thing | state when found |
//! |---|---|
//! | `credit_ledger_tx_type_check` | declared by 17 migrations, applied by none |
//! | provenance oracle | three construction sites, one wired |
//! | `forecast_agent_claims` | coded, wired, commented in detail, 0 rows |
//! | `anomaly_events` | CHECK extended for a new kind, 0 rows ever |
//! | `semantic_rules.application_count` | declared migration 010, 0 rows > 0 |
//!
//! All five have the same shape: **a declared write path that has never
//! executed.** Reading the code proves nothing, because in every case the code
//! is correct-looking and often carefully documented. `forecast_agent_claims`
//! has the most thorough comments in the repository and has never held a row.
//!
//! # Why nobody writes this check
//!
//! Because `count(*) = 0` is ambiguous. *Unused* and *broken* look identical
//! from the outside, so the check appears to be unactionable and gets skipped.
//!
//! The disambiguator is the **opportunity count**: how many times the path
//! *should* have fired. Zero claims alongside 14 multiplier-bearing episodes is
//! a broken path. Zero claims alongside zero opportunities is an unused one.
//! Same sink, same count, opposite meanings — and only the second is fine.
//!
//! # Liveness is binary, deliberately
//!
//! This module asks only whether a path has **ever** succeeded. It does not
//! ask whether the rate is right. Once a sink holds one row the path works, and
//! whether it fires often enough is a calibration question with a different
//! remedy and a different owner. Keeping that out is what stops this from
//! becoming a vague "does this number look plausible" check that nobody can act
//! on — the shape of check that gets ignored and then deleted.

/// How often a writer is supposed to fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expectation {
    /// Every opportunity should produce a row. An empty sink with
    /// opportunities on the clock is a broken path.
    EveryOpportunity,
    /// The writer fires only when it detects something, so an empty sink may
    /// be perfectly correct — there may simply have been no anomalies.
    ///
    /// Row counts cannot settle this, so a `Conditional` sink is **reported
    /// and never asserted**. What must be asserted instead is that the
    /// detector is *able* to fire, which is a different test entirely: the
    /// `the_taxonomy_cross_check_can_actually_fail` pattern, where you probe
    /// for the inverse condition and confirm the comparison is live rather
    /// than inert.
    Conditional,
}

/// One sink, and the evidence that its writer runs.
#[derive(Debug, Clone, Copy)]
pub struct LivenessContract {
    /// Sink being watched, as a reader would name it.
    pub sink: &'static str,
    /// The code that is supposed to write it. Named so that a `Silent` verdict
    /// points at a file instead of starting an investigation.
    pub writer: &'static str,
    /// Read-only query returning one row, one `bigint` column `writes`: how
    /// many times the path has succeeded.
    pub sink_sql: &'static str,
    /// Read-only query returning one row, one `bigint` column `opportunities`:
    /// how many times it should have fired.
    ///
    /// This is the whole design. Without it the check cannot tell unused from
    /// broken and is worth nothing.
    pub opportunity_sql: &'static str,
    pub expectation: Expectation,
    /// A `(table, column)` that must exist before the queries make sense.
    ///
    /// Contracts routinely race the deploy of the migration that creates their
    /// sink. Without this the query errors, and "the check could not run" would
    /// be indistinguishable from a finding — the `fermi_leaderboard` matview
    /// failure, where a probe that could never return healthy was ignored for
    /// eight releases.
    pub requires: Option<(&'static str, &'static str)>,
    /// What is lost while the path is silent. Not what the path does — what
    /// stops being knowable.
    pub why: &'static str,
    /// The next action, concretely. A finding with no remedy becomes furniture.
    pub remediation: &'static str,
}

/// What the live tier concluded about one contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Opportunities exist and the sink has rows. The path runs.
    Ok,
    /// Opportunities exist and the sink is empty.
    ///
    /// Named `Silent` rather than `Broken` on purpose: the cause may be a bug
    /// or may be a write path that is merely not deployed yet. Those have
    /// different remedies but the same consequence, which is that the signal
    /// does not exist, and that consequence is what deserves the name.
    Silent,
    /// No opportunities yet. The contract is watching and has proven nothing.
    ///
    /// Reported and counted, because a suite of entirely inert checks must not
    /// be able to present itself as a passing one.
    Inert,
    /// The sink's column does not exist yet. Distinct from every other status.
    NotDeployed,
    /// A query failed. Never a pass: an unrunnable check reports healthy for
    /// ever.
    Unrunnable,
}

/// Sinks known to be silent, with the reason.
///
/// The escape valve, deliberately shaped like [`crate::grounding_trust`]'s
/// cross-check exemptions: an entry must give a reason, and **the list may only
/// shrink**. The live tier additionally asserts that every entry is *still*
/// silent, so a stale one is flagged rather than becoming a standing excuse
/// nobody re-examines.
pub const KNOWN_SILENT: &[(&str, &str)] = &[(
    "semantic_rules.application_count",
    "The write (`kg_context::record_rule_retrievals`) is new, spawned off the \
     hot path, and has not yet been observed firing against this database. \
     1,829 episodes belong to agents holding retrievable rules, so the \
     opportunity is real and this is not merely unused. Until it fires, \
     `extraction_utility` cannot distinguish a rule the platform wanted back \
     from one nobody ever retrieved, which is the entire signal the \
     ontologist's own Loop 1 runs on. Remove this entry the first time any \
     rule reaches application_count > 0.",
)];

/// Is this sink a documented exception?
pub fn known_silent(sink: &str) -> Option<&'static str> {
    KNOWN_SILENT
        .iter()
        .find(|(s, _)| *s == sink)
        .map(|(_, why)| *why)
}

/// Every write path we have an opinion about.
///
/// Rule of thumb for adding one: **if a feature's value depends on a table
/// filling up, it belongs here.** A counter, a ledger, an event stream, an
/// audit trail. Anything whose absence is invisible because absence looks like
/// "nothing has happened yet".
pub const LIVENESS_CONTRACTS: &[LivenessContract] = &[
    LivenessContract {
        sink: "forecast_agent_claims",
        writer: "handlers::workspace::agent_params_hook::apply_agent_multipliers",
        sink_sql: "SELECT count(*)::bigint AS writes FROM forecast_agent_claims",
        // Both conditions are load-bearing. The hook is gated on a workspace
        // (`execution.rs`: `if let Some(ws_id) = ws_id_opt`) because
        // `forecast_agent_claims.workspace_id` is NOT NULL, so a standalone
        // evaluation genuinely cannot produce a claim and must not be counted
        // as a missed one. Counting bare multiplier lines instead would make
        // this contract permanently red for a reason the code is not able to
        // fix, and a permanently red check is one people learn to scroll past.
        //
        // The 14 multiplier lines produced OUTSIDE a workspace are a real
        // finding, but a different one: the platform discards an agent's
        // quantified output when there is no forecast to bind it to. That
        // belongs in the report, not in this assertion.
        opportunity_sql: "SELECT count(*)::bigint AS opportunities FROM episodes \
                           WHERE response_text ~ 'Suggested p50' \
                             AND context ->> 'workspace_id' IS NOT NULL",
        expectation: Expectation::EveryOpportunity,
        requires: None,
        why: "Without claims there is no input to the Shapley attribution engine, \
              so no forecast is attributable to the agents that moved it and no \
              agent has a track record. Every downstream idea that depends on \
              agent quality — recommendation, routing, pricing — rests on this \
              table, and it has been empty since it was created.",
        remediation: "Fix both breaks. (1) The `Suggested p50` regex cannot match \
                      the markdown the model actually emits: 8 of 14 lines carry \
                      `**1.15**` and `[\\d.]+` will not match an asterisk. (2) The \
                      binding is workspace-only, so standalone evaluations lose \
                      the output entirely — that needs the assertion layer, where \
                      the assertion is recorded per episode and a claim is an \
                      assertion bound to a driver.",
    },
    LivenessContract {
        sink: "semantic_rules.application_count",
        writer: "agent_backend::kg_context::record_rule_retrievals",
        sink_sql: "SELECT count(*)::bigint AS writes FROM semantic_rules \
                    WHERE application_count > 0",
        // An execution by an agent that holds retrievable rules is an
        // opportunity: `enrich_with_kg_context` runs on every execution and
        // credits whatever it retrieved. Requiring an embedding because
        // retrieval is embedding-based on both the ANN and fallback paths, so
        // an unembedded rule is invisible and could not have been credited.
        opportunity_sql: "SELECT count(*)::bigint AS opportunities FROM episodes e \
                           WHERE EXISTS (SELECT 1 FROM semantic_rules r \
                                          WHERE r.agent_id = e.agent_id \
                                            AND r.is_active \
                                            AND r.embedding IS NOT NULL)",
        expectation: Expectation::EveryOpportunity,
        requires: None,
        why: "`application_count` is how the platform knows whether a rule it \
              spent a dream cycle extracting was ever wanted back. While it is \
              zero, every rule looks equally useful, so the ontologist cannot be \
              scored on the only honest signal it has — whether its output got \
              retrieved — and Loop 1 for the extractor runs open-loop.",
        remediation: "Confirm `record_rule_retrievals` is deployed and that \
                      `enrich_with_kg_context` is reached on the execution paths \
                      those 52 rule-bearing agents actually run through. The \
                      update is spawned and its failure only logged, so check for \
                      `kg_retrieval_credit_failed`.",
    },
    // ── positive controls ───────────────────────────────────────────
    //
    // Two paths known to work, declared for a reason beyond completeness: a
    // suite with no known-good case cannot distinguish "every path is broken"
    // from "the runner is broken". Without these, `0 live` is ambiguous in
    // exactly the way this module exists to eliminate.
    //
    // The first also supplies the contrast that makes `anomaly_events`
    // diagnostic rather than merely worrying: the observability scanner
    // demonstrably runs and writes, and the detector downstream of it
    // demonstrably does not fire. One number cannot tell you that; two can.
    LivenessContract {
        sink: "agent_timeline_entries",
        writer: "handlers::live_observability::sweep_observability_once",
        sink_sql: "SELECT count(*)::bigint AS writes FROM agent_timeline_entries",
        opportunity_sql: "SELECT count(*)::bigint AS opportunities FROM episodes",
        expectation: Expectation::EveryOpportunity,
        requires: None,
        why: "The scanner behind every per-agent observability surface, including               the timeline the agent's owner reads. If it stopped, drift and               persona-version tracking would silently freeze at their last good               value rather than erroring, and the panels would keep rendering.",
        remediation: "Check the sweeper is scheduled and that                       `agent_observability_state.last_scan_completed_at` is                       advancing; a stalled sweeper leaves both tables readable                       and stale.",
    },
    LivenessContract {
        sink: "semantic_rules",
        writer: "agent_bestiary_memory::consolidation (dream cycles)",
        sink_sql: "SELECT count(*)::bigint AS writes FROM semantic_rules",
        opportunity_sql: "SELECT count(*)::bigint AS opportunities \
                            FROM episodes WHERE consolidated = true",
        expectation: Expectation::EveryOpportunity,
        requires: None,
        why: "Extraction is the write half of Loop 1, and the provenance floor is               computed here. If dream cycles stopped producing rules, the floor               would report a clean corpus for the most literal reason possible —               nothing new to grade — and the improvement would look like progress.",
        remediation: "Check `consolidation_jobs` for failures and confirm the                       dreaming budget has not been exhausted.",
    },
    LivenessContract {
        sink: "anomaly_events",
        writer: "handlers::live_observability + observability::anomaly",
        sink_sql: "SELECT count(*)::bigint AS writes FROM anomaly_events",
        opportunity_sql: "SELECT count(*)::bigint AS opportunities \
                            FROM agent_timeline_entries",
        // Conditional: a detector that finds nothing may be right. 1,275
        // scanned entries and zero events is suspicious but not proof, and
        // asserting on it would be asserting that anomalies must exist.
        expectation: Expectation::Conditional,
        requires: None,
        why: "`anomaly_events` is where drift, rupture, safety and grounding \
              findings surface for review. 1,275 timeline entries have been \
              scanned and it has never held a row. Either the platform has been \
              flawless or the detectors do not reach it — and the grounding kind \
              added in migration 200 has certainly never fired, because \
              `grounding_trust` violations are currently logged rather than \
              raised as events.",
        remediation: "Do not chase the row count. Write a firing probe per \
                      detector — feed it input it must flag and assert an event \
                      lands — the way the taxonomy cross-check proves it can go \
                      red before its zero is believed.",
    },
];

/// Contracts whose expectation is asserted rather than merely reported.
pub fn asserted() -> impl Iterator<Item = &'static LivenessContract> {
    LIVENESS_CONTRACTS
        .iter()
        .filter(|c| c.expectation == Expectation::EveryOpportunity)
}

/// Classify one contract from its two counts.
///
/// Split out from the runner so the decision table is unit-testable without a
/// database. The ordering matters: opportunities are checked before writes,
/// because a sink with rows but no current opportunities is still `Ok` — the
/// path demonstrably ran once, which is all liveness claims.
pub fn classify(writes: i64, opportunities: i64) -> Status {
    if writes > 0 {
        Status::Ok
    } else if opportunities > 0 {
        Status::Silent
    } else {
        Status::Inert
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mutating SQL in a contract would let a check alter the thing it audits.
    /// Same guard as `grounding_trust`'s cross-checks, for the same reason.
    #[test]
    fn every_query_is_read_only() {
        for c in LIVENESS_CONTRACTS {
            for (label, sql) in [("sink", c.sink_sql), ("opportunity", c.opportunity_sql)] {
                let head = sql.trim_start().to_ascii_uppercase();
                assert!(
                    head.starts_with("SELECT") || head.starts_with("WITH"),
                    "{}.{label} does not begin with SELECT or WITH",
                    c.sink
                );
                let upper = sql.to_ascii_uppercase();
                for bad in [
                    "INSERT ",
                    "UPDATE ",
                    "DELETE ",
                    "DROP ",
                    "ALTER ",
                    "TRUNCATE ",
                    "CREATE ",
                    "GRANT ",
                ] {
                    assert!(
                        !upper.contains(bad),
                        "{}.{label} contains `{}` — a contract must not be able \
                         to change what it measures",
                        c.sink,
                        bad.trim()
                    );
                }
            }
        }
    }

    /// Every query must return the column the runner reads by name, or the
    /// runner fails at decode time and the contract reports `Unrunnable`
    /// for a reason that has nothing to do with the platform.
    #[test]
    fn every_query_aliases_the_column_the_runner_reads() {
        for c in LIVENESS_CONTRACTS {
            assert!(
                c.sink_sql.contains("AS writes"),
                "{}: sink_sql must alias its count `AS writes`",
                c.sink
            );
            assert!(
                c.opportunity_sql.contains("AS opportunities"),
                "{}: opportunity_sql must alias its count `AS opportunities`",
                c.sink
            );
        }
    }

    /// A contract that cannot say what is lost, or what to do, is a to-do
    /// comment with a database connection.
    #[test]
    fn every_contract_names_a_writer_and_a_consequence() {
        assert!(
            !LIVENESS_CONTRACTS.is_empty(),
            "an empty contract list cannot fail"
        );
        for c in LIVENESS_CONTRACTS {
            assert!(!c.sink.is_empty(), "contract with no sink");
            assert!(
                c.writer.contains("::") || c.writer.contains('+'),
                "{}: `writer` must point at code, got `{}`",
                c.sink,
                c.writer
            );
            assert!(
                c.why.len() > 100,
                "{}: `why` must say what stops being knowable while the path is \
                 silent, not what the path does",
                c.sink
            );
            assert!(
                c.remediation.len() > 60,
                "{}: `remediation` must name the next action",
                c.sink
            );
        }
    }

    /// If every contract were `Conditional` the suite would assert nothing at
    /// all while looking complete — the exemptions-only failure that
    /// `the_empirical_tier_is_not_entirely_exemptions` guards against next
    /// door.
    #[test]
    fn at_least_one_contract_is_actually_asserted() {
        assert!(
            asserted().count() >= 1,
            "every contract is Conditional, so nothing is asserted and the \
             suite is decoration"
        );
    }

    /// The whole point of the module, as a decision table.
    #[test]
    fn zero_writes_means_broken_or_unused_depending_on_opportunity() {
        // The finding.
        assert_eq!(classify(0, 14), Status::Silent);
        // The same count, and nothing is wrong.
        assert_eq!(classify(0, 0), Status::Inert);
        // Ran at least once. Liveness claims nothing further.
        assert_eq!(classify(1, 14), Status::Ok);
        // Ran historically, no current opportunities. Still Ok: liveness is
        // about whether the path has ever worked, not about its rate.
        assert_eq!(classify(5, 0), Status::Ok);
    }

    /// `Inert` must not be spelled `Ok`. If it were, a contract watching a
    /// feature nobody has exercised would report healthy, which is how this
    /// entire class of bug survived in the first place.
    #[test]
    fn inert_is_not_ok() {
        assert_ne!(classify(0, 0), Status::Ok);
        assert_ne!(classify(0, 0), Status::Silent);
    }

    #[test]
    fn known_silent_entries_give_reasons() {
        for (sink, why) in KNOWN_SILENT {
            assert!(
                LIVENESS_CONTRACTS.iter().any(|c| c.sink == *sink),
                "KNOWN_SILENT names `{sink}`, which is not a declared contract"
            );
            assert!(
                why.len() > 80,
                "`{sink}` is excused without a real reason. The list may only \
                 shrink, so each entry has to justify itself to whoever tries \
                 to remove it."
            );
        }
    }
}
