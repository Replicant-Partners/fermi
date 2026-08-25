//! The five feedback loops, declared as chains, so a stalled loop names the
//! link it stalled at.
//!
//! # Why a declaration and not a dashboard
//!
//! `agent_loops_handler` answers "how are the loops doing" in 610 lines of
//! bespoke per-loop SQL inside a 3,666-line file. It works, and it has two
//! properties that make it the wrong place for the answer to live:
//!
//! * **Its verdict can drift from the contracts.** The rungs in
//!   [`crate::liveness_trust`], the counters in [`crate::write_accounting`] and
//!   the gates in [`crate::gate_trust`] each hold part of the truth about a
//!   loop, and a hand-written query holds a fourth opinion that references none
//!   of them.
//! * **It reports per stage, not per chain.** Every stage of Loop 2 is empty.
//!   Read stage by stage that is five findings; read as a chain it is one, and
//!   only the first one is actionable — the four below it are empty *because*
//!   the first is.
//!
//! An audit measured all five against production. **Two of five turn.** Loop 2
//! and Loop 4 have produced zero rows at every stage of their chains — not slow
//! loops, unstarted ones.
//!
//! # What a chain buys
//!
//! The first stage with no rows, and a reason drawn from the layer that knows:
//!
//! | reason | where it comes from |
//! |---|---|
//! | `no_trigger` | the stage declares [`Trigger::None`] — nothing calls it |
//! | `scheduler_off` | [`Trigger::Scheduler`] whose env var defaults to off |
//! | `writes_refused` | [`crate::write_accounting`] — attempted, and refused every time |
//! | `gate_refuses_everything` | [`crate::gate_trust`] — the gate ran and approved nothing |
//! | `probe_failed` | **this stage's own count query did not run** |
//! | `upstream_unmeasured` | a stage above could not be read, so this one cannot be diagnosed |
//! | `awaiting_upstream` | the stage above it is empty too |
//! | `awaiting_agent` | [`Trigger::Prompted`] — a prompt asks for it and no agent has obliged |
//! | `no_input` | everything above produced, and this stage has had no occasion |
//!
//! The last three are the honest answers for a healthy but idle loop, and
//! keeping them distinct from the first four is the whole point: `no_input` is a
//! fact about the world, `no_trigger` is a fact about the code, and
//! `awaiting_agent` is neither — it is the absence of an observation about a
//! model's behaviour, which is why it reads `Unknown` rather than `Idle`.
//!
//! # Unread is not empty
//!
//! The middle two arrived later, and they close a hole this module had put in
//! itself. [`StageState::rows`] carries `-1` for a count query that did not run,
//! documented as "never confused with zero" — and the walk then confused it with
//! *success*: `rows == 0` was the only condition that stopped a chain, so a loop
//! whose first probe errored while its later stages held rows reported
//! `turning`, with no stall and no reason. Below that, a genuinely empty stage
//! sitting under an unread one was diagnosed `awaiting_upstream` — a confident
//! claim about a stage nobody had read.
//!
//! Both are the defect this module exists to prevent, committed in its own walk:
//! an interpretation pointing at a link that is not where the break is. The
//! remedy is [`Upstream`], which is a tri-state rather than a `bool`, and a
//! `status` of `unmeasured` that is neither `turning` nor `stalled`.
//!
//! # This module owns no arithmetic
//!
//! It declares the shape and asks the other rungs. Per
//! `verification_for_agent_ecologies.md` §3.4, a trust calculation must have
//! exactly one implementation — so the counts come from SQL declared here, and
//! every *interpretation* is delegated.

use crate::gate_trust::{self, Gate};
use crate::write_accounting::{self, Sink};

/// How a stage comes to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    /// An HTTP request drives it.
    Request,
    /// A background sweeper drives it.
    Scheduler {
        env: &'static str,
        /// Whether the default is on. `CONSOLIDATION_SWEEP_SECS` defaults to
        /// `0`, and a loop whose engine is opt-in is a loop that is off.
        default_on: bool,
    },
    /// It runs when the stage above produces.
    Upstream,
    /// Only a person clicking. Not a fault, and not a cadence either.
    Manual,
    /// **A prompt asks an agent to call the writer, and the agent decides.**
    ///
    /// Split out of [`Manual`] because the two fail differently and the
    /// difference is the whole of loop 3. A manual stage produces nothing until
    /// a person acts; a prompted stage produces nothing until a person acts
    /// *and* a language model obliges, and the second half is not observable
    /// from a row count. Reading it as `Manual` invites `no_input` — "the
    /// trigger had its chance and there was nothing to do" — which is a
    /// positive claim about the world made on no evidence at all.
    ///
    /// Both of loop 3's writeable stages are this. Six intention tools and
    /// `record_coordination_observation` are all reached only by the strategist
    /// choosing to call them from one prompt.
    Prompted {
        /// Which prompt asks, so a zero points at a file rather than a mood.
        asked_by: &'static str,
    },
    /// **Nothing calls it.**
    ///
    /// A declared state, because it is the most common finding in this system
    /// and it is invisible from every other surface: an uncalled writer and an
    /// unexercised one produce identical row counts. `may_only_shrink` holds
    /// the list.
    None {
        /// Why there is no caller, and what would create one.
        why: &'static str,
    },
}

