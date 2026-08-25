//! Has this check ever been shown to go red?
//!
//! # The gap
//!
//! Every check in the trust modules was broken by hand and watched fail. Three
//! of them turned out to be incapable of catching their own motivating case:
//!
//! * the write-accounting scan was satisfied by a *declaration* rather than a
//!   call site, and passed while the instrumentation it checked had been
//!   deleted;
//! * the swallow detector could not see `if let Err(e) = … { warn!(…) }` — the
//!   commonest shape in this codebase and the shape of every defect the audit
//!   found;
//! * the refusal-ordering scan missed a seven-line gap that `rustc` catches
//!   exactly.
//!
//! All three were caught by habit. **Nothing in the build enforced that a check
//! had ever fired**, so the discipline would have left with its author.
//!
//! `native_evaluators::every_evaluator_can_produce_a_finding` is the exception,
//! and the reason it works is structural: an evaluator is a pure function over
//! a snapshot, so the world in which it must fire is a struct literal. This
//! file generalises that.
//!
//! # The convention, and it is the only subtle part
//!
//! Both closures return **the check's own permissive verdict** — *"nothing to
//! see here"*. So:
//!
//! * [`Falsification::passes`] runs the check in a world where it must be quiet
//!   and must come back `true`;
//! * [`Falsification::fires`] runs it in a world where it must speak and must
//!   come back **`false`**.
//!
//! A function that returns a value rather than a verdict is adapted at the call
//! site — `strength(v) >= 2`, `diagnose(..) == "no_input"` — and the adapter is
//! part of the falsification: it states which reading is the optimistic one.
//!
//! # One pair per incident, not per branch
//!
//! A registered check has been shown to distinguish the world it was written
//! for from a world where it must be quiet. It has **not** been shown to cover
//! every branch, and a reader should not assume it has. Two of
//! `outcome_trust::classify_discrimination`'s arms survive their own deletion
//! here and are caught by that module's unit tests instead — which is correct
//! under the rule that every finding is asserted by exactly one tier, and would
//! be a hole if nothing asserted them at all. Both were verified by deletion
//! before being left to the other tier.
//!
//! A branch gets its own pair when it models a *separate incident*. Three of
//! `agent_params_hook::classify_claim`'s pairs exist for that reason, and the
//! second and third only because the first two breaks came back green.
//!
//! # What this file cannot reach, said plainly
//!
//! The registry holds **pure library functions**. All three of the misses above
//! were **scans over source text**, living in `tests/`, whose world is the
//! filesystem. A registry that covered only the easy half, while every incident
//! it cites lies in the half it cannot see, would be the audit's own recurring
//! defect wearing the machinery built to prevent it.
//!
//! So there is a second half: [`SCANS`] requires every corpus-walking suite to
//! **name the test that proves its detector can fire**, and asserts that test
//! exists. That is a declaration check and is stated as one — it cannot tell a
//! real falsifier from a well-named empty one. What it does is make the absence
//! of a falsifier impossible to add silently, which is exactly what happened
//! five times before this file existed.
//!
//! # Every assertion here was broken and watched go red
//!
//! Including, on the first attempt, one that did not.
//! `every_falsification_names_the_incident_it_models` was broken by replacing
//! the first *line* of a `models` string — which left the remaining
//! continuation lines intact, so the value was still over the threshold and the
//! test stayed green. The edit applied; the state it was named for never
//! existed. That is the audit's first rule of method, committed in the file
//! written to enforce it, and it is why the harness now asserts the resulting
//! state rather than the substitution.

use fermi::gate_trust::GateAccount;
use fermi::grounding_trust as gt;
use fermi::liveness_trust::{self as lt, Expectation, LivenessContract, LivenessReport, Status};
use fermi::loop_api;
use fermi::loop_model::{self as lm, LoopState, Stage, StageState, Trigger, Upstream};
use fermi::native_evaluators::{Observation, Severity, Verdict};
use fermi::outcome_trust::{
    classify_discrimination as disc, classify_producers, classify_reach, known_gap, reach_pct,
    Discrimination, EventSpread, Producers, Reach,
};
use fermi::panel_absence::{self as pa, Kind, Panel, Reading, Resolver, Scope};
// `loop_api` is in `TRUST_MODULES`, so its public decisions must be registered
// or exempted — the same coverage scan that owns the other ten.
use fermi::projection_kind as pk;
use fermi::write_accounting::SinkAccount;
use std::path::Path;

// The claim-outcome decision lives in the binary crate's handler tree, which an
// integration test cannot reach, so it is re-exported by the library for this
// purpose. See `fermi::claim_outcome`.
use fermi::claim_outcome::{classify_claim, ClaimBinding, ClaimOutcome};

/// One check, and the two worlds that prove it can tell them apart.
struct Falsification {
    /// The decision under test, as a reader would name it. `module::function`,
    /// or `module::Type::method`.
    check: &'static str,
    owner: &'static str,
    /// A world in which the check must be quiet. Returns the check's permissive
    /// verdict, so this must be `true`.
    passes: fn() -> bool,
    /// A world in which it must fire. **The one that matters.** Returns the
    /// same permissive verdict, so this must be `false`.
    fires: fn() -> bool,
    /// What real defect the `fires` case is modelled on.
    ///
    /// Required. A falsification nobody can trace to an incident is a guess
    /// about how the check breaks, and the guesses are what missed three times.
    models: &'static str,
}

// ── worlds that need more than an expression ────────────────────────────

fn contract(sink: &'static str, expectation: Expectation) -> LivenessContract {
    LivenessContract {
        sink,
        writer: "a::b",
        sink_sql: "SELECT 0::bigint AS writes",
        opportunity_sql: "SELECT 0::bigint AS opportunities",
        expectation,
        requires: None,
        accounted: None,
        why: "a fixture",
        remediation: "a fixture",
    }
}

fn report(ok: usize, rejected: Vec<&'static str>) -> LivenessReport {
    LivenessReport {
        ran_at: "fixture".into(),
        ok,
        silent: 0,
        inert: 0,
        unrunnable: 0,
        undocumented_silent: vec![],
        rejected,
        outcomes: vec![],
    }
}

fn stage(trigger: Trigger) -> Stage {
    Stage {
        id: "x",
        what: "w",
        sink_sql: "SELECT 0::bigint AS n",
        writer: "a::b",
        trigger,
        accounted: None,
        gated_by: None,
    }
}

fn sink_account(attempts: u64, failures: u64) -> SinkAccount {
    SinkAccount {
        table: "t",
        writer: "a::b",
        attempts,
        failures,
        last_error: None,
    }
}

fn gate_account(approved: u64, refused: u64) -> GateAccount {
    GateAccount {
        id: "g",
        clock: fermi::gate_trust::Clock::Standing,
        retention: fermi::gate_trust::Retention::Counted,
        site: "a::b",
        approved,
        refused,
        undetermined: 0,
        last_refusal: None,
        reading: None,
        if_never_refuses: "a fixture",
    }
}

/// A platform-scoped panel backed by one loop stage.
fn loop_panel() -> Panel {
    Panel {
        id: "fixture.panel",
        kind: Kind::Register,
        scope: Scope::Platform,
        surface: "templates/fixture.html",
        shows: "a fixture",
        resolved_by: Resolver::LoopStage {
            loop_id: "loopX",
            stage: "x",
        },
        if_empty: "a fixture",
    }
}

