//! Does the write path ever actually run?
//!
//! **Rung 2 of the verification ladder** (`crate::ladder`), and the one that
//! would have caught the other four.
//!
//! Written fifth, which is why this line used to read "the fifth trust
//! contract". That is a chronology and it is not the ladder: liveness sits
//! BENEATH truth, grounding and binding, because a fabricated value in a table
//! nothing writes is not a grounding problem — it is an empty table.
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

use crate::is_projection_sql;
use crate::write_accounting::{self, Sink, SinkAccount};

/// When the projection-commit call site began to exist.
///
/// Projections generated before this cannot be anchored: a commitment written
/// after the measurement it precedes proves nothing, so backfilling the 61
/// historical runs would manufacture exactly the evidence Loop 5.A exists to
/// make unmanufacturable. They are therefore not counted as missed
/// opportunities, and this rung stays INERT until the next projection arrives.
///
/// A macro rather than a `const` because [`LIVENESS_CONTRACTS`] is a `const`
/// built with `concat!`, which takes literals. Quoting the date in the query
/// instead would put the authoritative value somewhere no reader looks.
macro_rules! commit_hook_live_from {
    () => {
        "2026-08-22T00:00:00Z"
    };
}

/// The same instant, readable, and asserted against the query below.
pub const COMMIT_HOOK_LIVE_FROM: &str = commit_hook_live_from!();

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
    /// The write-accounting sink for this contract's writer, when the writer is
    /// instrumented.
    ///
    /// A typed link rather than a string match on [`Self::sink`], which is a
    /// human label and not a table name. Deriving one from the other would be a
    /// proxy assertion of exactly the kind this module exists to catch.
    ///
    /// `None` means the writer propagates its errors, so there is nothing to
    /// count — or that it swallows them and has not been instrumented yet,
    /// which is a different thing and is reported as such.
    pub accounted: Option<Sink>,
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
// Empty, and that is a result rather than an oversight.
//
// The list held one entry: `semantic_rules.application_count`, excused because
// `kg_context::record_rule_retrievals` was new, spawned off the hot path, and
// had never been observed firing. Its own reason set the condition for its
// removal — "remove this entry the first time any rule reaches
// application_count > 0" — and the first live run of this suite reported
// 27 writes against 2,092 opportunities. So it went.
//
// The mechanism is worth noting because it is the part that usually rots: the
// live tier asserts that every excused sink is *still* silent, so the exemption
// could not quietly outlive its reason. It failed the run that made it
// obsolete. An exemption list that can only shrink, and that checks its own
// entries against reality, is the difference between an escape valve and a
// standing permission nobody re-reads.
pub const KNOWN_SILENT: &[(&str, &str)] = &[];

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
        sink: "consolidation_jobs (Loop 1 cadence)",
        writer: "handlers::consolidation::spawn_consolidation_sweeper \
                 (every 6h, agent-funded), plus the HTTP handler on demand",
        // Completed in the last 7 days, not "ever". Loop 1's claim is a
        // *cadence*, so a single successful run in 2024 satisfies "has this
        // path ever executed" while telling you nothing about whether the loop
        // is turning. This is the one contract here where liveness's usual
        // binary reading is too generous, and the window is how it is narrowed
        // without turning the rung into a "does this number look plausible"
        // check.
        sink_sql: "SELECT count(*)::bigint AS writes FROM consolidation_jobs \
                    WHERE status = 'completed' AND completed_at > NOW() - INTERVAL '7 days'",
        // An agent sitting on unconsolidated episodes is an agent whose
        // dreaming cycle is overdue. Ten is the floor the handler itself needs
        // before clustering produces anything, so below that there is no
        // missed opportunity to report.
        opportunity_sql: "SELECT count(*)::bigint AS opportunities FROM ( \
                            SELECT agent_id FROM episodes \
                             WHERE NOT consolidated \
                             GROUP BY agent_id HAVING count(*) >= 10 \
                          ) overdue",
        expectation: Expectation::EveryOpportunity,
        requires: Some(("consolidation_jobs", "completed_at")),
        accounted: Some(Sink::ConsolidationJobs),
        why: "Loop 1 is the only path by which an agent's own experience changes \
              how it reasons — episodes cluster into semantic rules and \
              `kg_context` injects those rules into the next prompt. The \
              architecture states a cadence of hours to days, and for a long \
              time nothing implemented one: agents accumulated episodes and \
              learned nothing from them, which is invisible because an agent \
              that has not learned looks exactly like one that had nothing to \
              learn. A sweeper now runs every six hours, so this contract has \
              changed job: it no longer reports a missing scheduler, it reports \
              whether the scheduler that exists is actually turning.",
        remediation: "SILENT here means the sweeper is not producing completed \
                      jobs despite agents being overdue. Check, in order: \
                      CONSOLIDATION_SWEEP_SECS is not 0; an extraction model \
                      resolves (the sweep refuses to run degraded, by design, \
                      because a cycle with no extractor consumes its episodes \
                      and produces nothing while reporting success); and the \
                      overdue agents have `dreaming_budget_credits` left, since \
                      autonomous dreaming is agent-funded and an agent with an \
                      exhausted budget is skipped rather than charged. The third \
                      is the expected steady state, not a fault — which is why \
                      this contract counts completed jobs in a window rather \
                      than ever.",
    },
    // ── Loop 5.A (projection accuracy), as the three links it actually is ──
    //
    // This was ONE contract, and it read: 0 writes, **12,167 opportunities**.
    // Both numbers were right and the pairing was not. The opportunity query
    // counted `sosa_observations WHERE extra ? 'projection_id'` and its comment
    // said "a real observation carrying a projection_id is a batch that
    // completed against a prior projection". Measured: all 12,167 of those rows
    // are the *projections themselves* — 61 distinct runs sampled at ~200
    // trajectory points each — and **zero** are measurements. Not one
    // projection_id has both a projection and a measurement against it.
    //
    // So the rung whose entire job is to make `count(*) = 0` mean something
    // was itself asserting a proxy: it counted predictions and called them
    // chances to score. The remediation it printed named the trigger site at
    // the far end of the chain, and following it would have wired a trigger
    // that fires zero times, against a reader that selects the empty set,
    // for want of an anchor that no code writes. Three breaks, and the one
    // number could not distinguish them.
    //
    // Splitting the chain is the fix, and it is what the rest of this module
    // already claims to do: a Silent verdict should point at a file rather
    // than start an investigation. Each link below can break alone, and the
    // first one that is SILENT with a live link above it is the break.
    LivenessContract {
        sink: "process_projection_commits (Loop 5.A · 1. anchor)",
        writer: "projection_commit::commit_projection, from handlers::observations::ingest \
                 and agent_backend::simops_tools",
        sink_sql: "SELECT count(*)::bigint AS writes FROM process_projection_commits",
        // Distinct projection RUNS, not trajectory points: one commitment per
        // projected value, but a contract that compared a run count to a point
        // count would read as 60/12,167 the moment it started working.
        //
        // Bounded by `generated_at` because a commitment written after the
        // measurement proves nothing, so the 61 historical projections cannot
        // be honestly backfilled and must not be counted as missed. A row with
        // no `generated_at` is not counted either — the conservative direction,
        // and the rule this module states: never claim an opportunity that
        // cannot be shown to have happened.
        opportunity_sql: concat!(
            "SELECT count(DISTINCT extra->>'projection_id')::bigint AS opportunities \
               FROM sosa_observations \
              WHERE extra ? 'projection_id' AND ",
            is_projection_sql!(),
            "    AND (extra->>'generated_at')::timestamptz > TIMESTAMPTZ '",
            commit_hook_live_from!(),
            "'"
        ),
        expectation: Expectation::EveryOpportunity,
        requires: Some(("process_projection_commits", "commitment_hash")),
        accounted: Some(Sink::ProcessProjectionCommits),
        why: "The anchor is what makes Loop 5.A a verification rather than a \
              transcription: it records that a value was predicted BEFORE any \
              measurement of it existed. Without it a score is just a \
              comparison of two numbers with no established order, which is \
              precisely the claim Loop 5.A is supposed to be immune to. \
              Nothing downstream can run — resolution joins against this \
              table, and scoring joins against resolution.",
        remediation: "`commit_projection` had NO callers. The site that should \
                      have called it was a `let _ = (…every argument…)` in \
                      `simops_tools`, described in its own comment as 'hooks for \
                      an observability path that may or may not be live', and it \
                      was on the wrong path regardless: the agent tool has \
                      written zero observations and the projections arrive over \
                      HTTP. Both call it now. SILENT here means the ingest hook \
                      is not firing on rows the shared predicate calls \
                      projections — check `projection_commit_failed` in the logs \
                      before suspecting the predicate.",
    },
    LivenessContract {
        sink: "process_spacetime (Loop 5.A · 2. resolution)",
        writer: "handlers::simops_benchmark::resolve_against_projection, \
                 from handlers::observations::ingest",
        sink_sql: "SELECT count(*)::bigint AS writes FROM process_spacetime",
        // A measurement is an opportunity only if there is something committed
        // for it to resolve against. Counting every measurement would make this
        // red for the absence of the link ABOVE it, which is the mistake the
        // single 12,167 contract made.
        opportunity_sql: concat!(
            "SELECT count(*)::bigint AS opportunities FROM sosa_observations r \
              WHERE NOT ",
            is_projection_sql!("r"),
            "  AND EXISTS (SELECT 1 FROM process_projection_commits c \
                            WHERE c.observable_property = r.observable_property)"
        ),
        expectation: Expectation::EveryOpportunity,
        requires: Some(("process_spacetime", "accuracy_score")),
        accounted: Some(Sink::ProcessSpacetime),
        why: "One row per point where the physical world spoke back to the \
              model. This is the research artefact the SimOps benchmark exists \
              to produce, and the input the scoring evaluator reads.",
        remediation: "INERT here with the anchor live means no measurement has \
                      yet been taken of anything that was projected. That is a \
                      fact about the deployment, not a bug: today the \
                      projections cover thirteen `chem:`/`bio:` properties and \
                      the measurement stream covers fourteen entirely different \
                      ones, with a single overlapping row. Loop 5.A cannot \
                      close until something measures what something else \
                      predicted.",
    },
    LivenessContract {
        sink: "eval_signals.projection_accuracy (Loop 5.A · 3. scoring)",
        writer: "evaluators::ProjectionScoringEvaluator, via handlers::eval::run_eval_cases",
        sink_sql: "SELECT count(*)::bigint AS writes FROM eval_signals \
                    WHERE evaluator_name ILIKE '%projection%'",
        // A resolved pair IS the scoreable event, and it is the only thing that
        // is. This is the number the old contract should have used; it was
        // unavailable because nothing had ever written the table.
        opportunity_sql: "SELECT count(*)::bigint AS opportunities FROM process_spacetime",
        expectation: Expectation::EveryOpportunity,
        requires: Some(("eval_signals", "evaluator_name")),
        accounted: Some(Sink::EvalSignals),
        why: "The hard-verified half of calibration, and the one signal an agent \
              cannot talk its way out of. The reader is fully wired — \
              `calibration.rs` surfaces projection_accuracy and the observatory \
              displays it — so from a dashboard the loop has always looked \
              closed.",
        remediation: "Two things must be true and only one is now. (1) The \
                      lookup must recognise a projection: it matched only the \
                      agent-tool tag on `extra->>'source'`, which no row in the \
                      table has ever carried, and now uses the shared \
                      `projection_kind` predicate. (2) A real observation must \
                      TRIGGER the evaluator with the `projection_id` in the \
                      bundle context; nothing does that yet. Wire it from the \
                      resolution hook, where both observations are already in \
                      hand. The 30-day heuristic that used to cover a missing \
                      id is now off unless a caller asks for it by name, so a \
                      trigger wired without the link scores nothing rather than \
                      scoring the wrong projection.",
    },
    LivenessContract {
        sink: "forecast_agent_claims",
        writer: "handlers::workspace::agent_params_hook::apply_agent_multipliers",
        sink_sql: "SELECT count(*)::bigint AS writes FROM forecast_agent_claims",
        // This clause used to read `context ->> 'workspace_id' IS NOT NULL`,
        // and the exemption was correct at the time: the hook was gated on a
        // workspace because `forecast_agent_claims.workspace_id` was NOT
        // NULL, so a standalone evaluation genuinely could not produce a
        // claim and counting it as a missed one would have made this
        // contract permanently red for a reason the code could not fix.
        //
        // Migration 213 removed that constraint. A run bound to a
        // (forecast, driver) can now write a claim — and the forecast id is
        // the STRONGER binding, the one `load_agent_claims` already prefers.
        // So the exemption became the thing it was guarding against: an
        // opportunity query that excludes the console, which is the platform's
        // main producer of quantified judgements, would have kept this
        // contract green while every one of those judgements was dropped.
        // The narrow query said 0 opportunities and reported INERT — "nothing
        // has tried yet" — for a path that had discarded 61 of 61.
        //
        // Either binding now counts as an opportunity. A multiplier produced
        // with NEITHER still does not: there is nothing to attach it to, and
        // `forecast_agent_claims_has_binding` would reject the row.
        opportunity_sql: "SELECT count(*)::bigint AS opportunities FROM episodes \
                           WHERE response_text ~ 'Suggested p50' \
                             AND (context ->> 'workspace_id' IS NOT NULL \
                              OR context #>> '{invocation,forecast_id}' IS NOT NULL)",
        expectation: Expectation::EveryOpportunity,
        requires: None,
        accounted: Some(Sink::ForecastAgentClaims),
        why: "Without claims there is no input to the Shapley attribution engine, \
              so no forecast is attributable to the agents that moved it and no \
              agent has a track record. Every downstream idea that depends on \
              agent quality — recommendation, routing, pricing — rests on this \
              table, and it has been empty since it was created.",
        remediation: "Both original breaks are now fixed; if this is still red, \
                      the cause is downstream of them. (1) The `Suggested p50` \
                      regex could not match the markdown the model emits — \
                      `[\\d.]+` will not match the asterisk in `**1.15**`, losing \
                      12 of 22 lines. `assertions::MULTIPLIER_RE` (multiplier_v2) \
                      tolerates the emphasis and recovers 22 of 22. (2) The \
                      binding was workspace-only, so every console run lost its \
                      output; mig-213 makes `workspace_id` nullable and accepts a \
                      (forecast, driver) binding instead, and both execute routes \
                      now pass one through from `invocation`. What to check next, \
                      in order: does the console actually send `invocation.\
                      forecast_id` (it only can for a SAVED forecast — a draft has \
                      no id and correctly produces no claim); does the agent's \
                      response contain a `Suggested p50` line at all; and is \
                      `write_accounting` recording refusals against \
                      Sink::ForecastAgentClaims, which distinguishes 'never tried' \
                      from 'tried and rejected by the CHECK'.",
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
        accounted: Some(Sink::SemanticRules),
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
    LivenessContract {
        sink: "episodes.assertions",
        writer: "agent_backend::tool_executor (assertion capture at episode write)",
        sink_sql: "SELECT count(*)::bigint AS writes FROM episodes \
                    WHERE assertions IS NOT NULL",
        // An episode whose response quantified something is an opportunity.
        // Deliberately the LOOSE pattern: any `Suggested p50` line at all,
        // including the eight of fourteen the old regex could not parse because
        // the model wrote `**1.15**`. That is the point — the assertion layer
        // exists so a format failure is a recorded fact rather than a silent
        // discard, so a line the extractor gave up on still counts as an
        // opportunity it should have recorded something about.
        opportunity_sql: "SELECT count(*)::bigint AS opportunities FROM episodes \
                           WHERE response_text ~ 'Suggested p50'",
        expectation: Expectation::EveryOpportunity,
        requires: Some(("episodes", "assertions")),
        accounted: Some(Sink::Episodes),
        why: "Without assertions an agent evaluated outside a workspace leaves no \
              trace of what it quantified, so it can never accumulate a track \
              record and the recommendation problem has no data underneath it. \
              Measured before migration 205: 14 quantified judgements, 14 \
              discarded, 0 claims.",
        remediation: "Capture assertions at episode write, unconditionally — not \
                      gated on a workspace, which is the gate that caused the \
                      loss. `NULL` means pre-205 and `[]` means asserted nothing; \
                      writing `[]` is what makes this contract go live.",
    },
    LivenessContract {
        sink: "assertion_verifications",
        writer: "the verification queue (automated route + forecast owner review)",
        sink_sql: "SELECT count(*)::bigint AS writes FROM assertion_verifications",
        // Only an assertion that is actually pending is an opportunity. A
        // corpus of fully-verified assertions owes no verifications, and
        // counting all assertions would make this permanently red for a reason
        // nobody can fix.
        opportunity_sql: "SELECT count(*)::bigint AS opportunities \
                            FROM episodes e, \
                                 jsonb_array_elements(e.assertions) AS a \
                           WHERE e.assertions IS NOT NULL \
                             AND a ->> 'provenance' IN ('pending_tool_check', \
                                                        'pending_human_check')",
        expectation: Expectation::EveryOpportunity,
        requires: Some(("assertion_verifications", "verdict")),
        accounted: None,
        why: "A queue nobody works is indistinguishable from trusting everything \
              in it. While this is empty, every pending assertion is presented \
              with a badge and never resolved, which is the failure mode the \
              pending tier was supposed to replace — unverified data reading as \
              acceptable because it has been sitting there long enough.",
        remediation: "Run the automated route first: `Grounding::Sourced` already \
                      names the tool and response field, so those checks need no \
                      new declarations. Only what no tool can settle should reach \
                      a person.",
    },
    LivenessContract {
        sink: "schema_migrations",
        writer: "api_server::record_migration_attempt (called from run_migrations)",
        sink_sql: "SELECT count(*)::bigint AS writes FROM schema_migrations",
        // If the platform has served anything at all, it has booted, and
        // `run_migrations` runs on every boot. So any episode is evidence that
        // the recorder had its chance. Crude, and correct in the direction that
        // matters: it cannot claim an opportunity that did not happen.
        opportunity_sql: "SELECT count(*)::bigint AS opportunities FROM episodes",
        expectation: Expectation::EveryOpportunity,
        requires: Some(("schema_migrations", "filename")),
        accounted: Some(Sink::SchemaMigrations),
        why: "The ledger is what makes a failing migration answerable. Without it \
              `run_migrations` prints the failure and continues, which is how a \
              CHECK constraint was declared by seventeen migrations and applied by \
              none — the repair was performed, believed and repeated for the life \
              of the project. An empty ledger means that blindness is back, and \
              the irony is available: the check that watches the recorder is \
              itself a write path that could silently never run.",
        remediation: "Confirm the deployed binary includes `record_migration_attempt` \
                      and that `ensure_migration_ledger` is not erroring at boot — \
                      both log to stderr and neither can fail the boot, by design.",
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
        accounted: Some(Sink::AgentTimelineEntries),
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
        accounted: Some(Sink::SemanticRules),
        why: "Extraction is the write half of Loop 1, and the provenance floor is               computed here. If dream cycles stopped producing rules, the floor               would report a clean corpus for the most literal reason possible —               nothing new to grade — and the improvement would look like progress.",
        remediation: "Check `consolidation_jobs` for failures and confirm the                       dreaming budget has not been exhausted.",
    },
    LivenessContract {
        sink: "anomaly_events",
        writer: "handlers::live_observability + observability::anomaly",
        sink_sql: "SELECT count(*)::bigint AS writes FROM anomaly_events",
        opportunity_sql: "SELECT count(*)::bigint AS opportunities \
                            FROM agent_timeline_entries",
        // Conditional: a detector that finds nothing may be right. Asserting on
        // the row count would be asserting that anomalies must exist.
        //
        // That is still the correct expectation and it is no longer the whole
        // story. `tests/anomaly_firing_probe.rs` now establishes the part a
        // test can own — that an anomaly, if one occurred, would be recorded —
        // so the zero below is a finding about the world rather than an
        // unfalsifiable claim about the code.
        expectation: Expectation::Conditional,
        requires: None,
        accounted: Some(Sink::AnomalyEvents),
        why: "`anomaly_events` is where drift, rupture, safety and grounding \
              findings surface for review, and it is Loop 2's ONLY input: with \
              none, the HITL queue is empty, no reviewer intervenes, no \
              AgentWide correction is made, `bump_persona_version` never fires, \
              every agent stays at v1, and the drift detector skips every entry \
              it scans because drift at v1 is undefined. The loop requires its \
              own output as its input, so an empty table is not one missing \
              feature but a stalled cycle.",
        remediation: "Do not chase the row count — the probe exists now and \
                      answers the part that is answerable. What it established: \
                      (1) the seed committed to break the deadlock wrote \
                      `severity = 'L1'` against a CHECK of \
                      ('info','warning','critical'), so every grounding anomaly \
                      was REJECTED BY THE DATABASE in a spawned task with the \
                      error only logged — fixed; (2) 262 of 1,417 timeline \
                      entries carry a flag and every one is `social:observed`, \
                      which is bookkeeping that no detector matches, by design. \
                      So nothing actionable has ever been flagged and the zero \
                      is honest. The open question is upstream of this table: \
                      WildGuard has never returned a safety flag on live \
                      traffic. Confirm that by feeding it something it must \
                      flag, not by waiting.",
    },
];

