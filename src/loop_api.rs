//! One shape for "where does this loop stand, and what can a person do about
//! it" — assembled, never recomputed.
//!
//! # Why an assembly layer and not a handler
//!
//! Four modules already answer the four questions a loop view needs:
//!
//! | question | owner |
//! |---|---|
//! | does the chain produce, stage by stage? | [`crate::loop_model`] |
//! | is an empty thing idle, faulty, or unknowable? | [`crate::panel_absence`] |
//! | does what it produces carry the signal the claim needs? | [`crate::outcome_trust`] |
//! | has the writer ever run? | [`crate::liveness_trust`] |
//!
//! Nothing here re-answers any of them. Per
//! `verification_for_agent_ecologies.md` §3.4 a trust calculation must have
//! exactly one implementation, and the reason this module exists at all is that
//! those four answers were reachable only through `/api/admin/schema-health` —
//! a diagnostic blob — and through `observatory::agent_loops_handler`, which is
//! 610 lines of bespoke SQL giving a *second* answer to `loop_model`'s question.
//!
//! # The part that is new: what a person can do
//!
//! Nothing in the codebase declared it. Half of these loops are human-gated by
//! design — Loop 2's `reviewed` stage *is* a person acting, Loop 4's `accepted`
//! is an owner clicking — and a loop stalled at a manual stage with no visible
//! door is indistinguishable from a broken one. Worse, it looks like a platform
//! defect when it is a queue nobody has been shown.
//!
//! [`STAGE_ACTIONS`] declares, per stage, the endpoint a UI should call and what
//! pressing it does. It is checked two ways: every entry names a real stage in
//! `loop_model`, and every path it advertises exists in the router
//! (`tests/loop_api_contract.rs`). A button that 404s is worse than no button.
//!
//! # Empty is never blank
//!
//! Every view carries a [`Reading`] and a sentence. A UI rendering this must
//! never show a bare zero: `idle` means correctly empty, `fault` means something
//! should have happened and did not, `unknown` means no contract can say. The
//! third is the one that matters — `unobserved` counters and `awaiting_agent`
//! stages are *not* healthy and *not* broken, and a surface that picks one is
//! lying in whichever direction it picked.

use crate::loop_model::{self, LoopState, Trigger};
use crate::outcome_trust;
use crate::panel_absence::{reading_for_reason, Reading};
use crate::surface::Door;

// ─── what a person can do ───────────────────────────────────────
//
// A [`Door`] from [`crate::surface`], with this domain's own key in `subject`:
// `loop2.reviewed`. The type and its rules are shared with gates and
// evaluators, because they are the same idea and the same mistakes; the
// vocabulary of subjects is not, because the three domains do not share a key.

/// A human action available at one loop stage.
///
/// Retains `loop_id` and `stage` alongside the [`Door`] so a client can group
/// without parsing `subject`, and so [`action_for`] does not have to build a
/// string to answer a lookup.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct StageAction {
    pub loop_id: &'static str,
    pub stage: &'static str,
    #[serde(flatten)]
    pub door: Door,
}

/// Every human door into a loop.
///
/// Rule for adding one: **if a stage's trigger is `Manual` or `Prompted`, a
/// person is the mechanism and this is where their door is declared.**
pub const STAGE_ACTIONS: &[StageAction] = &[
    StageAction {
        loop_id: "loop2",
        stage: "reviewed",
        door: Door {
            subject: "loop2.reviewed",
            method: "POST",
            path: "/api/observatory/hitl/:event_id/action",
            does: "Record a reviewer's decision on one anomaly: correct the episode, dismiss the event, or escalate it to an agent-wide intervention.",
            why_manual: "The correction is a judgement about whether an agent's output was actually wrong, which is the one thing the platform cannot check about itself — an automated reviewer would be the same model grading its own homework. `hitl_actions` is the only place a human verdict enters Loop 2, and it holds zero rows.",
        },
    },
    StageAction {
        loop_id: "loop2",
        stage: "consensus",
        door: Door {
            subject: "loop2.consensus",
            method: "POST",
            path: "/api/observatory/hitl/consensus/:request_id",
            does: "Confirm, as a second reviewer, an intervention that would apply to every run of an agent rather than to one episode.",
            why_manual: "A fleet-wide change on one reviewer's word is the failure mode two-reviewer consensus exists to prevent. The cost of being wrong scales with the agent's traffic, and nothing downstream would distinguish a considered change from a mistaken one.",
        },
    },
    StageAction {
        loop_id: "loop3",
        stage: "settled",
        door: Door {
            subject: "loop3.settled",
            method: "POST",
            path: "/api/workspaces/:workspace_id/coherence",
            does: "Measure the workspace's coherence now. At `depth=recommendations` it also runs the strategist, which is what produces Stage 0 intentions and the Stage 3 coordination brief.",
            why_manual: "It costs credits and it interrupts. A sweeper would bill every workspace on a cadence nobody asked for; 4 of 267 workspaces have ever been evaluated, and that is a product fact rather than an outage.",
        },
    },
    StageAction {
        loop_id: "loop4",
        stage: "accepted",
        door: Door {
            subject: "loop4.accepted",
            method: "POST",
        // Read from the router, not guessed. The first draft said
        // `/api/compositions/:composition_id/accept`, which does not exist, and the
        // router contract caught it before the surface shipped.
            path: "/api/workspaces/:workspace_id/composition/versions/:version_id/accept",
            does: "Apply a proposed roster change to the workspace, as the owner.",
            why_manual: "Who is on a team is the owner's decision. The platform may propose from measured contribution and may not act: an agent removed by an automated proposal has no route back into the measurement that would have vindicated it.",
        },
    },
];