fn loop_state(stops_at: Option<&'static str>, reason: Option<&'static str>) -> LoopState {
    LoopState {
        id: "loopX",
        name: "fixture",
        scope: "platform",
        claim: "a fixture",
        stages: vec![StageState {
            id: "x",
            what: "w",
            writer: "a::b",
            trigger: Trigger::Request,
            rows: if stops_at.is_some() { 0 } else { 1 },
        }],
        stops_at,
        reason,
        status: if stops_at.is_some() {
            "stalled"
        } else {
            "turning"
        },
    }
}

fn observation(l: LoopState) -> Observation {
    Observation {
        loops: vec![l],
        ..Default::default()
    }
}

/// The row shape that actually fills `observations` from the dynamics runner.
///
/// The tag comes from the owning module rather than being typed here, for the
/// reason the owning module exists: a fixture with its own copy of the literal
/// is a second reader, and every second reader of this tag agreed with every
/// other one and disagreed with the writer.
///
/// `projection_predicate_coverage` enforces that, and it enforced it against
/// this file: the first version spelled the tag in a JSON literal and in a
/// function name, and the scan named all five lines.
fn model_output_row() -> serde_json::Value {
    serde_json::json!({
        "projection_id": "proj-coupled-e0fceb",
        "source_kind": pk::SOURCE_KIND_DYNAMICS_PROJECTION,
        "model_uri": "kask:dynamics/kombucha_f2_carbonation@v1",
    })
}

fn sensor_reading() -> serde_json::Value {
    serde_json::json!({ "source": "sensor_ingest", "device": "ph-probe-2" })
}

/// `total` multi-subject events, of which `varied` have more than one value.
fn varied(total: usize, varied: usize) -> Vec<EventSpread> {
    (0..total)
        .map(|i| EventSpread {
            subjects: 4,
            distinct_values: if i < varied { 3 } else { 1 },
        })
        .collect()
}

// ── the registry ────────────────────────────────────────────────────────