/// Is a `Silent` verdict on this contract an actionable finding?
///
/// The one implementation. It was two: `sweep` pushed every silent sink into
/// `undocumented_silent`, and the runner in `tests/liveness_contract.rs`
/// additionally excused `Conditional` ones. So the library's report listed
/// `anomaly_events` as silent with no excuse while the script that runs the
/// same contracts reported `0 silent`.
///
/// Nothing noticed until `native_evaluators::UndocumentedSilence` read the
/// library's report rather than the test's, on its first production run. That
/// is §3.4 exactly — *a trust calculation must have exactly one
/// implementation* — and the copy that was believed was the one nearest the
/// reader.
pub fn is_actionable_silence(c: &LivenessContract) -> bool {
    // A `Conditional` writer fires only when it detects something, so an empty
    // sink may be perfectly correct; asserting on it would assert that
    // anomalies must exist.
    c.expectation != Expectation::Conditional && known_silent(c.sink).is_none()
}

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

impl Status {
    /// The wire/report name. One spelling, so the runner, the endpoint and the
    /// log cannot disagree about what a verdict is called.
    pub fn label(self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::Silent => "SILENT",
            Status::Inert => "INERT",
            Status::NotDeployed => "NOT DEPLOYED",
            Status::Unrunnable => "UNRUNNABLE",
        }
    }

    /// Is this a verdict a reader may treat as healthy?
    ///
    /// Only `Ok`. In particular **`Inert` is not a pass** — a contract watching
    /// a feature nobody has exercised, reporting healthy, is the original defect
    /// wearing the machinery built to prevent it.
    pub fn is_pass(self) -> bool {
        matches!(self, Status::Ok)
    }
}

