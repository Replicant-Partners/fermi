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
use fermi::gate_api;
use fermi::projection_kind as pk;
use fermi::surface::{caveat_problems, door_problems, router_declares, Caveat, Door};
use fermi::write_accounting::SinkAccount;
use std::path::Path;

// The claim-outcome decision lives in the binary crate's handler tree, which an
// integration test cannot reach, so it is re-exported by the library for this
// purpose. See `fermi::claim_outcome`.
use fermi::claim_outcome::{classify_claim, ClaimBinding, ClaimOutcome};
use fermi::coordination_note::{self, Delivery};
use fermi::declaration_ladder::{self as dl, Disposition, Legibility, Owner, Silence};
use fermi::gate_review::{self, ReviewTally};

/// Does `advanced_metrics` carry reliance, under this report?
///
/// A helper rather than two inline closures because both worlds must share the
/// document and the filter exactly -- the first version of this pair inlined them,
/// invented a block name `football_analyst` does not declare, and the resulting
/// empty filter made `all()` vacuously true in BOTH worlds. The registry's own
/// `fires` assertion caught it on the first run, which is the machinery working on
/// the machinery.
///
/// `is_empty()` is the guard that makes it impossible to repeat: if the contract's
/// paths are ever renamed the filter goes empty and this returns `false`, so the
/// **quiet** world fails loudly instead of the pair going quietly vacuous.
fn graded_block_carries_reliance(provenance: Vec<(String, &'static str)>) -> bool {
    let doc = serde_json::json!({"advanced_metrics": {"xg": 1.83}});
    let report = gt::Report {
        violations: vec![],
        provenance,
    };
    let fields: Vec<_> = gt::graded_fields("football_analyst", &doc, &report)
        .into_iter()
        .filter(|f| f.block == "advanced_metrics")
        .collect();
    !fields.is_empty() && fields.iter().all(|f| gt::strength(f.provenance) >= 2)
}

/// A review tally, for the `gate_review` pairs below.
///
/// `reviewed` is derived from the three counts by `ReviewTally::reviewed`, so
/// there is no fourth number here to get wrong — which is the point of the type
/// and worth not defeating in the fixture.
fn tally(upheld: i64, overturned: i64, unclear: i64) -> ReviewTally {
    ReviewTally {
        upheld,
        overturned,
        unclear,
    }
}

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

/// A loop state whose stages are named to match `loop_api::SUBJECT_SCOPES`.
///
/// The stage *ids* matter here and the rest does not: `agent_view` looks the
/// scope up by `(loop_id, stage)`, so a fixture with invented ids would
/// exercise only the `Platform` fallback and prove nothing.
fn agent_state(loop_id: &'static str, stages: &[(&'static str, i64)]) -> LoopState {
    LoopState {
        id: loop_id,
        name: "N",
        scope: "platform",
        claim: "C",
        stages: stages
            .iter()
            .map(|(id, rows)| StageState {
                id,
                what: "w",
                writer: "a::b",
                trigger: Trigger::Request,
                rows: *rows,
            })
            .collect(),
        stops_at: None,
        reason: None,
        status: "turning",
    }
}

/// A gate that promises durability, asked `n` times.
fn recorded_gate(approved: u64) -> GateAccount {
    GateAccount {
        retention: fermi::gate_trust::Retention::Recorded,
        ..gate_account(approved, 0)
    }
}