const FALSIFICATIONS: &[Falsification] = &[
    // ── liveness_trust ──────────────────────────────────────────────────
    Falsification {
        check: "liveness_trust::classify",
        owner: "src/liveness_trust.rs",
        passes: || lt::classify(3, 5).is_pass(),
        fires: || lt::classify(0, 5).is_pass(),
        models: "`forecast_agent_claims` — coded, wired, exhaustively \
                 commented, and 0 rows against a live opportunity count. A \
                 classifier that reads writes=0 with opportunities>0 as healthy \
                 is the defect the whole rung was built for.",
    },
    Falsification {
        check: "liveness_trust::Status::is_pass",
        owner: "src/liveness_trust.rs",
        passes: || Status::Ok.is_pass(),
        fires: || Status::Inert.is_pass(),
        models: "`Inert` is not a pass. A contract watching a feature nobody \
                 has exercised, reporting healthy, is the original defect \
                 wearing the machinery built to prevent it. `Unrunnable` is the \
                 same argument: a check that could not run reports healthy for \
                 ever.",
    },
    Falsification {
        check: "liveness_trust::is_actionable_silence",
        owner: "src/liveness_trust.rs",
        // Permissive reading: "this silence needs no action."
        passes: || !lt::is_actionable_silence(&contract("s", Expectation::Conditional)),
        fires: || !lt::is_actionable_silence(&contract("s", Expectation::EveryOpportunity)),
        models: "The runner script excused `Conditional` contracts and the \
                 library did not, so `anomaly_events` was listed as \
                 silent-with-no-excuse by one and clean by the other. Nothing \
                 noticed until `UndocumentedSilence` read the library's copy on \
                 its first production run — §3.4, and the copy that got \
                 believed was the one nearest the reader.",
    },
    Falsification {
        check: "liveness_trust::LivenessReport::has_positive_control",
        owner: "src/liveness_trust.rs",
        passes: || report(1, vec![]).has_positive_control(),
        fires: || report(0, vec![]).has_positive_control(),
        models: "`0 live` cannot distinguish \"every path is broken\" from \
                 \"the runner is broken\". The same rule as every scan in this \
                 repository asserting its own corpus is non-empty: a suite over \
                 an empty set passes for ever.",
    },
    Falsification {
        check: "liveness_trust::LivenessReport::is_healthy",
        owner: "src/liveness_trust.rs",
        passes: || report(1, vec![]).is_healthy(),
        fires: || report(1, vec!["anomaly_events"]).is_healthy(),
        models: "`severity = \"L1\"` against `('info','warning','critical')` — \
                 the writer ran and the database refused it, every time, in a \
                 spawned task. No exemption list excuses a statement Postgres \
                 will not accept, so `rejected` must defeat an otherwise clean \
                 report.",
    },
    // ── loop_model ──────────────────────────────────────────────────────
    //
    // Three entries for one function. `diagnose` ranks four different readings
    // of the same zero and each ranking was a separate incident, so one pair
    // would prove only that the function is not a constant.
    Falsification {
        check: "loop_model::diagnose",
        owner: "src/loop_model.rs",
        // Permissive reading: "everything above produced and this stage has
        // simply had no occasion" — a fact about the world, not the code.
        passes: || {
            lm::diagnose(&stage(Trigger::Request), Upstream::Produced, |_| None) == "no_input"
        },
        fires: || {
            lm::diagnose(
                &stage(Trigger::None {
                    why: "nothing calls it",
                }),
                Upstream::Produced,
                |_| None,
            ) == "no_input"
        },
        models: "Loop 3's `intentions`: six tools implemented, wired to \
                 dispatch, exposed to every workspace agent, and no prompt \
                 anywhere asking for them. An uncalled writer and an \
                 unexercised one produce identical row counts, so `no_trigger` \
                 is the only place the difference can be recorded.",
    },
    Falsification {
        check: "loop_model::diagnose",
        owner: "src/loop_model.rs",
        passes: || {
            lm::diagnose(&stage(Trigger::Request), Upstream::Produced, |_| None) == "no_input"
        },
        fires: || {
            let s = Stage {
                accounted: Some(fermi::write_accounting::Sink::AnomalyEvents),
                ..stage(Trigger::Request)
            };
            lm::diagnose(&s, Upstream::Produced, |_| None) == "no_input"
        },
        models: "`write_accounting`'s counters are process-local `AtomicU64`s \
                 that start at zero, so a fresh server has watched nothing. In \
                 that state `no_input` and \"this path has been failing since \
                 before the restart\" are indistinguishable, and answering \
                 `no_input` claims the first.",
    },
    Falsification {
        check: "loop_model::diagnose",
        owner: "src/loop_model.rs",
        passes: || {
            lm::diagnose(&stage(Trigger::Manual), Upstream::Produced, |_| None) == "no_input"
        },
        fires: || {
            lm::diagnose(
                &stage(Trigger::Prompted { asked_by: "p" }),
                Upstream::Produced,
                |_| None,
            ) == "no_input"
        },
        models: "Wiring Loop 3's caller would have moved its report from \
                 `NOTHING CALLS IT` to `no_input` without a single row being \
                 written — from a fault we can see to \"the world was quiet\". \
                 A prompted stage produces nothing until a model obliges, and a \
                 row count cannot tell that from a prompt that never ran.",
    },
    Falsification {
        check: "loop_model::StageState::measured",
        owner: "src/loop_model.rs",
        passes: || {
            StageState {
                id: "x",
                what: "w",
                writer: "a::b",
                trigger: Trigger::Request,
                rows: 0,
            }
            .measured()
        },
        fires: || {
            StageState {
                id: "x",
                what: "w",
                writer: "a::b",
                trigger: Trigger::Request,
                rows: -1,
            }
            .measured()
        },
        models: "`rows` carries `-1` for a count query that did not run, \
                 documented as \"never confused with zero\" — and the walk then \
                 confused it with *success*: `rows == 0` was the only condition \
                 that stopped a chain, so a loop whose first probe errored while \
                 its later stages held rows reported `turning`, with no stall \
                 and no reason.",
    },
    Falsification {
        check: "loop_model::LoopState::measured",
        owner: "src/loop_model.rs",
        passes: || loop_state(None, None).measured(),
        fires: || {
            let mut l = loop_state(None, None);
            l.stages[0].rows = -1;
            l.measured()
        },
        models: "The loop-level half of the same defect. A caller counting \
                 turning loops must exclude an unread one rather than fold it \
                 into either verdict; `native_evaluators::LoopStalledInCode` \
                 downgrades to `Inconclusive` on exactly this.",
    },
    // ── write_accounting ────────────────────────────────────────────────
    Falsification {
        check: "write_accounting::SinkAccount::is_totally_rejected",
        owner: "src/write_accounting.rs",
        // Permissive reading: "this write path is not being refused outright."
        passes: || !sink_account(5, 1).is_totally_rejected(),
        fires: || !sink_account(5, 5).is_totally_rejected(),
        models: "Loop 2's seed. Every grounding anomaly the handler built was \
                 refused by `anomaly_events_severity_check`, in a `tokio::spawn` \
                 whose error went to `tracing::warn!`, for the life of the \
                 feature. Liveness asks about rows and cannot see it; this asks \
                 about attempts and is the only reading that can.",
    },
    // ── gate_trust ──────────────────────────────────────────────────────
    Falsification {
        check: "gate_trust::GateAccount::never_asked",
        owner: "src/gate_trust.rs",
        passes: || !gate_account(1, 0).never_asked(),
        fires: || !gate_account(0, 0).never_asked(),
        models: "A gate never exercised is not a pass — the same rule as \
                 liveness's `Inert`. A control that is not wired and a control \
                 that has had nothing to refuse produce identical observations \
                 everywhere else, which is how `hud_contract::enforce` came to \
                 be a thousand lines of safety gate with no production caller.",
    },
    Falsification {
        check: "gate_trust::GateAccount::refuses_everything",
        owner: "src/gate_trust.rs",
        passes: || !gate_account(1, 1).refuses_everything(),
        fires: || !gate_account(0, 3).refuses_everything(),
        models: "The Γ bug's signature: the gate ran, and approved nothing. An \
                 inverted control is worse than an absent one because the \
                 counters look busy.",
    },
    Falsification {
        check: "gate_trust::GateAccount::admits_everything",
        owner: "src/gate_trust.rs",
        passes: || !gate_account(1, 1).admits_everything(),
        fires: || !gate_account(3, 0).admits_everything(),
        models: "Reported, never asserted — a gate legitimately refuses nothing \
                 when nothing has warranted refusal. Registered here anyway \
                 because the reading is the only thing that distinguishes a \
                 control that never fires from one that is not wired, and a \
                 predicate that cannot produce it is silently the latter.",
    },
    // ── seam_vocabulary ─────────────────────────────────────────────────
    Falsification {
        check: "seam_vocabulary::tokens_in_constraint",
        owner: "src/seam_vocabulary.rs",
        // Permissive reading: "a vocabulary was read out of this constraint."
        passes: || {
            !fermi::seam_vocabulary::tokens_in_constraint(
                "CHECK ((kind = ANY (ARRAY['drift'::text, 'safety'::text])))",
            )
            .is_empty()
        },
        fires: || {
            !fermi::seam_vocabulary::tokens_in_constraint("CHECK ((score >= 0.0))").is_empty()
        },
        models: "A constraint with no string literals must yield nothing rather \
                 than something wrong. The live tier treats an empty parse as a \
                 failure to read, not as an empty vocabulary — without that, a \
                 column whose CHECK the parser cannot understand reports as \
                 agreeing with Rust about the empty set.",
    },
    // ── projection_kind ─────────────────────────────────────────────────
    Falsification {
        check: "projection_kind::is_measurement",
        owner: "src/projection_kind.rs",
        passes: || pk::is_measurement(&sensor_reading()),
        fires: || pk::is_measurement(&model_output_row()),
        models: "`resolve_against_projection` negated the predicate by hand, \
                 comparing against the simulation tag with `!=`, which \
                 classified all 12,167 dynamics projections as real \
                 measurements. Nothing came of it only because the commitment \
                 table they would have been scored against is also empty.",
    },
    Falsification {
        check: "projection_kind::is_projection",
        owner: "src/projection_kind.rs",
        // Permissive reading: "this is not a model's output."
        passes: || !pk::is_projection(&sensor_reading()),
        fires: || !pk::is_projection(&model_output_row()),
        models: "The other half of the same incident, and the reason it hid: \
                 the runner writes `source_kind`, and the hand-rolled check \
                 read `source`. Two keys, one meaning, and the read returned \
                 the empty set rather than an error.",
    },
    // ── grounding_trust ─────────────────────────────────────────────────
    Falsification {
        check: "grounding_trust::strength",
        owner: "src/grounding_trust.rs",
        // Permissive reading: "this verdict is reproducible."
        passes: || gt::strength(gt::PROV_TOOL) >= 2,
        fires: || gt::strength("L1") >= 2,
        models: "An unrecognised verdict must score 0. A permissive default \
                 here means a value with no provenance at all outranks one with \
                 a citation, and every floor computed from it inverts — the \
                 `_ => 0` arm is load-bearing precisely because the vocabulary \
                 has drifted before.",
    },
    Falsification {
        check: "grounding_trust::floor",
        owner: "src/grounding_trust.rs",
        passes: || gt::strength(gt::floor([gt::PROV_TOOL, gt::PROV_DERIVED])) >= 2,
        fires: || gt::strength(gt::floor(std::iter::empty())) >= 2,
        models: "No sources means no evidence, which must not read as clean. An \
                 empty iterator returning the strongest value is the single \
                 most common way a floor calculation silently inverts, and the \
                 fold that does it looks correct on every non-empty input.",
    },
    Falsification {
        check: "grounding_trust::extracted_floor",
        owner: "src/grounding_trust.rs",
        // Permissive reading: "the extraction ceiling did not have to bind."
        passes: || gt::extracted_floor([gt::PROV_INFERRED]) == gt::floor([gt::PROV_INFERRED]),
        fires: || gt::extracted_floor([gt::PROV_TOOL]) == gt::floor([gt::PROV_TOOL]),
        models: "Extraction reads text and writes an assertion. No amount of \
                 well-sourced input makes the output a retrieval, so a rule \
                 distilled from a `tool_verified` block claiming `tool_verified` \
                 is a one-hop laundering path from a citation to a sentence \
                 nobody checked. `min(floor, EXTRACTION_CEILING)`, and this is \
                 the world where the ceiling is the operative half.",
    },
    Falsification {
        check: "grounding_trust::cross_check_exempt",
        owner: "src/grounding_trust.rs",
        // Permissive reading: "this Sourced field IS cross-checked."
        passes: || !gt::cross_check_exempt("football_analyst", "a/path/nobody/declared"),
        fires: || {
            let (agent, path, _) = gt::CROSS_CHECK_EXEMPTIONS[0];
            !gt::cross_check_exempt(agent, path)
        },
        models: "An exemption must be no broader than the thing it exempts. A \
                 file-scoped exemption written for a read filter covered the \
                 write path in the same file, and the deliberate break sailed \
                 through it. The `fires` world is read from the declared list \
                 rather than typed, so the pair cannot drift from what is \
                 actually exempt.",
    },
    // ── native_evaluators ───────────────────────────────────────────────
    Falsification {
        check: "native_evaluators::Verdict::is_failing",
        owner: "src/native_evaluators.rs",
        passes: || {
            !Verdict::Healthy {
                detail: "a fixture".into(),
            }
            .is_failing()
        },
        fires: || {
            !Verdict::Finding {
                severity: Severity::Critical,
                detail: "a fixture".into(),
                subjects: vec![],
                remedy: "a fixture",
            }
            .is_failing()
        },
        models: "`Notice` deliberately does not fail, which is what keeps \
                 `admits_everything` visible without asserting that violations \
                 must exist. A predicate tuned to let `Notice` through and \
                 accidentally letting `Critical` through with it would make the \
                 whole native tier decorative, and every report would be green.",
    },
    // ── panel_absence ───────────────────────────────────────────────────
    Falsification {
        check: "panel_absence::reading_for_reason",
        owner: "src/panel_absence.rs",
        // Permissive reading: "this panel is correctly empty."
        passes: || pa::reading_for_reason("no_input") == Reading::Idle,
        fires: || pa::reading_for_reason("unobserved") == Reading::Idle,
        models: "`unobserved` was added to `loop_model` and fell through a \
                 `_ => Idle` arm here, turning \"nothing has been watched\" into \
                 \"the system is idle\" on every panel backed by a loop. The \
                 benign default is how a new upstream token comes to report a \
                 healthy system.",
    },
    Falsification {
        check: "panel_absence::resolve",
        owner: "src/panel_absence.rs",
        passes: || {
            pa::resolve(&loop_panel(), &observation(loop_state(None, None))).reading
                == Reading::Idle
        },
        fires: || {
            pa::resolve(
                &loop_panel(),
                &observation(loop_state(Some("x"), Some("no_trigger"))),
            )
            .reading
                == Reading::Idle
        },
        models: "The panel this module exists for: a surface a reader could \
                 look at, see nothing, and draw a wrong conclusion from. A \
                 stage nothing calls must render as a fault, not as a quiet \
                 afternoon.",
    },
    Falsification {
        check: "panel_absence::rung_of",
        owner: "src/panel_absence.rs",
        passes: || pa::rung_of(&Resolver::Liveness("s")).is_some(),
        fires: || pa::rung_of(&Resolver::GateLedger).is_some(),
        models: "Loops and gates are chains and controls *over* the ladder \
                 rungs rather than rungs themselves. Giving them a position \
                 would put two different orderings in one column, and a reader \
                 sorting by it would get a sequence that means nothing.",
    },
    // ── loop_api ──────────────────────────────────────────────────
    Falsification {
        check: "loop_api::view",
        owner: "src/loop_api.rs",
        // Permissive reading: "this panel may be rendered as correctly empty."
        passes: || {
            loop_api::view(&LoopState {
                id: "l",
                name: "N",
                scope: "platform",
                claim: "C",
                stages: vec![StageState {
                    id: "x",
                    what: "w",
                    writer: "a::b",
                    trigger: Trigger::Request,
                    rows: 0,
                }],
                stops_at: Some("x"),
                reason: Some("no_input"),
                status: "stalled",
            })
            .reading
                == Reading::Idle
        },
        fires: || {
            loop_api::view(&LoopState {
                id: "l",
                name: "N",
                scope: "platform",
                claim: "C",
                stages: vec![StageState {
                    id: "x",
                    what: "w",
                    writer: "a::b",
                    trigger: Trigger::Request,
                    // The probe did not run. `rows == 0` reading as success is
                    // the defect `loop_model` was given a tri-state to prevent.
                    rows: -1,
                }],
                stops_at: Some("x"),
                reason: Some("probe_failed"),
                status: "unmeasured",
            })
            .reading
                == Reading::Idle
        },
        models: "An assembly layer is exactly where the tri-state gets \
                 flattened back to a boolean. `rows == 0` was once the only \
                 condition that stopped a chain, so a loop whose first probe \
                 errored while its later stages held rows reported `turning` \
                 with no stall and no reason — and a UI that renders \
                 `reading: idle` over an unread loop shows a green panel for a \
                 measurement that never happened.",
    },
    // ── outcome_trust ────────────────────────────────────────────────
    Falsification {
        check: "outcome_trust::classify_discrimination",
        owner: "src/outcome_trust.rs",
        // Permissive reading: "the metric can tell its subjects apart."
        passes: || {
            matches!(
                disc(&varied(10, 10), 5),
                Discrimination::Discriminates { .. }
            )
        },
        fires: || {
            matches!(
                disc(&varied(10, 0), 5),
                Discrimination::Discriminates { .. }
            )
        },
        models: "Loop 5.A's `scored` stage is declared as `per-agent \
                 calibration is recorded` and \
                 `record_forecast_calibration_signals` writes the FORECAST's \
                 Brier once per name in `agents_used`. Measured against \
                 production: 47 forecasts, every one with several agents, every \
                 one with exactly one distinct score. The loop is turning and \
                 the number it produces contains no agent-level information at \
                 all.",
    },
    Falsification {
        check: "outcome_trust::classify_discrimination",
        owner: "src/outcome_trust.rs",
        passes: || {
            matches!(
                disc(&varied(10, 10), 5),
                Discrimination::Discriminates { .. }
            )
        },
        fires: || {
            matches!(
                disc(&varied(50, 1), 5),
                Discrimination::Discriminates { .. }
            )
        },
        models: "The first version cleared the instrument on a single varying \
                 event, reasoning that the verdict is about whether the number \
                 CAN differ. Against production that returned \
                 `Discriminates { events: 50, varied: 1 }`, and the one varied \
                 event turned out to be eighteen aggregate rows from three \
                 unrelated agents sharing a `rationale` string — a grouping \
                 artifact, not variation. A rule that clears on one observation \
                 clears on noise.",
    },
    Falsification {
        check: "outcome_trust::classify_discrimination",
        owner: "src/outcome_trust.rs",
        // Permissive reading: "there is no finding here."
        passes: || {
            // A hundred events, one subject each: a platform where one agent
            // works each forecast. Nothing is wrong and nothing is measurable.
            let solo: Vec<EventSpread> = (0..100)
                .map(|_| EventSpread {
                    subjects: 1,
                    distinct_values: 1,
                })
                .collect();
            !disc(&solo, 5).is_finding()
        },
        fires: || !disc(&varied(10, 0), 5).is_finding(),
        models: "A single-subject event has one value by definition. Counting \
                 those as evidence of uniformity would report a fleet-wide \
                 defect on a platform where one agent works each forecast — the \
                 §5.2 failure, a check firing on correct behaviour, and the \
                 deletion that follows looks like cleanup.",
    },
    Falsification {
        check: "outcome_trust::Discrimination::is_finding",
        owner: "src/outcome_trust.rs",
        // Permissive reading: "there is nothing here to report."
        passes: || !Discrimination::Underpowered { events: 1, need: 5 }.is_finding(),
        fires: || !Discrimination::Uniform { events: 47 }.is_finding(),
        models: "`Underpowered` and `NoSharedEvents` are neither findings nor \
                 passes, and this predicate has to say so without folding them \
                 in with `Discriminates`. The same distinction `liveness_trust` \
                 draws with `Inert` and `loop_model` with `unobserved`, and both \
                 times the version that collapsed it reported a healthy system.",
    },
    Falsification {
        check: "outcome_trust::Discrimination::is_reading",
        owner: "src/outcome_trust.rs",
        // Permissive reading: "a reading is available."
        passes: || {
            Discrimination::Discriminates {
                events: 9,
                varied: 9,
            }
            .is_reading()
        },
        fires: || Discrimination::Underpowered { events: 1, need: 5 }.is_reading(),
        models: "The other direction, and it needs its own predicate: \
                 `is_finding` returns false for both a clean instrument and an \
                 unmeasured one. A single boolean would have to pick which of \
                 those to call a verdict, and either choice turns ‘we could not \
                 look’ into an answer — which is exactly how `_ => Idle` in \
                 `panel_absence` turned `unobserved` into an idle system.",
    },
    Falsification {
        check: "outcome_trust::classify_reach",
        owner: "src/outcome_trust.rs",
        // Permissive reading: "the loop returns to what fed it."
        passes: || matches!(classify_reach(84, 7, 8), Reach::Closes { .. }),
        fires: || matches!(classify_reach(84, 0, 8), Reach::Closes { .. }),
        models: "Loop 1 distils rules for 84 agents and 7 have ever had one \
                 retrieved. A rule nobody retrieves is a dream cycle nobody \
                 woke from: the agent paid for the consolidation, the row sits \
                 in `semantic_rules`, and the next prompt is built without it — \
                 so the loop's cost is real and its effect is zero. `turning` \
                 says nothing about this, because the rows are all there.",
    },
    Falsification {
        check: "outcome_trust::reach_pct",
        owner: "src/outcome_trust.rs",
        // Permissive reading: "reach is total."
        passes: || reach_pct(84, 84) == 100,
        fires: || reach_pct(0, 0) == 100,
        models: "A ratio with no denominator. Every other emptiness in this \
                 codebase has had a version that read as success — `rows == 0` \
                 as a turning chain, an empty `floor()` returning the strongest \
                 verdict, `0 live` as healthy — and `0/0 = 100%` would make a \
                 loop that has produced nothing report perfect reach, on the \
                 rung built to catch exactly that.",
    },
    Falsification {
        check: "outcome_trust::known_gap",
        owner: "src/outcome_trust.rs",
        // Permissive reading: "this finding is declared, with an exit condition
        // on file." The loud world is a metric nobody declared coming back
        // excused.
        passes: || known_gap("loop5a.scored", "uniform").is_some(),
        fires: || known_gap("loop9z.invented", "uniform").is_some(),
        models: "The baseline excuses a real finding from failing the suite, so \
                 a lookup that matches too broadly silently excuses \
                 everything — and the resulting green is indistinguishable from \
                 a platform with no gaps. The same shape as \
                 `seam_vocabulary_coverage`'s file-scoped exemption, which was \
                 written for a read filter and covered the write path in the \
                 same file.",
    },
    Falsification {
        check: "outcome_trust::classify_producers",
        owner: "src/outcome_trust.rs",
        // Permissive reading: "one producer, one denominator."
        passes: || classify_producers(1, false) == Producers::Single,
        fires: || classify_producers(2, false) == Producers::Single,
        models: "`dimension = 'forecast_calibration'` is written by \
                 `brier_forecast_resolver v1` (188 signals, one per resolved \
                 forecast) and by `brier v1` (51, one per aggregate over N \
                 forecasts). Both land in one column with no way to tell them \
                 apart, so a reader that averages it weights a single-forecast \
                 score equally with a mean over forty-eight. The same shape as \
                 the seam registry's findings one layer down: two \
                 independently-correct producers of one vocabulary, and nothing \
                 comparing them.",
    },
    // ── handlers::workspace::agent_params_hook ──────────────────────────
    //
    // Not a trust module, so the coverage scan does not require this entry.
    // Registered anyway: the rule this file now enforces — do not add a
    // decision without a falsification — has to apply to the session that
    // wrote it, and a registry whose author exempted himself is worth nothing.
    Falsification {
        check: "agent_params_hook::classify_claim",
        owner: "src/handlers/workspace/agent_params_hook.rs",
        // Permissive reading: "a claim was written for this run."
        passes: || {
            let b = ClaimBinding {
                workspace_id: Some(uuid::Uuid::from_u128(1)),
                forecast_id: None,
                driver: None,
            };
            classify_claim(&b, 3, 1, 2).recorded()
        },
        fires: || {
            let b = ClaimBinding {
                workspace_id: Some(uuid::Uuid::from_u128(1)),
                forecast_id: None,
                driver: None,
            };
            classify_claim(&b, 0, 0, 2).recorded()
        },
        models: "`forecast_agent_claims` has held zero rows since migration 187 \
                 and Loop 4 stalls on it. The caller discarded the old `bool` \
                 with `// no multiplier found, nothing to do`, which merged \
                 three states: no binding, a workspace whose program names no \
                 driver for this agent, and evidence carrying no number. The \
                 first bound run that still writes nothing is the observation \
                 the loop has been waiting for, and under a `bool` it would \
                 have arrived indistinguishable from the 65 unbound runs before \
                 it.",
    },
    // The second branch, and it needed its own pair.
    //
    // The entry above has `driver_prefixes = 0` in its loud world, so it
    // returns before the assertion check is ever reached. Deleting the
    // `assertions == 0` branch entirely left it **green** — a falsification
    // whose `models` cites three merged states while exercising one of the two
    // branches that separate them. Found by breaking it, which is the only way
    // it could have been found.
    Falsification {
        check: "agent_params_hook::classify_claim",
        owner: "src/handlers/workspace/agent_params_hook.rs",
        passes: || {
            let b = ClaimBinding {
                workspace_id: None,
                forecast_id: Some("fc-1".into()),
                driver: Some("d".into()),
            };
            classify_claim(&b, 1, 1, 2).recorded()
        },
        fires: || {
            let b = ClaimBinding {
                workspace_id: None,
                forecast_id: Some("fc-1".into()),
                driver: Some("d".into()),
            };
            classify_claim(&b, 1, 0, 2).recorded()
        },
        models: "Somewhere to put the judgement and no judgement to put. Kept \
                 separate from `no driver` because the last time the two were \
                 conflated the cause was an extraction pattern that could not \
                 read markdown emphasis — the model wrote `**1.15**` and 12 of \
                 the 22 lines this platform had produced were silently \
                 unreadable. A single reason code would have made that look \
                 like agents declining to quantify.",
    },
    // And the third, for the same reason the second was needed.
    //
    // Both pairs above read `recorded()`, and `Unbound` and `NoDriverForAgent`
    // are equally not-recorded — so collapsing the two into one left them both
    // green. The distinction between them *is* the fix: one is a fact about the
    // caller, the other is a fault someone can go and repair. A permissive
    // reading that cannot see it is the `bool` again, wearing an enum.
    Falsification {
        check: "agent_params_hook::classify_claim",
        owner: "src/handlers/workspace/agent_params_hook.rs",
        // Permissive reading: "there is no configuration fault to act on here."
        passes: || {
            let b = ClaimBinding {
                workspace_id: None,
                forecast_id: Some("fc-1".into()),
                driver: None,
            };
            classify_claim(&b, 0, 0, 2) != ClaimOutcome::NoDriverForAgent
        },
        fires: || {
            let b = ClaimBinding {
                workspace_id: Some(uuid::Uuid::from_u128(1)),
                forecast_id: None,
                driver: None,
            };
            classify_claim(&b, 0, 0, 2) != ClaimOutcome::NoDriverForAgent
        },
        models: "An agent running in a workspace whose FPL does not mention it \
                 can claim whatever it likes and reach no parameter. That is a \
                 fault someone can repair, and it is one `if` away from \
                 `unbound`, which is a fact about the caller and not repairable \
                 at all. `forecast_agent_claims` held zero rows for the life of \
                 the feature and no surface could say which of the two it was.",
    },
    // ── anomaly_vocabulary ───────────────────────────────────────────
    Falsification {
        check: "anomaly_vocabulary::is_actionable_flag",
        owner: "src/anomaly_vocabulary.rs",
        // Permissive reading: "some detector reads this flag."
        passes: || fermi::anomaly_vocabulary::is_actionable_flag("safety:blocked").is_some(),
        // A mistyped prefix, NOT `social:` — that one is inert on purpose, and
        // a falsification that fires on correct behaviour is §5.2's crying wolf.
        fires: || fermi::anomaly_vocabulary::is_actionable_flag("saftey:blocked").is_some(),
        models: "A producer that emits a shape no detector matches writes a \
                 flag nobody reads — a detector that cannot fire, and invisible, \
                 because an unmatched flag looks exactly like a quiet one. \
                 Migration 200 widened `anomaly_events.kind` for `grounding` and \
                 no enum variant was ever added; this is the same shape one \
                 layer out.",
    },
    Falsification {
        check: "anomaly_vocabulary::is_bookkeeping_flag",
        owner: "src/anomaly_vocabulary.rs",
        // Permissive reading: "this flag is not merely bookkeeping."
        passes: || fermi::anomaly_vocabulary::is_bookkeeping_flag("safety:blocked").is_none(),
        fires: || fermi::anomaly_vocabulary::is_bookkeeping_flag("social:observed").is_none(),
        models: "Without a declared inert list the vocabulary check would have \
                 to allow every unmatched prefix, which is the same as not \
                 having it. `social:observed` records work done, not a defect \
                 found, and it must be distinguishable from a detector nobody \
                 wrote.",
    },
];