// ---------------------------------------------------------------------------
// The runner
// ---------------------------------------------------------------------------
//
// This lived in `tests/liveness_contract.rs` and nowhere else, which meant the
// only way to learn whether a write path had ever run was for a human to type
// `cargo test --test liveness_contract -- --ignored`. Nothing scheduled it, CI
// did not run it, and the server could not answer the question at all.
//
// It is here rather than duplicated into a worker because of the rule in
// `verification_for_agent_ecologies.md` §3.4: a trust calculation must have
// exactly one implementation, and the layer that owns the vocabulary must own
// the arithmetic. Two copies of this would eventually disagree, and the one
// that got believed would be whichever sat nearest the writer.

/// One contract's verdict, with the two counts that produced it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContractOutcome {
    pub sink: &'static str,
    pub writer: &'static str,
    pub status: &'static str,
    /// `-1` when the query could not run (`NOT DEPLOYED` / `UNRUNNABLE`).
    pub writes: i64,
    pub opportunities: i64,
    /// Set when this sink is a documented exception in [`KNOWN_SILENT`].
    pub known_silent_reason: Option<&'static str>,
    /// Attempt counts for this contract's writer, when it is instrumented.
    ///
    /// This is the axis liveness cannot see. `writes = 0` is `Silent` whether
    /// the writer never ran or ran and was refused every time, and those have
    /// opposite remedies.
    pub accounting: Option<SinkAccount>,
    /// Why the sink is empty, in one word, when the counters can say.
    ///
    /// Deliberately **not** a fourth [`Status`]. The paper defines three
    /// liveness verdicts and they are answers about rows; attempts are a
    /// different question, and folding them into the same enum is how five
    /// verdicts came to occupy four report buckets.
    pub diagnosis: Option<&'static str>,
    pub why: &'static str,
    pub remediation: &'static str,
}

