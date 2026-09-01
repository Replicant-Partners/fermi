//! Evaluators that score **the platform's own machinery**.
//!
//! # Why this is a separate registry
//!
//! `agent_bestiary_evaluators::EvaluatorRegistry` scores an agent's *output*:
//! is this response harmful, in character, faithful to its sources. Those
//! evaluators are pluggable and third-party — WildGuard, Sotopia, a character
//! model — and they answer questions about a document.
//!
//! The evaluators here answer questions about the system: *is Loop 4 turning,
//! is any gate refusing everything, is a writer being refused by the database.*
//! Same modular shape, deliberately different registry, because mixing "is this
//! response harmful" with "is Loop 4 turning" in one list makes both harder to
//! reason about and gives them one health verdict when they need two.
//!
//! And the ordering matters: **none of the pluggable evaluators mean anything
//! if the loops they feed are not closing.** A perfect safety score on a
//! response whose episode is never consolidated, never scored and never
//! attributed is a measurement with nowhere to go.
//!
//! # Pure functions over a snapshot
//!
//! An evaluator takes an [`Observation`] and returns a [`Verdict`]. It reads no
//! globals and touches no database — [`Observation::collect`] does that once,
//! and every evaluator sees the same instant.
//!
//! That is not tidiness. It is what makes
//! [`every_evaluator_can_produce_a_finding`](tests) possible: constructing a
//! world in which an evaluator *must* fire is a struct literal, so §5.1 — *a
//! check that has never failed has not been tested* — becomes a structural
//! requirement of the registry rather than a ritual someone remembers to
//! perform.
//!
//! # Three verdicts
//!
//! `Healthy`, `Finding`, `Inconclusive`. The third is not a pass.
//!
//! `Inconclusive` covers both "no data yet" and "could not run", because for an
//! evaluator they have the same consequence — no information — and the reason
//! travels in the message rather than in a fourth variant. `liveness_trust`
//! carries five states in four report buckets for want of that decision.

use crate::gate_trust::{self, GateAccount};
use crate::liveness_trust::LivenessReport;
use crate::loop_model::{self, LoopState};
use crate::write_accounting::{self, SinkAccount};

/// How much a finding should interrupt someone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Worth knowing, asserts nothing. A gate that has never refused belongs
    /// here: it may be correct, and it may mean the control is not wired, and
    /// no count can tell them apart.
    Notice,
    /// A control is not doing its job, and the reason is in the code.
    Warning,
    /// A control is inverted — it runs and refuses everything, or a writer is
    /// refused every time.
    Critical,
}

/// What an evaluator concluded.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum Verdict {
    Healthy {
        detail: String,
    },
    Finding {
        severity: Severity,
        detail: String,
        /// The subjects concerned — sinks, gates, loops. Named so a finding
        /// points at a thing rather than starting an investigation.
        subjects: Vec<String>,
        remedy: &'static str,
    },
    /// Nothing could be concluded. **Not a pass.**
    Inconclusive {
        why: String,
    },
}

impl Verdict {
    pub fn severity(&self) -> Option<Severity> {
        match self {
            Verdict::Finding { severity, .. } => Some(*severity),
            _ => None,
        }
    }
    /// Does this verdict fail the report?
    ///
    /// `Notice` does not, by design — it is the "reported, never asserted" tier
    /// that keeps `admits_everything` visible without asserting that violations
    /// must exist. `Inconclusive` does not fail either, but it is counted
    /// separately so a report made entirely of them cannot look green.
    pub fn is_failing(&self) -> bool {
        matches!(
            self.severity(),
            Some(Severity::Warning) | Some(Severity::Critical)
        )
    }
}

/// One instant's view of the machinery.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Observation {
    pub writes: Vec<SinkAccount>,
    pub gates: Vec<GateAccount>,
    pub loops: Vec<LoopState>,
    pub liveness: Option<LivenessReport>,
    /// What the durable half of the gate ledger can claim.
    ///
    /// Separate from `gates` because it is a property of the recorder rather
    /// than of any one gate: the counters above are in-memory and reset on
    /// restart, and this is how a reader learns which of them say `since boot`.
    pub gate_ledger: Option<crate::gate_trust::LedgerStatus>,
    /// What the fleet has declared about itself.
    ///
    /// Here rather than fetched per panel because it answers a question no other
    /// field can: every other member of this struct measures what the PLATFORM
    /// did, and this measures what the SUBJECTS declared. Measured over
    /// production, 110 of 206 agents that have produced an episode are
    /// `test_agent_*` rows declaring nothing, and 89 of the 96 real ones have no
    /// field contract — so this is the dominant explanation for `unknown` across
    /// every surface, and it was the one thing a snapshot could not see.
    pub declarations: Option<crate::declaration_ladder::Census>,
    /// Whether a declared field contract is wired all the way through.
    ///
    /// `None` on any failure rather than an empty vec, for the same reason as
    /// `declarations`: no contracts measured and no contracts conformant are
    /// different claims, and the second is the alarming one.
    pub conformance: Option<Vec<ContractConformance>>,
    /// `(edges_naming_a_parent, edges_whose_parent_does_not_exist)`.
    ///
    /// The delegation chain is the platform's only agent-to-agent trace, and
    /// half of it pointed at nothing. `None` when the scan could not run.
    pub delegation: Option<(i64, i64)>,
}