// ── exemptions ──────────────────────────────────────────────────────────
//
// Shared reasons, referenced rather than paraphrased. Fifteen restatements of
// "returns a field" would be noise, and noise is what gets skimmed.

/// Returns a field, a declared constant, or a rendering of one.
const ACCESSOR: &str = "Returns a field, a declared constant, or a rendering of \
                        one. There is no branch on evidence, so there is no \
                        world in which it is wrong rather than merely different.";

/// Touches the database, the clock, the filesystem or a process-global.
const IMPURE: &str = "Performs I/O or mutates process-global state, so its \
                      world is not a struct literal. The decision it delegates \
                      to is registered above; this is the carriage.";

/// Walks a declared table and hands back what is in it.
const ENUMERATOR: &str = "Walks a declared table and yields what is in it. The \
                          table's own contract test asserts the contents; \
                          filtering a `const` array has nothing to falsify.";

const EXEMPT: &[(&str, &str)] = &[
    // liveness_trust
    (
        "liveness_trust::known_silent",
        "`KNOWN_SILENT` is empty, deliberately — its one entry was removed the \
         first live run that made it obsolete. There is therefore no world in \
         which this returns `Some`, and manufacturing one would falsify a \
         fixture rather than the function. Its `Some` branch reaches production \
         only through `is_actionable_silence`, which is registered, and the \
         live tier asserts every entry is still silent so a stale excuse cannot \
         outlive its reason.",
    ),
    ("liveness_trust::asserted", ENUMERATOR),
    ("liveness_trust::label", ACCESSOR),
    ("liveness_trust::column_exists", IMPURE),
    ("liveness_trust::evaluate_one", IMPURE),
    ("liveness_trust::sweep", IMPURE),
    ("liveness_trust::record_latest", IMPURE),
    ("liveness_trust::latest", IMPURE),
    ("liveness_trust::spawn_liveness_sweeper", IMPURE),
    // loop_model
    (
        "loop_model::evaluate",
        "The walk: async, over a `PgPool`, one query per stage. Every judgement \
         it makes is `diagnose`'s, which carries three registered pairs, and \
         the ordering it wraps them in is pinned by `loop_model`'s own unit \
         tests against struct literals.",
    ),
    // write_accounting
    ("write_accounting::spec", ACCESSOR),
    ("write_accounting::table", ACCESSOR),
    ("write_accounting::record", IMPURE),
    (
        "write_accounting::observe",
        "Records the outcome and hands the value back unchanged. The verdict \
         drawn from what it records is `is_totally_rejected`, registered above; \
         `observe_hands_the_value_back` pins the pass-through.",
    ),
    ("write_accounting::account", IMPURE),
    ("write_accounting::accounts", IMPURE),
    ("write_accounting::account_for_table", IMPURE),
    // gate_trust
    ("gate_trust::spec", ACCESSOR),
    ("gate_trust::id", ACCESSOR),
    ("gate_trust::as_str", ACCESSOR),
    ("gate_trust::asked", ACCESSOR),
    ("gate_trust::decided", IMPURE),
    ("gate_trust::decided_about", IMPURE),
    ("gate_trust::decided_ok", IMPURE),
    ("gate_trust::drain", IMPURE),
    ("gate_trust::requeue", IMPURE),
    ("gate_trust::flush", IMPURE),
    ("gate_trust::ledger_status", IMPURE),
    ("gate_trust::account", IMPURE),
    ("gate_trust::accounts", IMPURE),
    ("gate_trust::refusing_everything", IMPURE),
    ("gate_trust::spawn_gate_recorder", IMPURE),
    // seam_vocabulary
    (
        "seam_vocabulary::as_str",
        "Generated by `closed_vocabulary!` from the same literal as \
         `#[sqlx(rename)]`, so the two cannot disagree by construction. What \
         *can* go is sqlx honouring the attribute at all, and \
         `the_wire_form_is_the_declared_token` falsifies that by running each \
         variant through the real encoder — a stronger check than a struct \
         literal here could be.",
    ),
    // grounding_trust
    ("grounding_trust::cross_checks", ENUMERATOR),
    ("grounding_trust::contracts_for", ENUMERATOR),
    ("grounding_trust::cohort_scoped", ACCESSOR),
    ("grounding_trust::cohort_unscoped", ACCESSOR),
    ("grounding_trust::cohort_size_sql", ACCESSOR),
    (
        "grounding_trust::enforce",
        "Falsified by name in `grounding_trust`'s own suite: \
         `strips_every_ungrounded_field_and_keeps_what_it_removed` is the world \
         it must fire in, and `a_judgement_is_labelled_as_judgement_not_stripped` \
         and `placeholders_are_an_absence_not_a_fabrication` are the worlds it \
         must stay quiet in. Sixty-one tests over real agent cards is a better \
         falsification than a fixture here, and duplicating it would put two \
         answers to one question in the tree.",
    ),
    (
        "grounding_trust::reconcile",
        "Same suite, same shape: \
         `case_2_reconcile_corrects_it_against_the_creature_record` fires, \
         `reconcile_leaves_agreeing_and_absent_fields_alone` and \
         `reconcile_does_not_touch_fields_that_are_not_sourced` stay quiet.",
    ),
    (
        "grounding_trust::response_floor",
        "Falsified by `an_uncontracted_agent_has_an_unknown_floor_not_a_clean_one` \
         — the distinction that matters, because `None` must be read as unknown \
         and never as clean. It needs a registered card to mean anything, which \
         a fixture in this file could not supply honestly.",
    ),
    (
        "grounding_trust::matches",
        "A path matcher over a `FieldContract`. Exercised on every one of the \
         sixty-one contract tests; a pair here would test the same glob twice.",
    ),
    (
        "grounding_trust::is_clean",
        "`Report::is_clean` is `violations.is_empty()`. The falsifiable question \
         is whether `enforce` puts anything in that vector, which is exempted \
         above with the tests that answer it.",
    ),
    // native_evaluators
    (
        "native_evaluators::run",
        "The exemplar this whole file generalises: \
         `every_evaluator_can_produce_a_finding` already constructs, for each \
         evaluator, the world in which it must fire. Registering `run` here \
         would assert the same property one layer out and go red twice for one \
         state.",
    ),
    ("native_evaluators::severity", ACCESSOR),
    ("native_evaluators::collect", IMPURE),
    ("native_evaluators::registry", ENUMERATOR),
    // outcome_trust
    (
        "outcome_trust::shared_metric",
        "`SHARED_METRICS` is empty, deliberately: `forecast_calibration` has two \
         producers and that is the finding this module reports, not an \
         exemption to be granted. There is therefore no world in which this \
         returns `Some`, and fabricating one would falsify a fixture rather \
         than the function — the same reason `liveness_trust::known_silent` is \
         exempted. `classify_producers`, which consumes it, is registered.",
    ),
    ("outcome_trust::contract_for", ENUMERATOR),
    // loop_api
    (
        "loop_api::views",
        "Maps `view` over a walked set. The judgement is `view`'s, registered \
         above, and `only_the_first_empty_stage_is_flagged` pins the mapping.",
    ),
    (
        "loop_api::view_of",
        "`views` narrowed to one id. Same judgement; the 404 shape it enables \
         is the handler's business.",
    ),
    ("loop_api::action_for", ENUMERATOR),
    (
        "loop_api::tally",
        "Three counts over verdicts `view` already assigned. Registered \
         indirectly: `a_loop_that_could_not_be_read_is_unknown_not_idle` \
         asserts an unread loop lands in `unmeasured` and not `turning`, which \
         is the only judgement this makes.",
    ),
    // panel_absence
    ("panel_absence::label", ACCESSOR),
    ("panel_absence::panel", ENUMERATOR),
    ("panel_absence::scoped_probe", ENUMERATOR),
    (
        "panel_absence::resolve_all",
        "Maps `resolve` over `PANELS`. The judgement is `resolve`'s, registered \
         above.",
    ),
    (
        "panel_absence::resolve_for_subject",
        "Narrows `resolve` to one subject. Same judgement, and \
         `panel_absence`'s own suite pins the narrowing.",
    ),
];