/// Read the counters and say what they add to a row-count verdict.
fn diagnose(status: Status, acct: Option<&SinkAccount>) -> Option<&'static str> {
    if status == Status::Ok {
        return None;
    }
    match acct {
        // The finding this whole layer was built for: the writer runs and the
        // database refuses it. Not ambiguous, not "maybe unused", and invisible
        // from the row count alone.
        Some(a) if a.is_totally_rejected() => Some("rejected"),
        Some(a) if a.failures > 0 => Some("partially_rejected"),
        Some(a) if a.attempts == 0 => Some("never_attempted"),
        Some(_) => None,
        // No counters at all. Not a clean bill: it means the writer swallows its
        // failures and has not been instrumented, so they are still going
        // nowhere. Naming it is what stops the gap being read as an absence of
        // problems.
        None => Some("uninstrumented"),
    }
}

/// The result of one sweep across every declared write path.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LivenessReport {
    pub ran_at: String,
    pub ok: usize,
    pub silent: usize,
    pub inert: usize,
    pub unrunnable: usize,
    /// Silent sinks that are **not** in [`KNOWN_SILENT`]. This is the actionable
    /// list; everything else is context.
    pub undocumented_silent: Vec<&'static str>,
    /// Sinks whose writer has been attempted and refused **every** time.
    ///
    /// Reported separately from `undocumented_silent` because it is a stronger
    /// statement and it escapes the ambiguity the rest of this module is built
    /// to manage. `Silent` may mean unused; `Inert` may mean nothing has
    /// happened yet; `Conditional` may mean the detector was right to find
    /// nothing. **None of those readings survive a rejected write.** The code
    /// ran, the row was refused, and no interpretation of the sink's emptiness
    /// is honest.
    ///
    /// This is also what makes a `Conditional` contract falsifiable. Asserting
    /// on `anomaly_events`' row count would assert that anomalies must exist;
    /// asserting that its writer is not being refused asserts nothing about the
    /// world.
    pub rejected: Vec<&'static str>,
    pub outcomes: Vec<ContractOutcome>,
}