/// A gate account with the two counters that decide its reading.
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
    // ── surface — the two parts every trust domain shares ────────────────
    Falsification {
        check: "surface::router_declares",
        owner: "src/surface.rs",
        // Permissive reading: "this door's path is routed."
        //
        // The loud world has the router declaring only the *longer* path and
        // the door asking for the shorter one. That direction is the one
        // unquoted matching gets wrong, and the first version of this pair had
        // it backwards: it asked for the longer path against a router holding
        // the shorter, where `contains` is false either way. The break came
        // back green and the pair proved nothing.
        passes: || router_declares(r#".route("/api/loops", get(h))"#, "/api/loops"),
        fires: || router_declares(r#".route("/api/loops/actions", get(h))"#, "/api/loops"),
        models: "`/api/loops` and `/api/loops/actions` are different endpoints, \
                 and an unquoted substring match declares the *shorter* one \
                 present because the longer is. A door that 404s is worse than \
                 a missing one: the reviewer presses it, believes they recorded \
                 a correction, and the failure arrives after the belief — with \
                 `hitl_actions` at zero rows there is no traffic whose \
                 disappearance would say otherwise.",
    },
    Falsification {
        check: "surface::door_problems",
        owner: "src/surface.rs",
        // Permissive reading: "this door is well formed."
        passes: || {
            door_problems(&[Door {
                subject: "d.s",
                method: "POST",
                path: "/api/x",
                does: "A sentence long enough to describe what pressing it does.",
                why_manual: "A reason long enough to be an argument rather than \
                             a label, which is what the hundred-character floor \
                             is for.",
            }])
            .is_empty()
        },
        fires: || {
            door_problems(&[Door {
                subject: "d.s",
                method: "POST",
                path: "/api/x",
                does: "A sentence long enough to describe what pressing it does.",
                why_manual: "because",
            }])
            .is_empty()
        },
        models: "A manual step that cannot say why it is manual should be \
                 automated. Half these loops are human-gated by design and the \
                 argument for each is what a reviewer needs before deciding a \
                 queue is worth working — without the floor, `why_manual` \
                 becomes a label and the surface advertises work nobody can \
                 justify doing.",
    },
    Falsification {
        check: "surface::caveat_problems",
        owner: "src/surface.rs",
        // Permissive reading: "this caveat is a caveat."
        passes: || {
            caveat_problems(&[Caveat {
                subject: "d.s",
                checked: "The narrower proposition that was actually tested.",
                does_not_show: "A different sentence, long enough to clear the \
                                floor, saying what a green tick here fails to \
                                establish about the claim it serves.",
            }])
            .is_empty()
        },
        fires: || {
            let same = "One sentence used for both fields, long enough to clear \
                        the hundred-character floor so that the only thing under \
                        test is the restatement.";
            caveat_problems(&[Caveat {
                subject: "d.s",
                checked: same,
                does_not_show: same,
            }])
            .is_empty()
        },
        models: "Every check in this repository is narrower than the claim it \
                 serves, and a surface that renders a tick against the claim is \
                 the over-reading the whole audit is about — committed at the \
                 last possible moment, in the one artifact a non-author reads. \
                 A `does_not_show` that paraphrases `checked` satisfies the \
                 type and closes the gap with nothing.",
    },
    Falsification {
        check: "gate_api::ledger_claim",
        owner: "src/gate_api.rs",
        // Permissive reading: "this gate's durability claim is fine."
        //
        // Quiet world: recorded, asked, and the ledger has the rows. Loud
        // world: recorded, asked forty times, ledger empty — the surface says
        // these counters survive a restart and nothing is behind that.
        passes: || {
            !matches!(
                gate_api::ledger_claim(&recorded_gate(40), 40),
                gate_api::LedgerClaim::Unbacked { .. }
            )
        },
        fires: || {
            !matches!(
                gate_api::ledger_claim(&recorded_gate(40), 0),
                gate_api::LedgerClaim::Unbacked { .. }
            )
        },
        models: "`gate_decisions` was declared by migration 214, and until it \
                 ran the platform had a record of every request it served and \
                 none of any it refused — which is how a gate refusing 100% of \
                 agent-wide interventions stayed invisible. A gate reporting \
                 `since: ledger` over an empty table makes the same claim on \
                 the same evidence, and the table holds zero rows today.",
    },
    // ── coordination_note ─────────────────────────────────────────
    Falsification {
        check: "coordination_note::is_problem",
        owner: "src/coordination_note.rs",
        // Permissive reading: "nothing went wrong with this delivery."
        //
        // The quiet world is the outcome to *hope for*: the strategist already
        // wrote a targeted note, so the platform's floor was not needed. The
        // loud world is a write into the memory of an agent that was never in
        // the room.
        passes: || !coordination_note::is_problem(&Delivery::AlreadyTargeted),
        fires: || !coordination_note::is_problem(&Delivery::NotAMember),
        models: "The floor exists to be unnecessary. If the strategist writes a \
                 targeted note for every member, every delivery returns \
                 `AlreadyTargeted` — and a caller that logged that as a warning \
                 would fill the log on exactly the runs that went best, which is \
                 how a warning stops being read. `NotAMember` is the opposite: \
                 writing a coordination observation into the dreaming material \
                 of an agent that was never in the workspace is an injection, \
                 not a skip.",
    },
    // ── gate_review ───────────────────────────────────────────────
    // -- declaration_ladder ------------------------------------------
    Falsification {
        check: "declaration_ladder::whose_work",
        owner: "src/declaration_ladder.rs",
        // Permissive reading: "the platform will handle it; nothing is required
        // of the agent's author."
        passes: || dl::whose_work(&Silence::Unresolved) == Owner::Platform,
        fires: || dl::whose_work(&Silence::Undeclared { rung: "ports" }) == Owner::Platform,
        models: "The reason this module exists. `panel_absence::Resolver` had five \
                 ways to explain an absence and none of them was `the subject \
                 declared nothing`, so an undeclared agent's silence collapsed \
                 into `Unresolved { why }` -- which reads as *the platform has not \
                 written a contract for this*. Measured: 89 of 96 real producing \
                 agents have no field contract, so the platform appeared to owe 89 \
                 contracts it does not owe. Getting this wrong does not produce a \
                 wrong number, it produces a wrong BACKLOG, and a backlog nobody \
                 can act on is one nobody does.",
    },
    Falsification {
        check: "declaration_ladder::attribute",
        owner: "src/declaration_ladder.rs",
        // Permissive reading: "nobody has to do anything about this silence."
        passes: || dl::whose_work(&dl::attribute(true, &Legibility::Opaque, 0)) == Owner::NoOne,
        fires: || dl::whose_work(&dl::attribute(false, &Legibility::Opaque, 0)) == Owner::NoOne,
        models: "Two silences that were one word. On a freshly booted server every \
                 gate reads `never_asked`, every loop counter is zero, and none of \
                 it is a finding -- the counters are process-local and resolve \
                 themselves on the next request. An undeclared agent's silence \
                 looks identical and is permanent. Attributing the cold counter to \
                 a missing declaration sends an author to write a contract for a \
                 reading that fixes itself; attributing the missing declaration to \
                 a cold counter tells them to wait forever. The ordering inside \
                 `attribute` is the whole content of the function.",
    },
    Falsification {
        check: "declaration_ladder::disposition",
        owner: "src/declaration_ladder.rs",
        // Permissive reading: "this row is worth working on."
        passes: || dl::disposition("weather_oracle", &Legibility::Opaque) != Disposition::Prune,
        fires: || dl::disposition("test_agent_abc", &Legibility::Declared) != Disposition::Prune,
        models: "110 of the 206 agents that have produced an episode are \
                 `test_agent_<uuid>` fixtures declaring nothing at all. Pruning \
                 one is a delete behind a safety gate; retrofitting a real agent \
                 is authoring work with a domain expert, and reported as one \
                 number the retrofit looks twice its real size. The `fires` world \
                 is the ordering inside `disposition`: a fixture that somehow \
                 declared every rung must still be `Prune`, or the coverage \
                 numerator fills with rows that are about to be deleted. The fleet \
                 has no such row today, which is exactly why it has to be \
                 asserted rather than observed.",
    },
    Falsification {
        check: "declaration_ladder::legibility",
        owner: "src/declaration_ladder.rs",
        // Permissive reading: "this agent has declared something."
        passes: || dl::legibility(&["ports"]) != Legibility::Opaque,
        fires: || dl::legibility(&["something_new"]) != Legibility::Opaque,
        models: "Coverage rising by inventing a rung name. The ladder is served \
                 over the API, and a client -- or a future rung added in another \
                 module -- could offer a token this one does not declare. Counting \
                 an unrecognised token as progress is the same defect as \
                 `gate_review::tally_from_counts` folding an undeclared verdict \
                 into the nearest bucket: it makes the drift invisible in the one \
                 place it shows up as data.",
    },
    Falsification {
        check: "declaration_ladder::is_test_cruft",
        owner: "src/declaration_ladder.rs",
        // Permissive reading: "this is a real agent."
        passes: || !dl::is_test_cruft("weather_oracle"),
        fires: || !dl::is_test_cruft("test_agent_9f2c"),
        models: "v0.10.20's audit found 565 of these rows in the shared database, \
                 and 110 of them have produced episodes. Several surfaces filtered \
                 them with an inline prefix check and several -- notably the \
                 Observatory fleet endpoints -- did not, so the clinical view \
                 opened on a wall of fixtures instead of the operator's agents. \
                 Registered now rather than treated as a trivial string check, \
                 because it became load-bearing: it is the pivot deciding whether \
                 an agent lands on the retrofit worklist or the prune list, and \
                 those have different owners and different costs.",
    },
    // -- graded contracted fields ------------------------------------
    Falsification {
        check: "grounding_trust::graded_fields",
        owner: "src/grounding_trust.rs",
        // Permissive reading: "this field's grade carries reliance."
        passes: || {
            graded_block_carries_reliance(vec![("advanced_metrics".to_string(), gt::PROV_TOOL)])
        },
        // The same document, and a report that graded nothing -- which is what
        // `enforce` returns for any block it did not see.
        fires: || graded_block_carries_reliance(vec![]),
        models: "`enforce` emits a provenance entry only for blocks it actually \
                 saw, so a contracted field whose block is absent from the report \
                 is the COMMON case, not an edge one -- every contracted agent has \
                 more absent fields than present ones. Defaulting those to \
                 anything above the bottom rung would make an ungraded field \
                 indistinguishable from a verified one on the artifact trace, \
                 which is the same over-read as `gate_trust::never_asked` being \
                 coloured as a pass. The value the model claimed is carried \
                 verbatim beside it for `Violation.removed`'s reason: it is the \
                 only evidence that could ever answer which model fabricates \
                 what, and a null cannot be labelled.",
    },
    Falsification {
        check: "assertions::from_graded_field",
        owner: "src/assertions.rs",
        // Permissive reading: "nobody needs to check this claim."
        //
        // Registered voluntarily: `assertions` is not in `TRUST_MODULES`, so the
        // coverage scan does not demand it. It holds `entitled_provenance`,
        // `route` and `pending_verdict` -- three trust decisions -- and belongs
        // there. Adding it means registering the module's whole public surface,
        // which is its own task; this at least leaves the new decision covered.
        passes: || {
            let f = gt::GradedField {
                path: "form.xg_last_5",
                block: "form",
                value: serde_json::json!(1.83),
                provenance: gt::PROV_TOOL,
                settleable_by: Some("call_football_api"),
            };
            fermi::assertions::from_graded_field("football_analyst", &f)
                .map(|a| a.route(true) == fermi::assertions::Route::None)
                .unwrap_or(false)
        },
        fires: || {
            // The same field, sourced, with nothing behind it.
            let f = gt::GradedField {
                path: "form.xg_last_5",
                block: "form",
                value: serde_json::json!(1.83),
                provenance: gt::PROV_NO_MATCH,
                settleable_by: Some("call_football_api"),
            };
            fermi::assertions::from_graded_field("football_analyst", &f)
                .map(|a| a.route(true) == fermi::assertions::Route::None)
                .unwrap_or(false)
        },
        models: "A `Quantity` with an EMPTY basis floors at \
                 `pending_human_check` however well sourced its block was -- that \
                 is `entitled_provenance`'s documented and correct behaviour, \
                 because a measurement with no stated source is work to be done. \
                 So an assertion minted from a tool-verified field WITHOUT \
                 carrying the block's grade enqueues a person to re-check \
                 something a tool already answered, and a queue that contains \
                 everything is not a queue. This is the entire argument for the \
                 function existing rather than the caller building an `Assertion` \
                 inline, and the two worlds differ only in the grade.",
    },
    // -- verification_queue ------------------------------------------
    Falsification {
        check: "verification_queue::is_problem",
        owner: "src/verification_queue.rs",
        // Permissive reading: "nothing here needs anyone's attention."
        passes: || {
            !fermi::verification_queue::Enqueued {
                already_settled: 9,
                inherits_from_basis: 4,
                ..Default::default()
            }
            .is_problem()
        },
        fires: || {
            !fermi::verification_queue::Enqueued {
                not_representable: vec!["taxonomy.order: not numeric".to_string()],
                ..Default::default()
            }
            .is_problem()
        },
        models: "`assertion_verifications` has held 0 rows since migration 205 \
                 and nothing could say WHY, which is the `severity = 'L1'` shape: \
                 an empty table that might be an empty queue or might be a \
                 rejected write. Three causes have to stay apart. Nine claims \
                 already reproducible is the BEST case and must be quiet -- a \
                 caller that warned on it would fill the log on the runs that \
                 went well, and a queue containing everything is not a queue. \
                 `not_representable` must speak: a contracted field the queue \
                 cannot carry is a hole in its coverage, and \
                 `taxonomy.order = \"Coleoptera\"` is the canonical case -- the \
                 claim most worth verifying, in the `Antaxius beieri` failure \
                 where every check passed because the field was present, \
                 non-null and correctly typed. Skipped silently, the queue reads \
                 healthy while the checkable claims go unchecked.",
    },
    // -- artifact_trace ----------------------------------------------
    Falsification {
        check: "artifact_trace::narrow_by_age",
        owner: "src/artifact_trace.rs",
        // Permissive reading: "this absence is permanent and not a finding."
        passes: || {
            let t0 = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc);
            let t1 = chrono::DateTime::parse_from_rfc3339("2026-06-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc);
            // Artifact older than the gate's first recorded decision.
            fermi::artifact_trace::narrow_by_age(
                fermi::artifact_trace::Absent {
                    token: fermi::artifact_trace::NotRecordedReason::RetainedButAbsent,
                    because: "seed".to_string(),
                },
                Some(t0),
                Some(t1),
            )
            .token
                == fermi::artifact_trace::NotRecordedReason::PredatesRetention
        },
        fires: || {
            // No timestamp on the artifact at all. Nothing has been shown.
            let t0 = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc);
            fermi::artifact_trace::narrow_by_age(
                fermi::artifact_trace::Absent {
                    token: fermi::artifact_trace::NotRecordedReason::RetainedButAbsent,
                    because: "seed".to_string(),
                },
                None,
                Some(t0),
            )
            .token
                == fermi::artifact_trace::NotRecordedReason::PredatesRetention
        },
        models: "The audit's recurring defect in its newest costume: a true fact \
                 about an artifact reported as a fact about the system. Of the \
                 four reasons a rung has no ledger row, three are permanent or \
                 by design and exactly one -- `retained_but_absent` -- is a \
                 finding. The promotion date is written down nowhere, so the \
                 gate's earliest recorded decision is the ONLY evidence that an \
                 artifact predates retention. Claiming it without that evidence \
                 relabels a dropped recorder row as `this is old, nothing to \
                 see`, and the calm version is the one that ships, because a \
                 belt of grey rings that all explain themselves looks finished.",
    },
    Falsification {
        check: "artifact_trace::reports_exactly_one_way",
        owner: "src/artifact_trace.rs",
        // Permissive reading: "this rung is well-formed."
        passes: || {
            fermi::artifact_trace::belt("agent.execute")
                .iter()
                .all(|r| r.reports_exactly_one_way())
        },
        fires: || {
            // A verdict written beside the absence it supersedes.
            let mut r = fermi::artifact_trace::belt("agent.execute")
                .into_iter()
                .next()
                .expect("`agent.execute` declares gates");
            r.decided = Some(fermi::artifact_trace::Decided {
                decision: "approved".to_string(),
                reason: None,
                at: None,
                decision_id: None,
            });
            r.reports_exactly_one_way()
        },
        models: "The belt's two absence-or-verdict fields are filled in two \
                 different places -- `belt()` fills the absence from the gate \
                 registry, and `episode_trace_handler` overwrites it from \
                 `gate_decisions` -- which is the same split that produced the \
                 defect migration 220 was written for. A handler that sets the \
                 verdict without clearing the absence emits a rung claiming both \
                 an `approved` and a reason there is no row, and every client \
                 branching on `decided` first would silently render the verdict \
                 and drop the contradiction. The UX team asked for this as an \
                 invariant by name, because their renderer is a two-way branch \
                 and a rung with both is a state it cannot draw.",
    },
    Falsification {
        check: "verification_queue::reviewer_may_write",
        owner: "src/verification_queue.rs",
        // Permissive reading: "a person may assert this verdict."
        passes: || fermi::verification_queue::reviewer_may_write("human_sourced"),
        fires: || fermi::verification_queue::reviewer_may_write("tool_verified"),
        models: "`grounding_trust::strength` scores `human_sourced` as high as \
                 `tool_verified`, and the reason it is allowed to is the \
                 citation: someone else can follow it to the same source. \
                 `tool_verified` and `derived` mean a tool call or a transform \
                 REPRODUCES the value, which a person cannot bring about by \
                 saying so -- accepting one from a reviewer launders an opinion \
                 into the strength of a tool call, on the exact surface built to \
                 raise trust. The `pending_*` tier is the mirror defect: it is \
                 what a claim is queued AS, so accepting it would let an item be \
                 cleared from the queue by re-queueing it. The column's CHECK \
                 cannot express this, because the same column is written by the \
                 platform's own enqueue with `pending_*`.",
    },
    Falsification {
        check: "verification_queue::classify_settle_error",
        owner: "src/verification_queue.rs",
        // Permissive reading: "the reviewer forgot the citation."
        passes: || {
            fermi::verification_queue::classify_settle_error(
                Some("assertion_verifications_citation_check"),
                "new row violates check constraint",
            ) == fermi::verification_queue::SettleRefusal::CitationRequired
        },
        fires: || {
            fermi::verification_queue::classify_settle_error(None, "deadlock detected")
                == fermi::verification_queue::SettleRefusal::CitationRequired
        },
        models: "Two of this session's own probes blamed the thing they were \
                 written to check, and both did it by matching on error TEXT \
                 rather than on a pinned constraint name -- one of them matched \
                 `LIKE '%verdict%'` and got the citation check back. Here the \
                 same shortcut tells a reviewer to add a citation when the \
                 database deadlocked, so they add one, resubmit, and get the \
                 same message. The settle endpoint deliberately does NOT \
                 re-implement migration 205's CHECK in Rust -- that is two \
                 implementations of one trust rule -- so this translation is the \
                 only thing standing between a Postgres error and the reviewer.",
    },
    Falsification {
        check: "verification_queue::settle_is_client_error",
        owner: "src/verification_queue.rs",
        // Permissive reading: "this refusal is the caller's to fix."
        passes: || {
            fermi::verification_queue::settle_is_client_error(
                &fermi::verification_queue::SettleRefusal::CitationRequired,
            )
        },
        fires: || {
            fermi::verification_queue::settle_is_client_error(
                &fermi::verification_queue::SettleRefusal::UnknownVerdict,
            )
        },
        models: "`UnknownVerdict` means the database refused a verdict the \
                 provenance ladder declares -- so `PROVENANCE_VALUES` and \
                 migration 205's CHECK have drifted, which is migration 219's \
                 incident exactly: `GATE_IDS` gained `output_schema`, 214's \
                 CHECK was widened to match and 216's was not. Returning 400 for \
                 it hands a platform seam defect to the reviewer as though they \
                 had typed something wrong, and a 4xx is not paged on, so the \
                 drift persists with a human being blamed for it once per \
                 attempt. The two halves of the same error deserve opposite \
                 status codes and that judgement lives here.",
    },
    Falsification {
        check: "artifact_trace::reading",
        owner: "src/artifact_trace.rs",
        // Permissive reading: "this artifact's journey is a pass."
        passes: || {
            let g = vec![fermi::grounding_trust::GradedField {
                path: "advanced_metrics.xg",
                block: "advanced_metrics",
                value: serde_json::json!(1.83),
                provenance: gt::PROV_TOOL,
                settleable_by: Some("call_football_api"),
            }];
            fermi::artifact_trace::reading(0, &g, &Legibility::Declared).0 == Reading::Idle
        },
        fires: || {
            // Nothing graded, because the agent declares no field contract.
            fermi::artifact_trace::reading(0, &[], &Legibility::Opaque).0 == Reading::Idle
        },
        models: "3,571 of 3,576 episodes carry no grounding stamp, because 89 of \
                 96 real producing agents have no field contract. So an artifact \
                 with NO checkpoints is what this endpoint returns most of the \
                 time, and it is the default screen rather than an edge case. \
                 Rendering it as a clean journey end to end is the over-read the \
                 whole architecture refuses -- the same rule as \
                 `gate_trust::never_asked` and `liveness_trust::Inert`, applied \
                 to the one surface a non-author reads. The misleading version \
                 also looks BETTER, which is why it has to be asserted rather \
                 than left to judgement.",
    },
    Falsification {
        check: "artifact_trace::whose_journey_is_incomplete",
        owner: "src/artifact_trace.rs",
        // Permissive reading: "the platform owes something here."
        //
        // Adapted on the `Owner` the reading returns, which is the field a
        // backlog is built from.
        passes: || {
            fermi::artifact_trace::reading(1, &[], &Legibility::Declared).3 == Owner::Platform
        },
        fires: || fermi::artifact_trace::reading(0, &[], &Legibility::Opaque).3 == Owner::Platform,
        models: "The same misattribution `declaration_ladder` exists to prevent, \
                 arriving on the artifact surface instead of the census. A \
                 violation IS the platform's -- the contract fired and something \
                 got through. An agent that declared no contract is its author's, \
                 and 89 of 96 producing agents are in that state, so billing them \
                 to the platform produces a backlog of 89 items nobody can act \
                 on. This surface is the one a non-author reads, so it is where \
                 getting it wrong costs the most.",
    },
    // -- artifact_hash -----------------------------------------------
    Falsification {
        check: "artifact_hash::of_episode",
        owner: "src/artifact_hash.rs",
        // Permissive reading: "grounding changed nothing about this document."
        passes: || {
            let doc = serde_json::json!({"taxonomy": {"order": "Orthoptera"}});
            let wrapped = format!("Here you go:\n\n```json\n{doc}\n```\n");
            fermi::artifact_hash::of_episode(Some("q"), Some(&wrapped), Some(&doc))
                .enforcement_changed_the_bytes
                == Some(false)
        },
        fires: || {
            let doc = serde_json::json!({"taxonomy": {"order": "Orthoptera"}});
            let wrapped = format!("Here you go:\n\n```json\n{doc}\n```\n");
            let nulled = serde_json::json!({"taxonomy": {"order": null}});
            fermi::artifact_hash::of_episode(Some("q"), Some(&wrapped), Some(&nulled))
                .enforcement_changed_the_bytes
                == Some(false)
        },
        models: "Comparing the RAW response against the ENFORCED document would \
                 report that grounding modified the document on every response \
                 wrapped in prose -- and 64 of 94 retained responses from \
                 contracted agents are wrapped that way, so the field would read \
                 `true` almost always and mean nothing. The comparison therefore \
                 re-extracts and compares document to document. The same \
                 confusion, one layer down, is what made `response_floor` return \
                 `unavailable` for 94 of 94 responses: a fact about a parse \
                 reported as a fact about the artifact.",
    },
    Falsification {
        check: "artifact_hash::of_document",
        owner: "src/artifact_hash.rs",
        // Permissive reading: "these two documents are the same."
        passes: || {
            let a: serde_json::Value =
                serde_json::from_str(r#"{"a":1,"b":{"x":1,"y":2}}"#).unwrap();
            let b: serde_json::Value =
                serde_json::from_str(r#"{"b":{"y":2,"x":1},"a":1}"#).unwrap();
            fermi::artifact_hash::of_document(&a) == fermi::artifact_hash::of_document(&b)
        },
        fires: || {
            let a = serde_json::json!({"genome": {"estimated_size_mb": 2400}});
            let b = serde_json::json!({"genome": {"estimated_size_mb": 2401}});
            fermi::artifact_hash::of_document(&a) == fermi::artifact_hash::of_document(&b)
        },
        models: "A document digest is meaningless unless serialisation is \
                 canonical: with insertion-ordered maps `{\"a\":1,\"b\":2}` and \
                 `{\"b\":2,\"a\":1}` are the same document and different bytes. \
                 `serde_json`'s default `Map` is a `BTreeMap` so keys sort -- but \
                 that is one feature flag away from false and ANY dependency in \
                 the tree can enable `preserve_order` without this crate \
                 noticing. If it happens, two identical documents get different \
                 digests and any determinism check built on them reports drift \
                 that is not there. Asserted rather than inferred from the \
                 absence of a flag.",
    },
    Falsification {
        check: "gate_review::standing",
        owner: "src/gate_review.rs",
        // Permissive reading: "this gate has no finding against it."
        passes: || {
            gate_review::reading(gate_review::standing(50, tally(50, 0, 0))).0 != Reading::Fault
        },
        fires: || {
            gate_review::reading(gate_review::standing(50, tally(40, 1, 3))).0 != Reading::Fault
        },
        models: "The Γ arithmetic bug's successor, which every counter in the \
                 system is blind to. `gate_trust::refuses_everything` catches the \
                 extreme — asked, and approved nothing — and that is how a \
                 coherence gate rejecting 100% of agent-wide interventions was \
                 eventually found. A gate that approves 90% of what it sees and \
                 refuses the other 10% WRONGLY reads `discriminating`, which \
                 `/api/gates` renders as the healthy state, and no arrangement of \
                 approve/refuse totals distinguishes it from a gate working \
                 perfectly. Correctness is not a property of a count. One \
                 reviewer's overturn is the entire signal, so a `standing` that \
                 folded it in with the upheld ones would leave the platform \
                 exactly where it was before migration 216.",
    },
    Falsification {
        check: "gate_review::reading",
        owner: "src/gate_review.rs",
        // Permissive reading: "this standing is a pass."
        //
        // Registered separately from `standing` because it is a separate
        // incident, not a second branch: `standing` is about noticing an
        // overturn, this is about refusing to call an *unread* ledger green.
        passes: || {
            gate_review::reading(gate_review::standing(50, tally(50, 0, 0))).0 == Reading::Idle
        },
        fires: || {
            gate_review::reading(gate_review::standing(400, tally(0, 0, 0))).0 == Reading::Idle
        },
        models: "A full ledger nobody has read. That is the state every gate \
                 starts in and the state the platform was in for its entire life \
                 — migration 214 gave the two recorded gates a ledger and nothing \
                 anywhere let a person judge a row in it, which is why \
                 `GATE_DOORS` was `&[]`. `0 overturned` is literally true of an \
                 unreviewed gate, and a surface that coloured it as a pass would \
                 assert the gate is sound on evidence about none of its \
                 decisions. Same rule as `liveness_trust::Inert` and \
                 `gate_trust::never_asked`: not asked is not a pass.",
    },
    Falsification {
        check: "gate_review::tally_from_counts",
        owner: "src/gate_review.rs",
        // Permissive reading: "every verdict in the column is one I know."
        passes: || {
            gate_review::tally_from_counts(&[(
                fermi::seam_vocabulary::GateReviewVerdict::Unclear
                    .as_str()
                    .to_string(),
                3,
            )])
            .is_ok()
        },
        fires: || gate_review::tally_from_counts(&[("needs_more_thought".to_string(), 3)]).is_ok(),
        models: "Half of `severity = 'L1'`, from the other side. That defect was \
                 a Rust write site holding a token the CHECK refused; this is a \
                 CHECK holding a token no Rust variant spells, which happens \
                 whenever a constraint is widened in a migration and the enum is \
                 not. `seam_vocabulary_contract` catches it against the schema; \
                 this catches it against the ROWS, which is the only place a \
                 value written before the constraint was narrowed can show up. \
                 Folding the unknown token into the nearest bucket — the obvious \
                 implementation — makes both invisible.",
    },
    Falsification {
        check: "gate_review::classify_write_error",
        owner: "src/gate_review.rs",
        // Permissive reading: "nothing recognisable; hand the caller the
        // database's own words." Which is right for a deadlock and wrong for a
        // constraint this module named itself.
        passes: || {
            matches!(
                gate_review::classify_write_error(None, "deadlock detected"),
                gate_review::Refusal::Rejected { .. }
            )
        },
        fires: || {
            matches!(
                gate_review::classify_write_error(
                    Some("gate_decision_reviews_rationale_check"),
                    "violates check constraint"
                ),
                gate_review::Refusal::Rejected { .. }
            )
        },
        models: "The rationale rule has exactly one implementation and it is \
                 Postgres's — a Rust pre-check would be the §3.4 violation that \
                 reads as defensive good practice. The cost of that choice is \
                 that a missing rationale arrives as a database error, and \
                 untranslated it becomes a 500 with a Postgres string in it at \
                 the exact moment a reviewer was told their finding had been \
                 filed. Pinned on the CONSTRAINT NAME rather than the message, \
                 because message text is a locale-and-version artifact and the \
                 name is something migration 216 chose; if 216 renames it, this \
                 is what says the translation stopped working.",
    },
    Falsification {
        check: "gate_review::is_client_error",
        owner: "src/gate_review.rs",
        // Permissive reading: "this one is ours, not the caller's."
        passes: || {
            !gate_review::is_client_error(&gate_review::Refusal::UnknownToken { column: "verdict" })
        },
        fires: || !gate_review::is_client_error(&gate_review::Refusal::RationaleRequired),
        models: "Both errors here are wrong in a way that trains a reviewer to \
                 stop. A missing rationale answered 500 says the platform is \
                 broken, so they stop reviewing and file a bug against the wrong \
                 thing. Drift between the CHECK and the type answered 400 says \
                 their input was malformed, so they retype a verdict that cannot \
                 be accepted by any input at all — the typed path cannot produce \
                 a token the column refuses, so if it happens it is ours. That \
                 second one is the `severity = 'L1'` failure wearing a client \
                 error's clothes.",
    },
    // ── evaluator_api ─────────────────────────────────────────────
    Falsification {
        check: "evaluator_api::read",
        owner: "src/evaluator_api.rs",
        // Permissive reading: "this verdict is a pass."
        passes: || {
            fermi::evaluator_api::read(&Verdict::Healthy { detail: "d".into() }).0 == Reading::Idle
        },
        fires: || {
            fermi::evaluator_api::read(&Verdict::Inconclusive { why: "w".into() }).0
                == Reading::Idle
        },
        models: "`Inconclusive` is not a pass, and three of the six evaluators \
                 are usually in it — most of the counters they read are \
                 process-local and reset on restart, so a cold snapshot \
                 honestly concludes nothing. A surface that renders it green \
                 reports a healthy platform on every fresh boot, which is the \
                 one moment it is least entitled to.",
    },
    // ── gate_api ────────────────────────────────────────────────
    Falsification {
        check: "gate_api::read",
        owner: "src/gate_api.rs",
        // Permissive reading: "this gate is discriminating."
        passes: || gate_api::read(&gate_account(3, 1)).0 == Reading::Idle,
        fires: || gate_api::read(&gate_account(0, 0)).0 == Reading::Idle,
        models: "`approved: 0, refused: 0` and `approved: 40, refused: 0` are \
                 both ‘no refusals’ and mean opposite things: a control nobody \
                 has exercised, and one that has run forty times and stopped \
                 nothing. The gate audit exists because a surface rendered the \
                 counters and left a reader to notice, and nobody did.",
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
    Falsification {
        check: "loop_api::agent_view",
        owner: "src/loop_api.rs",
        // Permissive reading: "this stage is where THIS AGENT's chain stops."
        //
        // The quiet world is a per-agent stage at zero, which really is the
        // agent's first empty link. The loud world is a platform-scoped stage
        // at zero — `forecast_commitments` has no agent column at all — and
        // reporting it as the agent's stop is the defect the 610-line handler
        // had: platform figures under an agent's name, two of them hardcoded.
        passes: || {
            let s = agent_state("loop5a", &[("committed", 0), ("scored", 0)]);
            loop_api::agent_view(&s, &[("scored", Some(0))]).stops_at == Some("scored")
        },
        fires: || {
            let s = agent_state("loop5a", &[("committed", 0), ("scored", 0)]);
            loop_api::agent_view(&s, &[("scored", Some(0))]).stops_at == Some("committed")
        },
        models: "`observatory::agent_loops_handler`, 610 lines of bespoke SQL, \
                 rendered platform-wide figures in an agent's status column — \
                 and its own comment records that two of its rows were \
                 hardcoded constants doing it. Fifteen of twenty-three stages \
                 have no agent dimension at all; a view that lets one of them \
                 be an agent's stop is answering a question about forecasts as \
                 though it were about the agent.",
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
    (
        "gate_trust::decided_for_episode",
        "The same counter-and-enqueue as `decided`, plus the artifact the \
         decision was about. Its one judgement -- that a reference the batched \
         recorder writes must resolve, since migration 220 declines a foreign \
         key so one bad row cannot reject a whole flush -- can only be settled \
         against a live database, and is, by \
         `gate_decision_lineage::no_gate_decision_points_at_an_episode_that_is_not_there`.",
    ),
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
        "Falsified by three tests in its own module, each needing a registered \
         card that a fixture in this file could not supply honestly. \
         `an_uncontracted_agent_has_an_unknown_floor_not_a_clean_one` holds the \
         distinction that matters most — `None` must be read as unknown and never \
         as clean. \
         `a_document_wrapped_in_prose_is_graded_rather_than_dismissed` covers the \
         measured defect: a bare `from_str` dismissed 94 of 94 retained responses \
         from contracted agents as `ungrounded by construction` while the \
         platform's own `extract_json` could read 64 of them. \
         `recovering_a_document_is_not_the_same_as_finding_content` covers the \
         symmetric error — a recovered document that contains none of its \
         contracted fields must still floor on the fields it is missing. Both \
         breaks are in `scripts/break_response_floor.py`.",
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
    (
        "native_evaluators::declaration_census",
        "A gatherer, not a decision: it runs `declaration_ladder::CENSUS_SQL` and \
         folds the rows through `census`, which is itself exempted with its own \
         reason. Its one judgement — `None` on failure rather than an empty \
         `Census`, because zero coverage everywhere is the most alarming \
         available reading and must not be inferred from a query that did not \
         run — is asserted by `panel_absence`'s \
         `a_missing_census_is_not_reported_as_zero_coverage`.",
    ),
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
    // surface
    (
        "surface::doors_missing_from",
        "Filters `router_declares` over a list and formats the offenders. The \
         judgement is `router_declares`'s, registered above with the prefix \
         case that motivated it; `a_missing_door_is_reported_with_its_subject` \
         pins the formatting.",
    ),
    // declaration_ladder
    (
        "declaration_ladder::has_field_contract",
        "Delegates to `grounding_trust::contracts_for`, which owns the roster. A \
         second opinion about whether an agent has a field contract is the \
         Section 3.4 violation this whole ladder is built to surface one layer up.",
    ),
    (
        "declaration_ladder::census",
        "A fold over `legibility` and `is_test_cruft`, both registered above. Its \
         own judgement -- that cruft belongs in the denominator and not the \
         numerator -- cannot be falsified by input alone, because both closures \
         would call the same fold; it is pinned by \
         `the_census_keeps_the_two_worklists_separate`, which asserts the ports \
         rung reads 2 of 2 real agents rather than 2 of 5.",
    ),
    // artifact_hash
    (
        "artifact_hash::of_text",
        "A SHA-256 over bytes, prefixed with its algorithm name. There is no \
         judgement in it: the two decisions in this module are canonicality \
         (`of_document`) and what counts as a change (`of_episode`), and both are \
         registered. The prefix itself is pinned by \
         `a_changed_value_changes_the_digest`.",
    ),
    // artifact_trace
    (
        "artifact_trace::belt",
        "Assembles `command_registry::Command.gates` against \
         `gate_trust::GATES` and holds no opinion: the rung order, the \
         control-or-metric status and the refusal text all belong to those two. \
         Pinned by `every_rung_says_whether_it_can_actually_refuse`, which \
         asserts over BOTH execute commands that a rung demoted to a metric says \
         why -- the field a belt diagram most easily hides.",
    ),
    (
        "artifact_trace::fields",
        "Dresses `GradedField` for a client and delegates the only calculation \
         in it to `grounding_trust::floor`. Pinned by \
         `the_documents_floor_is_the_weakest_of_its_blocks`, which asserts \
         against `floor` itself rather than a literal, so the test cannot drift \
         from the ladder while still looking like it tested something.",
    ),
    // verification_queue
    (
        "verification_queue::enqueue",
        "The database write. Its routing judgement is \
         `assertions::Assertion::route`, which the contract answers via \
         `settleable_by`, and its emptiness judgement is `is_problem`, \
         registered above. What is left here is an INSERT and a fold, and the \
         part of it no offline world can exercise — whether the row the platform \
         writes is one `assertion_verifications` accepts — is asserted by \
         `tests/verification_queue_contract.rs` against a real server, for the \
         same reason `gate_review_contract.rs` exists: the constraint names and \
         the CHECK vocabularies are only knowable from Postgres.",
    ),
    // gate_review
    (
        "gate_review::reviewed",
        "`upheld + overturned + unclear`. Deliberately derived and not stored, so \
         it cannot disagree with the three counts it summarises — which is why \
         `gate_api::tally` needs a whole separate partition test and this needs \
         none. Pinned by `the_tally_partitions_by_construction`.",
    ),
    // coordination_note
    (
        "coordination_note::deliver",
        "Two queries and an episode write. The membership refusal and the \
         duplicate-suppression window are the judgements, and both are visible \
         in the `Delivery` it returns — `is_problem` is registered above with \
         the case that matters. The write itself is counted through \
         `write_accounting::Sink::Episodes`, which owns the question of whether \
         it landed.",
    ),
    // evaluator_api
    (
        "evaluator_api::views",
        "Runs the declared registry over one snapshot and dresses each verdict. \
         The verdicts are `native_evaluators`', falsified by \
         `every_evaluator_can_produce_a_finding`; the one judgement added is \
         `read`, registered above. \
         `nothing_observed_produces_no_healthy_verdict` pins the case that \
         matters — a cold snapshot must conclude nothing.",
    ),
    (
        "evaluator_api::tally",
        "Four counts over tokens `read` already assigned. \
         `the_buckets_partition_the_evaluators_and_a_notice_is_not_a_finding` \
         asserts nothing falls through and that a `Notice` is not counted as a \
         finding, which is the only judgement available to it.",
    ),
    // gate_api
    (
        "gate_api::view",
        "Dresses a `GateAccount` for a surface. The one judgement it makes is \
         `read`'s, registered above; the rest is `gate_trust`'s fields carried \
         through, and `the_view_says_whether_the_counters_survive_a_restart` \
         pins the field that matters — whether `0 refusals` may be read as a \
         lifetime total.",
    ),
    (
        "gate_api::views",
        "Maps `view` over the live counters. `every_live_gate_account_matches_a_declared_gate` \
         pins both directions of the join, which is the only thing this adds.",
    ),
    (
        "gate_api::tally",
        "Four counts over tokens `read` already assigned. \
         `the_buckets_partition_the_gates` asserts nothing falls through, which \
         is the only failure available to it — there is no catch-all arm to \
         silently absorb a new upstream token.",
    ),
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
        "loop_api::subject_scope",
        "A lookup into `SUBJECT_SCOPES`, whose completeness is what matters and \
         is pinned by `every_stage_declares_its_subject_scope_exactly_once` — \
         one entry per stage, no default, and no entry for a stage that does \
         not exist. The judgement drawn from it is `agent_view`'s, registered \
         above with the platform-figure-under-an-agent's-name case.",
    ),
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
        "tests/episode_lineage_coverage.rs",
        Proof::Falsifier("the_scan_sees_a_bare_none_and_accepts_an_argued_one"),
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
    "gate_api",
    "evaluator_api",
    "coordination_note",
    "gate_review",
    "declaration_ladder",
    "verification_queue",
    "artifact_trace",
    "artifact_hash",
    "surface",
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