// ── the scans, which this registry cannot reach ─────────────────────────

/// How a corpus-walking suite is kept from going quietly blind.
enum Proof {
    /// A named test that puts a known-bad input in front of the detector.
    Falsifier(&'static str),
    /// Not a pattern detector: it compares two computations, or two
    /// declarations, over a corpus. Such a check cannot be *blind* — only
    /// **vacuous** — so what it needs is a named non-vacuity guard instead.
    ///
    /// A real distinction, not a softer tier. "We have not written one yet" is
    /// not a reason and there is no variant for it.
    Parity {
        why: &'static str,
        non_vacuity: &'static str,
    },
}

/// Every suite that walks this repository's own files, and its proof.
///
/// A declaration check, and stated as one: it cannot tell a real falsifier from
/// a well-named empty function. What it makes impossible is adding a scan with
/// no falsifier *silently*, which had happened five times — and three of the
/// scans that reached production that way turned out to be unable to catch
/// their own motivating case.
///
/// **May only grow with the suites.** A file here that stops walking the tree
/// should leave; a new file that walks it must arrive with a named proof.
const SCANS: &[(&str, Proof)] = &[
    (
        "tests/write_accounting_coverage.rs",
        Proof::Falsifier("the_scan_sees_the_shape_that_caused_every_finding"),
    ),
    (
        "tests/gate_trust_coverage.rs",
        Proof::Falsifier("the_scan_only_counts_call_sites"),
    ),
    (
        "tests/grounding_execute_coverage.rs",
        Proof::Falsifier("the_scan_can_actually_fail"),
    ),
    (
        "tests/rollup_contract.rs",
        Proof::Falsifier("detector_flags_real_reads_and_ignores_innocent_ones"),
    ),
    (
        "tests/seam_vocabulary_coverage.rs",
        Proof::Falsifier("the_fence_sees_a_token_spelled_in_a_sql_string"),
    ),
    (
        "tests/grounding_raise_coverage.rs",
        Proof::Falsifier("the_scan_sees_an_enforcing_path_that_does_not_raise"),
    ),
    (
        "tests/projection_predicate_coverage.rs",
        Proof::Falsifier("the_scan_sees_a_tag_spelled_outside_the_owning_module"),
    ),
    (
        "tests/provenance_floor_coverage.rs",
        Proof::Falsifier("the_scan_sees_a_rule_written_without_a_floor"),
    ),
    (
        "tests/constraint_trust.rs",
        Proof::Falsifier("the_linter_sees_a_non_atomic_constraint_migration"),
    ),
    (
        "tests/loop_api_contract.rs",
        Proof::Falsifier("the_scan_sees_a_path_the_router_does_not_have"),
    ),
    (
        "tests/taxonomy_parity.rs",
        Proof::Parity {
            why: "It runs `fermi::taxonomy` and `scripts/taxonomy.py` over the \
                  same card corpus and asserts they agree rank for rank. There \
                  is no pattern for it to be blind to — a disagreement fails by \
                  construction. What it can be is empty, and an empty corpus \
                  would make it agree about nothing.",
            non_vacuity: "corpus_walk_finds_the_known_cards",
        },
    ),
];

/// The modules whose public decisions must be registered or exempted.
const TRUST_MODULES: &[&str] = &[
    "liveness_trust",
    "loop_model",
    "write_accounting",
    "gate_trust",
    "seam_vocabulary",
    "projection_kind",
    "grounding_trust",
    "native_evaluators",
    "panel_absence",
    "anomaly_vocabulary",
    "outcome_trust",
    "loop_api",
];

// ── assertions ──────────────────────────────────────────────────────────

/// The one that matters: can each check tell its two worlds apart?
#[test]
fn every_falsification_distinguishes_its_two_worlds() {
    assert!(
        FALSIFICATIONS.len() >= 20,
        "only {} falsification(s) registered; the coverage scan below is the \
         thing that keeps this honest, but a registry this small has probably \
         lost entries rather than gained exemptions",
        FALSIFICATIONS.len()
    );

    let mut broken = Vec::new();
    for f in FALSIFICATIONS {
        if !(f.passes)() {
            broken.push(format!(
                "{}: the quiet world is not quiet. Either the fixture is wrong \
                 or the check fires on correct behaviour — and §5.2 says what \
                 happens to a check that cries wolf.",
                f.check
            ));
        }
        if (f.fires)() {
            broken.push(format!(
                "{}: **the check does not fire in the world it was written \
                 for.**\n         models: {}\n         owner: {}",
                f.check, f.models, f.owner
            ));
        }
    }
    assert!(
        broken.is_empty(),
        "\n{} falsification(s) failed:\n\n  {}\n",
        broken.len(),
        broken.join("\n\n  ")
    );
    println!(
        "  {} check(s) demonstrated able to go red.",
        FALSIFICATIONS.len()
    );
}

/// A falsification nobody can trace to an incident is a guess.
#[test]
fn every_falsification_names_the_incident_it_models() {
    let mut thin = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for f in FALSIFICATIONS {
        if f.models.len() < 80 {
            thin.push(format!("{}: `models` is a label, not an incident", f.check));
        }
        if !f.owner.starts_with("src/") || !f.owner.ends_with(".rs") {
            thin.push(format!("{}: `owner` must name a file", f.check));
        }
        // One function may carry several pairs — `diagnose` carries three — but
        // two pairs modelling the same incident are one pair and a copy.
        if !seen.insert((f.check, f.models)) {
            thin.push(format!("{}: this incident is registered twice", f.check));
        }
    }
    assert!(thin.is_empty(), "\n  {}\n", thin.join("\n  "));
}

/// Every public decision in the trust modules is registered or exempted.
///
/// Without this the registry rots exactly like every other list here: the
/// checks written the day it landed stay in it, and everything after does not.
#[test]
fn every_decision_function_is_registered_or_exempted() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut all: Vec<(String, String)> = Vec::new();