/// One declared field contract, and how far it actually gets.
///
/// A contract can be declared, applied, fitted and recorded, and it fails at
/// exactly one of those first. **Each failure has a different owner**, which is
/// why this is four numbers rather than a boolean: the bestiary card reported
/// nine agents as "not declared" when all nine declare a contract, and blamed
/// the author for what was a platform gap.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContractConformance {
    pub agent: &'static str,
    /// Pulses this agent has produced at all.
    pub pulses: i64,
    /// Pulses whose response carried a document to grade. **Rung 1**, and the
    /// author's: grounding grades fields in a JSON document, and an agent that
    /// answers in prose has nothing to grade however well the platform is wired.
    pub with_document: i64,
    /// Pulses carrying a grounding tag, so `enforce` demonstrably ran.
    /// **Rung 2**, and the platform's.
    pub graded: i64,
    /// Pulses with a row in the gate ledger. **Rung 3**, and the platform's.
    pub recorded: i64,
}

impl ContractConformance {
    /// The first rung that fails, or `None` when the contract is wired through.
    ///
    /// Ordered by ownership so the answer names who acts, not just what is
    /// missing. `pulses == 0` is deliberately not a failure: a contract on an
    /// agent nobody has invoked is untested, not broken.
    pub fn first_gap(&self) -> Option<(&'static str, &'static str)> {
        if self.pulses == 0 {
            return None;
        }
        if self.with_document == 0 {
            return Some((
                "emits_no_document",
                "the agent's card or prompt — it answers in prose, so a field \
                 contract has nothing to grade",
            ));
        }
        if self.graded == 0 {
            return Some((
                "never_enforced",
                "the platform — documents exist and no path ran `enforce` on them",
            ));
        }
        if self.recorded == 0 {
            return Some((
                "never_recorded",
                "the platform — the gate decided and wrote no ledger row, so the \
                 belt cannot show what it decided",
            ));
        }
        None
    }
}

impl Observation {
    /// Gather everything once, so every evaluator sees the same instant.
    pub async fn collect(pool: &sqlx::PgPool) -> Self {
        Self {
            writes: write_accounting::accounts(),
            gates: gate_trust::accounts(),
            loops: loop_model::evaluate(pool).await,
            liveness: crate::liveness_trust::latest(),
            gate_ledger: Some(gate_trust::ledger_status()),
            declarations: declaration_census(pool).await,
            conformance: contract_conformance(pool).await,
            delegation: delegation_integrity(pool).await,
        }
    }
}

/// How much of the delegation chain resolves.
///
/// An episode may name a parent whose row was never written: the id is minted
/// inside the tool loop and handed to the child before the parent persists, so a
/// parent run that fails leaves the reference dangling for good. Measured at 6
/// of 12 before `reserve_episode` existed.
pub async fn delegation_integrity(pool: &sqlx::PgPool) -> Option<(i64, i64)> {
    let row: (i64, i64) = sqlx::query_as(
        "SELECT count(*)::bigint,
                count(*) FILTER (WHERE pe.episode_id IS NULL)::bigint
           FROM episodes e
           LEFT JOIN episodes pe ON pe.episode_id = e.parent_episode_id
          WHERE e.parent_episode_id IS NOT NULL",
    )
    .fetch_one(pool)
    .await
    .ok()?;
    Some(row)
}