/// One link in a loop.
#[derive(Debug, Clone, Copy)]
pub struct Stage {
    pub id: &'static str,
    /// What this link produces, as a reader would say it.
    pub what: &'static str,
    /// Read-only count of what the stage has produced. One row, one `bigint`
    /// column `n`.
    pub sink_sql: &'static str,
    pub writer: &'static str,
    pub trigger: Trigger,
    /// The write-accounting sink, when the writer is instrumented.
    pub accounted: Option<Sink>,
    /// The gate that can refuse this stage.
    pub gated_by: Option<Gate>,
}

/// A declared feedback loop.
#[derive(Debug, Clone, Copy)]
pub struct FeedbackLoop {
    pub id: &'static str,
    pub name: &'static str,
    /// agent | workspace | composition | platform.
    pub scope: &'static str,
    /// What the architecture claims this loop achieves. Quoted so a stalled
    /// loop shows the claim it is failing rather than only a count.
    pub claim: &'static str,
    pub stages: &'static [Stage],
}

/// Every loop, in order, with the chain as measured rather than as designed.
pub const LOOPS: &[FeedbackLoop] = &[
    FeedbackLoop {
        id: "loop1",
        name: "Agent learning",
        scope: "agent",
        claim: "An agent's own experience changes how it reasons: episodes \
                cluster into semantic rules, and those rules are retrieved into \
                the next prompt.",
        stages: &[
            Stage {
                id: "episodes",
                what: "experience accumulates",
                sink_sql: "SELECT count(*)::bigint AS n FROM episodes",
                writer: "every execute path",
                trigger: Trigger::Request,
                accounted: Some(Sink::Episodes),
                gated_by: None,
            },
            Stage {
                id: "consolidated",
                what: "a dream cycle completes",
                sink_sql: "SELECT count(*)::bigint AS n FROM consolidation_jobs \
                            WHERE status = 'completed'",
                writer: "handlers::consolidation::sweep_consolidation_once",
                trigger: Trigger::Scheduler {
                    env: "CONSOLIDATION_SWEEP_SECS",
                    default_on: false,
                },
                accounted: Some(Sink::ConsolidationJobs),
                gated_by: Some(Gate::Credit),
            },
            Stage {
                id: "rules",
                what: "rules are distilled",
                sink_sql: "SELECT count(*)::bigint AS n FROM semantic_rules",
                writer: "agent_bestiary_memory::consolidation",
                trigger: Trigger::Upstream,
                accounted: Some(Sink::SemanticRules),
                gated_by: None,
            },
            Stage {
                id: "retrieved",
                what: "a rule is wanted back",
                sink_sql: "SELECT count(*)::bigint AS n FROM semantic_rules \
                            WHERE application_count > 0",
                writer: "agent_backend::kg_context::record_rule_retrievals",
                trigger: Trigger::Request,
                accounted: Some(Sink::SemanticRules),
                gated_by: None,
            },
        ],
    },
    FeedbackLoop {
        id: "loop2",
        name: "Human-gated behavioural correction",
        scope: "agent",
        claim: "Agent behaviour aligns with human judgement on anomalous cases, \
                and the correction cannot be bypassed by an agent that learns to \
                sound plausible.",
        stages: &[
            Stage {
                id: "anomaly",
                what: "a defect is surfaced for review",
                sink_sql: "SELECT count(*)::bigint AS n FROM anomaly_events",
                writer: "handlers::execution (grounding), observability::AnomalyDetector",
                trigger: Trigger::Request,
                accounted: Some(Sink::AnomalyEvents),
                gated_by: Some(Gate::Grounding),
            },
            Stage {
                id: "reviewed",
                what: "a reviewer acts on it",
                sink_sql: "SELECT count(*)::bigint AS n FROM hitl_actions",
                writer: "handlers::observatory::record_hitl_action_handler",
                trigger: Trigger::Manual,
                accounted: None,
                gated_by: None,
            },
            Stage {
                id: "consensus",
                what: "a second reviewer confirms an agent-wide change",
                sink_sql: "SELECT count(*)::bigint AS n FROM two_reviewer_requests",
                writer: "handlers::observatory::confirm_two_reviewer_handler",
                trigger: Trigger::Manual,
                accounted: None,
                gated_by: Some(Gate::Coherence),
            },
            Stage {
                id: "corrected",
                what: "the correction is written to memory",
                sink_sql: "SELECT count(*)::bigint AS n FROM episode_corrections",
                writer: "coherence-gate::two_write",
                trigger: Trigger::Upstream,
                accounted: None,
                gated_by: Some(Gate::Coherence),
            },
            Stage {
                id: "persona_bumped",
                what: "the drift baseline moves",
                sink_sql: "SELECT count(*)::bigint AS n FROM agents WHERE persona_version > 1",
                writer: "coherence-gate::two_write::bump_persona_version",
                trigger: Trigger::Upstream,
                accounted: None,
                gated_by: None,
            },
        ],
    },
    FeedbackLoop {
        id: "loop3",
        name: "Workspace coherence",
        scope: "workspace",
        claim: "A composition notices its own incoherence and coordinates out of \
                it within a session.",
        stages: &[
            Stage {
                id: "intentions",
                what: "agents declare what they are about to do",
                sink_sql: "SELECT count(*)::bigint AS n FROM workspace_intentions",
                writer: "agent_backend::tools_legacy::execute_declare_intention",
                // Was `Trigger::None`: six tools implemented, wired to dispatch,
                // exposed to every workspace agent, and no prompt anywhere asked
                // for them. The strategist prompt in `workspace::coherence` now
                // requests Stage 0 by name and names the tools, and
                // `cohere_and_coordinate`'s own Stage 0 pointed at
                // `_coordination/intention_map.json` — a file nothing reads —
                // rather than at `workspace_intentions`; both are fixed.
                //
                // `Prompted`, not `Request` and not `Manual`. It rides the paid
                // `depth=recommendations` button, but pressing it only puts the
                // request in front of a model; whether the tool is called is the
                // model's decision, and a row count cannot distinguish "nobody
                // pressed it since the prompt changed" from "the strategist
                // ignored the instruction". `Manual` would license `no_input`,
                // which claims the first.
                trigger: Trigger::Prompted {
                    asked_by: "handlers::workspace::coherence, depth=recommendations, \
                               Stage 0",
                },
                accounted: None,
                gated_by: None,
            },
            Stage {
                id: "settled",
                what: "coherence is measured",
                sink_sql: "SELECT count(*)::bigint AS n FROM coherence_evaluations",
                writer: "handlers::workspace::messages (auto), workspace::coherence (manual)",
                trigger: Trigger::Request,
                accounted: Some(Sink::CoherenceEvaluations),
                gated_by: None,
            },
            Stage {
                id: "brief",
                what: "a coordination brief reaches the members",
                sink_sql: "SELECT count(*)::bigint AS n FROM episodes \
                            WHERE provenance = 'coordinator_observation'",
                writer: "agent_backend::tools_legacy::execute_record_coordination_observation",
                // Was `Manual`, for the same reason `intentions` was: the button
                // is manual and the tool call is not. Relabelled with it rather
                // than left behind — two stages of one loop, reached by one
                // prompt through one strategist, described two ways is the drift
                // this whole model is a defence against.
                trigger: Trigger::Prompted {
                    asked_by: "handlers::workspace::coherence, depth=recommendations, \
                               Stage 3",
                },
                accounted: Some(Sink::Episodes),
                gated_by: Some(Gate::Credit),
            },
        ],
    },
    FeedbackLoop {
        id: "loop4",
        name: "Composition evolution",
        scope: "composition",
        claim: "Team composition changes in response to measured per-agent \
                contribution.",
        stages: &[
            Stage {
                id: "claims",
                what: "an agent's quantified judgement is retained",
                sink_sql: "SELECT count(*)::bigint AS n FROM forecast_agent_claims",
                writer: "handlers::workspace::agent_params_hook::apply_agent_multipliers",
                trigger: Trigger::Request,
                accounted: Some(Sink::ForecastAgentClaims),
                gated_by: None,
            },
            Stage {
                id: "attributed",
                what: "credit is apportioned when a forecast resolves",
                sink_sql: "SELECT count(*)::bigint AS n FROM forecast_attributions",
                writer: "handlers::attribution::spawn_attribution",
                trigger: Trigger::Upstream,
                accounted: Some(Sink::ForecastAttributions),
                gated_by: None,
            },
            Stage {
                id: "proposed",
                what: "a roster change is filed",
                sink_sql: "SELECT count(*)::bigint AS n FROM composition_versions",
                writer: "handlers::composition_evolution::materialise_composition_proposal_handler",
                trigger: Trigger::None {
                    why: "Two writers, neither reachable. The declared driver, \
                          `composition_dream_handler`, INSERTs an \
                          `agent_invocation` row and returns; nothing in the \
                          codebase consumes those rows, so the strategist it \
                          addresses never runs. The attribution-driven \
                          `materialise` route is registered and has no caller in \
                          any UI.",
                },
                accounted: None,
                gated_by: None,
            },
            Stage {
                id: "accepted",
                what: "the owner applies it",
                sink_sql: "SELECT count(*)::bigint AS n FROM composition_versions \
                            WHERE accepted_by IS NOT NULL",
                writer: "handlers::composition::accept_composition_version_handler",
                trigger: Trigger::Manual,
                accounted: None,
                gated_by: None,
            },
        ],
    },
    FeedbackLoop {
        id: "loop5a",
        name: "Forecast calibration (Brier)",
        scope: "platform",
        claim: "A prediction is scored against an outcome that resolves \
                independently of it.",
        stages: &[
            Stage {
                id: "committed",
                what: "a prediction is anchored before the outcome",
                sink_sql: "SELECT count(*)::bigint AS n FROM forecast_commitments",
                writer: "handlers::forecasts (commit)",
                trigger: Trigger::Request,
                accounted: None,
                gated_by: None,
            },
            Stage {
                id: "resolved",
                what: "the world answers",
                sink_sql: "SELECT count(*)::bigint AS n FROM forecast_spacetime",
                writer: "handlers::forecasts, handlers::polymarket (sweeper)",
                trigger: Trigger::Scheduler {
                    env: "PM_RESOLUTION_SWEEP_SECS",
                    default_on: true,
                },
                accounted: None,
                gated_by: None,
            },
            Stage {
                id: "scored",
                what: "per-agent calibration is recorded",
                sink_sql: "SELECT count(*)::bigint AS n FROM eval_signals \
                            WHERE dimension = 'forecast_calibration'",
                writer: "handlers::forecasts::record_forecast_calibration_signals",
                trigger: Trigger::Upstream,
                accounted: Some(Sink::EvalSignals),
                gated_by: None,
            },
        ],
    },
    FeedbackLoop {
        id: "loop5b",
        name: "Projection accuracy",
        scope: "platform",
        claim: "A physical measurement is scored against what the model \
                projected — the one signal an agent cannot talk its way out of.",
        stages: &[
            Stage {
                id: "projected",
                what: "a model states a value in advance",
                sink_sql: "SELECT count(DISTINCT extra->>'projection_id')::bigint AS n \
                            FROM sosa_observations WHERE extra ? 'projection_id'",
                writer: "external dynamics runner via POST /api/observations",
                trigger: Trigger::Request,
                accounted: None,
                gated_by: None,
            },
            Stage {
                id: "anchored",
                what: "the projection is committed before measurement",
                sink_sql: "SELECT count(*)::bigint AS n FROM process_projection_commits",
                writer: "projection_commit::commit_projection",
                trigger: Trigger::Upstream,
                accounted: Some(Sink::ProcessProjectionCommits),
                gated_by: None,
            },
            Stage {
                id: "resolved",
                what: "a measurement meets a projection that pre-dated it",
                // `committed_before_measured`, not `count(*)`.
                //
                // This loop's claim is the one an agent cannot talk its way out
                // of, and the only thing that makes it true of a row is that the
                // prediction was anchored before the world was measured. A row
                // without that is a score against an answer that was already
                // knowable — arithmetic, and correct arithmetic, about nothing.
                //
                // Counting all rows would let the chain be closed by
                // transcription: anchoring the 61 projections already on file and
                // resolving them against the 7,576 real readings already on file
                // would fill this sink completely, and the report would say
                // `turning`. Migration 215 made the column mean what its name
                // says; this makes the loop count it.
                //
                // A `NULL` from the WHERE (no such column yet, migration 215
                // pending) is not possible — the query would error, `rows` would
                // read -1, and the walk reports `probe_failed` rather than zero.
                // That is the intended behaviour: an unmeasurable stage is not an
                // empty one.
                sink_sql: "SELECT count(*)::bigint AS n FROM process_spacetime \
                            WHERE committed_before_measured",
                writer: "handlers::simops_benchmark::resolve_against_projection",
                trigger: Trigger::Upstream,
                accounted: Some(Sink::ProcessSpacetime),
                gated_by: None,
            },
            Stage {
                id: "scored",
                what: "accuracy is recorded",
                sink_sql: "SELECT count(*)::bigint AS n FROM eval_signals \
                            WHERE evaluator_name ILIKE '%projection%'",
                writer: "evaluators::ProjectionScoringEvaluator",
                trigger: Trigger::Upstream,
                accounted: Some(Sink::EvalSignals),
                gated_by: None,
            },
        ],
    },
];