impl LivenessReport {
    /// Has anything at all been demonstrated to work?
    ///
    /// The positive-control question. `0 live` cannot distinguish "every path is
    /// broken" from "the runner is broken", so a report with no passing contract
    /// is never healthy regardless of what else it says.
    pub fn has_positive_control(&self) -> bool {
        self.ok > 0
    }

    /// Healthy means: something is proven to run, nothing is silently broken
    /// without a written reason, and **no writer is being refused.**
    ///
    /// The last clause is not subject to the exemption list. A sink may be
    /// excused for being empty; nothing excuses a statement the database will
    /// not accept.
    pub fn is_healthy(&self) -> bool {
        self.has_positive_control()
            && self.undocumented_silent.is_empty()
            && self.unrunnable == 0
            && self.rejected.is_empty()
    }
}

/// Does `table.column` exist yet?
///
/// Contracts routinely race the deploy of the migration that creates their
/// sink. Without this the query errors, and "the check could not run" would be
/// indistinguishable from a finding.
pub async fn column_exists(pool: &sqlx::PgPool, table: &str, column: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM information_schema.columns \
          WHERE table_name = $1 AND column_name = $2",
    )
    .bind(table)
    .bind(column)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
        > 0
}

/// Evaluate one contract against a live database.
///
/// Returns `(status, writes, opportunities)`; the counts are `-1` when a query
/// could not run, so a caller cannot mistake "did not run" for zero.
pub async fn evaluate_one(pool: &sqlx::PgPool, c: &LivenessContract) -> (Status, i64, i64) {
    if let Some((table, column)) = c.requires {
        if !column_exists(pool, table, column).await {
            return (Status::NotDeployed, -1, -1);
        }
    }

    // Read by the column alias, not positionally: `every_query_aliases_the_
    // column_the_runner_reads` asserts every contract names `writes` /
    // `opportunities`, and that assertion is only worth anything if the runner
    // actually depends on the alias.
    async fn count(pool: &sqlx::PgPool, sql: &str, col: &str) -> Option<i64> {
        use sqlx::Row;
        sqlx::query(sql)
            .fetch_one(pool)
            .await
            .ok()
            .and_then(|r| r.try_get::<i64, _>(col).ok())
    }

    match (
        count(pool, c.sink_sql, "writes").await,
        count(pool, c.opportunity_sql, "opportunities").await,
    ) {
        (Some(w), Some(o)) => (classify(w, o), w, o),
        _ => (Status::Unrunnable, -1, -1),
    }
}