/// Measure every declared field contract against what actually happened.
///
/// The check that would have caught the defect it was written after: nine of the
/// ten agents declaring a contract had never produced a graded field, and no
/// surface could say so because nothing asked. A contract is a claim about the
/// platform, and an unexercised claim is the defect class this whole line of
/// work exists to name.
pub async fn contract_conformance(pool: &sqlx::PgPool) -> Option<Vec<ContractConformance>> {
    use sqlx::Row;
    use std::collections::HashMap;

    // Two queries, grouped by agent. It was two queries PER CONTRACT.
    //
    // The counts are per agent: 105 contracts name ten distinct agents, so the
    // loop ran 210 sequential scans of `episodes` — each with a `LIKE '%{%'`
    // over `response_text` and an `ANY(tags)` — to produce ten distinct rows.
    // `/api/specimen/:agent_name` waited **46 seconds** for that, on every
    // agent, because the Health tab reads the platform-wide census; the page
    // read as broken rather than slow.
    //
    // The old comment said "the set is ten long and this runs on the standing
    // clock". Both halves were true when written: the set WAS ten, because it
    // meant agents, and this did run on a clock rather than in a request. A
    // sentence that outlived its fact, and the fact it outlived was a factor of
    // ten in the number of queries.
    //
    // Shape is unchanged — one entry per contract, as before — so no consumer
    // moves. Worth knowing separately: those entries duplicate their agent's
    // numbers once per contract, so `EveryContractGrades` lists a gap ten times
    // for one agent. That is a real wart and a different change.
    let mut agents: Vec<&'static str> = Vec::new();
    for c in crate::grounding_trust::FIELD_CONTRACTS.iter() {
        if !agents.contains(&c.agent_id) {
            agents.push(c.agent_id);
        }
    }
    let names: Vec<String> = agents.iter().map(|a| a.to_string()).collect();

    let rows = sqlx::query(
        "SELECT a.agent_name,
                count(*)                                                        AS pulses,
                count(*) FILTER (WHERE e.response_text LIKE '%{%')              AS with_document,
                count(*) FILTER (WHERE 'grounding:enforced'   = ANY(e.tags)
                                    OR 'grounding:violations' = ANY(e.tags))    AS graded
           FROM episodes e
           JOIN agents a ON a.agent_id = e.agent_id
          WHERE a.agent_name = ANY($1)
          GROUP BY a.agent_name",
    )
    .bind(&names)
    .fetch_all(pool)
    .await
    .ok()?;

    let recorded_rows = sqlx::query(
        "SELECT a.agent_name, count(*)::bigint AS recorded
           FROM gate_decisions gd
           JOIN episodes e ON e.episode_id = gd.episode_id
           JOIN agents a  ON a.agent_id = e.agent_id
          WHERE gd.gate = 'grounding' AND a.agent_name = ANY($1)
          GROUP BY a.agent_name",
    )
    .bind(&names)
    .fetch_all(pool)
    .await
    .ok()?;

    // An agent with no pulses returns no row, and zero is the right answer for
    // it — `untested`, which the evaluator reports and never scores. A missing
    // row must not drop the contract from the list, or a declared-and-never-run
    // contract would become invisible, which is the finding this exists for.
    let mut by_agent: HashMap<String, (i64, i64, i64)> = HashMap::new();
    for r in &rows {
        let Ok(name) = r.try_get::<String, _>("agent_name") else {
            continue;
        };
        by_agent.insert(
            name,
            (
                r.try_get("pulses").unwrap_or(0),
                r.try_get("with_document").unwrap_or(0),
                r.try_get("graded").unwrap_or(0),
            ),
        );
    }
    let mut recorded: HashMap<String, i64> = HashMap::new();
    for r in &recorded_rows {
        if let Ok(name) = r.try_get::<String, _>("agent_name") {
            recorded.insert(name, r.try_get("recorded").unwrap_or(0));
        }
    }

    Some(
        crate::grounding_trust::FIELD_CONTRACTS
            .iter()
            .map(|c| {
                let (pulses, with_document, graded) =
                    by_agent.get(c.agent_id).copied().unwrap_or((0, 0, 0));
                ContractConformance {
                    agent: c.agent_id,
                    pulses,
                    with_document,
                    graded,
                    recorded: recorded.get(c.agent_id).copied().unwrap_or(0),
                }
            })
            .collect(),
    )
}

/// Measure what the fleet has declared, for [`Observation::collect`].
///
/// `None` on any failure rather than a default, and the distinction is the whole
/// point of the `Option`: an empty `Census` would report every rung at zero
/// coverage, which is indistinguishable from a fleet that has declared nothing
/// and is the most alarming available reading. A resolver that cannot get the
/// census must say so, not infer the worst.
pub async fn declaration_census(pool: &sqlx::PgPool) -> Option<crate::declaration_ladder::Census> {
    use sqlx::Row;
    let rows = sqlx::query(crate::declaration_ladder::CENSUS_SQL)
        .fetch_all(pool)
        .await
        .ok()?;
    let mut measured: Vec<(String, Vec<&'static str>)> = Vec::new();
    for r in &rows {
        let Ok(name) = r.try_get::<String, _>("agent_name") else {
            continue;
        };
        let mut rungs: Vec<&'static str> = Vec::new();
        if r.try_get::<bool, _>("ports").unwrap_or(false) {
            rungs.push("ports");
        }
        if r.try_get::<bool, _>("output_type").unwrap_or(false) {
            rungs.push("output_type");
        }
        if r.try_get::<bool, _>("output_schema").unwrap_or(false) {
            rungs.push("output_schema");
        }
        // The fourth rung is a Rust const and no SQL can see it. Asked of the
        // owner rather than duplicated into the query.
        if crate::declaration_ladder::has_field_contract(&name) {
            rungs.push("field_contract");
        }
        measured.push((name, rungs));
    }
    Some(crate::declaration_ladder::census(&measured))
}

/// A check on the platform's own machinery.
pub trait NativeEvaluator: Send + Sync {
    /// Stable identifier.
    fn id(&self) -> &'static str;
    /// The question it answers, in one line.
    fn asks(&self) -> &'static str;
    fn evaluate(&self, o: &Observation) -> Verdict;
}

// ── The evaluators ──────────────────────────────────────────────────────

/// Is any writer being refused by the database every time it runs?
pub struct RefusedWrites;
impl NativeEvaluator for RefusedWrites {
    fn id(&self) -> &'static str {
        "refused_writes"
    }
    fn asks(&self) -> &'static str {
        "Is a write path attempted and refused every single time?"
    }
    fn evaluate(&self, o: &Observation) -> Verdict {
        let attempted: Vec<_> = o.writes.iter().filter(|w| w.attempts > 0).collect();
        if attempted.is_empty() {
            return Verdict::Inconclusive {
                why: "no instrumented write has been attempted since boot".into(),
            };
        }
        let refused: Vec<String> = attempted
            .iter()
            .filter(|w| w.is_totally_rejected())
            .map(|w| format!("{} ({} attempts, 0 landed)", w.table, w.attempts))
            .collect();
        if refused.is_empty() {
            return Verdict::Healthy {
                detail: format!(
                    "{} write path(s) attempted, none wholly refused",
                    attempted.len()
                ),
            };
        }
        Verdict::Finding {
            severity: Severity::Critical,
            detail: format!("{} write path(s) refused every time", refused.len()),
            subjects: refused,
            remedy: "The code runs and the database will not take the row. Read \
                     `last_error` on the sink — a CHECK violation means a \
                     vocabulary drifted; a foreign-key violation means the row \
                     it points at is written later than this one.",
        }
    }
}

