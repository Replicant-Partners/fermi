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

/// The route whose belt an episode travelled.
///
/// Two commands persist an episode, and they declare the same rungs in the same
/// order because `grounding_execute_coverage` holds them to it. The stream is the
/// one the Fermi Console actually prefers, which is why it is not an afterthought
/// here.
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
    /// What is known about this episode at this rung.
    pub outcome: Outcome,
}

/// What the platform can say about one episode at one rung.
///
/// Three states, and the middle one is the honest majority. Collapsing it into
/// either neighbour is how a trace comes to look complete.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    /// The rung produced a per-field verdict, recomputed from the retained
    /// response.
    ///
    /// Only `grounding` can reach this today, and only because migration 199
    /// retained `response_text` — so the contract can be re-run over the exact
    /// bytes the agent produced rather than trusting a summary of them.
    Graded { fields: usize, violations: usize },
    /// The rung ran, and **its individual decision for this episode was not
    /// recorded.**
    ///
    /// Not a pass and not a failure. `gate_trust` counts decisions in memory and
    /// `gate_decisions` holds the two `Retention::Recorded` gates' rows, but
    /// nothing joins a row to an episode: `gate_decisions.episode_id` does not
    /// exist. One column would change this, and until it does the honest answer
    /// is that the platform knows the rung ran and not what it said here.
    NotRecorded { because: &'static str },
    /// The rung cannot apply to this episode, and why.
    NotApplicable { because: String },
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
                // Every rung starts here. `gate_decisions` has no `episode_id`,
                // so nothing can join a recorded decision to this artifact, and
                // saying so is more useful than omitting the rung — a belt that
                // silently drops the checkpoints it cannot report on is a belt
                // that looks shorter and safer than it is.
                outcome: Outcome::NotRecorded {
                    because: "this gate's decisions are counted, and \
                              `gate_decisions` carries no `episode_id`, so no \
                              row can be joined to this artifact. One column \
                              would change it.",
                },
            }
        })
        .collect()
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
}

/// Dress the graded fields, and compute the document's weakest link.
///
/// The floor comes from [`grounding_trust::floor`] rather than from a `min` here:
/// it is a trust calculation and it has exactly one implementation.
pub fn fields(graded: &[GradedField]) -> (Vec<Field>, &'static str) {
    let out: Vec<Field> = graded
        .iter()
        .map(|f| Field {
            name: f.path,
            value: f.value.clone(),
            grade: f.provenance,
            strength: grounding_trust::strength(f.provenance),
            settleable_by: f.settleable_by,
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
        subject: "trace.rung.not_recorded",
        checked: "This rung is declared on the route the episode travelled, and \
                  `gate_trust` counts its decisions.",
        does_not_show: "What it decided about THIS artifact. `gate_decisions` \
                        carries no `episode_id`, so no recorded decision can be \
                        joined to an episode, and the counters are process-local \
                        for five of the seven gates. The rung is shown rather \
                        than omitted on purpose: a belt that drops the \
                        checkpoints it cannot report on looks shorter and safer \
                        than it is.",
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
        let (dressed, floor) = fields(&g);
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
        let (dressed, _) = fields(&[g]);
        assert_eq!(dressed[0].value, serde_json::json!("2.4 Gb"));
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

    /// Every caveat is a caveat, and the majority state has one.
    #[test]
    fn the_default_reading_carries_a_caveat() {
        let problems = crate::surface::caveat_problems(TRACE_CAVEATS);
        assert!(problems.is_empty(), "\n  {}\n", problems.join("\n  "));
        for subject in [
            "trace.checked_clean",
            "trace.nothing_checked",
            "trace.rung.not_recorded",
        ] {
            assert!(
                TRACE_CAVEATS.iter().any(|c| c.subject == subject),
                "`{subject}` is a state this surface renders and it carries no \
                 caveat"
            );
        }
    }
}
