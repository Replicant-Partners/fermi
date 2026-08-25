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

// ─── what a person can do ────────────────────────────────────────────────

/// A human action available at one loop stage.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct StageAction {
    pub loop_id: &'static str,
    pub stage: &'static str,
    /// HTTP method, as a UI would call it.
    pub method: &'static str,
    /// Path template, verbatim from the router so a UI can substitute params.
    pub path: &'static str,
    /// What pressing it does. One line, suitable for a button tooltip.
    pub does: &'static str,
    /// **Why a person rather than the platform.**
    ///
    /// Required. A manual stage that cannot say why it is manual should be
    /// automated, and the field exists to make that argument happen at
    /// declaration time rather than never. `loop_model` records that these
    /// stages are `Manual` or `Prompted`; it does not record why, and "why"
    /// is what a reviewer needs before deciding the queue is worth working.
    pub why_manual: &'static str,
}

/// Every human door into a loop.
///
/// Rule for adding one: **if a stage's trigger is `Manual` or `Prompted`, a
/// person is the mechanism and this is where their door is declared.**
pub const STAGE_ACTIONS: &[StageAction] = &[
    StageAction {
        loop_id: "loop2",
        stage: "reviewed",
        method: "POST",
        path: "/api/observatory/hitl/:event_id/action",
        does: "Record a reviewer's decision on one anomaly: correct the \
               episode, dismiss the event, or escalate it to an agent-wide \
               intervention.",
        why_manual: "The correction is a judgement about whether an agent's \
                     output was actually wrong, which is the one thing the \
                     platform cannot check about itself — an automated \
                     reviewer would be the same model grading its own \
                     homework. `hitl_actions` is the only place a human \
                     verdict enters Loop 2, and it holds zero rows.",
    },
    StageAction {
        loop_id: "loop2",
        stage: "consensus",
        method: "POST",
        path: "/api/observatory/hitl/consensus/:request_id",
        does: "Confirm, as a second reviewer, an intervention that would apply \
               to every run of an agent rather than to one episode.",
        why_manual: "A fleet-wide change on one reviewer's word is the failure \
                     mode two-reviewer consensus exists to prevent. The cost of \
                     being wrong scales with the agent's traffic, and nothing \
                     downstream would distinguish a considered change from a \
                     mistaken one.",
    },
    StageAction {
        loop_id: "loop3",
        stage: "settled",
        method: "POST",
        path: "/api/workspaces/:workspace_id/coherence",
        does: "Measure the workspace's coherence now. At `depth=recommendations` \
               it also runs the strategist, which is what produces Stage 0 \
               intentions and the Stage 3 coordination brief.",
        why_manual: "It costs credits and it interrupts. A sweeper would bill \
                     every workspace on a cadence nobody asked for; 4 of 267 \
                     workspaces have ever been evaluated, and that is a \
                     product fact rather than an outage.",
    },
    StageAction {
        loop_id: "loop4",
        stage: "accepted",
        method: "POST",
        // Read from the router, not guessed. The first draft of this entry said
        // `/api/compositions/:composition_id/accept`, which does not exist, and
        // `loop_api_contract::every_declared_action_path_exists_in_the_router`
        // caught it before the surface shipped — which is the case that check
        // was written for, arriving immediately.
        path: "/api/workspaces/:workspace_id/composition/versions/:version_id/accept",
        does: "Apply a proposed roster change to the workspace, as the owner.",
        why_manual: "Who is on a team is the owner's decision. The platform may \
                     propose from measured contribution and may not act: an \
                     agent removed by an automated proposal has no route back \
                     into the measurement that would have vindicated it.",
    },
];

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

    /// Every action says why a person has to do it.
    #[test]
    fn every_action_argues_for_being_manual() {
        for a in STAGE_ACTIONS {
            assert!(
                a.why_manual.len() > 100,
                "{}.{}: a manual stage that cannot say why it is manual should \
                 be automated",
                a.loop_id,
                a.stage
            );
            assert!(
                a.does.len() > 40,
                "{}.{}: say what pressing it does",
                a.loop_id,
                a.stage
            );
            assert!(
                a.path.starts_with("/api/"),
                "{}.{}: `{}` is not an API path",
                a.loop_id,
                a.stage,
                a.path
            );
            assert!(
                matches!(a.method, "GET" | "POST" | "PATCH" | "DELETE"),
                "{}.{}: `{}` is not a method",
                a.loop_id,
                a.stage,
                a.method
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
            (
                "loop2",
                "corrected",
                "Written by the platform as a consequence of `reviewed`, not by \
                 a separate action. A reviewer who has recorded a correction \
                 has already done the only thing a person does here.",
            ),
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