// ─── what one agent's chain looks like ──────────────────────────────
//
// A different question from `loop_model`'s, not a filtered version of it, which
// is why the SQL is declared here and not there. `loop_model` answers "has this
// stage produced, platform-wide"; this answers "has it produced *for this
// agent*", and for eight of the twenty-three stages there is no such question —
// the table has no agent column and never will.
//
// That distinction is the whole point. The handler this replaces rendered
// platform figures under an agent's name, and its own comment records that two
// rows of it were hardcoded constants shown in a live status column.
// `panel_absence` already encodes the principle for panels: *"turning
// platform-wide does not say whether THIS subject has reached it"*.

/// Whether a stage can be asked about one agent.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum SubjectScope {
    /// Counted for one agent. `$1` is the agent id.
    PerAgent {
        #[serde(skip)]
        sql: &'static str,
    },
    /// No per-agent answer exists, and why.
    ///
    /// **Rendered, never hidden.** A stage omitted because it has no agent
    /// dimension looks identical to a stage at zero, and the second is a
    /// finding.
    Platform { because: &'static str },
}

/// One entry per stage. Every stage, with no default.
///
/// A missing entry would mean "no per-agent answer" by omission, which is the
/// benign default that turned `unobserved` into an idle system one module over.
/// `every_stage_declares_its_subject_scope` requires exactly one entry each.
pub const SUBJECT_SCOPES: &[(&str, &str, SubjectScope)] = &[
    // ── loop1 ────────────────────────────────────────────────────────────
    (
        "loop1",
        "episodes",
        SubjectScope::PerAgent {
            sql: "SELECT count(*)::bigint FROM episodes WHERE agent_id = $1",
        },
    ),
    (
        "loop1",
        "consolidated",
        SubjectScope::PerAgent {
            sql: "SELECT count(*)::bigint FROM consolidation_jobs WHERE agent_id = $1",
        },
    ),
    (
        "loop1",
        "rules",
        SubjectScope::PerAgent {
            sql: "SELECT count(*)::bigint FROM semantic_rules WHERE agent_id = $1",
        },
    ),
    (
        "loop1",
        "retrieved",
        SubjectScope::PerAgent {
            sql: "SELECT count(*)::bigint FROM semantic_rules \
               WHERE agent_id = $1 AND application_count > 0",
        },
    ),
    // ── loop2 ────────────────────────────────────────────────────────────
    (
        "loop2",
        "anomaly",
        SubjectScope::PerAgent {
            sql: "SELECT count(*)::bigint FROM anomaly_events WHERE agent_id = $1",
        },
    ),
    (
        "loop2",
        "reviewed",
        SubjectScope::PerAgent {
            sql: "SELECT count(*)::bigint FROM hitl_actions WHERE agent_id = $1",
        },
    ),
    (
        "loop2",
        "consensus",
        SubjectScope::PerAgent {
            sql: "SELECT count(*)::bigint FROM two_reviewer_requests WHERE agent_id = $1",
        },
    ),
    (
        "loop2",
        "corrected",
        SubjectScope::PerAgent {
            sql: "SELECT count(*)::bigint FROM episode_corrections WHERE agent_id = $1",
        },
    ),
    (
        "loop2",
        "persona_bumped",
        SubjectScope::PerAgent {
            sql: "SELECT count(*)::bigint FROM agents \
               WHERE agent_id = $1 AND persona_version > 1",
        },
    ),
    // ── loop3 ────────────────────────────────────────────────────────────
    (
        "loop3",
        "plans",
        SubjectScope::PerAgent {
            // `agent_id`, not `declared_by`. The question this answers is "has
            // anyone ever asked THIS agent what it intends to do", which is
            // about the subject of the row. `declared_by` would answer "how
            // often has this agent asked others", which is a fact about one
            // coordinator and would read as zero for every member on a
            // perfectly coordinated team.
            sql: "SELECT count(*)::bigint FROM workspace_intentions \
                  WHERE agent_id = $1 AND source = 'solicited'",
        },
    ),
    (
        "loop3",
        "intentions",
        SubjectScope::PerAgent {
            sql: "SELECT count(*)::bigint FROM workspace_intentions WHERE agent_id = $1",
        },
    ),
    (
        "loop3",
        "settled",
        SubjectScope::Platform {
            because: "`coherence_evaluations` is keyed by workspace. Coherence is a \
                  property of a composition, not of a member — an agent does not \
                  have a Γ of its own — so there is no per-agent count to give, \
                  and giving the workspace's would credit one member with the \
                  whole team's reading.",
        },
    ),
    (
        "loop3",
        "brief",
        SubjectScope::PerAgent {
            sql: "SELECT count(*)::bigint FROM episodes \
               WHERE agent_id = $1 AND provenance = 'coordinator_observation'",
        },
    ),
    // ── loop4 ────────────────────────────────────────────────────────────
    (
        "loop4",
        "conformed",
        SubjectScope::PerAgent {
            // The whole point of putting this in `eval_signals` rather than in
            // `gate_decisions`: it has an agent dimension, so "is THIS member
            // getting better or worse" is answerable. A process-local counter
            // could never have answered it.
            sql: "SELECT count(*)::bigint FROM eval_signals \
                  WHERE evaluator_name = 'schema_conformance' AND agent_id = $1",
        },
    ),
    (
        "loop4",
        "claims",
        SubjectScope::PerAgent {
            sql: "SELECT count(*)::bigint FROM forecast_agent_claims WHERE agent_id = $1",
        },
    ),
    (
        "loop4",
        "attributed",
        SubjectScope::Platform {
            because: "`forecast_attributions` carries neither an agent nor a \
                  workspace column. Per-agent credit lives in \
                  `forecast_agent_credit`, which this stage does not count — so \
                  the honest answer is that this chain link cannot be narrowed, \
                  rather than a number from a table that is about forecasts.",
        },
    ),
    (
        "loop4",
        "proposed",
        SubjectScope::Platform {
            because: "`composition_versions` is keyed by workspace. A roster change \
                  is a statement about a team; attributing one to a member \
                  would invert the direction of the loop.",
        },
    ),
    (
        "loop4",
        "accepted",
        SubjectScope::Platform {
            because: "As `proposed` — the same `composition_versions` table, \
                      filtered on `accepted_by`, and keyed by workspace. The \
                      owner who accepts is a person rather than an agent, so \
                      there is no agent to narrow this to even in principle.",
        },
    ),
    // ── loop5a ───────────────────────────────────────────────────────────
    (
        "loop5a",
        "committed",
        SubjectScope::Platform {
            because: "`forecast_commitments` has no agent column. A commitment is \
                  the forecast's, and which agents contributed to it lives in \
                  `fermi_forecasts.agents_used` — a JSONB array of names, not a \
                  join this count can make honestly.",
        },
    ),
    (
        "loop5a",
        "resolved",
        SubjectScope::Platform {
            because: "`forecast_spacetime` has no agent column, for the same reason: \
                  the world resolves a forecast, not an agent.",
        },
    ),
    (
        "loop5a",
        "scored",
        SubjectScope::PerAgent {
            sql: "SELECT count(*)::bigint FROM eval_signals \
               WHERE agent_id = $1 AND dimension = 'forecast_calibration'",
        },
    ),
    // ── loop5b ───────────────────────────────────────────────────────────
    (
        "loop5b",
        "projected",
        SubjectScope::Platform {
            because: "`sosa_observations` records what a sensor or a model runner \
                  said about the world. No agent is party to it — which is the \
                  property that makes this loop's signal one an agent cannot \
                  talk its way out of.",
        },
    ),
    (
        "loop5b",
        "anchored",
        SubjectScope::Platform {
            because: "`process_projection_commits` is keyed by workspace. As above: \
                  the anchor is the model's, not an agent's.",
        },
    ),
    (
        "loop5b",
        "resolved",
        SubjectScope::Platform {
            because: "`process_spacetime` is keyed by workspace. The row records \
                      a model's projection meeting a sensor reading; neither \
                      party is an agent, which is the property that makes this \
                      loop's signal one an agent cannot argue with.",
        },
    ),
    (
        "loop5b",
        "scored",
        SubjectScope::PerAgent {
            sql: "SELECT count(*)::bigint FROM eval_signals \
               WHERE agent_id = $1 AND evaluator_name ILIKE '%projection%'",
        },
    ),
];

