//! One artifact, and the checkpoints it passed.
//!
//! # The inversion this serves
//!
//! [`crate::surface`] is the **population-level** trust abstraction: how many
//! loops are turning, how many gates discriminate, how many evaluators are
//! inconclusive. Its primary object is the census, and the UX team's verdict on
//! it was correct — *"it is only legible to someone who already holds the machine
//! in their head, which is the team that built it."*
//!
//! This is the **instance-level** counterpart. Its primary object is the artifact
//! travelling: one episode crossing one belt, passing checkpoints where rungs
//! fire, getting marked or routed to a person. The two are the same structure
//! read from opposite ends:
//!
//! > A loop is a path an artifact takes. A gate is a checkpoint on that path.
//!
//! # It holds no verdict of its own, and that is the rule
//!
//! Every judgement rendered here belongs to a module that already owns it and
//! already has a falsification registered:
//!
//! | part | owner |
//! |---|---|
//! | which rungs, in order, and control-or-metric | [`crate::command_registry`] |
//! | what each rung refuses, and its clock | [`crate::gate_trust::GATES`] |
//! | the per-field grade and the claimed value | [`crate::grounding_trust::graded_fields`] |
//! | the weakest-link floor | [`crate::grounding_trust::floor`] |
//! | where an unverified claim routes | [`crate::assertions::Assertion::route`] |
//! | **why an empty trace is empty** | [`crate::declaration_ladder::attribute`] |
//! | the three-word reading | [`crate::panel_absence::Reading`] |
//!
//! The moment this module computes a grade there are two answers to one question,
//! and the surface is the one people will read. So it assembles and never
//! decides — except for [`reading`], which is a *composition* of the readings
//! above and is registered as such.
//!
//! # The empty trace is the default, not the edge case
//!
//! Measured: **3,571 of 3,576 episodes carry no grounding stamp**, because 89 of
//! 96 real producing agents have no field contract. So a trace with no
//! checkpoints is what this endpoint returns most of the time, and rendering it
//! as a clean journey end-to-end would be the over-read the whole architecture
//! refuses.
//!
//! It is therefore **not** allowed to be blank. `declaration_ladder` supplies the
//! cause, the owner and the next declaration that would change it, so the empty
//! case is a sourced answer rather than this module's guess.

use crate::command_registry::{self, Enforcement};
use crate::declaration_ladder::{self, Legibility, Owner, Silence};
use crate::gate_trust::{self, Clock, Gate};
use crate::grounding_trust::{self, GradedField};
use crate::panel_absence::Reading;
use crate::surface::Caveat;

/// The routes that persist an episode.
///
/// **They do not declare the same belt**, and an earlier version of this comment
/// said they did. `agent.execute` declares four rungs -- `credit`, `attachment`,
/// `grounding`, `input_binding` -- and `agent.execute_stream` declares two,
/// `credit` and `grounding`. `grounding_execute_coverage` holds them to both
/// declaring *grounding*, which is a narrower promise than the one that was
/// written here.
///
/// This matters to any caller that has an episode and wants its belt: `episodes`
/// records no route, so **which of these two an artifact travelled is not
/// recoverable**. See `episode_trace_handler`, which says so in its payload
/// rather than picking one and calling it correct.
///
/// The stream is the one the Fermi Console actually prefers, which is why it is
/// not an afterthought here.
pub const EXECUTE_COMMANDS: &[&str] = &["agent.execute", "agent.execute_stream"];

/// One checkpoint on the belt, as declared, plus what is known about this
/// episode's passage through it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Rung {
    /// The gate id. `grounding`, `credit`, `attachment`, `input_binding`.
    pub rung: &'static str,
    /// `admission` | `invocation` | `standing` — when it fires.
    ///
    /// `standing` is the platform's word for what `compositions_v16` calls
    /// *sweep*. Settled in favour of `standing` because `Clock::Standing`'s own
    /// doc reads *"Boot and sweep"*: sweep is one of the two occasions it covers,
    /// so it is a narrower word for a wider clock.
    pub clock: &'static str,
    /// `control` if it can refuse, `metric` if it only records.
    ///
    /// The distinction a reader most needs and the one a belt diagram most easily
    /// hides: a checkpoint drawn identically whether or not it can stop anything
    /// is a diagram that lies about the platform's safety properties.
    pub enforcement: &'static str,
    /// Required when `enforcement` is not `control`. `command_registry`'s own
    /// words, carried through rather than paraphrased.
    pub why_not_control: Option<&'static str>,
    /// What this rung refuses, from the gate's own declaration.
    pub refuses: &'static str,
    /// The code that runs it, so a finding points at a file.
    pub site: &'static str,
    /// What the **ledger** recorded for this artifact at this rung.
    ///
    /// `None` exactly when [`Rung::decided_absent`] is `Some`. Never both,
    /// never neither -- [`Rung::reports_exactly_one_way`] is that invariant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided: Option<Decided>,
    /// Why there is no recorded decision, when there is none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_absent: Option<Absent>,
    /// What re-running the contract says **now**. `grounding` only.
    ///
    /// Deliberately a sibling of `decided` rather than a field inside it: they
    /// are two independent observations and merging them destroys the one
    /// finding the pair produces. See [`Decided`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recomputed: Option<Recomputed>,
}