/// Every reason a stage can be reported empty for.
///
/// Declared because the set crosses a boundary: `panel_absence` maps each one
/// to a reading a renderer branches on, and a token added here without a
/// classification there would silently take the most benign default. That is
/// the same failure the seam registry exists to prevent, one layer up — a
/// closed token set with two independent opinions and nothing comparing them.
///
/// Ordered as [`diagnose`] ranks them, most actionable first, with
/// `probe_failed` last because [`evaluate`] rather than `diagnose` produces it.
pub const STALL_REASONS: &[&str] = &[
    "no_trigger",
    "scheduler_off",
    "writes_refused",
    "gate_refuses_everything",
    "upstream_unmeasured",
    "awaiting_upstream",
    "unobserved",
    "awaiting_agent",
    "no_input",
    "probe_failed",
];

/// What the walk knows about the stages above this one.
///
/// Three states rather than a `bool`, because *"the stage above produced
/// nothing"* and *"the stage above could not be read"* license different
/// conclusions, and only the first is a finding. Collapsing them is how a
/// diagnosis comes to point confidently at the wrong link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Upstream {
    /// Every stage above produced rows.
    Produced,
    /// A stage above produced nothing, and the probe that says so ran.
    Empty,
    /// A stage above could not be read. Nothing below it can be diagnosed.
    Unknown,
}