/// The subject scope for a stage.
pub fn subject_scope(loop_id: &str, stage: &str) -> Option<&'static SubjectScope> {
    SUBJECT_SCOPES
        .iter()
        .find(|(l, s, _)| *l == loop_id && *s == stage)
        .map(|(_, _, sc)| sc)
}

/// The action available at a stage, if any.
pub fn action_for(loop_id: &str, stage: &str) -> Option<&'static StageAction> {
    STAGE_ACTIONS
        .iter()
        .find(|a| a.loop_id == loop_id && a.stage == stage)
}

// ─── the view ────────────────────────────────────────────────────────────

/// One stage, as a UI needs it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StageView {
    pub id: &'static str,
    /// What this link produces, as a reader would say it.
    pub what: &'static str,
    pub writer: &'static str,
    pub trigger: Trigger,
    /// `request` | `sweeper` | `upstream` | `manual` | `prompted` | `nothing_calls_it`.
    ///
    /// A flat label so a UI does not have to destructure the enum, and
    /// `nothing_calls_it` rather than `none` because it is a finding and should
    /// read as one in a table.
    pub trigger_label: &'static str,
    /// `-1` is never rendered. `measured` says whether to render at all.
    pub rows: i64,
    pub measured: bool,
    /// True for the first stage in the chain that has produced nothing.
    ///
    /// The only actionable link. Everything below it is empty *because of* it,
    /// and a UI that highlights all of them turns one finding into four.
    pub is_first_empty: bool,
    pub action: Option<StageAction>,
}

/// Whether what the stage produces means anything.
#[derive(Debug, Clone, serde::Serialize)]
pub struct OutcomeView {
    pub stage: &'static str,
    /// The narrower proposition checked, never the loop's claim.
    pub proposition: &'static str,
    /// **What passing does not establish.** Carried into the API on purpose:
    /// a UI that shows a green tick against the loop's claim would be
    /// over-reading, and this is the sentence that stops it.
    pub does_not_show: &'static str,
    /// `declared_gap` when the finding is a known one with an exit condition.
    pub declared_gap: Option<&'static str>,
}

/// One loop, assembled.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LoopView {
    pub id: &'static str,
    pub name: &'static str,
    pub scope: &'static str,
    /// What the architecture claims this loop achieves.
    pub claim: &'static str,
    /// `turning` | `stalled` | `unmeasured`.
    pub status: &'static str,
    pub stops_at: Option<&'static str>,
    pub reason: Option<&'static str>,
    /// `idle` | `fault` | `unknown` — never a bare zero.
    pub reading: Reading,
    /// One sentence for a human, naming the loop and the link.
    pub detail: String,
    pub stages: Vec<StageView>,
    pub outcomes: Vec<OutcomeView>,
}