    for module in TRUST_MODULES {
        let path = repo.join("src").join(format!("{module}.rs"));
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{module} is declared in scope and unreadable: {e}"));

        // Everything from the unit-test module down is fixtures, not surface.
        // Truncating at a column-0 `#[cfg(test)]` is what makes that cheap, and
        // it is also how the scan could silently shrink to nothing — so the
        // per-module floor below is not decoration.
        let surface = match body.find("\n#[cfg(test)]") {
            Some(i) => &body[..i],
            None => &body[..],
        };

        let mut found = 0usize;
        for line in surface.lines() {
            let t = line.trim_start();
            let Some(rest) = t
                .strip_prefix("pub fn ")
                .or_else(|| t.strip_prefix("pub const fn "))
                .or_else(|| t.strip_prefix("pub async fn "))
            else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                continue;
            }
            found += 1;
            let key = (module.to_string(), name);
            if !all.contains(&key) {
                all.push(key);
            }
        }
        assert!(
            found > 0,
            "no public function was found in `{module}`. Either the module \
             moved or the `#[cfg(test)]` truncation ate the file, and a scan \
             over an empty set passes for ever."
        );
    }

    assert!(
        all.len() >= 60,
        "the scan found only {} public function(s) across {} modules; that is \
         too few to be the real surface",
        all.len(),
        TRUST_MODULES.len()
    );

    let covered = |module: &str, name: &str| {
        let head = format!("{module}::");
        let tail = format!("::{name}");
        FALSIFICATIONS
            .iter()
            .any(|f| f.check.starts_with(&head) && f.check.ends_with(&tail))
            || EXEMPT
                .iter()
                .any(|(c, _)| c.starts_with(&head) && c.ends_with(&tail))
    };

    let missing: Vec<String> = all
        .iter()
        .filter(|(m, n)| !covered(m, n))
        .map(|(m, n)| format!("{m}::{n}"))
        .collect();

    println!(
        "  {} public function(s); {} registered, {} exempted.",
        all.len(),
        FALSIFICATIONS.len(),
        EXEMPT.len()
    );

    assert!(
        missing.is_empty(),
        "\n{} public decision(s) are neither registered nor exempted:\n\n  {}\n\n\
         Add a `Falsification` — a world where it must be quiet and a world \
         where it must fire, with the incident the second is drawn from — or \
         add it to `EXEMPT` with a reason. An unregistered check has never been \
         shown to go red, and three of the ones that had not could not have.\n",
        missing.len(),
        missing.join("\n  ")
    );
}