impl Rung {
    /// Invariant 2 of the belt contract: a rung reports **one way**.
    ///
    /// Asserted rather than assumed because the two fields are set at different
    /// places -- `belt()` fills the absence from the registry and the handler
    /// overwrites it from the ledger -- so a path that sets one without clearing
    /// the other produces a rung claiming both a verdict and a reason there
    /// isn't one. A client branching on `decided` would then silently ignore a
    /// contradiction the server could have caught.
    pub fn reports_exactly_one_way(&self) -> bool {
        self.decided.is_some() != self.decided_absent.is_some()
    }
}

/// Why a rung has no recorded decision for this artifact.
///
/// # Why a closed token and not a sentence
///
/// It was a sentence. The UX team's report is the argument for changing it: *"a
/// missing ledger row now means three unrelated things, and prose cannot be
/// branched on, so we render one grey ring for all three."*
///
/// And the harm is not symmetric. `credit` decides whether the run may happen at
/// all, so it can never name an artifact — its NULL is **permanent and correct**,
/// and rendering it like a gate that should have recorded and did not puts a
/// standing debt on every belt forever. One of these is nobody's work and one is
/// a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotRecordedReason {
    /// The gate decides **before** the artifact exists, so it can never name
    /// one. Permanent, correct, nobody's work.
    ///
    /// `credit` and `rate_limit`. A refused `credit` check is also the reason
    /// there is no artifact.
    FiresBeforeArtifact,
    /// The gate is `Retention::Counted`: it writes no ledger row at all, by
    /// design. Its counters are process-local and reset on restart.
    ///
    /// Changing this is a decision about durable write volume, not a bug.
    RetentionCounted,
    /// The gate records now, and this artifact is older than the promotion.
    ///
    /// Derived rather than declared: an episode earlier than the gate's first
    /// recorded decision predates its retention. Nothing is backfilled, so this
    /// is permanent for the artifacts it applies to.
    PredatesRetention,
    /// The gate records, the artifact is recent enough, and **there is no row.**
    ///
    /// The only one of the four that is a finding. Either the recorder dropped
    /// it — `gate_trust::ledger_status().dropped` counts that — or the flush
    /// failed, which `write_accounting::Sink::GateDecisions` counts.
    RetainedButAbsent,
}

/// What the ledger says about one episode at one rung.
///
/// # `decided` and `recomputed` are different fields, on purpose
///
/// This enum carries only what the **ledger recorded**. What re-running the
/// contract says now lives in [`Rung::recomputed`], and the two are never merged.
///
/// That is the UX team's request and it is right: migration 221 records that
/// re-running the contract finds 10 violations the ledger never had, because the
/// contract was tightened after those episodes ran. **A recorded `approved`
/// beside a recomputed `2 violations` is the platform's only finding about its
/// own drift** — and it exists only while both numbers reach the client
/// unreconciled. Anything that picked one, or emitted a single `agrees` boolean,
/// would delete the finding while looking tidier.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Decided {
    /// `approved` | `refused` | `undetermined`, from `gate_trust::DECISIONS`.
    ///
    /// `undetermined` is first-class and is never folded into either neighbour.
    /// It is the honest answer for a gate that ran and **could not decide** --
    /// the largest single case being an agent with no field contract, where
    /// grounding has nothing to grade. Migration 221 expects ~3,065 of them.
    /// Folding that into `approved` would colour the majority of the corpus
    /// green on evidence about none of it.
    pub decision: String,
    /// Why it refused. `None` for approvals by design: one reason per pass
    /// would make the table mostly noise.
    pub reason: Option<String>,
    /// When the gate decided, not when the row landed. The recorder batches.
    pub at: Option<String>,
    /// `gate_decisions.id`, so a reviewer can act from the artifact.
    ///
    /// This is what makes `POST /api/gates/:gate_id/decisions/:decision_id/review`
    /// reachable from the trace instead of only from the gate list -- judging a
    /// decision while looking at the thing it was about, rather than having to
    /// find it again in a different surface.
    pub decision_id: Option<i64>,
}

/// Why a rung has **no** recorded decision for this artifact.
///
/// The token is what a client branches on and the sentence is what a person
/// reads. Neither substitutes for the other, which is why both are carried:
/// prose cannot be branched on, and a bare token cannot explain itself.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Absent {
    pub token: NotRecordedReason,
    pub because: String,
}

/// What re-running the contract says **now**, independent of the ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Recomputed {
    pub fields: usize,
    pub violations: usize,
}