/// Run every contract. Read-only; every query is a bare `SELECT`, asserted by
/// `every_query_is_read_only`.
pub async fn sweep(pool: &sqlx::PgPool) -> LivenessReport {
    let mut outcomes = Vec::with_capacity(LIVENESS_CONTRACTS.len());
    let (mut ok, mut silent, mut inert, mut unrunnable) = (0, 0, 0, 0);
    let mut undocumented_silent = Vec::new();
    let mut rejected = Vec::new();

    for c in LIVENESS_CONTRACTS {
        let (status, writes, opportunities) = evaluate_one(pool, c).await;
        let accounting = c.accounted.map(write_accounting::account);
        let diagnosis = diagnose(status, accounting.as_ref());

        // Independent of the row-count verdict, and deliberately so. A refused
        // statement is broken whether the sink reads Ok, Silent or Inert.
        if accounting
            .as_ref()
            .is_some_and(SinkAccount::is_totally_rejected)
        {
            rejected.push(c.sink);
        }
        match status {
            Status::Ok => ok += 1,
            Status::Silent => {
                silent += 1;
                if is_actionable_silence(c) {
                    undocumented_silent.push(c.sink);
                }
            }
            // NotDeployed counts with Inert: both mean "proven nothing".
            Status::Inert | Status::NotDeployed => inert += 1,
            Status::Unrunnable => unrunnable += 1,
        }
        outcomes.push(ContractOutcome {
            sink: c.sink,
            writer: c.writer,
            status: status.label(),
            writes,
            opportunities,
            known_silent_reason: known_silent(c.sink),
            accounting,
            diagnosis,
            why: c.why,
            remediation: c.remediation,
        });
    }

    LivenessReport {
        ran_at: chrono::Utc::now().to_rfc3339(),
        ok,
        silent,
        inert,
        unrunnable,
        undocumented_silent,
        rejected,
        outcomes,
    }
}

/// The most recent sweep, for the read endpoint.
///
/// `None` until the first sweep completes, and the endpoint reports that as
/// `never_run` rather than as healthy — the distinction this whole module
/// exists to make.
static LATEST: std::sync::OnceLock<std::sync::RwLock<Option<LivenessReport>>> =
    std::sync::OnceLock::new();

fn latest_cell() -> &'static std::sync::RwLock<Option<LivenessReport>> {
    LATEST.get_or_init(|| std::sync::RwLock::new(None))
}

pub fn record_latest(report: LivenessReport) {
    if let Ok(mut guard) = latest_cell().write() {
        *guard = Some(report);
    }
}