/// Does every delegation edge resolve to an episode that exists?
///
/// The chain is the only agent-to-agent trace the platform has. A child that
/// names a parent with no row is not "not delegated" - it is a broken edge, and
/// it is invisible everywhere except here.
pub struct DelegationChainIntact;
impl NativeEvaluator for DelegationChainIntact {
    fn id(&self) -> &'static str {
        "delegation_chain_intact"
    }
    fn asks(&self) -> &'static str {
        "Does every delegated episode resolve to a parent that exists?"
    }
    fn evaluate(&self, o: &Observation) -> Verdict {
        let Some((total, dangling)) = o.delegation else {
            return Verdict::Inconclusive {
                why: "the delegation scan did not run".into(),
            };
        };
        if total == 0 {
            // No delegation is a product fact, not a broken chain.
            return Verdict::Inconclusive {
                why: "no episode names a parent, so there is no chain to check".into(),
            };
        }
        if dangling == 0 {
            return Verdict::Healthy {
                detail: format!("all {total} delegation edge(s) resolve"),
            };
        }
        Verdict::Finding {
            severity: Severity::Critical,
            detail: format!(
                "{dangling} of {total} delegation edge(s) name a parent that does not exist"
            ),
            subjects: vec![format!(
                "{dangling} child episode(s) were delegated and their caller cannot be identified"
            )],
            remedy: "The parent's id is handed to the child before the parent's row is \
                     written, so a parent run that fails orphans everything it already \
                     spawned. `MemoryStore::reserve_episode` writes the row first; a \
                     rising count here means a path mints an id without reserving it.",
        }
    }
}

/// Is a declared field contract wired all the way through?
///
/// The ratchet on the defect that produced it. A contract can be declared and
/// never emit a document, never be enforced, or never be recorded, and the
/// surfaces read all three as the same grey "unknown" - one of them blamed the
/// agent's author for a platform gap.
///
/// **A declared contract that has never graded a field is not a passing
/// contract.** It is an untested claim, which is the defect class of section 1
/// wearing the machinery built to prevent it.
pub struct ContractWiredThrough;
impl NativeEvaluator for ContractWiredThrough {
    fn id(&self) -> &'static str {
        "contract_wired_through"
    }
    fn asks(&self) -> &'static str {
        "Does every declared field contract actually grade something end to end?"
    }
    fn evaluate(&self, o: &Observation) -> Verdict {
        let Some(rows) = o.conformance.as_ref() else {
            return Verdict::Inconclusive {
                why: "the conformance scan did not run, so no contract can be \
                      called wired or broken"
                    .into(),
            };
        };
        if rows.is_empty() {
            return Verdict::Inconclusive {
                why: "no field contract is declared anywhere".into(),
            };
        }

        // An agent nobody has invoked is untested, not broken - and counting it
        // as either is the mistake. Reported, never scored.
        let untested: Vec<&ContractConformance> = rows.iter().filter(|r| r.pulses == 0).collect();
        let exercised: Vec<&ContractConformance> = rows.iter().filter(|r| r.pulses > 0).collect();

        if exercised.is_empty() {
            return Verdict::Inconclusive {
                why: format!(
                    "{} contract(s) declared and not one of their agents has been \
                     invoked, so nothing has been exercised",
                    rows.len()
                ),
            };
        }

        let gaps: Vec<String> = exercised
            .iter()
            .filter_map(|r| {
                r.first_gap().map(|(token, owner)| {
                    format!(
                        "{} — {} ({} pulses, {} with a document, {} graded, {} recorded) — {}",
                        r.agent, token, r.pulses, r.with_document, r.graded, r.recorded, owner
                    )
                })
            })
            .collect();

        if gaps.is_empty() {
            return Verdict::Healthy {
                detail: format!(
                    "{} contract(s) grade end to end{}",
                    exercised.len(),
                    if untested.is_empty() {
                        String::new()
                    } else {
                        format!(", {} untested", untested.len())
                    }
                ),
            };
        }

        Verdict::Finding {
            // Critical when nothing conforms at all: a contract nothing enforces
            // is a safety claim the platform is making and cannot keep.
            severity: if gaps.len() == exercised.len() {
                Severity::Critical
            } else {
                Severity::Warning
            },
            detail: format!(
                "{} of {} exercised contract(s) never grade a field",
                gaps.len(),
                exercised.len()
            ),
            subjects: gaps,
            remedy: "Read the token. `emits_no_document` is the card's problem — the \
                     agent answers in prose, so a field contract has nothing to bind \
                     to, and no platform wiring changes that. `never_enforced` is \
                     ours: documents exist and no path ran `enforce`. \
                     `never_recorded` is also ours: the gate decided and wrote no \
                     ledger row, so the belt cannot show what it decided.",
        }
    }
}

