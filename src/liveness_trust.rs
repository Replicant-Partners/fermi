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
    LivenessContract {
        sink: "eval_signals.projection_accuracy (Loop 5b)",
        writer: "evaluators::ProjectionScoringEvaluator, via handlers::eval::run_eval_cases",
        sink_sql: "SELECT count(*)::bigint AS writes FROM eval_signals \
                    WHERE evaluator_name ILIKE '%projection%'",
        // A real observation carrying a projection_id is a batch that completed
        // against a prior projection — exactly the event the architecture says
        // triggers scoring. Migration 130 indexes this lookup, so the
        // opportunity is both real and cheap to count.
        opportunity_sql: "SELECT count(*)::bigint AS opportunities FROM sosa_observations \
                           WHERE extra ? 'projection_id'",
        expectation: Expectation::EveryOpportunity,
        requires: Some(("sosa_observations", "extra")),
        why: "Loop 5b is the hard-verified half of calibration: a physical \
              measurement scored against what the model projected, which is the \
              one signal an agent cannot talk its way out of. The reader is \
              fully wired — `calibration.rs` surfaces projection_accuracy and \
              the observatory displays it — so from a dashboard the loop looks \
              closed. The producing edge does not exist: writing a real \
              observation does not invoke the evaluator, and the arc the \
              architecture describes as 'real batch completes -> \
              ProjectionScoringEvaluator' is not present in code.",
        remediation: "The hook site already exists and already has every value it \
                      needs in scope, including `projection_id`: see the \
                      `let _ = (...)` in `simops_tools::execute_simops_write_observation`, \
                      whose comment calls it 'hooks for an observability path that \
                      may or may not be live'. It is not live. Either connect it \
                      to the evaluator registry, or delete the stub and stop \
                      claiming 5b — a maybe in a comment is how this stayed \
                      unresolved. Note also that `projection_id` is never placed \
                      into the eval bundle context, so even a manual eval run \
                      falls back to the 30-day heuristic rather than matching \
                      the projection it is scoring.",
    },
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
    pub why: &'static str,
    pub remediation: &'static str,
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

    /// Healthy means: something is proven to run, and nothing is silently
    /// broken without a written reason.
    pub fn is_healthy(&self) -> bool {
        self.has_positive_control() && self.undocumented_silent.is_empty() && self.unrunnable == 0
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

    for c in LIVENESS_CONTRACTS {
        let (status, writes, opportunities) = evaluate_one(pool, c).await;
        match status {
            Status::Ok => ok += 1,
            Status::Silent => {
                silent += 1;
                if known_silent(c.sink).is_none() {
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
            outcomes: Vec::new(),
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