/// Assemble the declared belt for an execute route.
///
/// Pure over the registries, so the shape a surface receives is testable without
/// a database. `grounding` is left with no outcome here: the caller fills it from
/// the episode, because only the caller has the response.
pub fn belt(command_id: &str) -> Vec<Rung> {
    let Some(cmd) = command_registry::command(command_id) else {
        return Vec::new();
    };
    cmd.gates
        .iter()
        .map(|g| {
            let spec = gate_trust::GATES.iter().find(|s| s.id == gate_id(g.gate));
            Rung {
                rung: gate_id(g.gate),
                clock: spec.map(|s| clock_word(s.clock)).unwrap_or("unknown"),
                enforcement: match g.enforcement {
                    Enforcement::Control => "control",
                    _ => "metric",
                },
                why_not_control: g.why_not_control,
                refuses: spec.map(|s| s.refuses).unwrap_or("(undeclared gate)"),
                site: g.site,
                // Every rung starts unrecorded and the caller fills in what
                // the ledger holds. Stated rather than omitted: a belt that
                // silently drops the checkpoints it cannot report on is a belt
                // that looks shorter and safer than it is.
                decided: None,
                decided_absent: Some(not_recorded(spec)),
                recomputed: None,
            }
        })
        .collect()
}

/// The default outcome for a rung with no ledger row, from the gate's own
/// declaration.
///
/// Split out so the two reasons that are decidable **without** a database — the
/// gate fires before the artifact, or it is `Counted` — are answered here, and
/// only the two that need a query are left to the caller.
fn not_recorded(spec: Option<&'static gate_trust::GateSpec>) -> Absent {
    let Some(spec) = spec else {
        return Absent {
            token: NotRecordedReason::RetentionCounted,
            because: "this gate is not declared in `gate_trust::GATES`, so \
                      nothing is known about whether it records."
                .to_string(),
        };
    };
    if spec.decides_before_the_artifact {
        return Absent {
            token: NotRecordedReason::FiresBeforeArtifact,
            because: format!(
                "`{}` decides before the artifact exists, so it can never name \
                 one. This absence is permanent and correct — and a refusal \
                 here would be the reason there is no artifact at all.",
                spec.id
            ),
        };
    }
    if spec.retention != gate_trust::Retention::Recorded {
        return Absent {
            token: NotRecordedReason::RetentionCounted,
            because: format!(
                "`{}` is counted in memory only and writes no ledger row, by \
                 design. Its counters are process-local and reset on restart. \
                 Promoting it is a decision about durable write volume rather \
                 than a defect.",
                spec.id
            ),
        };
    }
    Absent {
        token: NotRecordedReason::RetainedButAbsent,
        because: format!(
            "`{}` records its decisions and there is no row for this artifact. \
             Either the artifact predates the promotion, or the recorder dropped \
             it — see `gate_trust::ledger_status().dropped`.",
            spec.id
        ),
    }
}

/// Narrow `RetainedButAbsent` to `PredatesRetention` when the artifact is older
/// than the gate's first recorded decision.
///
/// A caller with the two timestamps can make this distinction and nothing else
/// can: the promotion date is not written down anywhere, so the earliest recorded
/// decision is the only evidence of when the gate started recording. Nothing is
/// backfilled, so for an artifact that predates it the absence is permanent.
///
/// Separated from [`not_recorded`] because it is the only part needing a query,
/// and a function that mixes what it can decide alone with what it must be told
/// invites a caller to skip the telling.
pub fn narrow_by_age(
    absent: Absent,
    episode_at: Option<chrono::DateTime<chrono::Utc>>,
    gate_first_recorded_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Absent {
    if absent.token != NotRecordedReason::RetainedButAbsent {
        return absent;
    }
    let predates = match (episode_at, gate_first_recorded_at) {
        // The gate has recorded nothing ever, so everything predates it.
        (_, None) => true,
        (Some(ep), Some(first)) => ep < first,
        // No timestamp on the artifact: cannot tell, so do not claim to.
        (None, Some(_)) => false,
    };
    if !predates {
        return absent;
    }
    Absent {
        token: NotRecordedReason::PredatesRetention,
        because: "this gate records now, and this artifact is older than its \
                  earliest recorded decision. Nothing is backfilled, so the \
                  absence is permanent for this artifact and is not a finding."
            .to_string(),
    }
}

/// The gate's stable id, from `gate_trust`'s own vocabulary.
fn gate_id(g: Gate) -> &'static str {
    g.spec().id
}

fn clock_word(c: Clock) -> &'static str {
    match c {
        Clock::Admission => "admission",
        Clock::Invocation => "invocation",
        Clock::Standing => "standing",
    }
}