/// An exemption must cover something that exists, and only one list may claim it.
#[test]
fn every_exemption_is_real_reasoned_and_unique() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut problems = Vec::new();

    for (check, why) in EXEMPT {
        if why.len() < 60 {
            problems.push(format!(
                "{check}: an exemption without a reason is a permanent one"
            ));
        }
        // A stale exemption exempts nothing and looks like it exempts something.
        let Some((module, _)) = check.split_once("::") else {
            problems.push(format!("{check}: not a `module::name`"));
            continue;
        };
        let path = repo.join("src").join(format!("{module}.rs"));
        let name = check.rsplit("::").next().unwrap_or("");
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        // `fn observe<T, E: Display>(..)` — a generic decision is still a
        // decision, and the first version of this check reported two of them
        // as stale exemptions because it only looked for `fn name(`.
        if !body.contains(&format!("fn {name}(")) && !body.contains(&format!("fn {name}<")) {
            problems.push(format!(
                "{check}: `src/{module}.rs` has no `fn {name}`, so this \
                 exemption covers nothing. The list may only shrink."
            ));
        }
        if FALSIFICATIONS.iter().any(|f| f.check == *check) {
            problems.push(format!(
                "{check}: registered AND exempted. One of the two is a leftover, \
                 and the exemption is the one that would win silently."
            ));
        }
    }

    assert!(problems.is_empty(), "\n  {}\n", problems.join("\n  "));
}