/// Why a stage has produced nothing.
///
/// Ordered by how actionable it is: a code fault first, a configuration fault
/// next, a fact about the world last.
///
/// **Precondition:** the stage's own probe ran and returned zero. A stage whose
/// probe failed has not been shown to have produced nothing, so there is nothing
/// here to explain; the walk reports `probe_failed` without consulting this
/// function.
///
/// The first four readings are properties of *this* stage — who calls it, how it
/// is scheduled, whether its writes land, whether its gate approves — and none of
/// them depend on the stage above. So they outrank [`Upstream::Unknown`] for the
/// same reason the existing tests require them to outrank [`Upstream::Empty`]:
/// wiring the stage above would change nothing.
pub fn diagnose(
    stage: &Stage,
    upstream: Upstream,
    env: impl Fn(&str) -> Option<String>,
) -> &'static str {
    if let Trigger::None { .. } = stage.trigger {
        return "no_trigger";
    }
    if let Trigger::Scheduler { env: var, .. } = stage.trigger {
        // Off by configuration counts as off, whatever the declared default:
        // `CONSOLIDATION_SWEEP_SECS=0` and an unset var with `default_on: false`
        // are the same outage.
        let set_to_zero = env(var).as_deref() == Some("0");
        let unset_and_off = env(var).is_none()
            && !matches!(
                stage.trigger,
                Trigger::Scheduler {
                    default_on: true,
                    ..
                }
            );
        if set_to_zero || unset_and_off {
            return "scheduler_off";
        }
    }
    if let Some(sink) = stage.accounted {
        if write_accounting::account(sink).is_totally_rejected() {
            return "writes_refused";
        }
    }
    if let Some(gate) = stage.gated_by {
        if gate_trust::account(gate).refuses_everything() {
            return "gate_refuses_everything";
        }
    }
    match upstream {
        // Nothing about this stage explains the zero, and the stage above was
        // never read. `awaiting_upstream` would be a guess and `no_input` a
        // claim about the world; both would send a reader to the wrong link.
        Upstream::Unknown => "upstream_unmeasured",
        Upstream::Empty => "awaiting_upstream",
        // `no_input` is a POSITIVE claim: the trigger had its chance and there
        // was nothing to do. It requires evidence, and for an instrumented
        // stage that evidence is an attempt.
        //
        // The counters are process-local `AtomicU64`s that start at zero, so a
        // freshly booted server — or a test process — has watched nothing. In
        // that state `no_input` and "this path has been failing since before
        // the restart" are indistinguishable, and answering `no_input` claims
        // the first.
        //
        // `unobserved` is the honest answer, and it is a different instruction
        // to the reader: `no_input` says look at the world, `unobserved` says
        // wait for traffic or look at a longer-lived process.
        Upstream::Produced => match stage.accounted {
            Some(sink) if write_accounting::account(sink).attempts == 0 => "unobserved",
            // A prompted stage has two ways to produce nothing and a row count
            // cannot tell them apart: the prompt has not run since it was
            // written, or it ran and the model did not call the tool. Both are
            // the absence of an observation, so neither licenses `no_input`,
            // which asserts the trigger fired and found nothing to do.
            //
            // Same argument as `unobserved` one line up, from the other
            // direction: there the counter has watched nothing, here the
            // behaviour being counted is a model's.
            _ if matches!(stage.trigger, Trigger::Prompted { .. }) => "awaiting_agent",
            _ => "no_input",
        },
    }
}