/// One claimed field, dressed for a belt.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Field {
    pub name: &'static str,
    /// **What the agent actually claimed, never stripped.**
    ///
    /// The UX request was explicit and it is right: nulling this destroys the only
    /// evidence that could answer which model fabricates what, and a null cannot
    /// be labelled. It survives here because `enforce` runs on a copy.
    pub value: serde_json::Value,
    /// The ladder rung this field's block earned.
    pub grade: &'static str,
    /// How much reliance the grade carries: 0, 1 or 2.
    ///
    /// Served beside the token because the token alone misleads —
    /// `tool_no_match` sorts above `unavailable_no_tool_source` and both are
    /// **0**. That mistake was made in this repository's own measurement probe
    /// before it was caught.
    pub strength: u8,
    /// The tool that could settle it, when the contract names one. `null` routes
    /// to a person — and per the paper that is also a prioritised request for the
    /// data integration that would close it.
    pub settleable_by: Option<&'static str>,
    /// **Did the agent put a value here at all?**
    ///
    /// The state the surfaces had no word for. A contracted field left null is
    /// not a violation — nothing was fabricated — and it is not a pass either.
    /// It earns its own reading because it implies a different action from every
    /// neighbouring state: **fix the agent**, not the evidence and not the
    /// platform. `squad_value` on the reference episode is two of four values
    /// absent under blocks graded `tool_verified`.
    ///
    /// Grounding cannot see it. It asks whether a tool *could* have supplied a
    /// value, never whether the agent *did* produce one, so an empty field
    /// inherits its block's grade and reads as sourced. That is the one question
    /// on the trace with no gate behind it.
    ///
    /// Deliberately a fact about the document and **not** a verdict in
    /// `assertion_verifications`. Nobody decided it; it is observable from the
    /// retained bytes, which is also why it needs no migration and cannot drift
    /// from its subject.
    ///
    /// **Read it with `absence_expected`.** On its own it says only that the
    /// value is missing, and missing is the *required* answer for an `unsourced`
    /// field. A surface that treats `produced: false` as a fault will report 31
    /// of the platform's 108 contracted fields as failing when they are
    /// complying.
    pub produced: bool,
    /// Why no verdict can be attached to this field, when none can.
    ///
    /// `None` means the field is representable as an assertion and the queue can
    /// hold it. `Some(why)` is the sentence [`crate::assertions::from_graded_field`]
    /// already computes and the platform already throws away.
    ///
    /// It was thrown away, and the trace then said "Nothing queued this claim, so
    /// there is nothing to settle yet" — eleven times on one artifact, which
    /// reads as the queue being broken when the truth was that the agent returned
    /// nothing. Two different remedies wearing one sentence.
    pub not_checkable: Option<&'static str>,
    /// Can [`crate::field_probe`] actually run the tool named above?
    ///
    /// Served rather than inferred from the name, because a surface cannot know
    /// it. Five of the sixteen tools named across the contracts need a
    /// workspace, a memory store or credentials of their own and are not
    /// reachable from a read-only page — and a button the endpoint then refuses
    /// is worse than no button, because the refusal arrives after the click.
    /// Which of the five kinds of claim this is: `sourced`, `unsourced`,
    /// `inferred`, `derived`, `narrative`.
    ///
    /// The question `settleable_by: None` could not answer. It meant "no tool",
    /// which three different situations satisfy, and every surface rendered all
    /// three as *needs a person* — including the two that are nobody's work.
    pub kind: crate::grounding_trust::GroundingKind,
    /// Is the absence of a value **correct** for this field?
    ///
    /// True for `unsourced`, where the contract says no tool exists and the
    /// field must therefore be null. `squad_value` is that case, and its two
    /// absent totals were rendered `not produced` in the colour of a fault,
    /// beside `advanced_metrics.xg` — which is `sourced`, also null, and a real
    /// finding. Opposite situations, one badge.
    pub absence_expected: bool,
    /// Can any verdict ever settle this field?
    ///
    /// False for `inferred`: `assessment`'s own contract says "no database holds
    /// them — which is why they cannot be verified directly". Queuing such a
    /// claim as though it were waiting is how a scoreboard reads empty forever
    /// while appearing to work.
    pub settleable: bool,
    pub tool_runnable: bool,
    /// What the contract says the answer lives in.
    ///
    /// Prose as often as a path: `fixtures/headtohead` sits next to
    /// `standings (rank, points, form, home/away splits)`. Handed to the reader
    /// as a hint for composing the query, never used to compose it — which is
    /// also why the tool call cannot be built by the platform alone.
    pub response_hint: Option<&'static str>,
}

/// Dress the graded fields, and compute the document's weakest link.
///
/// The floor comes from [`grounding_trust::floor`] rather than from a `min` here:
/// it is a trust calculation and it has exactly one implementation.
pub fn fields(agent_id: &str, graded: &[GradedField]) -> (Vec<Field>, &'static str) {
    // Why each field can or cannot carry a verdict, from the one function that
    // decides it. Re-derived rather than remembered: `from_graded_fields` is pure
    // over the graded fields, so asking it here cannot disagree with what the
    // queue did at write time — which a stored copy could, and a second
    // implementation certainly would.
    let (_, skipped) = crate::assertions::from_graded_fields(agent_id, graded);
    let out: Vec<Field> = graded
        .iter()
        .map(|f| Field {
            name: f.path,
            value: f.value.clone(),
            grade: f.provenance,
            strength: grounding_trust::strength(f.provenance),
            settleable_by: f.settleable_by,
            produced: !f.value.is_null(),
            not_checkable: skipped.iter().find(|s| s.path == f.path).map(|s| s.why),
            kind: f.kind,
            absence_expected: f.kind.absence_is_expected(),
            settleable: f.kind.is_settleable(),
            tool_runnable: f.settleable_by.is_some_and(crate::field_probe::is_runnable),
            response_hint: crate::field_probe::response_hint(agent_id, f.path),
        })
        .collect();
    let floor = grounding_trust::floor(graded.iter().map(|f| f.provenance));
    (out, floor)
}