fn trigger_label(t: &Trigger) -> &'static str {
    match t {
        Trigger::Request => "request",
        Trigger::Scheduler { .. } => "sweeper",
        Trigger::Upstream => "upstream",
        Trigger::Manual => "manual",
        Trigger::Prompted { .. } => "prompted",
        Trigger::None { .. } => "nothing_calls_it",
    }
}

/// Assemble one loop's view from its measured state.
///
/// Pure over [`LoopState`], so the shape a UI receives is unit-testable without
/// a database — the same split as `liveness_trust::classify` and
/// `panel_absence::resolve`, and for the same reason.
pub fn view(state: &LoopState) -> LoopView {
    // `Unknown` when the chain could not be read, whatever else is true. An
    // unread loop supports no verdict, and this is the field a UI branches on.
    let reading = match (state.measured(), state.reason) {
        (false, _) => Reading::Unknown,
        (true, Some(r)) => reading_for_reason(r),
        // Every stage produced. Idle is right: there is nothing to report.
        (true, None) => Reading::Idle,
    };

    let detail = match (state.stops_at, state.reason) {
        (None, _) => format!("{} is turning: every stage has produced.", state.name),
        (Some(at), Some(r)) => format!("{} stops at `{at}` ({r}). {}", state.name, state.claim),
        (Some(at), None) => format!("{} stops at `{at}`.", state.name),
    };

    let stages = state
        .stages
        .iter()
        .map(|s| StageView {
            id: s.id,
            what: s.what,
            writer: s.writer,
            trigger: s.trigger,
            trigger_label: trigger_label(&s.trigger),
            rows: s.rows,
            measured: s.measured(),
            is_first_empty: state.stops_at == Some(s.id),
            action: action_for(state.id, s.id).copied(),
        })
        .collect();

    let outcomes = outcome_trust::OUTCOME_CONTRACTS
        .iter()
        .filter(|c| c.loop_id == state.id)
        .map(|c| OutcomeView {
            stage: c.stage,
            proposition: c.proposition,
            does_not_show: c.does_not_show,
            declared_gap: outcome_trust::KNOWN_GAPS
                .iter()
                .find(|g| g.metric == format!("{}.{}", c.loop_id, c.stage))
                .map(|g| g.cleared_by),
        })
        .collect();

    LoopView {
        id: state.id,
        name: state.name,
        scope: state.scope,
        claim: state.claim,
        status: state.status,
        stops_at: state.stops_at,
        reason: state.reason,
        reading,
        detail,
        stages,
        outcomes,
    }
}

/// Assemble every loop.
pub fn views(states: &[LoopState]) -> Vec<LoopView> {
    states.iter().map(view).collect()
}

/// One loop by id, from an already-walked set.
pub fn view_of<'a>(states: &'a [LoopState], loop_id: &str) -> Option<LoopView> {
    states.iter().find(|s| s.id == loop_id).map(view)
}

/// The header, in five buckets.
///
/// # Why five
///
/// The first version had three — `turning`, `stalled`, `unmeasured` — and
/// against production it printed **"2 turning, 0 stalled, 4 unmeasured"**,
/// which is wrong twice over. Nothing was unmeasured: every probe ran. And "0
/// stalled" invites a reader to conclude nothing is wrong when four loops have
/// stopped.
///
/// The words were borrowed from `loop_model`, where `unmeasured` means *a
/// stage's probe did not run*. Here they were being used for *the reason is
/// unknowable*, and those are the two states this whole codebase most insists
/// on separating. So:
///
/// | bucket | means |
/// |---|---|
/// | `turning` | every stage produced |
/// | `stalled_by_fault` | stopped, and the reason is in the code |
/// | `stalled_idle` | stopped, correctly — nothing has had occasion |
/// | `no_reading` | stopped, and no contract can say why. **Not healthy, not broken** |
/// | `unreadable` | a probe did not run, so the chain supports no verdict |
///
/// A UI may colour the first two and must not colour `no_reading` as either.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct LoopTally {
    pub total: usize,
    pub turning: usize,
    pub stalled_by_fault: usize,
    pub stalled_idle: usize,
    pub no_reading: usize,
    pub unreadable: usize,
}

pub fn tally(views: &[LoopView]) -> LoopTally {
    let mut t = LoopTally {
        total: views.len(),
        turning: 0,
        stalled_by_fault: 0,
        stalled_idle: 0,
        no_reading: 0,
        unreadable: 0,
    };
    for v in views {
        // Unreadable first: a chain with an unread stage supports no verdict,
        // whatever its reason field says.
        if v.status == "unmeasured" {
            t.unreadable += 1;
        } else if v.stops_at.is_none() {
            t.turning += 1;
        } else {
            match v.reading {
                Reading::Fault => t.stalled_by_fault += 1,
                Reading::Idle => t.stalled_idle += 1,
                Reading::Unknown => t.no_reading += 1,
            }
        }
    }
    t
}

// ─── the per-agent view ─────────────────────────────────────────────────

/// One stage, narrowed to one agent.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentStageView {
    pub id: &'static str,
    pub what: &'static str,
    pub trigger_label: &'static str,
    /// This agent's count, or `None` when the stage has no per-agent answer.
    ///
    /// `None` and `Some(0)` are different states and must render differently:
    /// the first means the question does not apply, the second means it applies
    /// and the answer is nothing.
    pub rows: Option<i64>,
    #[serde(flatten)]
    pub scope: SubjectScope,
    /// The platform-wide count for the same stage, for context.
    ///
    /// Carried so a reader can see "nothing here, and nothing anywhere" apart
    /// from "nothing here, and plenty elsewhere" — which are different
    /// questions about the same zero. Never rendered as this agent's figure;
    /// that substitution is the defect this view replaces.
    pub platform_rows: i64,
    pub platform_measured: bool,
    pub action: Option<StageAction>,
}