pub fn latest() -> Option<LivenessReport> {
    latest_cell().read().ok().and_then(|g| g.clone())
}

/// Run the standing clock on a schedule.
///
/// Liveness is the only rung with no gate behind it: nothing waits on it, so
/// nothing stalls when it is missing and its absence is observationally
/// identical to its passing. That is precisely why it had no scheduler, no
/// endpoint and no CI step while the other four did — the same shape as the
/// resolution sweeper and the observability sweeper before them, and the third
/// time this repository has had to schedule a loop that had gone cold.
///
/// The remedy is not a better check. It is a worse hiding place: a verdict that
/// is written somewhere a person reads on somebody else's schedule.
///
/// `LIVENESS_SWEEP_SECS=0` disables it, matching `OBSERVABILITY_SCAN_SECS`.
pub fn spawn_liveness_sweeper(db: sqlx::PgPool) {
    const DEFAULT_SWEEP_SECS: u64 = 3600;

    let interval_secs = std::env::var("LIVENESS_SWEEP_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_SWEEP_SECS);

    if interval_secs == 0 {
        println!("[liveness] standing sweep disabled (LIVENESS_SWEEP_SECS=0)");
        return;
    }
    println!("[liveness] sweeping declared write paths every {interval_secs}s");

    tokio::spawn(async move {
        // Stagger past boot so migrations and schema ensures have the pool.
        tokio::time::sleep(std::time::Duration::from_secs(120)).await;
        loop {
            let report = sweep(&db).await;

            // Loud only when it is actionable. A sweep that prints every hour
            // regardless of outcome is one people filter, and a filtered check
            // is an unread check.
            if !report.undocumented_silent.is_empty() {
                eprintln!(
                    "[liveness] SILENT: {} — declared write path(s) with opportunities and no rows. \
                     This signal does not exist.",
                    report.undocumented_silent.join(", ")
                );
            }
            if report.unrunnable > 0 {
                eprintln!(
                    "[liveness] {} contract(s) UNRUNNABLE — a check that cannot run reports \
                     healthy for ever.",
                    report.unrunnable
                );
            }
            if !report.has_positive_control() {
                eprintln!(
                    "[liveness] 0 live. Nothing has been demonstrated to work, which cannot be \
                     distinguished from the sweep itself being broken."
                );
            } else {
                println!(
                    "[liveness] {} ok, {} silent, {} inert, {} unrunnable",
                    report.ok, report.silent, report.inert, report.unrunnable
                );
            }

            record_latest(report);
            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(ok: usize, silent: Vec<&'static str>, unrunnable: usize) -> LivenessReport {
        LivenessReport {
            ran_at: "1970-01-01T00:00:00Z".into(),
            ok,
            silent: silent.len(),
            inert: 0,
            unrunnable,
            undocumented_silent: silent,
            rejected: Vec::new(),
            outcomes: Vec::new(),
        }
    }

    /// A conditional sink's silence is reported, never asserted — and the
    /// library must say so as loudly as the test runner does.
    ///
    /// The regression: `sweep` and the test applied different rules, so
    /// `anomaly_events` was `0 silent` on the script and "silent with no
    /// excuse" in the library's own report. Whichever a reader reached first
    /// was the answer they got.
    #[test]
    fn a_conditional_sink_is_never_an_undocumented_silence() {
        let conditional: Vec<_> = LIVENESS_CONTRACTS
            .iter()
            .filter(|c| c.expectation == Expectation::Conditional)
            .collect();
        assert!(
            !conditional.is_empty(),
            "no contract is Conditional, so this test proves nothing"
        );
        for c in conditional {
            assert!(
                !is_actionable_silence(c),
                "`{}` is Conditional and would still be reported as an \
                 unexplained silence — which asserts that its detector must \
                 find something",
                c.sink
            );
        }
        // And an ordinary silent sink still is one, or the fix has simply
        // switched the check off.
        let asserted = LIVENESS_CONTRACTS
            .iter()
            .find(|c| c.expectation == Expectation::EveryOpportunity)
            .expect("at least one asserted contract");
        assert!(is_actionable_silence(asserted));
    }

    /// A refused write fails the report whatever the row counts say.
    ///
    /// The escape hatches in this module all address one ambiguity: an empty
    /// sink may be unused. None of them apply here. `rejected` means the writer
    /// ran and the database would not take the row, so there is no reading of
    /// the emptiness that is honest — which is why it is checked outside
    /// `KNOWN_SILENT` and outside the `Conditional` exemption.
    #[test]
    fn a_rejected_write_is_never_healthy_however_clean_the_counts() {
        let mut r = report(3, vec![], 0);
        assert!(r.is_healthy());
        r.rejected.push("anomaly_events");
        assert!(
            !r.is_healthy(),
            "a sink whose writer is refused every time reported healthy because \
             its row count was excused elsewhere"
        );
    }

    /// The diagnosis distinguishes the two readings of `Silent` — which is the
    /// entire reason the accounting layer exists.
    #[test]
    fn silence_is_diagnosed_as_untried_or_refused() {
        use crate::write_accounting::{LastError, SinkAccount};
        let acct = |attempts, failures| SinkAccount {
            table: "anomaly_events",
            writer: "x::y",
            attempts,
            failures,
            last_error: failures.gt(&0).then(|| LastError {
                at: "1970-01-01T00:00:00Z".into(),
                message: "violates foreign key constraint".into(),
            }),
        };

        // Nobody tried: a missing scheduler or an unexercised feature.
        assert_eq!(
            diagnose(Status::Silent, Some(&acct(0, 0))),
            Some("never_attempted")
        );
        // Everybody tried and the database refused: a broken statement.
        assert_eq!(
            diagnose(Status::Silent, Some(&acct(340, 340))),
            Some("rejected")
        );
        // Same row count, three different meanings — the point of the layer.
        assert_eq!(
            diagnose(Status::Silent, Some(&acct(340, 12))),
            Some("partially_rejected")
        );
        // Not instrumented is not a clean bill: the failures still go nowhere.
        assert_eq!(diagnose(Status::Silent, None), Some("uninstrumented"));
        // A working path needs no diagnosis.
        assert_eq!(diagnose(Status::Ok, Some(&acct(10, 0))), None);
    }

    /// Every instrumented contract must point at a sink whose table matches the
    /// sink label it watches.
    ///
    /// The link is typed, so it cannot be misspelled — but it can still be
    /// wired to the wrong variant, which would attribute one writer's failures
    /// to another contract and would look entirely plausible in the report.
    #[test]
    fn each_contract_is_accounted_against_the_table_it_watches() {
        for c in LIVENESS_CONTRACTS {
            let Some(sink) = c.accounted else { continue };
            let table = sink.table();
            assert!(
                c.sink.starts_with(table) || c.sink_sql.contains(table),
                "`{}` is accounted against `{table}`, which appears in neither \
                 its label nor its query",
                c.sink
            );
        }
    }

    /// The positive-control rule, as an assertion rather than a comment.
    ///
    /// `0 live` cannot distinguish "every path is broken" from "the sweeper is
    /// broken". A report that has demonstrated nothing must never be able to
    /// present itself as healthy, however clean the rest of it looks.
    #[test]
    fn a_report_with_nothing_proven_is_never_healthy() {
        let r = report(0, vec![], 0);
        assert!(!r.has_positive_control());
        assert!(
            !r.is_healthy(),
            "a sweep with no passing contract reported healthy — which is the \
             defect this module exists to catch, reproduced in the module itself"
        );

        // ...and the same report becomes healthy the moment one path is proven.
        assert!(report(1, vec![], 0).is_healthy());
    }

    /// Silence needs a written reason or it is a finding. An entry in
    /// `KNOWN_SILENT` is an excuse someone had to type; anything else is not.
    #[test]
    fn an_unexplained_silent_sink_fails_the_report() {
        assert!(!report(1, vec!["some_ledger"], 0).is_healthy());
        assert!(report(1, vec![], 0).is_healthy());
    }

    /// An unrunnable check reports healthy for ever, so it must never be
    /// folded into a pass.
    #[test]
    fn an_unrunnable_query_fails_the_report() {
        assert!(!report(1, vec![], 1).is_healthy());
    }

    /// Only `Ok` is a pass. In particular `Inert` is not — the whole reason
    /// the status exists is that a contract watching an unexercised feature
    /// must not look green.
    #[test]
    fn only_ok_is_a_pass() {
        assert!(Status::Ok.is_pass());
        for s in [
            Status::Silent,
            Status::Inert,
            Status::NotDeployed,
            Status::Unrunnable,
        ] {
            assert!(!s.is_pass(), "{} must not be a pass", s.label());
        }
    }

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

    /// An opportunity query must count chances to write, not things already
    /// written by somebody else.
    ///
    /// The regression this is here for: Loop 5.A's single contract counted
    /// `sosa_observations WHERE extra ? 'projection_id'` as "chances to score a
    /// measurement against a projection". Every one of those 12,167 rows is a
    /// projection. It was comparing the sink to a census of the *predictions*,
    /// so it reported 12,167 missed scorings in a world where nothing had ever
    /// been measured against a projection at all — and it named a remediation
    /// at the wrong end of the chain.
    ///
    /// The scoreable event is a resolved pair, which is a `process_spacetime`
    /// row and nothing else. Asserting the query's *source table* is a crude
    /// check and it is the one that would have caught this: no reading of
    /// `sosa_observations` can tell you a measurement met a prediction.
    #[test]
    fn the_scoring_rung_counts_resolved_pairs_and_not_projections() {
        let scoring = LIVENESS_CONTRACTS
            .iter()
            .find(|c| c.sink.starts_with("eval_signals.projection_accuracy"))
            .expect("the Loop 5.A scoring contract");

        assert!(
            scoring.opportunity_sql.contains("process_spacetime"),
            "the scoring rung must count resolved pairs; got `{}`",
            scoring.opportunity_sql
        );
        assert!(
            !scoring.opportunity_sql.contains("sosa_observations"),
            "the scoring rung is counting raw observations again. A projection \
             is not a chance to score one; it is the thing being scored."
        );

        // And the anchor rung is bounded by the instant its call site began to
        // exist, so historical projections are not reported as missed writes
        // that could only be supplied dishonestly.
        let anchor = LIVENESS_CONTRACTS
            .iter()
            .find(|c| c.sink.starts_with("process_projection_commits"))
            .expect("the Loop 5.A anchor contract");
        assert!(
            anchor.opportunity_sql.contains(COMMIT_HOOK_LIVE_FROM),
            "the anchor rung must bound its opportunities by {COMMIT_HOOK_LIVE_FROM}"
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