/// The trace's three-word reading, and why.
///
/// A **composition** of readings this module does not own, in the order that
/// makes each one honest:
///
/// 1. **a violation is a fault**, whatever else is true. A field the contract says
///    could have no source, populated anyway, is the thing the reader came for.
/// 2. **nothing graded is `unknown`, and the cause comes from
///    [`declaration_ladder`]** — not from this module. An episode from an agent
///    with no field contract has an empty journey, and that is the agent's
///    missing declaration rather than a platform defect or a clean run. This is
///    the majority case: 3,571 of 3,576.
/// 3. **graded and clean is `idle`** — correctly empty, and narrow. It means the
///    fields under contract were sound, not that the output was.
pub fn reading(
    violations: usize,
    graded: &[GradedField],
    legibility: &Legibility,
) -> (Reading, &'static str, Silence, Owner) {
    if violations > 0 {
        return (
            Reading::Fault,
            "violations",
            Silence::Unresolved,
            Owner::Platform,
        );
    }
    if graded.is_empty() {
        // `traversed = 1`: the episode exists, so something did travel. Passing 0
        // here would let `attribute` reach `NothingTraversed`, which is a claim
        // about throughput and false of an artifact we are looking at.
        let silence = declaration_ladder::attribute(false, legibility, 1);
        return (
            Reading::Unknown,
            "nothing_checked",
            silence,
            declaration_ladder::whose_work(&silence),
        );
    }
    (
        Reading::Idle,
        "checked_clean",
        Silence::NothingTraversed,
        Owner::NoOne,
    )
}