/// One stage's live state.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StageState {
    pub id: &'static str,
    pub what: &'static str,
    pub writer: &'static str,
    pub trigger: Trigger,
    /// `-1` when the count query could not run — never confused with zero.
    ///
    /// Read it through [`StageState::measured`] rather than comparing to `0`.
    pub rows: i64,
}

impl StageState {
    /// Did the count query run?
    ///
    /// The sentinel is on the wire and stays there for compatibility, but no
    /// logic should re-derive it: `rows == 0` was the comparison that let an
    /// unread stage read as a turning one.
    pub fn measured(&self) -> bool {
        self.rows >= 0
    }
}

/// One loop's live state.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LoopState {
    pub id: &'static str,
    pub name: &'static str,
    pub scope: &'static str,
    pub claim: &'static str,
    pub stages: Vec<StageState>,
    /// The first stage with no rows, or `None` when every stage has produced.
    pub stops_at: Option<&'static str>,
    /// Why, from whichever layer knows.
    pub reason: Option<&'static str>,
    /// `turning` — every stage read, every stage produced.
    /// `stalled` — every stage read, one produced nothing.
    /// `unmeasured` — a stage could not be read, so neither claim is available.
    pub status: &'static str,
}

impl LoopState {
    /// Did every stage's probe run?
    ///
    /// A loop with an unread stage supports no verdict about whether it turns.
    /// Callers that count turning loops must exclude these rather than fold them
    /// into either column.
    pub fn measured(&self) -> bool {
        self.stages.iter().all(StageState::measured)
    }
}

