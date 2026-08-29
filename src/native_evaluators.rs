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
        }
    }
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
        }
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