/// What a clean trace does **not** establish.
pub const TRACE_CAVEATS: &[Caveat] = &[
    Caveat {
        subject: "trace.checked_clean",
        checked: "Every field this agent has a contract for was graded, and none \
                  of them violated it.",
        does_not_show: "That the output is correct, or even that most of it was \
                        checked. A field contract covers the fields somebody wrote \
                        an entry for — `grounding_trust::FIELD_CONTRACTS` holds 98 \
                        across 10 agents — and everything else in the document is \
                        unexamined. `Antaxius beieri` is the case to remember: a \
                        bush-cricket reported as a longhorn beetle, with every \
                        check passing because the field was present, non-null and \
                        correctly typed.",
    },
    Caveat {
        subject: "trace.nothing_checked",
        checked: "This agent declares no field contract, so no rung could \
                  produce a per-field verdict.",
        does_not_show: "That the platform failed to check something it should \
                        have. The declaration is the agent author's to make, and \
                        `owner` says so — 89 of 96 real producing agents are in \
                        this state. Rendering it as a platform fault moves a \
                        backlog onto the wrong team; rendering it as a pass is \
                        worse, because it colours 3,571 of 3,576 episodes green \
                        on evidence about none of them.",
    },
    Caveat {
        subject: "trace.belt_route",
        checked: "Every rung `agent.execute` declares is shown, in the order the \
                  command registry declares it.",
        does_not_show: "That this artifact travelled that route. `episodes` \
                        records no route discriminator, and the two commands \
                        that persist an episode declare DIFFERENT belts -- \
                        `agent.execute` four rungs, `agent.execute_stream` two. \
                        A streamed artifact is therefore shown `attachment` and \
                        `input_binding` rungs its route never had. The wider \
                        belt is served deliberately, because the opposite error \
                        drops two real checkpoints for the majority of \
                        artifacts, but it is an unverified claim either way and \
                        `belt_route.recoverable` is `false` for that reason.",
    },
    Caveat {
        subject: "trace.rung.decided_absent",
        checked: "This rung is declared on the route the episode travelled, and \
                  `decided_absent.token` says which of four reasons there is no \
                  ledger row for it.",
        does_not_show: "That anything is wrong. Three of the four tokens are not \
                        findings: `fires_before_artifact` is permanent and \
                        correct for `credit` and `rate_limit`, which decide \
                        whether to run at all and can never name an artifact; \
                        `retention_counted` is a declared design choice about \
                        durable write volume; `predates_retention` is expected \
                        for any artifact older than the gate's promotion, and \
                        nothing is backfilled. Only `retained_but_absent` is a \
                        finding. The rung is shown rather than omitted on \
                        purpose: a belt that drops the checkpoints it cannot \
                        report on looks shorter and safer than it is.",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounding_trust::{PROV_NO_MATCH, PROV_TOOL, PROV_UNAVAILABLE};

    fn graded(path: &'static str, provenance: &'static str) -> GradedField {
        GradedField {
            path,
            block: path.split('.').next().unwrap_or(path),
            value: serde_json::json!(1.0),
            provenance,
            settleable_by: None,
            // `Sourced` despite carrying no tool name: these fixtures exercise
            // the strength ladder, and `Unsourced` would make every one of them
            // a field whose absence is expected, which is a different subject.
            kind: grounding_trust::GroundingKind::Sourced,
        }
    }

    /// An episode nobody could check is `unknown`, and the cause is sourced.
    ///
    /// The majority case — 3,571 of 3,576 — and the one most likely to ship
    /// misleading, because the misleading version looks better. The cause must
    /// come from `declaration_ladder` rather than from a sentence invented here:
    /// this module is not entitled to an opinion about why an agent has not
    /// declared itself.
    #[test]
    fn an_unchecked_episode_is_unknown_and_says_whose_work_it_is() {
        let (r, token, silence, owner) = reading(0, &[], &Legibility::Opaque);
        assert_eq!(r, Reading::Unknown);
        assert_eq!(token, "nothing_checked");
        assert!(
            matches!(silence, Silence::Undeclared { .. }),
            "the cause must be a missing declaration, not a platform gap: {silence:?}"
        );
        assert_eq!(
            owner,
            Owner::AgentAuthor,
            "an agent that has not declared a contract is its author's work; \
             billing it to the platform is a backlog nobody can act on"
        );
    }

    /// `NothingTraversed` must be unreachable for an episode.
    ///
    /// We are looking at an artifact, so something travelled by definition. If
    /// `attribute` were handed `traversed = 0` it could answer *nothing has come
    /// through here*, which is a claim about throughput and false of the row in
    /// front of the reader.
    #[test]
    fn an_episode_is_never_reported_as_no_traffic() {
        for legibility in [
            Legibility::Opaque,
            Legibility::Partial {
                present: vec!["ports"],
                missing: vec!["field_contract"],
            },
            Legibility::Declared,
        ] {
            let (_, _, silence, _) = reading(0, &[], &legibility);
            assert!(
                !matches!(silence, Silence::NothingTraversed),
                "an episode was reported as no traffic under {legibility:?}"
            );
        }
    }

    /// A violation outranks everything, including a fully declared agent.
    #[test]
    fn a_violation_is_a_fault_whatever_else_is_true() {
        let (r, token, _, owner) = reading(1, &[graded("g.x", PROV_TOOL)], &Legibility::Declared);
        assert_eq!(r, Reading::Fault);
        assert_eq!(token, "violations");
        assert_eq!(owner, Owner::Platform);
    }

    /// The floor is the weakest link, and it comes from `grounding_trust`.
    ///
    /// Asserted against `floor` directly rather than a literal, because the point
    /// is that this module does not implement the rule. A hardcoded expectation
    /// would drift from the ladder while still looking like it tested something.
    #[test]
    fn the_documents_floor_is_the_weakest_of_its_blocks() {
        let g = vec![
            graded("a.x", PROV_TOOL),
            graded("b.y", PROV_NO_MATCH),
            graded("c.z", PROV_TOOL),
        ];
        let (dressed, floor) = fields("no_such_agent", &g);
        assert_eq!(dressed.len(), 3);
        assert_eq!(
            floor,
            grounding_trust::floor(g.iter().map(|f| f.provenance))
        );
        assert_eq!(grounding_trust::strength(floor), 0);

        // And the strength travels with every field, because the token alone
        // misleads: `tool_no_match` sorts above `unavailable_no_tool_source` and
        // both are 0.
        let no_match = dressed.iter().find(|f| f.name == "b.y").unwrap();
        assert_eq!(no_match.grade, PROV_NO_MATCH);
        assert_eq!(no_match.strength, 0);
        assert_eq!(
            grounding_trust::strength(PROV_UNAVAILABLE),
            no_match.strength,
            "these two tokens differ in name and not in reliance, which is why \
             the strength is served"
        );
    }

    /// The claimed value is never stripped.
    #[test]
    fn the_belt_carries_what_the_model_actually_claimed() {
        let mut g = graded("genome.estimated_size_mb", PROV_UNAVAILABLE);
        g.value = serde_json::json!("2.4 Gb");
        let (dressed, _) = fields("no_such_agent", &[g]);
        assert_eq!(dressed[0].value, serde_json::json!("2.4 Gb"));
    }

    /// A field the agent left null says so, and says a verdict cannot attach.
    ///
    /// The two halves are separate on purpose. `produced: false` is the state the
    /// surfaces had no word for — nothing fabricated, nothing supplied, and the
    /// remedy is the agent rather than the evidence. `not_checkable` is why the
    /// queue holds nothing for it, which the platform computed and discarded, and
    /// which the trace then reported as its own queue being empty.
    #[test]
    fn an_absent_value_is_reported_as_absent_rather_than_as_unqueued() {
        let mut g = graded("squad_value.arsenal_total", PROV_UNAVAILABLE);
        g.value = serde_json::Value::Null;
        let (dressed, _) = fields("no_such_agent", &[g]);

        assert!(
            !dressed[0].produced,
            "a null contracted field must read as not produced; grounding cannot \
             see this, so nothing else on the platform reports it"
        );
        let why = dressed[0]
            .not_checkable
            .expect("a null field cannot be queued, and the reason must travel with it");
        assert!(
            why.len() > 20,
            "the reason is the whole point: `nothing queued this claim` sent the \
             reader to look at the queue, when the fault was the agent's"
        );
    }

    /// A value that IS there carries no obstruction.
    ///
    /// The mirror, because a field that reports a reason it does not have would
    /// make every row look blocked and the distinction worthless.
    #[test]
    fn a_present_value_reports_no_obstruction() {
        let mut g = graded("league_context.season", PROV_TOOL);
        g.value = serde_json::json!("2024-25");
        let (dressed, _) = fields("no_such_agent", &[g]);
        assert!(dressed[0].produced);
    }

    /// Both execute routes declare a belt, and every rung says whether it can
    /// refuse.
    ///
    /// The `enforcement` field is the one a diagram most easily hides: a
    /// checkpoint drawn identically whether or not it can stop anything is a
    /// diagram that lies about the platform's safety properties. Grounding on
    /// `/execute` is the live case — `command_registry` declares it a `metric`
    /// with the reason attached.
    #[test]
    fn every_rung_says_whether_it_can_actually_refuse() {
        for id in EXECUTE_COMMANDS {
            let b = belt(id);
            assert!(!b.is_empty(), "`{id}` declares no gates");
            for r in &b {
                assert!(
                    matches!(r.enforcement, "control" | "metric"),
                    "{}: enforcement is `{}`",
                    r.rung,
                    r.enforcement
                );
                if r.enforcement == "metric" {
                    assert!(
                        r.why_not_control.is_some_and(|w| w.len() > 40),
                        "`{}` on `{id}` is a metric and does not say why. A gate \
                         demoted to a metric is a decision somebody made, and \
                         the reason is what tells a later reader whether it was \
                         deliberate or drift.",
                        r.rung
                    );
                }
                assert!(
                    matches!(r.clock, "admission" | "invocation" | "standing"),
                    "{}: clock is `{}`",
                    r.rung,
                    r.clock
                );
            }
            // The grounding rung must be on both belts, or the trace's only
            // gradeable checkpoint is missing from one of the two paths callers
            // actually use.
            assert!(
                b.iter().any(|r| r.rung == "grounding"),
                "`{id}` does not declare the grounding rung"
            );
        }
    }

    /// **Invariant 2.** Exactly one of `decided` / `decided_absent`, everywhere.
    ///
    /// The two fields are filled in different places — `belt()` fills the
    /// absence from the gate registry and the handler overwrites it from the
    /// ledger — so the failure this catches is a path that sets a verdict
    /// without clearing the absence beside it. A client branching on `decided`
    /// would never notice; it would just quietly ignore a rung that claimed both
    /// a verdict and a reason there wasn't one.
    #[test]
    fn every_rung_reports_exactly_one_way() {
        for id in EXECUTE_COMMANDS {
            let b = belt(id);
            assert!(!b.is_empty(), "`{id}` declares no gates");
            for r in &b {
                assert!(
                    r.reports_exactly_one_way(),
                    "`{}` on `{id}` has decided={} and decided_absent={} — \
                     exactly one must be set",
                    r.rung,
                    r.decided.is_some(),
                    r.decided_absent.is_some()
                );
            }
        }
    }

    /// **Invariants 3, 4 and 7.** The absence token is the gate's own
    /// declaration, and it always carries a sentence.
    ///
    /// Asserted against `gate_trust::GATES` rather than a literal list of gate
    /// names, which is the whole point: a literal list is a second declaration
    /// of the same fact and it drifts. Migration 219 exists because exactly that
    /// happened one layer down — `GATE_IDS` gained `output_schema`, 214's CHECK
    /// was widened to match and 216's was not, so a decision was recordable and
    /// its review was not.
    ///
    /// The harm is asymmetric and that is why it is pinned. `credit` and
    /// `rate_limit` decide whether to run at all, so their NULL is permanent and
    /// correct; mislabelling it as a gap puts a standing debt on every belt
    /// forever, and a debt that can never be paid is one a reader learns to
    /// ignore — including on the rungs where it is real.
    #[test]
    fn the_absence_token_comes_from_the_gate_registry() {
        let mut checked = 0usize;
        for id in EXECUTE_COMMANDS {
            for r in belt(id) {
                let spec = gate_trust::GATES
                    .iter()
                    .find(|s| s.id == r.rung)
                    .unwrap_or_else(|| {
                        panic!("`{}` is on `{id}`'s belt and not in `GATES`", r.rung)
                    });
                let a = r
                    .decided_absent
                    .as_ref()
                    .expect("`belt()` is pure over the registries and records no verdicts");

                assert_eq!(
                    a.token == NotRecordedReason::FiresBeforeArtifact,
                    spec.decides_before_the_artifact,
                    "`{}`: token is {:?} but `decides_before_the_artifact` is {}",
                    r.rung,
                    a.token,
                    spec.decides_before_the_artifact
                );

                // Only meaningful for gates that could have recorded at all.
                // Firing before the artifact outranks retention: a gate that can
                // never name an artifact is not withholding a row it owed.
                if !spec.decides_before_the_artifact {
                    assert_eq!(
                        a.token == NotRecordedReason::RetentionCounted,
                        spec.retention != gate_trust::Retention::Recorded,
                        "`{}`: token is {:?} but retention is {:?}",
                        r.rung,
                        a.token,
                        spec.retention
                    );
                }

                // Invariant 7. The token is what a client branches on; the
                // sentence is the only part a person reads, and it is the one
                // thing a client cannot generate for itself.
                assert!(
                    a.because.trim().len() > 40,
                    "`{}` on `{id}` gives token {:?} with no usable sentence",
                    r.rung,
                    a.token
                );
                checked += 1;
            }
        }
        // `all()` over an empty iterator is `true`, and a belt walk that found
        // nothing would pass every assertion above.
        // Measured, not guessed: `agent.execute` declares 4 rungs and
        // `agent.execute_stream` declares 2. The first version of this line said
        // 8 -- it assumed the two belts matched, which is the same false
        // assumption two comments in this file made, and the guard caught it.
        assert_eq!(
            checked,
            6,
            "expected 6 rungs across {} commands (4 on `agent.execute`, 2 on \
             `agent.execute_stream`). A different number means a gate was added \
             or removed from a belt, which is a change to what the platform \
             claims it checks -- update this and say which.",
            EXECUTE_COMMANDS.len()
        );
    }

    /// **Invariants 5 and 6.** `recomputed` is never pre-filled, and there are
    /// exactly three verdicts.
    ///
    /// `belt()` is pure over the registries and has no episode, so it cannot
    /// know what a re-run would say. Pre-filling `recomputed` here — with a zero,
    /// most plausibly — would put "0 violations" on every rung of every belt,
    /// which reads as a clean run and is a claim about a document this function
    /// has never seen.
    #[test]
    fn the_declared_belt_asserts_nothing_about_an_episode() {
        for id in EXECUTE_COMMANDS {
            for r in belt(id) {
                assert!(
                    r.recomputed.is_none(),
                    "`{}` on `{id}` arrives with a recomputation and `belt()` has \
                     no episode to have computed it from",
                    r.rung
                );
            }
        }
        // Invariant 6, at its source. `undetermined` is the one that gets
        // dropped: it is the awkward third reading, it is the expected verdict
        // for the majority of the corpus, and folding it into either neighbour
        // is how an absent check becomes indistinguishable from a passing one.
        assert_eq!(
            gate_trust::DECISIONS,
            &["approved", "refused", "undetermined"],
            "the belt contract serves exactly three verdicts and this is the \
             vocabulary it serves them from"
        );
    }

    /// `narrow_by_age` calls an absence permanent only on evidence.
    ///
    /// The distinction is the difference between a calm explanation and the only
    /// finding of the four, so getting it wrong in the quiet direction hides a
    /// dropped recorder row behind "this is old, nothing to see".
    #[test]
    fn an_absence_is_only_permanent_with_a_timestamp_to_prove_it() {
        let t0 = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let t1 = chrono::DateTime::parse_from_rfc3339("2026-06-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let absent = || Absent {
            token: NotRecordedReason::RetainedButAbsent,
            because: "seed".to_string(),
        };

        // Older than the gate's first recorded decision: permanent, not a finding.
        assert_eq!(
            narrow_by_age(absent(), Some(t0), Some(t1)).token,
            NotRecordedReason::PredatesRetention
        );
        // Newer: the gate was recording and there is still no row. A finding.
        assert_eq!(
            narrow_by_age(absent(), Some(t1), Some(t0)).token,
            NotRecordedReason::RetainedButAbsent
        );
        // No timestamp on the artifact — cannot tell, so must not claim to.
        assert_eq!(
            narrow_by_age(absent(), None, Some(t0)).token,
            NotRecordedReason::RetainedButAbsent
        );
        // The gate has recorded nothing ever, so everything predates it.
        assert_eq!(
            narrow_by_age(absent(), Some(t1), None).token,
            NotRecordedReason::PredatesRetention
        );
        // It narrows one token and passes the other three through untouched.
        for t in [
            NotRecordedReason::FiresBeforeArtifact,
            NotRecordedReason::RetentionCounted,
            NotRecordedReason::PredatesRetention,
        ] {
            let a = Absent {
                token: t,
                because: "seed".to_string(),
            };
            assert_eq!(
                narrow_by_age(a, Some(t0), Some(t1)).token,
                t,
                "{t:?} was rewritten by a narrowing that only owns \
                 `retained_but_absent`"
            );
        }
    }

    /// Every caveat is a caveat, and the majority state has one.
    #[test]
    fn the_default_reading_carries_a_caveat() {
        let problems = crate::surface::caveat_problems(TRACE_CAVEATS);
        assert!(problems.is_empty(), "\n  {}\n", problems.join("\n  "));
        for subject in [
            "trace.checked_clean",
            "trace.nothing_checked",
            "trace.rung.decided_absent",
        ] {
            assert!(
                TRACE_CAVEATS.iter().any(|c| c.subject == subject),
                "`{subject}` is a state this surface renders and it carries no \
                 caveat"
            );
        }
    }
}