/// Every scan over source text names the test that proves it can fire.
///
/// The half the registry above cannot reach, and the half every cited incident
/// came from. Three scans in this repository could not catch their own
/// motivating case; each was found by hand, and nothing would have found the
/// fourth.
#[test]
fn every_source_scan_declares_the_test_that_proves_it_can_fire() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut problems = Vec::new();

    for (file, proof) in SCANS {
        let body = match std::fs::read_to_string(repo.join(file)) {
            Ok(b) => b,
            Err(e) => {
                problems.push(format!("{file}: declared here and unreadable — {e}"));
                continue;
            }
        };
        let named = match proof {
            Proof::Falsifier(t) => t,
            Proof::Parity { why, non_vacuity } => {
                if why.len() < 100 {
                    problems.push(format!(
                        "{file}: `Parity` is the variant that excuses a suite \
                         from proving it can fire. Say why it cannot be blind."
                    ));
                }
                non_vacuity
            }
        };
        if !body.contains(&format!("fn {named}(")) {
            problems.push(format!(
                "{file}: names `{named}` as its proof and does not contain it. \
                 Either write it, or — if the suite is gone — remove the entry."
            ));
        }
        // Staleness, not shape. The discovery half below looks for `read_dir`
        // because a *walker* is what can silently arrive; an entry that names
        // its files (`grounding_execute_coverage`) is declared by hand and is
        // no less a detector. What makes an entry stale is the file no longer
        // reading this repository at all.
        if !body.contains("read_to_string") && !body.contains("read_dir") {
            problems.push(format!(
                "{file}: no longer reads this repository, so it is not a \
                 detector over it and does not belong in this list."
            ));
        }
    }

    // And the other direction: a scan that arrives without an entry.
    let mut undeclared = Vec::new();
    for entry in std::fs::read_dir(repo.join("tests"))
        .expect("tests/ is readable")
        .flatten()
    {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let rel = format!("tests/{}", p.file_name().unwrap().to_string_lossy());
        if rel == "tests/falsification_registry.rs" || SCANS.iter().any(|(f, _)| *f == rel) {
            continue;
        }
        let Ok(body) = std::fs::read_to_string(&p) else {
            continue;
        };
        // The signature of a suite over this repository's own files: it starts
        // at the manifest directory and enumerates. A test that opens one named
        // fixture is not walking anything and is not being asked for a proof.
        //
        // The first version of this looked for `join("src")`, which is narrower
        // and wrong in the way that matters: `provenance_floor_coverage` walks
        // from `repo_root()` and would have been missed — a detector that
        // cannot find one of its own declared entries. That is the failure this
        // whole file is about, so it is worth having made here rather than
        // discovered later.
        if body.contains("CARGO_MANIFEST_DIR") && body.contains("read_dir") {
            undeclared.push(rel);
        }
    }

    assert!(
        undeclared.is_empty(),
        "\n{} suite(s) walk this repository and are not in `SCANS`:\n  {}\n\n\
         Name the test that proves the detector can see its motivating case, \
         or declare it `Parity` and name what stops it being vacuous. Three of \
         the scans here could not catch the case they were written for, and \
         each was found by hand.\n",
        undeclared.len(),
        undeclared.join("\n  ")
    );
    assert!(problems.is_empty(), "\n  {}\n", problems.join("\n  "));

    println!(
        "  {} corpus suite(s) declare a proof; {} of them a falsifier.",
        SCANS.len(),
        SCANS
            .iter()
            .filter(|(_, p)| matches!(p, Proof::Falsifier(_)))
            .count()
    );
}