/// Is any gate refusing everything it is asked?
pub struct GateRefusingEverything;
impl NativeEvaluator for GateRefusingEverything {
    fn id(&self) -> &'static str {
        "gate_refusing_everything"
    }
    fn asks(&self) -> &'static str {
        "Has a gate been asked, and approved nothing?"
    }
    fn evaluate(&self, o: &Observation) -> Verdict {
        let asked: Vec<_> = o.gates.iter().filter(|g| g.asked() > 0).collect();
        if asked.is_empty() {
            return Verdict::Inconclusive {
                why: "no gate has been asked since boot".into(),
            };
        }
        let bad: Vec<String> = asked
            .iter()
            .filter(|g| g.refuses_everything())
            .map(|g| format!("{} ({} asked, 0 approved)", g.id, g.asked()))
            .collect();
        if bad.is_empty() {
            return Verdict::Healthy {
                detail: format!(
                    "{} gate(s) exercised, none refusing everything",
                    asked.len()
                ),
            };
        }
        Verdict::Finding {
            severity: Severity::Critical,
            detail: format!("{} gate(s) approve nothing", bad.len()),
            subjects: bad,
            remedy: "A control that refuses every input is indistinguishable \
                     from a strict one working well, which is how the coherence \
                     gate rejected 100% of agent-wide interventions for \
                     arithmetic reasons. Check the threshold arithmetic before \
                     the inputs.",
        }
    }
}

/// Is any gate letting everything through?
///
/// Reported, never asserted — hence `Notice`. A gate legitimately refuses
/// nothing when nothing warranted refusal, and asserting otherwise would assert
/// that violations must exist.
pub struct GateAdmittingEverything;
impl NativeEvaluator for GateAdmittingEverything {
    fn id(&self) -> &'static str {
        "gate_admitting_everything"
    }
    fn asks(&self) -> &'static str {
        "Has a gate been asked, and refused nothing?"
    }
    fn evaluate(&self, o: &Observation) -> Verdict {
        let asked: Vec<_> = o.gates.iter().filter(|g| g.asked() > 0).collect();
        if asked.is_empty() {
            return Verdict::Inconclusive {
                why: "no gate has been asked since boot".into(),
            };
        }
        let open: Vec<String> = asked
            .iter()
            .filter(|g| g.admits_everything())
            .map(|g| {
                format!(
                    "{} ({} asked, 0 refused) — {}",
                    g.id,
                    g.asked(),
                    g.if_never_refuses
                )
            })
            .collect();
        if open.is_empty() {
            return Verdict::Healthy {
                detail: "every exercised gate has refused at least once".into(),
            };
        }
        Verdict::Finding {
            severity: Severity::Notice,
            detail: format!("{} gate(s) have never refused anything", open.len()),
            subjects: open,
            remedy: "Not necessarily a fault. A control that never fires and a \
                     control that is not wired produce identical observations \
                     everywhere else, so this is the only surface on which the \
                     question can be asked at all.",
        }
    }
}

/// Is a loop stopped by something in the code?
pub struct LoopStalledInCode;
impl NativeEvaluator for LoopStalledInCode {
    fn id(&self) -> &'static str {
        "loop_stalled_in_code"
    }
    fn asks(&self) -> &'static str {
        "Is a feedback loop stopped by a fault rather than by an absence of work?"
    }
    fn evaluate(&self, o: &Observation) -> Verdict {
        if o.loops.is_empty() {
            return Verdict::Inconclusive {
                why: "the loop model has not been walked".into(),
            };
        }
        let stalled: Vec<String> = o
            .loops
            .iter()
            .filter(|l| {
                matches!(
                    l.reason,
                    Some("no_trigger") | Some("writes_refused") | Some("gate_refuses_everything")
                )
            })
            .map(|l| {
                format!(
                    "{}.{}: {}",
                    l.id,
                    l.stops_at.unwrap_or("?"),
                    l.reason.unwrap_or("?")
                )
            })
            .collect();
        let turning = o
            .loops
            .iter()
            .filter(|l| l.measured() && l.stops_at.is_none())
            .count();
        let unread: Vec<String> = o
            .loops
            .iter()
            .filter(|l| !l.measured())
            .map(|l| format!("{}.{}: probe_failed", l.id, l.stops_at.unwrap_or("?")))
            .collect();

        if stalled.is_empty() {
            // "The rest are idle rather than broken" is a claim about every loop
            // not named above. It cannot be made about a loop whose probe did
            // not run, so an unread loop downgrades the verdict rather than
            // being absorbed by it.
            if !unread.is_empty() {
                return Verdict::Inconclusive {
                    why: format!(
                        "{} of {} loop(s) could not be read; no loop is idle \
                         rather than broken until its stages have been counted \
                         ({})",
                        unread.len(),
                        o.loops.len(),
                        unread.join(", ")
                    ),
                };
            }
            return Verdict::Healthy {
                detail: format!(
                    "{turning} of {} loop(s) turning; the rest are idle rather \
                     than broken",
                    o.loops.len()
                ),
            };
        }
        Verdict::Finding {
            severity: Severity::Warning,
            detail: format!("{} loop(s) stopped by a fault in the code", stalled.len()),
            subjects: stalled,
            remedy: "Fix the FIRST empty link only. Every stage below it is \
                     empty because of it, and repairing a lower one produces \
                     nothing while looking like progress.",
        }
    }
}