/// One loop, narrowed to one agent.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentLoopView {
    pub id: &'static str,
    pub name: &'static str,
    pub claim: &'static str,
    /// How much of this loop can be asked about one agent at all.
    ///
    /// `answerable` of `total` stages. A loop where most stages are
    /// platform-scoped is not a loop this agent can be judged by, and a surface
    /// that shows four zeroes without saying so implies otherwise.
    pub answerable: usize,
    pub total: usize,
    /// The first stage that is answerable for this agent and reads zero.
    ///
    /// Platform-scoped stages are skipped: they cannot be this agent's first
    /// empty link, because they are not about this agent.
    pub stops_at: Option<&'static str>,
    pub stages: Vec<AgentStageView>,
}

/// Assemble one agent's view of one loop.
///
/// Pure over the platform state and this agent's counts, so the shape is
/// testable without a database — the same split as [`view`].
pub fn agent_view(state: &LoopState, counts: &[(&'static str, Option<i64>)]) -> AgentLoopView {
    let stages: Vec<AgentStageView> = state
        .stages
        .iter()
        .map(|s| {
            let scope = *subject_scope(state.id, s.id).unwrap_or(&SubjectScope::Platform {
                // Unreachable while `every_stage_declares_its_subject_scope`
                // holds. Stated rather than `unwrap`ped so a missing
                // declaration degrades to "no per-agent answer" — the reading
                // that claims least — instead of panicking a request.
                because: "This stage declares no subject scope, which is a gap \
                          in the declaration rather than a fact about the agent.",
            });
            AgentStageView {
                id: s.id,
                what: s.what,
                trigger_label: trigger_label(&s.trigger),
                rows: counts
                    .iter()
                    .find(|(id, _)| *id == s.id)
                    .and_then(|(_, n)| *n),
                scope,
                platform_rows: s.rows,
                platform_measured: s.measured(),
                action: action_for(state.id, s.id).copied(),
            }
        })
        .collect();

    let answerable = stages
        .iter()
        .filter(|s| matches!(s.scope, SubjectScope::PerAgent { .. }))
        .count();

    // The first answerable stage reading zero. A platform-scoped stage cannot
    // be this agent\'s first empty link — it is not about this agent — and
    // treating it as one is how the handler this replaces came to show platform
    // figures in an agent\'s status column.
    let stops_at = stages
        .iter()
        .filter(|s| matches!(s.scope, SubjectScope::PerAgent { .. }))
        .find(|s| s.rows == Some(0))
        .map(|s| s.id);

    AgentLoopView {
        id: state.id,
        name: state.name,
        claim: state.claim,
        answerable,
        total: stages.len(),
        stops_at,
        stages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loop_model::StageState;

    fn stage(id: &'static str, rows: i64, trigger: Trigger) -> StageState {
        StageState {
            id,
            what: "w",
            writer: "a::b",
            trigger,
            rows,
        }
    }

    fn state(
        id: &'static str,
        stages: Vec<StageState>,
        stops_at: Option<&'static str>,
        reason: Option<&'static str>,
        status: &'static str,
    ) -> LoopState {
        LoopState {
            id,
            name: "N",
            scope: "platform",
            claim: "C",
            stages,
            stops_at,
            reason,
            status,
        }
    }

    /// An unread loop reads `unknown`, whatever its reason says.
    ///
    /// The field a UI branches on, and the one that must not default to
    /// something renderable. `rows == 0` reading as success is the defect
    /// `loop_model` was given a tri-state to prevent, and an assembly layer is
    /// exactly where it would be reintroduced.
    #[test]
    fn a_loop_that_could_not_be_read_is_unknown_not_idle() {
        let v = view(&state(
            "l",
            vec![stage("x", -1, Trigger::Request)],
            Some("x"),
            Some("probe_failed"),
            "unmeasured",
        ));
        assert_eq!(v.reading, Reading::Unknown);
        assert!(!v.stages[0].measured);

        // And it is not counted as turning by the tally, even though a caller
        // filtering on `stops_at.is_none()` alone would have to decide.
        let t = tally(&[v]);
        assert_eq!(t.turning, 0);
        assert_eq!(t.unreadable, 1, "a probe that did not run is `unreadable`");
        assert_eq!(
            t.no_reading, 0,
            "and not `no_reading`, which is a different state"
        );
    }

    /// Every loop lands in exactly one bucket.
    ///
    /// The property a header depends on. A loop falling through would make the
    /// buckets sum to less than the total and a UI would render a count that
    /// silently omits it — the same shape as `panel_absence`'s `_ => Idle`
    /// fallthrough, in the summary line rather than the panel.
    #[test]
    fn the_buckets_partition_the_loops_and_name_five_different_states() {
        let cases = vec![
            // turning
            state(
                "a",
                vec![stage("x", 1, Trigger::Request)],
                None,
                None,
                "turning",
            ),
            // stopped, code fault
            state(
                "b",
                vec![stage("x", 0, Trigger::None { why: "y" })],
                Some("x"),
                Some("no_trigger"),
                "stalled",
            ),
            // stopped, correctly
            state(
                "c",
                vec![stage("x", 0, Trigger::Request)],
                Some("x"),
                Some("no_input"),
                "stalled",
            ),
            // stopped, no reading available
            state(
                "d",
                vec![stage("x", 0, Trigger::Request)],
                Some("x"),
                Some("unobserved"),
                "stalled",
            ),
            // probe did not run
            state(
                "e",
                vec![stage("x", -1, Trigger::Request)],
                Some("x"),
                Some("probe_failed"),
                "unmeasured",
            ),
        ];
        let t = tally(&views(&cases));
        assert_eq!(t.total, 5);
        assert_eq!(t.turning, 1);
        assert_eq!(t.stalled_by_fault, 1);
        assert_eq!(t.stalled_idle, 1);
        assert_eq!(t.no_reading, 1);
        assert_eq!(t.unreadable, 1);
        assert_eq!(
            t.turning + t.stalled_by_fault + t.stalled_idle + t.no_reading + t.unreadable,
            t.total,
            "a loop fell through the buckets, so the header omits it silently"
        );
    }

    /// A code fault reads `fault`; an idle loop reads `idle`.
    #[test]
    fn the_reading_comes_from_the_reason_and_not_from_the_row_count() {
        let dead = view(&state(
            "l",
            vec![stage("x", 0, Trigger::None { why: "y" })],
            Some("x"),
            Some("no_trigger"),
            "stalled",
        ));
        assert_eq!(dead.reading, Reading::Fault);
        assert_eq!(dead.stages[0].trigger_label, "nothing_calls_it");

        let idle = view(&state(
            "l",
            vec![stage("x", 0, Trigger::Request)],
            Some("x"),
            Some("no_input"),
            "stalled",
        ));
        assert_eq!(idle.reading, Reading::Idle);

        // `awaiting_agent` is neither: a prompt asked and no model has obliged,
        // and a UI that renders that as a healthy empty panel is wrong in the
        // direction that stops anyone looking.
        let waiting = view(&state(
            "l",
            vec![stage("x", 0, Trigger::Prompted { asked_by: "p" })],
            Some("x"),
            Some("awaiting_agent"),
            "stalled",
        ));
        assert_eq!(waiting.reading, Reading::Unknown);
        assert_eq!(waiting.stages[0].trigger_label, "prompted");
    }

    /// Exactly one stage is marked actionable.
    ///
    /// Everything below the first empty link is empty *because of* it. A UI that
    /// highlights all four turns one finding into four, which is the failure
    /// `loop_model`'s stage-by-stage view exists to prevent.
    #[test]
    fn only_the_first_empty_stage_is_flagged() {
        let v = view(&state(
            "l",
            vec![
                stage("a", 5, Trigger::Request),
                stage("b", 0, Trigger::Upstream),
                stage("c", 0, Trigger::Upstream),
            ],
            Some("b"),
            Some("no_input"),
            "stalled",
        ));
        let flagged: Vec<&str> = v
            .stages
            .iter()
            .filter(|s| s.is_first_empty)
            .map(|s| s.id)
            .collect();
        assert_eq!(flagged, vec!["b"]);
    }

    /// Every declared action names a real loop and stage.
    ///
    /// The cross-boundary pin. An action naming a renamed stage would render a
    /// button on nothing, and the entry would look like coverage.
    #[test]
    fn every_declared_action_names_a_real_stage() {
        for a in STAGE_ACTIONS {
            let l = loop_model::LOOPS
                .iter()
                .find(|l| l.id == a.loop_id)
                .unwrap_or_else(|| panic!("{} is not a declared loop", a.loop_id));
            assert!(
                l.stages.iter().any(|s| s.id == a.stage),
                "{} has no stage `{}`",
                a.loop_id,
                a.stage
            );
        }
    }

    /// A door is never declared on a stage the platform drives itself.
    ///
    /// The first version of this asserted `Manual | Prompted`, and
    /// `loop3.settled` failed it — correctly. Its trigger is `Request`, and a
    /// request *is* a person pressing something; the coherence endpoint costs
    /// credits and interrupts, which is exactly why it is not on a sweeper.
    /// The rule was too strict and the test found it before the API shipped.
    ///
    /// What must never carry a door is [`Trigger::Upstream`] and
    /// [`Trigger::Scheduler`]: those run without anyone, so advertising a
    /// manual step for them would send a reviewer to do work the platform has
    /// already done. [`Trigger::None`] must not either — a door onto a stage
    /// nothing calls is a button that writes into a dead end.
    #[test]
    fn no_door_is_declared_on_a_stage_the_platform_drives_itself() {
        for a in STAGE_ACTIONS {
            let l = loop_model::LOOPS
                .iter()
                .find(|l| l.id == a.loop_id)
                .unwrap();
            let s = l.stages.iter().find(|s| s.id == a.stage).unwrap();
            assert!(
                !matches!(
                    s.trigger,
                    Trigger::Upstream | Trigger::Scheduler { .. } | Trigger::None { .. }
                ),
                "{}.{} declares a human action and its trigger is {:?} — no \
                 person is needed, so the door would send a reviewer to do \
                 work already done (or into a stage nothing calls)",
                a.loop_id,
                a.stage,
                s.trigger
            );
        }
    }

    /// Every door satisfies the shared rules.
    ///
    /// Delegated to [`crate::surface::door_problems`] rather than restated. The
    /// rules — an argument for being manual, a real method, an `/api/` path, no
    /// duplicates — are the same for gates and evaluators, and three copies of
    /// them would be three chances for one to soften.
    #[test]
    fn every_door_satisfies_the_shared_rules() {
        let doors: Vec<_> = STAGE_ACTIONS.iter().map(|a| a.door).collect();
        let problems = crate::surface::door_problems(&doors);
        assert!(problems.is_empty(), "\n  {}\n", problems.join("\n  "));
    }

    /// Each door\'s `subject` is its own key, and the key is not decorative.
    ///
    /// A client may group by `subject` without parsing, and the router contract
    /// reports failures by it. A subject that disagreed with the `loop_id` and
    /// `stage` beside it would send a reader to the wrong stage with a correct
    /// error message, which is worse than no message.
    #[test]
    fn every_doors_subject_matches_the_stage_it_sits_on() {
        for a in STAGE_ACTIONS {
            assert_eq!(
                a.door.subject,
                format!("{}.{}", a.loop_id, a.stage),
                "the door\'s subject disagrees with the stage it is declared on"
            );
        }
    }

    /// Every stage a person must *work* has a door, or the loop cannot be run.
    ///
    /// The other direction, and narrower than the test above. `Manual` and
    /// `Prompted` are stages that go nowhere until a **reviewer or owner acts
    /// on a queue** — those need a visible way in, because a queue with no door
    /// is indistinguishable from a platform defect.
    ///
    /// `Request` stages are deliberately *not* in this set. They are driven by
    /// clients in the normal course of using the product — an agent executing,
    /// a projection arriving — and demanding a declared "door" for each would
    /// fill this table with endpoints nobody thinks of as a queue.
    #[test]
    fn every_manual_stage_has_a_declared_door_or_is_named_here() {
        // Stages a person drives that deliberately have no direct endpoint,
        // with the reason. May only shrink.
        const NO_DOOR: &[(&str, &str, &str)] = &[
            // `loop2.corrected` was here, and the stale check below found it on
            // its first run. The excuse was true — the platform writes it as a
            // consequence of `reviewed` — but the stage is `Trigger::Upstream`,
            // so the loop never reached the entry and the excuse was never
            // read. `Upstream`'s own declaration already says it, in the one
            // place that cannot drift from the trigger.
            (
                "loop3",
                "brief",
                "Produced by the strategist inside the `settled` run at \
                 `depth=recommendations`, so its door is that stage's. A second \
                 endpoint would let a brief be requested without the coherence \
                 measurement it is supposed to be about.",
            ),
            (
                "loop3",
                "intentions",
                "The same strategist run, Stage 0 rather than Stage 3, so its \
                 door is `settled`'s as well. Declaring intentions without the \
                 coherence measurement they are meant to be coordinated \
                 against would let the map be filled by something with no view \
                 of the workspace.",
            ),
            // `loop3.plans` was here while it was `Trigger::Prompted`. The
            // platform now asks during the `settled` request itself, so the
            // stage is `Request`-driven and needs no excuse — removed rather
            // than left, per the stale check below.
        ];

        let mut missing = Vec::new();
        for l in loop_model::LOOPS {
            for s in l.stages {
                let driven = matches!(s.trigger, Trigger::Manual | Trigger::Prompted { .. });
                if !driven {
                    continue;
                }
                let excused = NO_DOOR.iter().any(|(li, si, _)| *li == l.id && *si == s.id);
                if action_for(l.id, s.id).is_none() && !excused {
                    missing.push(format!("{}.{}", l.id, s.id));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "{} stage(s) are driven by a person and advertise no way in: {:?}. \
             Declare the endpoint in `STAGE_ACTIONS`, or add it to `NO_DOOR` \
             with the reason its door is somewhere else.",
            missing.len(),
            missing
        );
        for (l, s, why) in NO_DOOR {
            assert!(why.len() > 60, "{l}.{s}: say where the door actually is");
        }

        // **The list may only shrink.** An excuse for a stage that is no longer
        // person-driven is not harmless: the loop above skips undriven stages
        // before it ever consults `NO_DOOR`, so a stale entry is never read and
        // never fails, and the next person to look sees a documented reason
        // that has quietly stopped being true.
        //
        // `loop3.plans` is the case that prompted this. It earned an entry as
        // `Trigger::Prompted` — the strategist may or may not call
        // `solicit_agent_plan` — and when the platform took the asking over,
        // the trigger became `Request` and the excuse became fiction. Nothing
        // would have said so.
        let stale: Vec<String> = NO_DOOR
            .iter()
            .filter(|(li, si, _)| {
                !loop_model::LOOPS.iter().any(|l| {
                    l.id == *li
                        && l.stages.iter().any(|s| {
                            s.id == *si
                                && matches!(s.trigger, Trigger::Manual | Trigger::Prompted { .. })
                        })
                })
            })
            .map(|(l, s, _)| format!("{l}.{s}"))
            .collect();
        assert!(
            stale.is_empty(),
            "{} NO_DOOR entr(ies) excuse a stage that is no longer driven by a \
             person (or no longer exists): {:?}. Delete them — an excuse nothing \
             reads is a documented reason that has stopped being true, and this \
             list only earns its keep by shrinking.",
            stale.len(),
            stale
        );
    }

    /// Every stage declares whether it can be asked about one agent.
    ///
    /// Exactly one entry each, with no default. A missing entry would mean "no
    /// per-agent answer" by omission, and the benign default is how `unobserved`
    /// once came to display as an idle system on every panel backed by a loop.
    #[test]
    fn every_stage_declares_its_subject_scope_exactly_once() {
        let mut missing = Vec::new();
        for l in loop_model::LOOPS {
            for s in l.stages {
                let n = SUBJECT_SCOPES
                    .iter()
                    .filter(|(li, si, _)| *li == l.id && *si == s.id)
                    .count();
                if n != 1 {
                    missing.push(format!("{}.{} declared {n} time(s)", l.id, s.id));
                }
            }
        }
        assert!(missing.is_empty(), "\n  {}\n", missing.join("\n  "));

        // And nothing declared for a stage that does not exist — an entry with
        // no stage is a query nobody runs and looks like coverage.
        for (l, st, _) in SUBJECT_SCOPES {
            assert!(
                loop_model::LOOPS
                    .iter()
                    .any(|lp| lp.id == *l && lp.stages.iter().any(|s| s.id == *st)),
                "`{l}.{st}` declares a subject scope and is not a stage"
            );
        }
    }

    /// Every per-agent probe is a read of one agent, and every skip says why.
    #[test]
    fn every_subject_probe_is_read_only_and_takes_the_agent() {
        let mut answerable = 0;
        for (l, st, sc) in SUBJECT_SCOPES {
            match sc {
                SubjectScope::PerAgent { sql } => {
                    answerable += 1;
                    let q = sql.to_ascii_lowercase();
                    assert!(q.trim_start().starts_with("select"), "{l}.{st}");
                    for w in ["insert", "update ", "delete", "drop", "alter", "truncate"] {
                        assert!(!q.contains(w), "{l}.{st} contains `{w}`");
                    }
                    // Without `$1` it counts the whole platform and reports it
                    // as the agent's — the exact substitution this view exists
                    // to stop.
                    assert!(
                        sql.contains("$1"),
                        "{l}.{st} is declared per-agent and does not bind the \
                         agent, so it would report the platform's count as this \
                         agent's"
                    );
                }
                SubjectScope::Platform { because } => {
                    assert!(
                        because.len() > 60,
                        "{l}.{st}: say why there is no per-agent answer — a bare \
                         omission reads as a zero"
                    );
                }
            }
        }
        assert!(
            answerable >= 8,
            "only {answerable} stage(s) can be asked about an agent, which is \
             too few for a per-agent view to be worth serving"
        );
    }

    /// A platform-scoped stage is never this agent's first empty link.
    ///
    /// The defect this view replaces, stated as a test: the old handler showed
    /// platform figures in an agent's status column, and two of its rows were
    /// hardcoded constants. A stage that is not about this agent cannot be where
    /// this agent's chain stops.
    #[test]
    fn a_platform_scoped_stage_is_not_the_agents_first_empty_link() {
        // loop5a: committed (platform), resolved (platform), scored (per-agent).
        let s = state(
            "loop5a",
            vec![
                stage("committed", 1354, Trigger::Request),
                stage(
                    "resolved",
                    2180,
                    Trigger::Scheduler {
                        env: "X",
                        default_on: true,
                    },
                ),
                stage("scored", 239, Trigger::Upstream),
            ],
            None,
            None,
            "turning",
        );
        // This agent has no calibration signal of its own.
        let v = agent_view(&s, &[("scored", Some(0))]);
        assert_eq!(
            v.stops_at,
            Some("scored"),
            "the agent's chain stops at the first stage that is about the agent \
             and reads zero"
        );
        assert_eq!(v.answerable, 1, "only `scored` can be asked per agent");
        assert_eq!(v.total, 3);

        // The platform figures are carried and are not the agent's.
        let committed = v.stages.iter().find(|x| x.id == "committed").unwrap();
        assert!(matches!(committed.scope, SubjectScope::Platform { .. }));
        assert_eq!(
            committed.rows, None,
            "a platform-scoped stage must have no agent count at all — `None` \
             and `Some(0)` are different states"
        );
        assert_eq!(committed.platform_rows, 1354);
    }

    /// `None` and `Some(0)` must not collapse.
    #[test]
    fn no_answer_is_distinguishable_from_an_answer_of_zero() {
        let s = state(
            "loop1",
            vec![
                stage("episodes", 3576, Trigger::Request),
                stage("consolidated", 213, Trigger::Upstream),
            ],
            None,
            None,
            "turning",
        );
        let v = agent_view(&s, &[("episodes", Some(0)), ("consolidated", None)]);
        let ep = v.stages.iter().find(|x| x.id == "episodes").unwrap();
        let co = v.stages.iter().find(|x| x.id == "consolidated").unwrap();
        assert_eq!(
            ep.rows,
            Some(0),
            "the question applies and the answer is none"
        );
        assert_eq!(co.rows, None, "the probe did not run for this agent");
        // Both are `PerAgent` by declaration, so the difference is only in the
        // count — which is exactly why the count is an `Option`.
        assert!(matches!(ep.scope, SubjectScope::PerAgent { .. }));
        assert!(matches!(co.scope, SubjectScope::PerAgent { .. }));
        assert_eq!(
            v.stops_at,
            Some("episodes"),
            "a stage whose probe did not run must not be reported as the stop"
        );
    }

    /// The outcome view carries its own limits.
    #[test]
    fn an_outcome_view_cannot_be_read_as_the_claim() {
        for c in outcome_trust::OUTCOME_CONTRACTS {
            let s = state(c.loop_id, vec![], None, None, "turning");
            let v = view(&s);
            for o in &v.outcomes {
                assert_ne!(
                    o.proposition, v.claim,
                    "{}.{}: the API would show the loop's claim as the thing \
                     that was checked",
                    c.loop_id, o.stage
                );
                assert!(
                    o.does_not_show.len() > 100,
                    "{}.{}: a UI showing a tick with no caveat over-reads it",
                    c.loop_id,
                    o.stage
                );
            }
        }
    }
}