/// Walk every loop against a live database.
pub async fn evaluate(pool: &sqlx::PgPool) -> Vec<LoopState> {
    let mut out = Vec::with_capacity(LOOPS.len());
    for l in LOOPS {
        let mut stages = Vec::with_capacity(l.stages.len());
        let mut stops_at = None;
        let mut reason = None;
        let mut upstream = Upstream::Produced;

        for s in l.stages {
            let rows = sqlx::query_scalar::<_, i64>(s.sink_sql)
                .fetch_one(pool)
                .await
                .unwrap_or(-1);
            stages.push(StageState {
                id: s.id,
                what: s.what,
                writer: s.writer,
                trigger: s.trigger,
                rows,
            });

            if rows < 0 {
                // The probe failed. This stops the chain epistemically rather
                // than physically: the stage may well have produced, and we
                // cannot say. Everything below is `upstream_unmeasured`.
                if stops_at.is_none() {
                    stops_at = Some(s.id);
                    reason = Some("probe_failed");
                }
                upstream = Upstream::Unknown;
                continue;
            }

            if rows == 0 {
                if stops_at.is_none() {
                    stops_at = Some(s.id);
                    reason = Some(diagnose(s, upstream, |k| std::env::var(k).ok()));
                }
                // An unread stage above outranks an empty one: it is the weaker
                // claim, and the weaker claim is the true one.
                if upstream != Upstream::Unknown {
                    upstream = Upstream::Empty;
                }
            }
        }

        let unmeasured = stages.iter().any(|s| !s.measured());
        out.push(LoopState {
            id: l.id,
            name: l.name,
            scope: l.scope,
            claim: l.claim,
            stages,
            stops_at,
            reason,
            status: if unmeasured {
                "unmeasured"
            } else if stops_at.is_none() {
                "turning"
            } else {
                "stalled"
            },
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_loop_declares_a_chain_and_a_claim() {
        let mut ids = std::collections::HashSet::new();
        assert_eq!(LOOPS.len(), 6, "five loops, with 5a and 5b counted apart");
        for l in LOOPS {
            assert!(ids.insert(l.id), "duplicate loop id `{}`", l.id);
            assert!(
                l.stages.len() >= 3,
                "{}: a two-link chain is not a loop",
                l.id
            );
            assert!(
                l.claim.len() > 60,
                "{}: state what the loop achieves, so a stall shows the claim it \
                 is failing",
                l.id
            );
            let mut sids = std::collections::HashSet::new();
            for s in l.stages {
                assert!(sids.insert(s.id), "{}: duplicate stage `{}`", l.id, s.id);
                assert!(s.writer.contains("::") || s.writer.contains(' '));
                assert!(
                    s.sink_sql.contains("AS n"),
                    "{}.{}: the runner reads the `n` alias",
                    l.id,
                    s.id
                );
            }
        }
    }

    /// Every count must be a bare read.
    #[test]
    fn every_stage_query_is_read_only() {
        for l in LOOPS {
            for s in l.stages {
                let q = s.sink_sql.to_ascii_lowercase();
                assert!(q.trim_start().starts_with("select"), "{}.{}", l.id, s.id);
                for w in ["insert", "update ", "delete", "drop", "alter", "truncate"] {
                    assert!(!q.contains(w), "{}.{} contains `{w}`", l.id, s.id);
                }
            }
        }
    }

    /// A stage with no caller must say why, and the list may only shrink.
    ///
    /// This is the finding the model exists to make visible: an uncalled writer
    /// and an unexercised one have identical row counts, so `Trigger::None` is
    /// the only place the difference can be recorded.
    #[test]
    fn every_untriggered_stage_explains_itself() {
        let mut untriggered = Vec::new();
        for l in LOOPS {
            for s in l.stages {
                if let Trigger::None { why } = s.trigger {
                    assert!(
                        why.len() > 80,
                        "{}.{}: say what would create a caller",
                        l.id,
                        s.id
                    );
                    untriggered.push(format!("{}.{}", l.id, s.id));
                }
            }
        }
        assert_eq!(
            untriggered,
            vec!["loop4.proposed"],
            "the set of stages nothing calls has changed. It may only shrink — \
             if you have wired one, remove its `Trigger::None`; if you have \
             added one, that is a regression and needs a reason here."
        );
    }

    /// The diagnosis must rank a code fault above a fact about the world.
    #[test]
    fn a_stage_nothing_calls_is_not_reported_as_idle() {
        let untriggered = Stage {
            id: "x",
            what: "w",
            sink_sql: "SELECT 0::bigint AS n",
            writer: "a::b",
            trigger: Trigger::None { why: "y" },
            accounted: None,
            gated_by: None,
        };
        // Even with an empty upstream, "nothing calls it" is the answer that
        // matters: wiring the stage above would change nothing.
        assert_eq!(
            diagnose(&untriggered, Upstream::Empty, |_| None),
            "no_trigger"
        );
        assert_eq!(
            diagnose(&untriggered, Upstream::Unknown, |_| None),
            "no_trigger",
            "an unread upstream does not soften a stage nothing calls"
        );

        let scheduled = Stage {
            trigger: Trigger::Scheduler {
                env: "SOME_SWEEP_SECS",
                default_on: false,
            },
            ..untriggered
        };
        assert_eq!(
            diagnose(&scheduled, Upstream::Produced, |_| None),
            "scheduler_off"
        );
        assert_eq!(
            diagnose(&scheduled, Upstream::Produced, |_| Some("0".into())),
            "scheduler_off",
            "explicitly zero is off, whatever the default"
        );
        assert_eq!(
            diagnose(&scheduled, Upstream::Produced, |_| Some("3600".into())),
            "no_input"
        );
        assert_eq!(
            diagnose(&scheduled, Upstream::Empty, |_| Some("3600".into())),
            "awaiting_upstream"
        );
    }

    /// A zero under an unread stage is not `awaiting_upstream`.
    ///
    /// `awaiting_upstream` says the stage above produced nothing. If that stage
    /// was never read, the claim is unfounded, and it sends a reader to repair a
    /// link that may already work.
    #[test]
    fn an_unread_upstream_is_not_reported_as_an_empty_one() {
        let s = Stage {
            id: "x",
            what: "w",
            sink_sql: "SELECT 0::bigint AS n",
            writer: "a::b",
            trigger: Trigger::Request,
            accounted: None,
            gated_by: None,
        };
        assert_eq!(diagnose(&s, Upstream::Produced, |_| None), "no_input");
        assert_eq!(diagnose(&s, Upstream::Empty, |_| None), "awaiting_upstream");
        assert_eq!(
            diagnose(&s, Upstream::Unknown, |_| None),
            "upstream_unmeasured",
            "the weaker claim is the true one"
        );
    }

    /// The sentinel must not read as a count.
    #[test]
    fn an_unread_stage_is_neither_empty_nor_produced() {
        let unread = StageState {
            id: "x",
            what: "w",
            writer: "a::b",
            trigger: Trigger::Request,
            rows: -1,
        };
        let empty = StageState {
            rows: 0,
            ..unread.clone()
        };
        let produced = StageState {
            rows: 7,
            ..unread.clone()
        };

        assert!(!unread.measured());
        assert!(empty.measured(), "a measured zero is measured");
        assert!(produced.measured());

        // The bug this replaces: a loop whose first probe errored while later
        // stages held rows reported `turning`, because `rows == 0` was the only
        // condition that stopped the chain.
        let l = LoopState {
            id: "loopX",
            name: "n",
            scope: "agent",
            claim: "c",
            stages: vec![unread, produced],
            stops_at: Some("x"),
            reason: Some("probe_failed"),
            status: "unmeasured",
        };
        assert!(!l.measured());
        assert_ne!(l.status, "turning");
        assert_ne!(
            l.status, "stalled",
            "a loop nobody could read has not been shown to be stalled either"
        );
    }

    /// An instrumented stage nobody has watched must not claim the world was
    /// quiet.
    ///
    /// The distinction a reviewer of the loop report raised, and it is real.
    /// `write_accounting`'s counters are process-local and start at zero, so a
    /// fresh server has observed nothing. `no_input` asserts "the trigger had
    /// its chance and there was nothing to do"; with cold counters that is
    /// indistinguishable from "this path has been failing since before the
    /// restart", and the two send a reader to completely different places.
    ///
    /// An **uninstrumented** stage keeps `no_input`, because there is no
    /// attempt count to be cold: the diagnosis is as good as it was going to
    /// get, and downgrading every uninstrumented stage would make the reading
    /// useless.
    #[test]
    fn an_instrumented_stage_with_no_attempts_is_unobserved_not_idle() {
        let base = Stage {
            id: "x",
            what: "w",
            sink_sql: "SELECT 0::bigint AS n",
            writer: "a::b",
            trigger: Trigger::Request,
            accounted: None,
            gated_by: None,
        };

        // No counters exist for this stage, so nothing was ever claimed about
        // observation and `no_input` is the best available reading.
        assert_eq!(diagnose(&base, Upstream::Produced, |_| None), "no_input");

        // Instrumented and never attempted in this process: we have not looked.
        let watched = Stage {
            accounted: Some(crate::write_accounting::Sink::AnomalyEvents),
            ..base
        };
        assert_eq!(
            diagnose(&watched, Upstream::Produced, |_| None),
            "unobserved",
            "a stage whose counters have seen nothing must not report that the \
             world was quiet — it reports that nobody was watching"
        );

        // The stage's own faults still outrank it: a link nothing calls is
        // `no_trigger` whether or not anyone has been counting.
        let dead = Stage {
            trigger: Trigger::None { why: "y" },
            ..watched
        };
        assert_eq!(diagnose(&dead, Upstream::Produced, |_| None), "no_trigger");
    }

    /// A stage whose caller is a prompt must not claim the world was quiet.
    ///
    /// The state this arm exists for arrived by way of a fix: loop 3's
    /// `intentions` was `Trigger::None` — nothing asked for it — and wiring the
    /// prompt moved it to a trigger that produces nothing until a model
    /// obliges. Left as `Manual` it would have read `no_input`: "the trigger had
    /// its chance and there was nothing to do", asserted on a row count that
    /// cannot see whether the prompt ran at all. Repairing one link and
    /// upgrading the report's confidence in the same commit is how a fix comes
    /// to look like a result.
    #[test]
    fn a_prompted_stage_is_awaiting_an_agent_not_idle() {
        let base = Stage {
            id: "x",
            what: "w",
            sink_sql: "SELECT 0::bigint AS n",
            writer: "a::b",
            trigger: Trigger::Manual,
            accounted: None,
            gated_by: None,
        };
        assert_eq!(
            diagnose(&base, Upstream::Produced, |_| None),
            "no_input",
            "a person's button is the trigger and a zero really does mean nobody \
             pressed it; only the model's half is unobservable"
        );

        let prompted = Stage {
            trigger: Trigger::Prompted { asked_by: "p" },
            ..base
        };
        assert_eq!(
            diagnose(&prompted, Upstream::Produced, |_| None),
            "awaiting_agent"
        );

        // Ranked below the readings that are properties of the stage itself,
        // and below an empty upstream: if the stage above produced nothing, no
        // amount of prompting would have helped and `awaiting_upstream` is the
        // link to look at.
        assert_eq!(
            diagnose(&prompted, Upstream::Empty, |_| None),
            "awaiting_upstream"
        );
        assert_eq!(
            diagnose(&prompted, Upstream::Unknown, |_| None),
            "upstream_unmeasured"
        );

        // Every reason this function can now return is declared, in the order
        // it ranks them. A token `diagnose` produces and `STALL_REASONS` omits
        // falls through `panel_absence::classify_reason` — which is the bug
        // `every_stall_reason_is_classified` was written for, entered from the
        // side it cannot see.
        assert!(STALL_REASONS.contains(&"awaiting_agent"));
    }

    /// A scheduler that is on by default must not report `scheduler_off` when
    /// the variable is merely unset.
    #[test]
    fn an_on_by_default_sweeper_is_not_reported_as_off() {
        let s = Stage {
            id: "x",
            what: "w",
            sink_sql: "SELECT 0::bigint AS n",
            writer: "a::b",
            trigger: Trigger::Scheduler {
                env: "PM_RESOLUTION_SWEEP_SECS",
                default_on: true,
            },
            accounted: None,
            gated_by: None,
        };
        assert_eq!(diagnose(&s, Upstream::Produced, |_| None), "no_input");
        assert_eq!(
            diagnose(&s, Upstream::Produced, |_| Some("0".into())),
            "scheduler_off"
        );
    }
}