/// Has the liveness sweep found a sink that is empty without a written reason?
pub struct UndocumentedSilence;
impl NativeEvaluator for UndocumentedSilence {
    fn id(&self) -> &'static str {
        "undocumented_silence"
    }
    fn asks(&self) -> &'static str {
        "Is a declared write path silent with no recorded excuse?"
    }
    fn evaluate(&self, o: &Observation) -> Verdict {
        let Some(r) = &o.liveness else {
            return Verdict::Inconclusive {
                why: "no liveness sweep has completed since boot".into(),
            };
        };
        if r.undocumented_silent.is_empty() {
            return Verdict::Healthy {
                detail: format!("{} contract(s) live, none silently broken", r.ok),
            };
        }
        Verdict::Finding {
            severity: Severity::Warning,
            detail: format!(
                "{} sink(s) silent with no reason",
                r.undocumented_silent.len()
            ),
            subjects: r
                .undocumented_silent
                .iter()
                .map(|s| s.to_string())
                .collect(),
            remedy: "Either the writer is broken or it is not deployed. Both \
                     mean the signal does not exist, which is why the verdict \
                     does not guess between them.",
        }
    }
}

/// Has anything at all been demonstrated to work?
pub struct PositiveControl;
impl NativeEvaluator for PositiveControl {
    fn id(&self) -> &'static str {
        "positive_control"
    }
    fn asks(&self) -> &'static str {
        "Has any part of the machinery been demonstrated to work?"
    }
    fn evaluate(&self, o: &Observation) -> Verdict {
        let Some(r) = &o.liveness else {
            return Verdict::Inconclusive {
                why: "no liveness sweep has completed since boot".into(),
            };
        };
        // A loop with an unread stage is not evidence that anything works.
        let turning = o
            .loops
            .iter()
            .filter(|l| l.measured() && l.stops_at.is_none())
            .count();
        if r.has_positive_control() && (o.loops.is_empty() || turning > 0) {
            return Verdict::Healthy {
                detail: format!("{} live contract(s), {turning} turning loop(s)", r.ok),
            };
        }
        Verdict::Finding {
            severity: Severity::Critical,
            detail: "nothing has been demonstrated to work".into(),
            subjects: vec![format!("{} live contracts, {turning} turning loops", r.ok)],
            remedy: "This cannot distinguish `every path is broken` from `the \
                     observer is broken`, and it must never be able to present \
                     itself as green. Check the sweeper before the paths.",
        }
    }
}

/// Every native evaluator, in report order.
pub fn registry() -> Vec<Box<dyn NativeEvaluator>> {
    vec![
        Box::new(PositiveControl),
        Box::new(RefusedWrites),
        Box::new(GateRefusingEverything),
        Box::new(LoopStalledInCode),
        Box::new(UndocumentedSilence),
        Box::new(GateAdmittingEverything),
        Box::new(ContractWiredThrough),
        Box::new(DelegationChainIntact),
    ]
}

/// One evaluator's result.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Scored {
    pub id: &'static str,
    pub asks: &'static str,
    #[serde(flatten)]
    pub verdict: Verdict,
}

/// The whole registry's verdict.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NativeReport {
    pub ran_at: String,
    pub healthy: usize,
    pub notices: usize,
    pub findings: usize,
    pub inconclusive: usize,
    pub status: &'static str,
    pub scored: Vec<Scored>,
}

/// Run every native evaluator over one observation.
pub fn run(o: &Observation) -> NativeReport {
    let mut scored = Vec::new();
    let (mut healthy, mut notices, mut findings, mut inconclusive) = (0, 0, 0, 0);

    for e in registry() {
        let verdict = e.evaluate(o);
        match &verdict {
            Verdict::Healthy { .. } => healthy += 1,
            Verdict::Inconclusive { .. } => inconclusive += 1,
            Verdict::Finding { severity, .. } => {
                if *severity == Severity::Notice {
                    notices += 1
                } else {
                    findings += 1
                }
            }
        }
        scored.push(Scored {
            id: e.id(),
            asks: e.asks(),
            verdict,
        });
    }

    // A registry that concluded nothing must not read as healthy. The same rule
    // as the liveness positive control, one level up: `0 findings` from six
    // evaluators that all declined to answer is not a clean bill.
    let status = if findings > 0 {
        "findings"
    } else if healthy == 0 {
        "inconclusive"
    } else {
        "healthy"
    };

    NativeReport {
        ran_at: chrono::Utc::now().to_rfc3339(),
        healthy,
        notices,
        findings,
        inconclusive,
        status,
        scored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate_trust::Clock;
    use crate::write_accounting::Sink;

    fn sink(table: &'static str, attempts: u64, failures: u64) -> SinkAccount {
        SinkAccount {
            table,
            writer: "a::b",
            attempts,
            failures,
            last_error: None,
        }
    }

    fn gate(id: &'static str, approved: u64, refused: u64) -> GateAccount {
        GateAccount {
            id,
            clock: Clock::Invocation,
            retention: crate::gate_trust::Retention::Counted,
            site: "a::b",
            approved,
            refused,
            undetermined: 0,
            last_refusal: None,
            reading: None,
            if_never_refuses: "x",
        }
    }

    fn loop_state(
        id: &'static str,
        stops_at: Option<&'static str>,
        reason: Option<&'static str>,
    ) -> LoopState {
        LoopState {
            id,
            name: "n",
            scope: "agent",
            claim: "c",
            timescale: "hours",
            stages: vec![],
            stops_at,
            reason,
            status: if stops_at.is_none() {
                "turning"
            } else {
                "stalled"
            },
        }
    }

    /// A loop whose first stage could not be counted.
    fn unread_loop_state(id: &'static str) -> LoopState {
        LoopState {
            id,
            name: "n",
            scope: "agent",
            claim: "c",
            timescale: "hours",
            stages: vec![crate::loop_model::StageState {
                id: "first",
                what: "w",
                writer: "a::b",
                trigger: crate::loop_model::Trigger::Request,
                rows: -1,
            }],
            stops_at: Some("first"),
            reason: Some("probe_failed"),
            status: "unmeasured",
        }
    }

    fn liveness(ok: usize, silent: Vec<&'static str>) -> LivenessReport {
        LivenessReport {
            ran_at: "1970-01-01T00:00:00Z".into(),
            ok,
            silent: silent.len(),
            inert: 0,
            unrunnable: 0,
            undocumented_silent: silent,
            rejected: vec![],
            outcomes: vec![],
        }
    }

    /// A healthy world, used as the control for every falsification below.
    fn healthy_world() -> Observation {
        Observation {
            writes: vec![sink("anomaly_events", 10, 0)],
            gates: vec![gate("grounding", 90, 10)],
            loops: vec![loop_state("loop1", None, None)],
            liveness: Some(liveness(6, vec![])),
            gate_ledger: None,
            // `None`, not an empty census. No evaluator here reads it — the
            // declaration ladder answers panels rather than scoring the
            // platform's machinery — and an empty `Census` would assert every
            // rung is at zero coverage, which is a claim this control world has
            // no business making.
            declarations: None,
            // A conformant contract, so the control is genuinely healthy on
            // this evaluator rather than merely inconclusive. `None` would pass
            // too, and passing because nothing was measured is the reading this
            // whole module exists to refuse.
            conformance: Some(vec![ContractConformance {
                agent: "control_agent",
                pulses: 10,
                with_document: 10,
                graded: 10,
                recorded: 10,
            }]),
            // An intact chain, for the same reason: healthy because it was
            // measured and found whole, not because nothing was looked at.
            delegation: Some((4, 0)),
        }
    }

    /// A contract declared, exercised, and never grading a field is a finding.
    /// This is the falsification for the defect the evaluator was written after:
    /// nine of ten contracts had never graded anything and every surface read it
    /// as grey.
    #[test]
    fn a_contract_that_never_grades_is_a_finding() {
        let mut o = healthy_world();
        o.conformance = Some(vec![ContractConformance {
            agent: "football_analyst",
            pulses: 217,
            with_document: 8,
            graded: 0,
            recorded: 0,
        }]);
        let r = run(&o);
        assert_ne!(r.status, "healthy", "{:#?}", r.scored);
    }

    /// An agent nobody has invoked is untested, not broken. Scoring it as a
    /// finding would put work on somebody for a contract that has had no
    /// occasion to fire.
    #[test]
    fn an_uninvoked_contract_is_not_a_finding() {
        let mut o = healthy_world();
        o.conformance = Some(vec![ContractConformance {
            agent: "hud_field_scout",
            pulses: 0,
            with_document: 0,
            graded: 0,
            recorded: 0,
        }]);
        let r = run(&o);
        assert_eq!(r.status, "healthy", "{:#?}", r.scored);
    }

    /// The gap must name the owner, because the card previously blamed the
    /// author for a platform failure. Prose-only output is the author's; a
    /// document nothing enforced is ours.
    #[test]
    fn the_gap_distinguishes_whose_problem_it_is() {
        let prose = ContractConformance {
            agent: "a",
            pulses: 9,
            with_document: 0,
            graded: 0,
            recorded: 0,
        };
        let unenforced = ContractConformance {
            agent: "b",
            pulses: 9,
            with_document: 9,
            graded: 0,
            recorded: 0,
        };
        assert_eq!(prose.first_gap().unwrap().0, "emits_no_document");
        assert_eq!(unenforced.first_gap().unwrap().0, "never_enforced");
    }

    #[test]
    fn the_control_world_is_healthy() {
        let r = run(&healthy_world());
        assert_eq!(r.status, "healthy", "{:#?}", r.scored);
        assert_eq!(r.findings, 0);
    }

    /// An unread loop must not be absorbed into "idle rather than broken".
    ///
    /// That phrase is a positive claim about every loop the evaluator did not
    /// name as a fault. A loop whose probe did not run has not earned it, and
    /// letting it pass silently is how an observer failure comes to present as a
    /// healthy system.
    #[test]
    fn a_loop_that_could_not_be_read_is_not_reported_as_idle() {
        let mut w = healthy_world();
        w.loops.push(unread_loop_state("loop4"));

        let v = LoopStalledInCode.evaluate(&w);
        assert!(
            matches!(v, Verdict::Inconclusive { .. }),
            "expected Inconclusive, got {v:?}"
        );
        assert!(
            v.severity().is_none(),
            "an unread probe is not a fault in the loop"
        );

        // And it must not be counted as evidence that anything works.
        let pc = PositiveControl.evaluate(&w);
        if let Verdict::Healthy { detail } = &pc {
            assert!(
                detail.contains("1 turning loop(s)"),
                "the unread loop was counted as turning: {detail}"
            );
        }
    }

    /// §5.1 as a structural requirement.
    ///
    /// A check that has never failed has not been tested, and an evaluator that
    /// *cannot* fail is decoration. Because evaluators are pure functions over a
    /// snapshot, the world in which each must fire is a struct literal — so this
    /// is enforceable for the whole registry rather than remembered one
    /// evaluator at a time.
    #[test]
    fn every_evaluator_can_produce_a_finding() {
        let mut broken = healthy_world();
        broken.writes.push(sink("process_spacetime", 340, 340));
        broken.gates.push(gate("coherence", 0, 47));
        broken.gates.push(gate("attachment", 12, 0));
        broken
            .loops
            .push(loop_state("loop4", Some("proposed"), Some("no_trigger")));
        broken.liveness = Some(liveness(0, vec!["forecast_agent_claims"]));
        // A contract declared, exercised, emitting documents, and never graded:
        // the state nine of ten real contracts were in.
        broken.conformance = Some(vec![ContractConformance {
            agent: "prey_locator",
            pulses: 94,
            with_document: 9,
            graded: 0,
            recorded: 0,
        }]);
        // Half the chain dangling: the measured state before `reserve_episode`.
        broken.delegation = Some((12, 6));

        let report = run(&broken);
        let silent: Vec<_> = report
            .scored
            .iter()
            .filter(|s| s.verdict.severity().is_none())
            .map(|s| s.id)
            .collect();

        assert!(
            silent.is_empty(),
            "{} evaluator(s) did not fire in a world built to break every one of \
             them: {silent:?}. An evaluator that cannot produce a finding is \
             decoration, and it will read healthy for ever.",
            silent.len()
        );
        assert_eq!(report.status, "findings");
    }

    /// Each evaluator must fire for its OWN reason, not be carried by another.
    #[test]
    fn each_evaluator_fires_on_its_own_condition_alone() {
        let cases: Vec<(&str, Box<dyn Fn(&mut Observation)>)> = vec![
            (
                "refused_writes",
                Box::new(|o: &mut Observation| o.writes.push(sink("process_spacetime", 340, 340))),
            ),
            (
                "gate_refusing_everything",
                Box::new(|o: &mut Observation| o.gates.push(gate("coherence", 0, 47))),
            ),
            (
                "gate_admitting_everything",
                Box::new(|o: &mut Observation| o.gates.push(gate("attachment", 12, 0))),
            ),
            (
                "loop_stalled_in_code",
                Box::new(|o: &mut Observation| {
                    o.loops
                        .push(loop_state("loop4", Some("proposed"), Some("no_trigger")))
                }),
            ),
            (
                "undocumented_silence",
                Box::new(|o: &mut Observation| {
                    o.liveness = Some(liveness(6, vec!["forecast_agent_claims"]))
                }),
            ),
            (
                "positive_control",
                Box::new(|o: &mut Observation| o.liveness = Some(liveness(0, vec![]))),
            ),
        ];

        for (id, break_it) in cases {
            let mut w = healthy_world();
            break_it(&mut w);
            let r = run(&w);
            let fired: Vec<_> = r
                .scored
                .iter()
                .filter(|s| s.verdict.severity().is_some())
                .map(|s| s.id)
                .collect();
            assert_eq!(
                fired,
                vec![id],
                "breaking only `{id}`'s condition should fire exactly `{id}`, \
                 not {fired:?}. An evaluator that fires on another's condition \
                 makes both findings unactionable."
            );
        }
    }

    /// A registry that concluded nothing must not read healthy.
    #[test]
    fn a_registry_that_answered_nothing_is_not_healthy() {
        let empty = Observation::default();
        let r = run(&empty);
        assert_eq!(r.healthy, 0);
        assert_eq!(r.status, "inconclusive");
        assert_ne!(r.status, "healthy");
    }

    /// A `Notice` is reported and does not fail the report.
    #[test]
    fn a_notice_is_visible_and_not_a_failure() {
        let mut w = healthy_world();
        w.gates.push(gate("attachment", 12, 0));
        let r = run(&w);
        assert_eq!(r.notices, 1);
        assert_eq!(r.findings, 0);
        assert_eq!(
            r.status, "healthy",
            "a gate that has never refused must be visible without asserting \
             that violations must exist"
        );
    }

    #[test]
    fn every_evaluator_declares_a_distinct_question() {
        let mut ids = std::collections::HashSet::new();
        for e in registry() {
            assert!(ids.insert(e.id()), "duplicate evaluator id `{}`", e.id());
            assert!(e.asks().len() > 25, "{}: state the question", e.id());
            assert!(
                e.asks().ends_with('?'),
                "{}: `asks` must be a question",
                e.id()
            );
        }
        assert!(ids.len() >= 5, "a registry this small proves little");
    }

    /// `Inconclusive` must never count as a pass.
    #[test]
    fn inconclusive_is_not_healthy() {
        let v = Verdict::Inconclusive { why: "x".into() };
        assert!(!v.is_failing());
        assert!(v.severity().is_none());
        // ...and the report-level rule is what stops it reading green.
        let r = run(&Observation::default());
        assert_eq!(r.inconclusive, registry().len());
        assert_ne!(r.status, "healthy");
    }

    #[test]
    fn unused_sink_variant_is_reachable() {
        // Guards the test helpers against drifting from the real types.
        let _ = Sink::AnomalyEvents.table();
    }
}
