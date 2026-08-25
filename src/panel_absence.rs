//! Why is this panel empty?
//!
//! # The rule this enforces
//!
//! **No panel authors its own empty state.** A surface that has nothing to show
//! receives a stamped answer derived from the trust contracts — the same
//! substrate the audit interrogates — rather than printing a string a frontend
//! author guessed at.
//!
//! # What goes wrong without it
//!
//! `docs/architecture/FEEDBACK_LOOPS.md` enumerates nine ways a loop can look
//! fine and not be fine. Four of them are invisible at the surface *by
//! construction*, and they render identically:
//!
//! | class | what the reader sees | what is actually true |
//! |---|---|---|
//! | severed read path | an empty panel | the writer filled a different table |
//! | closed ≠ turning | an empty panel | every hop works; the corpus is ineligible |
//! | reachable ≠ reached | an empty panel | a gate declined, correctly, 248 times |
//! | called ≠ succeeded | an empty panel | the callee failed non-fatally, always |
//!
//! Four diagnoses, one rendering, and today that rendering is `"No data yet"`.
//! A UI that collapses them is not neutral between them: it converts a
//! verification signal into a shrug, which is the defect class this codebase
//! has spent six months naming.
//!
//! # This module owns no arithmetic
//!
//! Per `verification_for_agent_ecologies.md` §3.4, a trust calculation must have
//! exactly one implementation. Nothing here counts anything. It is a **routing
//! table**: for each panel, which rung of [`crate::ladder`] can answer, and the
//! answer is fetched from whichever contract owns it.
//!
//! | resolver | contract | answers |
//! |---|---|---|
//! | [`Resolver::Liveness`] | [`crate::liveness_trust`] | *silent* (opportunities, no rows) vs *inert* (no opportunities) |
//! | [`Resolver::LoopStage`] | [`crate::loop_model`] | which link a chain stops at, and why |
//! | [`Resolver::Gate`] | [`crate::gate_trust`] | never asked · refuses everything · admits everything |
//! | [`Resolver::Unresolved`] | — | **nothing can answer yet.** A work item, not a state |
//!
//! # The vocabulary is three words, not six
//!
//! An earlier draft proposed a six-state reading vocabulary for the UI. It was
//! dropped: `loop_model` already has eight stall reasons and `liveness_trust`
//! five statuses, and a third overlapping set would be a second answer to the
//! same question. So [`Absence`] carries the **source contract's own token**
//! verbatim, plus a three-way [`Reading`] that is the only thing a renderer has
//! to branch on.
//!
//! # Unresolved is the point
//!
//! Most of the interesting panels cannot be answered today, and
//! [`Resolver::Unresolved`] records that with a reason instead of letting the
//! panel fall back to a cheerful blank. The test at the foot of this file pins
//! the list and **it may only shrink** — the same ratchet as
//! `liveness_trust::KNOWN_SILENT` and
//! `loop_model::every_untriggered_stage_explains_itself`.

use crate::gate_trust::Gate;
use crate::liveness_trust::LivenessReport;
use crate::loop_model::LoopState;
use crate::native_evaluators::Observation;

/// The closed set of panel kinds.
///
/// Closed on purpose: every kind costs one implementation per renderer, so a
/// new variant is a decision rather than a convenience. See
/// `docs/DESIGN_UX_PANEL_ARCHITECTURE.md` §3.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// identity · contract · evidence · availability
    Card,
    /// sortable rows under a lens
    Register,
    /// n readings
    ReadingGrid,
    /// glyph + word + provenance
    VerdictList,
    /// topology with measured rates
    Flow,
    /// slotted assembly under constraint
    Fitting,
    /// things awaiting a decision
    Queue,
    /// time-ordered events
    Record,
}

/// What the panel is scoped to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Agent,
    Workspace,
    Composition,
    Account,
    Platform,
}

/// Which contract can explain this panel's emptiness.
#[derive(Debug, Clone, Copy)]
pub enum Resolver {
    /// A liveness contract, named by its `sink` label.
    ///
    /// The strongest available answer, because the opportunity count separates
    /// *unused* from *broken* — the disambiguation nobody else can make.
    Liveness(&'static str),
    /// One stage of a declared feedback loop.
    LoopStage {
        loop_id: &'static str,
        stage: &'static str,
    },
    /// A gate's counters, and its authored `if_never_refuses` sentence.
    Gate(Gate),
    /// The durable half of the gate ledger: what is pending, what was dropped,
    /// and which gates are memory-only.
    GateLedger,
    /// No contract answers this panel yet.
    ///
    /// `why` must say what would make it answerable. An unresolved panel is a
    /// work item; it must never render as though the system were idle.
    Unresolved { why: &'static str },
}

/// One panel that can be empty.
#[derive(Debug, Clone, Copy)]
pub struct Panel {
    /// Stable identifier. `surface.panel`.
    pub id: &'static str,
    pub kind: Kind,
    pub scope: Scope,
    /// Where it renders today, or `"unbuilt"`.
    pub surface: &'static str,
    /// What the reader came here to see.
    pub shows: &'static str,
    pub resolved_by: Resolver,
    /// What an empty panel means, when no contract has a stronger opinion.
    ///
    /// Required, and modelled on `gate_trust::GateSpec::if_never_refuses`: an
    /// empty panel is only informative if someone has written down what empty
    /// would mean.
    pub if_empty: &'static str,
}

/// What a renderer branches on.
///
/// Three, not the eight-plus tokens beneath them. The specific token travels in
/// [`Absence::token`] for a reader who wants it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reading {
    /// Correctly empty. Nothing that should have happened has happened.
    Idle,
    /// Something should have happened and did not. A finding.
    Fault,
    /// No contract can say. Neither of the above is available.
    ///
    /// Never rendered as a blank, a spinner or a zero.
    Unknown,
}

impl Reading {
    pub fn label(self) -> &'static str {
        match self {
            Reading::Idle => "idle",
            Reading::Fault => "fault",
            Reading::Unknown => "unknown",
        }
    }
}

/// A stamped answer to "why is this panel empty".
#[derive(Debug, Clone, serde::Serialize)]
pub struct Absence {
    pub panel: &'static str,
    /// The ladder rung that answered, when a rung owns the contract.
    pub rung: Option<u8>,
    /// The module that answered, or `"none"`.
    pub answered_by: &'static str,
    pub reading: Reading,
    /// The answering contract's own token, verbatim. Not normalised.
    pub token: &'static str,
    /// One sentence, for a human.
    pub detail: String,
    /// What to do next, when the answering contract carries one.
    pub remediation: Option<&'static str>,
}

/// Every panel that can be empty, and who explains it.
///
/// Rule of thumb for adding one: **if a reader could look at this surface, see
/// nothing, and draw a wrong conclusion, it belongs here.**
pub const PANELS: &[Panel] = &[
    // ── Observatory ──────────────────────────────────────────────────────
    Panel {
        id: "observatory.loops",
        kind: Kind::VerdictList,
        scope: Scope::Agent,
        surface: "templates/observatory.html — Loops tab",
        shows: "which feedback loops are turning for this agent",
        // The whole chain, so the panel's own emptiness is the chain's.
        resolved_by: Resolver::LoopStage {
            loop_id: "loop1",
            stage: "episodes",
        },
        if_empty: "This agent has never executed, so no loop has had an \
                   occasion to turn on its behalf. Not a fault.",
    },
    Panel {
        id: "observatory.anomalies",
        kind: Kind::Record,
        scope: Scope::Agent,
        surface: "templates/observatory.html — History tab",
        shows: "defects surfaced for human review",
        resolved_by: Resolver::Liveness("anomaly_events"),
        if_empty: "No anomaly has ever been recorded. The detector is \
                   Conditional, so an empty sink can be correct — but Loop 2 \
                   has never turned at any stage, so check the writer before \
                   concluding the agent is clean.",
    },
    Panel {
        id: "observatory.dreaming",
        kind: Kind::ReadingGrid,
        scope: Scope::Agent,
        surface: "templates/observatory.html — Dreaming tab",
        shows: "what consolidation has extracted",
        resolved_by: Resolver::Liveness("consolidation_jobs (Loop 1 cadence)"),
        if_empty: "No dream cycle has completed. Distinguish an agent with no \
                   episodes to consolidate from a sweeper that is switched off.",
    },
    Panel {
        id: "observatory.learned",
        kind: Kind::ReadingGrid,
        scope: Scope::Agent,
        surface: "templates/observatory.html — Vital signs, `Learned`",
        shows: "ontology rows this agent has accumulated",
        resolved_by: Resolver::Liveness("semantic_rules"),
        if_empty: "Consolidation has produced no rules. A cycle that ran and \
                   extracted nothing is a zero-yield cycle and is a fault, not \
                   an idle state.",
    },
    Panel {
        id: "observatory.timeline",
        kind: Kind::Record,
        scope: Scope::Agent,
        surface: "templates/observatory.html — History tab, timeline table",
        shows: "per-execution dimension scores over time",
        resolved_by: Resolver::Liveness("agent_timeline_entries"),
        if_empty: "No timeline entry has been stamped. Historic entries sit at \
                   persona_version = 1 and the drift monitor skips them by \
                   design, so an empty panel here may mean the corpus is \
                   ineligible rather than absent.",
    },
    Panel {
        id: "observatory.rule_retrieval",
        kind: Kind::ReadingGrid,
        scope: Scope::Agent,
        surface: "templates/observatory.html — Dreaming tab, ontology ledger",
        shows: "whether anything learned is ever read back",
        resolved_by: Resolver::Liveness("semantic_rules.application_count"),
        if_empty: "Rules exist and none has been retrieved into a prompt. The \
                   loop writes and nothing reads: learning that changes no \
                   behaviour.",
    },
    // ── Review queue ─────────────────────────────────────────────────────
    Panel {
        id: "hitl.queue",
        kind: Kind::Queue,
        scope: Scope::Account,
        surface: "templates/observatory_hitl.html",
        shows: "anomalies awaiting a human decision",
        resolved_by: Resolver::LoopStage {
            loop_id: "loop2",
            stage: "anomaly",
        },
        if_empty: "Nothing is waiting for you. Loop 2 has never turned at any \
                   stage, so an empty queue is currently evidence about the \
                   writer rather than about the agents.",
    },
    Panel {
        id: "hitl.consensus",
        kind: Kind::Queue,
        scope: Scope::Account,
        surface: "templates/observatory_hitl.html — consensus banner",
        shows: "agent-wide interventions awaiting a second reviewer",
        resolved_by: Resolver::Gate(Gate::Coherence),
        if_empty: "No agent-wide intervention is pending. The coherence gate \
                   sits upstream of this queue, so a gate refusing everything \
                   empties it while looking like consent.",
    },
    // ── Workspace / composition ──────────────────────────────────────────
    Panel {
        id: "workspace.coherence",
        kind: Kind::ReadingGrid,
        scope: Scope::Workspace,
        surface: "templates/workspace.html",
        shows: "Γ(C) and when it was last measured",
        resolved_by: Resolver::LoopStage {
            loop_id: "loop3",
            stage: "settled",
        },
        if_empty: "This workspace has never been evaluated for coherence.",
    },
    Panel {
        id: "workspace.proposals",
        kind: Kind::Queue,
        scope: Scope::Composition,
        surface: "templates/workspace.html",
        shows: "roster changes proposed from measured contribution",
        resolved_by: Resolver::LoopStage {
            loop_id: "loop4",
            stage: "proposed",
        },
        if_empty: "No roster change has been proposed. Nothing calls the \
                   writer, so this panel will stay empty however well the \
                   composition performs.",
    },
    // ── Gates ────────────────────────────────────────────────────────────
    Panel {
        id: "gates.register",
        kind: Kind::VerdictList,
        scope: Scope::Platform,
        surface: "unbuilt",
        shows: "every gate, what it guards, and its verdicts",
        resolved_by: Resolver::Unresolved {
            why: "The register renders `gate_trust::accounts()`, which is \
                  never empty — there is always a row per declared gate. It is \
                  listed here so the ratchet notices if that stops being true, \
                  not because it has an absence to resolve.",
        },
        if_empty: "The gate declaration table is empty, which means the \
                   constant was emptied — a code change, not a system state.",
    },
    Panel {
        id: "gates.decisions",
        kind: Kind::Queue,
        scope: Scope::Platform,
        surface: "unbuilt",
        shows: "individual refusals, durably recorded",
        resolved_by: Resolver::GateLedger,
        if_empty: "No refusal has been recorded. Two gates are Recorded and \
                   survive a restart; the other five are counted in memory \
                   only, so for those an empty panel is a statement about \
                   uptime rather than about the gates.",
    },
    // ── Ecology ──────────────────────────────────────────────────────────
    Panel {
        id: "ecology.cohabitation",
        kind: Kind::Register,
        scope: Scope::Platform,
        surface: "templates/ecology.html — Co-habitation",
        shows: "agent pairs recurring across distinct rosters",
        resolved_by: Resolver::Unresolved {
            why: "No contract watches roster composition. The panel's own \
                  empty state is currently the best available answer — \
                  \"teams are still assembled from the template rather than \
                  composed\" — and it was authored by hand, which is exactly \
                  what this module exists to stop. Resolve with a liveness \
                  contract over distinct-roster pairs.",
        },
        if_empty: "No pair recurs across more than one distinct team.",
    },
    Panel {
        id: "ecology.seams",
        kind: Kind::Register,
        scope: Scope::Platform,
        surface: "unbuilt — Population lens, label-set health",
        shows: "which declared ports can actually form a seam",
        resolved_by: Resolver::Unresolved {
            why: "Measured once by `scripts/port_census.py` (513 labels, 14 \
                  on both sides, 499 orphans) and never since. A census in a \
                  comment is not a contract. Resolve by promoting the census \
                  to a rung so the ratio moves on the panel when the \
                  vocabulary converges.",
        },
        if_empty: "No declared port label appears on both an accepts and a \
                   produces, so no two agents can be shown to compose.",
    },
    // ── Agent detail ─────────────────────────────────────────────────────
    Panel {
        id: "agent.claims",
        kind: Kind::Record,
        scope: Scope::Agent,
        surface: "templates/agent_detail.html — Overview, Manager Effect",
        shows: "this agent's quantified judgements, retained for attribution",
        resolved_by: Resolver::Liveness("forecast_agent_claims"),
        if_empty: "This agent has never had a quantified judgement retained. \
                   The sink is coded, wired, thoroughly commented and has held \
                   zero rows for its whole history.",
    },
    Panel {
        id: "agent.assertions",
        kind: Kind::Record,
        scope: Scope::Agent,
        surface: "templates/agent_detail.html — Activity, evidence cards",
        shows: "checkable claims extracted from this agent's output",
        resolved_by: Resolver::Liveness("episodes.assertions"),
        if_empty: "No assertion has been extracted from any episode.",
    },
    Panel {
        id: "agent.eval_runs",
        kind: Kind::Record,
        scope: Scope::Agent,
        surface: "templates/observatory.html — Sessions tab",
        shows: "eval runs and their per-dimension signals",
        resolved_by: Resolver::Unresolved {
            why: "`eval_runs` has no liveness contract, so an agent that has \
                  never been evaluated and an eval writer that has stopped \
                  are indistinguishable here. Resolve by adding a contract \
                  whose opportunity count is agents with test cases.",
        },
        if_empty: "This agent has never been evaluated. Test cases are seeded \
                   from sample_queries automatically, so an agent with queries \
                   and no runs has had the eval offered and not taken — which \
                   is a different thing from an agent nobody has written cases \
                   for.",
    },
    Panel {
        id: "agent.dyads",
        kind: Kind::Register,
        scope: Scope::Agent,
        surface: "templates/observatory.html — Relationships tab",
        shows: "human–agent pairs with rapport and trust",
        resolved_by: Resolver::Unresolved {
            why: "No contract watches dyad formation. A dyad forms after 3 \
                  interactions, which is a natural opportunity count and makes \
                  this a straightforward contract to add.",
        },
        if_empty: "No dyad has formed. A dyad needs the same human and agent \
                   to have interacted three times.",
    },
];

/// The same two counts as a liveness contract, for one subject.
///
/// `liveness_trust` answers *"does this writer ever run"* platform-wide. A
/// scoped probe asks the narrower question the UI actually poses — *"should
/// THIS agent have a row"* — with the same shape, so the verdict can go through
/// [`crate::liveness_trust::classify`] rather than growing a second decision
/// table. The SQL is per panel; the arithmetic is not.
#[derive(Debug, Clone, Copy)]
pub struct ScopedProbe {
    /// One row, one `bigint` column `writes`. `$1` is the subject id.
    pub writes_sql: &'static str,
    /// One row, one `bigint` column `opportunities`. `$1` is the subject id.
    ///
    /// The whole design, exactly as in `liveness_trust`: without it the probe
    /// cannot tell an agent that has produced nothing from an agent that has
    /// had nothing to produce from.
    pub opportunities_sql: &'static str,
    /// What one opportunity is, in a reader's words.
    pub opportunity_is: &'static str,
    /// The `liveness_trust` sink whose opportunity definition this narrows.
    ///
    /// **A scoped probe inherits the platform contract's definition and adds
    /// only a subject filter.** Inventing a looser one produces false faults:
    /// the first draft of `agent.claims` counted all 300 of an agent's episodes
    /// as chances to record a quantified judgement, where the platform contract
    /// counts only those whose response carries a `Suggested p50` bound to a
    /// workspace or forecast. The real count was zero, so a correct `inert`
    /// was reported as `silent` — a check that cries wolf, which §5.2 notes
    /// gets deleted with the deletion looking like cleanup.
    ///
    /// `None` means no platform contract covers this sink and the definition
    /// originates here.
    pub inherits_opportunity_from: Option<&'static str>,
}

/// Scoped probes, by panel id.
///
/// A side table rather than a field on [`Panel`] so the 18 declarations stay
/// readable; `every_agent_scoped_panel_is_probed_or_declared_unresolved` is
/// what stops an entry going missing.
pub const SCOPED_PROBES: &[(&str, ScopedProbe)] = &[
    (
        "observatory.learned",
        ScopedProbe {
            writes_sql: "SELECT count(*)::bigint AS writes FROM semantic_rules \
                         WHERE agent_id = $1",
            opportunities_sql: "SELECT count(*)::bigint AS opportunities FROM episodes \
                                WHERE agent_id = $1 AND consolidated = true",
            opportunity_is: "a consolidated episode a rule could have come from",
            inherits_opportunity_from: Some("semantic_rules"),
        },
    ),
    (
        "observatory.dreaming",
        ScopedProbe {
            writes_sql: "SELECT count(*)::bigint AS writes FROM consolidation_jobs \
                         WHERE agent_id = $1 AND status = 'completed'",
            opportunities_sql: "SELECT count(*)::bigint AS opportunities FROM ( \
                                  SELECT agent_id FROM episodes \
                                   WHERE agent_id = $1 AND NOT consolidated \
                                   GROUP BY agent_id HAVING count(*) >= 10 \
                                ) overdue",
            opportunity_is: "a backlog of 10+ unconsolidated episodes, the declared cadence",
            inherits_opportunity_from: Some("consolidation_jobs (Loop 1 cadence)"),
        },
    ),
    (
        "observatory.timeline",
        ScopedProbe {
            writes_sql: "SELECT count(*)::bigint AS writes FROM agent_timeline_entries \
                         WHERE agent_id = $1",
            opportunities_sql: "SELECT count(*)::bigint AS opportunities FROM episodes \
                                WHERE agent_id = $1",
            opportunity_is: "an execution that should have been stamped",
            inherits_opportunity_from: Some("agent_timeline_entries"),
        },
    ),
    (
        "observatory.rule_retrieval",
        ScopedProbe {
            writes_sql: "SELECT count(*)::bigint AS writes FROM semantic_rules \
                         WHERE agent_id = $1 AND application_count > 0",
            // A rule can only be retrieved if it exists. Scoping the
            // opportunity to this agent's own rules is what separates "learned
            // nothing" from "learned and never reads it back" — two different
            // faults that the platform-wide count merges.
            opportunities_sql: "SELECT count(*)::bigint AS opportunities FROM episodes e \
                                 WHERE e.agent_id = $1 \
                                   AND EXISTS (SELECT 1 FROM semantic_rules r \
                                                WHERE r.agent_id = e.agent_id \
                                                  AND r.is_active \
                                                  AND r.embedding IS NOT NULL)",
            opportunity_is: "an execution that had a retrievable rule available to it",
            inherits_opportunity_from: Some("semantic_rules.application_count"),
        },
    ),
    (
        "agent.assertions",
        ScopedProbe {
            writes_sql: "SELECT count(*)::bigint AS writes FROM episodes \
                         WHERE agent_id = $1 AND assertions IS NOT NULL \
                           AND jsonb_array_length(assertions) > 0",
            opportunities_sql: "SELECT count(*)::bigint AS opportunities FROM episodes \
                                WHERE agent_id = $1 AND response_text ~ 'Suggested p50'",
            opportunity_is: "an episode whose response states a quantified estimate",
            inherits_opportunity_from: Some("episodes.assertions"),
        },
    ),
    (
        "agent.eval_runs",
        ScopedProbe {
            writes_sql: "SELECT count(*)::bigint AS writes FROM eval_runs WHERE agent_id = $1",
            opportunities_sql: "SELECT count(*)::bigint AS opportunities FROM eval_test_cases \
                                WHERE agent_id = $1",
            opportunity_is: "a test case waiting to be run",
            inherits_opportunity_from: None,
        },
    ),
    (
        "agent.claims",
        ScopedProbe {
            writes_sql: "SELECT count(*)::bigint AS writes FROM forecast_agent_claims \
                         WHERE agent_id = $1",
            opportunities_sql: "SELECT count(*)::bigint AS opportunities FROM episodes \
                                WHERE agent_id = $1 \
                                  AND response_text ~ 'Suggested p50' \
                                  AND (context ->> 'workspace_id' IS NOT NULL \
                                   OR context #>> '{invocation,forecast_id}' IS NOT NULL)",
            opportunity_is: "a quantified estimate bound to a workspace or forecast",
            inherits_opportunity_from: Some("forecast_agent_claims"),
        },
    ),
    (
        "workspace.coherence",
        ScopedProbe {
            writes_sql: "SELECT count(*)::bigint AS writes FROM coherence_evaluations \
                         WHERE workspace_id = $1",
            // Coherence is a property of a composition, so a workspace with
            // fewer than two members has nothing to be incoherent about and
            // must not read as a fault.
            opportunities_sql: "SELECT (CASE WHEN count(*) >= 2 THEN 1 ELSE 0 END)::bigint \
                                  AS opportunities \
                                FROM workspace_agents WHERE workspace_id = $1",
            opportunity_is: "a composition of two or more members to evaluate",
            inherits_opportunity_from: None,
        },
    ),
    (
        "workspace.proposals",
        ScopedProbe {
            writes_sql: "SELECT count(*)::bigint AS writes FROM composition_versions \
                         WHERE workspace_id = $1",
            // Loop 4 runs claims → attributed → proposed. `forecast_attributions`
            // is keyed by forecast alone — no agent, no workspace — so the
            // middle stage cannot be scoped to a composition at all. The
            // opportunity is therefore the stage above it, which can:
            // judgements retained for this workspace.
            opportunities_sql: "SELECT count(*)::bigint AS opportunities \
                                FROM forecast_agent_claims WHERE workspace_id = $1",
            opportunity_is: "a quantified judgement retained for this workspace",
            inherits_opportunity_from: None,
        },
    ),
    (
        "agent.dyads",
        ScopedProbe {
            writes_sql: "SELECT count(*)::bigint AS writes FROM dyad_state WHERE agent_id = $1",
            // A dyad forms after three interactions with the same human, so the
            // opportunity count is humans who have crossed that line — not
            // episodes, which would report a fault for an agent with many
            // one-off callers and no repeat relationship.
            opportunities_sql: "SELECT count(*)::bigint AS opportunities FROM ( \
                                  SELECT user_id FROM episodes \
                                  WHERE agent_id = $1 AND user_id IS NOT NULL \
                                  GROUP BY user_id HAVING count(*) >= 3 \
                                ) q",
            opportunity_is: "a human who has interacted with this agent 3+ times",
            inherits_opportunity_from: None,
        },
    ),
];

/// The scoped probe for a panel, if one is declared.
pub fn scoped_probe(panel_id: &str) -> Option<&'static ScopedProbe> {
    SCOPED_PROBES
        .iter()
        .find(|(id, _)| *id == panel_id)
        .map(|(_, p)| p)
}

/// Can a platform-scoped contract answer a panel at this scope?
///
/// Liveness and the loop model both count rows **platform-wide**: they answer
/// *"does this writer ever run"*, not *"should this agent have a row"*. For a
/// platform-scoped panel those are the same question. For an agent-scoped one
/// they are not, and treating a healthy platform verdict as an answer produces
/// the sentence that exposed this: *"the write path has run (253 rows); …
/// consolidation has produced no rules"* — both halves true, the conjunction
/// nonsense.
///
/// The benign direction again: a coarse contract reporting healthy would mark
/// every scoped panel `idle`, which is the most reassuring available answer and
/// the least supported. Scoped resolvers are the fix; until they exist the
/// honest reading is `Unknown`.
fn answers_scope(scope: Scope) -> bool {
    matches!(scope, Scope::Platform | Scope::Account)
}

/// The rung of [`crate::ladder`] a resolver draws on, when one owns it.
pub fn rung_of(r: &Resolver) -> Option<u8> {
    match r {
        Resolver::Liveness(_) => crate::ladder::rung("liveness").map(|x| x.position),
        // Loops and gates are chains and controls over the rungs rather than
        // rungs themselves. Claiming a position for them would put two
        // different orderings in one column.
        Resolver::LoopStage { .. }
        | Resolver::Gate(_)
        | Resolver::GateLedger
        | Resolver::Unresolved { .. } => None,
    }
}

/// Look one up.
pub fn panel(id: &str) -> Option<&'static Panel> {
    PANELS.iter().find(|p| p.id == id)
}

/// Answer "why is this panel empty", from whichever contract knows.
///
/// Pure over a snapshot, so the decision table is unit-testable without a
/// database — the same split as `liveness_trust::classify`.
pub fn resolve(p: &Panel, o: &Observation) -> Absence {
    match p.resolved_by {
        Resolver::Liveness(sink) => resolve_liveness(p, sink, o.liveness.as_ref()),
        Resolver::LoopStage { loop_id, stage } => resolve_loop(p, loop_id, stage, &o.loops),
        Resolver::Gate(g) => resolve_gate(p, g, o),
        Resolver::GateLedger => resolve_gate_ledger(p, o),
        Resolver::Unresolved { why } => Absence {
            panel: p.id,
            rung: None,
            answered_by: "none",
            reading: Reading::Unknown,
            token: "unresolved",
            detail: format!("{} {}", p.if_empty, why),
            remediation: None,
        },
    }
}

fn resolve_liveness(p: &Panel, sink: &str, report: Option<&LivenessReport>) -> Absence {
    let rung = crate::ladder::rung("liveness").map(|r| r.position);
    let Some(report) = report else {
        return Absence {
            panel: p.id,
            rung,
            answered_by: "liveness_trust",
            reading: Reading::Unknown,
            token: "no_sweep",
            detail: "No liveness sweep has completed since boot, so unused and broken cannot be \
                 told apart for this panel."
                .into(),
            remediation: None,
        };
    };
    let Some(o) = report.outcomes.iter().find(|c| c.sink == sink) else {
        return Absence {
            panel: p.id,
            rung,
            answered_by: "liveness_trust",
            reading: Reading::Unknown,
            token: "no_contract",
            detail: format!(
                "This panel names the liveness sink `{sink}`, and the last sweep reported no \
                 such contract."
            ),
            remediation: None,
        };
    };

    // `status` is the contract's own label. The tri-state is the only thing
    // added, and `Inert` is deliberately not a pass: a check watching a feature
    // nobody has exercised, reporting healthy, is the original defect wearing
    // the machinery built to prevent it.
    let (reading, token) = match o.status {
        // A platform-wide "the writer runs" does not answer an agent-scoped
        // question. See `answers_scope`.
        "OK" if !answers_scope(p.scope) => (Reading::Unknown, "out_of_scope"),
        "OK" => (Reading::Idle, "ok"),
        "SILENT" => (Reading::Fault, "silent"),
        "INERT" => (Reading::Idle, "inert"),
        "NOT DEPLOYED" => (Reading::Fault, "not_deployed"),
        _ => (Reading::Unknown, "unrunnable"),
    };

    let detail = match token {
        "silent" => format!(
            "{} had {} opportunity(s) to fire and has never written a row. {}",
            o.writer, o.opportunities, o.why
        ),
        "inert" => format!(
            "{} has had no occasion to fire yet ({} opportunities). {}",
            o.writer, o.opportunities, p.if_empty
        ),
        "not_deployed" => format!(
            "The column this panel reads does not exist yet. {}",
            o.remediation
        ),
        "unrunnable" => {
            "The liveness probe for this panel could not run, so no verdict is available."
                .to_string()
        }
        "out_of_scope" => format!(
            "{} has written {} row(s) platform-wide, so the write path works. That does not \
             say whether THIS subject should have data, and no contract answers the narrower \
             question yet.",
            o.writer, o.writes
        ),
        _ => format!(
            "The write path has run ({} rows) and this panel is platform-scoped, so its \
             emptiness is a fact about the filter rather than about the writer.",
            o.writes
        ),
    };

    Absence {
        panel: p.id,
        rung,
        answered_by: "liveness_trust",
        reading,
        token,
        detail,
        remediation: (reading == Reading::Fault).then_some(o.remediation),
    }
}

fn resolve_loop(p: &Panel, loop_id: &str, stage: &str, loops: &[LoopState]) -> Absence {
    let Some(l) = loops.iter().find(|l| l.id == loop_id) else {
        return Absence {
            panel: p.id,
            rung: None,
            answered_by: "loop_model",
            reading: Reading::Unknown,
            token: "no_walk",
            detail: format!("The loop model has not been walked, so `{loop_id}` has no state."),
            remediation: None,
        };
    };

    // An unread stage anywhere in the chain means the chain supports no verdict.
    if !l.measured() {
        return Absence {
            panel: p.id,
            rung: None,
            answered_by: "loop_model",
            reading: Reading::Unknown,
            token: "probe_failed",
            detail: format!(
                "A stage of {} could not be counted, so this panel's emptiness cannot be \
                 attributed to the loop or excused by it.",
                l.name
            ),
            remediation: None,
        };
    }

    let reason = l.reason.unwrap_or("no_input");
    let reading = reading_for_reason(reason);

    let detail = match l.stops_at {
        // The panel's own stage is where the chain stops: this panel is the
        // finding rather than a casualty of one.
        Some(s) if s == stage => format!("{} stops here: {}. {}", l.name, reason, l.claim),
        Some(s) => format!(
            "{} stops earlier, at `{s}` ({reason}), so this stage has had no input. Repairing \
             this panel's writer would produce nothing.",
            l.name
        ),
        None if !answers_scope(p.scope) => format!(
            "{} is turning platform-wide, which does not say whether THIS subject has \
             reached it. The chain counts rows across every agent; no contract answers the \
             narrower question yet.",
            l.name
        ),
        None => format!(
            "{} is turning, so this panel is empty for a narrower reason than the loop.",
            l.name
        ),
    };

    let (reading, token) = match l.stops_at {
        None if !answers_scope(p.scope) => (Reading::Unknown, "out_of_scope"),
        None => (Reading::Idle, "turning"),
        Some(_) => (reading, reason),
    };

    Absence {
        panel: p.id,
        rung: None,
        answered_by: "loop_model",
        reading,
        token,
        detail,
        remediation: None,
    }
}

/// Which side of the line one of `loop_model`'s stall reasons falls.
///
/// The split the whole module exists for: a fact about the code, versus a fact
/// about the world, versus no fact at all.
///
/// There is no catch-all. An unrecognised reason returns [`Reading::Unknown`]
/// rather than defaulting to `Idle`, because the benign default is how a new
/// upstream token comes to report a healthy system — which is precisely what
/// happened when `loop_model` grew `unobserved` and this function still had a
/// `_ => Idle` arm. `every_stall_reason_is_classified` stops it recurring.
pub fn reading_for_reason(reason: &str) -> Reading {
    classify_reason(reason).unwrap_or(Reading::Unknown)
}

/// `None` means the reason hit no arm.
///
/// Split from [`reading_for_reason`] so a test can tell an unclassified token
/// from one deliberately classified as [`Reading::Unknown`] — by return value
/// alone the two are identical, which is exactly how the fallthrough hid.
fn classify_reason(reason: &str) -> Option<Reading> {
    Some(match reason {
        // A fault in the code. Volume makes these worse, not better.
        "no_trigger" | "scheduler_off" | "writes_refused" | "gate_refuses_everything" => {
            Reading::Fault
        }
        // Nothing was watched, or the watching failed. No claim is available.
        // `unobserved` belongs here and not in `Idle`: the counters are
        // process-local, so on a fresh process "nothing happened" and "this has
        // been failing since before the restart" are the same observation.
        //
        // `awaiting_agent` is here for the same reason from the other end: the
        // prompt asks a model to call the writer, and "the prompt has not run"
        // and "the model declined" are one row count. `Idle` would render that
        // as a healthy empty panel.
        "probe_failed" | "upstream_unmeasured" | "unobserved" | "awaiting_agent" => {
            Reading::Unknown
        }
        // A fact about the world. The loop is healthy and has had no occasion.
        "awaiting_upstream" | "no_input" => Reading::Idle,
        _ => return None,
    })
}

fn resolve_gate(p: &Panel, g: Gate, o: &Observation) -> Absence {
    let id = g.id();
    let Some(a) = o.gates.iter().find(|a| a.id == id) else {
        return Absence {
            panel: p.id,
            rung: None,
            answered_by: "gate_trust",
            reading: Reading::Unknown,
            token: "no_account",
            detail: format!("No counters were gathered for gate `{id}`."),
            remediation: None,
        };
    };

    // `refuses_everything` is the Γ signature: a gate that has been asked and
    // approved nothing empties every queue downstream of it while presenting as
    // a strict control working well.
    let (reading, token, detail) = if a.refuses_everything() {
        (
            Reading::Fault,
            "refuses_everything",
            format!(
                "The `{id}` gate was asked {} time(s) and approved none. Everything downstream \
                 of it is empty for that reason.",
                a.asked()
            ),
        )
    } else if a.never_asked() {
        (
            Reading::Idle,
            "never_asked",
            format!(
                "The `{id}` gate has not been asked since boot, so nothing has reached this \
                 panel. {}",
                p.if_empty
            ),
        )
    } else {
        (
            Reading::Idle,
            "asked",
            format!(
                "The `{id}` gate approved {} of {}, so this panel is empty for a narrower \
                 reason than the gate. {}",
                a.approved,
                a.asked(),
                p.if_empty
            ),
        )
    };

    Absence {
        panel: p.id,
        rung: None,
        answered_by: "gate_trust",
        reading,
        token,
        detail,
        remediation: None,
    }
}

fn resolve_gate_ledger(p: &Panel, o: &Observation) -> Absence {
    let Some(l) = &o.gate_ledger else {
        return Absence {
            panel: p.id,
            rung: None,
            answered_by: "gate_trust",
            reading: Reading::Unknown,
            token: "no_ledger_status",
            detail: "The gate ledger's status was not gathered, so what is durable cannot be \
                     distinguished from what is merely counted."
                .into(),
            remediation: None,
        };
    };

    // A dropped decision is a hole in the audit trail with no second copy
    // anywhere, so it outranks every other reading this panel can produce.
    if l.dropped > 0 {
        return Absence {
            panel: p.id,
            rung: None,
            answered_by: "gate_trust",
            reading: Reading::Fault,
            token: "dropped",
            detail: format!(
                "{} decision(s) were dropped because the queue filled while the recorder \
                 could not write. Those refusals are not recoverable, and this panel is \
                 incomplete by that many rows.",
                l.dropped
            ),
            remediation: Some(
                "Read `last_error` on the `gate_decisions` write sink. The recorder is \
                 non-fatal by design, so it will keep dropping until the write succeeds.",
            ),
        };
    }

    if l.pending > 0 {
        return Absence {
            panel: p.id,
            rung: None,
            answered_by: "gate_trust",
            reading: Reading::Idle,
            token: "pending_flush",
            detail: format!(
                "{} decision(s) are queued and not yet written. The recorder batches, so a \
                 panel read between flushes trails the counters rather than disagreeing \
                 with them.",
                l.pending
            ),
            remediation: None,
        };
    }

    // Nothing queued, nothing dropped. Whether that means "no gate refused"
    // or "only memory-only gates were asked" depends on which tier was
    // exercised, and the honest answer names the tiers.
    let asked: u64 = o
        .gates
        .iter()
        .filter(|a| l.recorded_gates.contains(&a.id))
        .map(|a| a.asked())
        .sum();

    Absence {
        panel: p.id,
        rung: None,
        answered_by: "gate_trust",
        reading: Reading::Idle,
        token: if asked == 0 { "never_asked" } else { "flushed" },
        detail: if asked == 0 {
            format!(
                "No Recorded gate ({}) has been asked since boot, so there is nothing to \
                 record. The other {} gate(s) are counted in memory only and never reach \
                 this panel at all.",
                l.recorded_gates.join(", "),
                l.counted_only_gates.len()
            )
        } else {
            format!(
                "{asked} decision(s) by Recorded gates have been flushed. An empty panel \
                 now means the ledger holds no refusal matching the filter, not that \
                 nothing was refused: {} gate(s) are counted in memory only.",
                l.counted_only_gates.len()
            )
        },
        remediation: None,
    }
}

/// Resolve every declared panel against one snapshot.
pub fn resolve_all(o: &Observation) -> Vec<Absence> {
    PANELS.iter().map(|p| resolve(p, o)).collect()
}

/// Resolve a panel for one subject, using its scoped probe when it has one.
///
/// Falls back to [`resolve`] when no probe is declared, so a panel is never
/// worse off for having a subject. The pure snapshot path above stays the
/// testable core; this adds the two counts only a database can supply.
pub async fn resolve_for_subject(
    pool: &sqlx::PgPool,
    p: &'static Panel,
    subject: uuid::Uuid,
    o: &Observation,
) -> Absence {
    let Some(probe) = scoped_probe(p.id) else {
        return resolve(p, o);
    };

    let writes = sqlx::query_scalar::<_, i64>(probe.writes_sql)
        .bind(subject)
        .fetch_one(pool)
        .await;
    let opportunities = sqlx::query_scalar::<_, i64>(probe.opportunities_sql)
        .bind(subject)
        .fetch_one(pool)
        .await;

    let (Ok(writes), Ok(opportunities)) = (writes, opportunities) else {
        return Absence {
            panel: p.id,
            rung: crate::ladder::rung("liveness").map(|r| r.position),
            answered_by: "panel_absence::scoped",
            reading: Reading::Unknown,
            token: "probe_failed",
            detail: format!(
                "The scoped probe for {} could not run against subject {subject}, so this \
                 panel's emptiness cannot be attributed or excused.",
                p.id
            ),
            remediation: None,
        };
    };

    // One implementation of the arithmetic: the same decision table the
    // platform tier uses, applied to two narrower counts.
    let status = crate::liveness_trust::classify(writes, opportunities);
    let (reading, token) = match status {
        crate::liveness_trust::Status::Ok => (Reading::Idle, "ok"),
        crate::liveness_trust::Status::Silent => (Reading::Fault, "silent"),
        crate::liveness_trust::Status::Inert => (Reading::Idle, "inert"),
        _ => (Reading::Unknown, "unrunnable"),
    };

    let detail = match token {
        "silent" => format!(
            "This subject had {opportunities} opportunit{} ({}) and produced nothing. {}",
            if opportunities == 1 { "y" } else { "ies" },
            probe.opportunity_is,
            p.if_empty
        ),
        "inert" => format!(
            "This subject has had no opportunity yet ({}), so there is nothing to show. {}",
            probe.opportunity_is, p.if_empty
        ),
        "ok" => format!(
            "This subject has {writes} row(s) against {opportunities} opportunit{} ({}), so an \
             empty panel here is a fact about the filter rather than about the subject.",
            if opportunities == 1 { "y" } else { "ies" },
            probe.opportunity_is
        ),
        _ => "The scoped probe returned an unusable verdict.".to_string(),
    };

    Absence {
        panel: p.id,
        rung: crate::ladder::rung("liveness").map(|r| r.position),
        answered_by: "panel_absence::scoped",
        reading,
        token,
        detail,
        remediation: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loop_model;

    fn empty_observation() -> Observation {
        Observation::default()
    }

    #[test]
    fn every_panel_declares_a_distinct_id_and_says_what_empty_means() {
        let mut ids = std::collections::HashSet::new();
        for p in PANELS {
            assert!(ids.insert(p.id), "duplicate panel id `{}`", p.id);
            assert!(
                p.id.contains('.'),
                "{}: ids are `surface.panel`, so a reader can find it",
                p.id
            );
            assert!(
                p.if_empty.len() > 40,
                "{}: say what an empty panel MEANS. A blank a frontend author \
                 guessed at is the thing this module exists to replace.",
                p.id
            );
            assert!(p.shows.len() > 10, "{}: say what the reader came for", p.id);
        }
    }

    /// A panel naming a liveness sink that no contract watches would resolve to
    /// `no_contract` for ever, silently.
    #[test]
    fn every_named_liveness_sink_is_watched_by_a_contract() {
        for p in PANELS {
            if let Resolver::Liveness(sink) = p.resolved_by {
                assert!(
                    crate::liveness_trust::LIVENESS_CONTRACTS
                        .iter()
                        .any(|c| c.sink == sink),
                    "{} names liveness sink `{sink}`, which no contract declares",
                    p.id
                );
            }
        }
    }

    /// Same for loop stages: a typo here is invisible at runtime.
    #[test]
    fn every_named_loop_stage_exists_in_the_loop_model() {
        for p in PANELS {
            if let Resolver::LoopStage { loop_id, stage } = p.resolved_by {
                let l = loop_model::LOOPS
                    .iter()
                    .find(|l| l.id == loop_id)
                    .unwrap_or_else(|| panic!("{} names unknown loop `{loop_id}`", p.id));
                assert!(
                    l.stages.iter().any(|s| s.id == stage),
                    "{} names `{loop_id}.{stage}`, and that loop has no such stage",
                    p.id
                );
            }
        }
    }

    /// The ratchet.
    ///
    /// Every panel nothing can explain is listed here with a reason. The list
    /// **may only shrink**: resolving one means deleting its entry, and adding
    /// one is a regression that has to be argued for in the `why`.
    #[test]
    fn the_unresolved_list_may_only_shrink() {
        let mut unresolved = Vec::new();
        for p in PANELS {
            if let Resolver::Unresolved { why } = p.resolved_by {
                assert!(
                    why.len() > 80,
                    "{}: say what would make this answerable, concretely",
                    p.id
                );
                unresolved.push(p.id);
            }
        }
        assert_eq!(
            unresolved,
            vec![
                "gates.register",
                "ecology.cohabitation",
                "ecology.seams",
                "agent.eval_runs",
                "agent.dyads",
            ],
            "the set of panels no contract can explain has changed. It may only \
             shrink — if you have wired one, remove its `Resolver::Unresolved`; \
             if you have added one, that is a regression and needs its reason \
             written into the `why`."
        );
    }

    /// The §1.2 rule, as an assertion.
    #[test]
    fn a_panel_nothing_can_explain_is_never_reported_as_idle() {
        let o = empty_observation();
        for p in PANELS {
            if matches!(p.resolved_by, Resolver::Unresolved { .. }) {
                let a = resolve(p, &o);
                assert_eq!(
                    a.reading,
                    Reading::Unknown,
                    "{}: an unresolved panel must not claim the system is idle",
                    p.id
                );
                assert!(
                    a.detail.len() > p.if_empty.len(),
                    "{}: the reason must reach the reader, not just the source",
                    p.id
                );
            }
        }
    }

    /// With nothing gathered, nothing may be reported as a fault either.
    ///
    /// The symmetric error: an observer that has collected no data must not
    /// manufacture findings from the absence of its own inputs.
    #[test]
    fn an_empty_snapshot_produces_no_faults() {
        let o = empty_observation();
        for a in resolve_all(&o) {
            assert_ne!(
                a.reading,
                Reading::Fault,
                "{} claimed a fault from an empty snapshot: {}",
                a.panel,
                a.detail
            );
        }
    }

    /// The disambiguation the whole module is for: same empty panel, opposite
    /// meanings, decided by the opportunity count.
    #[test]
    fn unused_and_broken_are_told_apart() {
        let p = panel("observatory.anomalies").expect("declared");

        let mut broken = empty_observation();
        broken.liveness = Some(report_with("anomaly_events", "SILENT", 0, 14));
        let a = resolve(p, &broken);
        assert_eq!(a.reading, Reading::Fault);
        assert_eq!(a.token, "silent");
        assert!(a.remediation.is_some(), "a fault must carry a next action");
        assert!(
            a.detail.contains("14"),
            "the opportunity count is the evidence"
        );

        let mut unused = empty_observation();
        unused.liveness = Some(report_with("anomaly_events", "INERT", 0, 0));
        let a = resolve(p, &unused);
        assert_eq!(a.reading, Reading::Idle);
        assert_eq!(a.token, "inert");
        assert!(a.remediation.is_none(), "an idle panel is not a work item");
    }

    /// A chain that stops above this panel must not blame this panel.
    #[test]
    fn a_downstream_panel_names_the_link_that_actually_stopped() {
        let p = panel("workspace.proposals").expect("declared");
        let mut o = empty_observation();
        o.loops = vec![LoopState {
            id: "loop4",
            name: "Composition evolution",
            scope: "composition",
            claim: "c",
            stages: vec![loop_model::StageState {
                id: "claims",
                what: "w",
                writer: "a::b",
                trigger: loop_model::Trigger::Request,
                rows: 0,
            }],
            stops_at: Some("claims"),
            reason: Some("no_input"),
            status: "stalled",
        }];

        let a = resolve(p, &o);
        assert!(
            a.detail.contains("claims"),
            "the reader must be sent to the link that stopped: {}",
            a.detail
        );
        assert!(
            a.detail.contains("would produce nothing"),
            "and told that repairing this one is wasted: {}",
            a.detail
        );
    }

    /// Every scoped panel has a probe, or is declared unanswerable.
    ///
    /// Without this, adding an agent-scoped panel silently produces
    /// `out_of_scope` for ever — an honest reading, and a permanent one that
    /// nobody is prompted to fix.
    #[test]
    fn every_agent_scoped_panel_is_probed_or_declared_unresolved() {
        let mut unprobed = Vec::new();
        for p in PANELS {
            if answers_scope(p.scope) {
                continue;
            }
            if matches!(p.resolved_by, Resolver::Unresolved { .. }) {
                continue;
            }
            if scoped_probe(p.id).is_none() {
                unprobed.push(p.id);
            }
        }
        assert_eq!(
            unprobed,
            vec!["observatory.loops", "observatory.anomalies"],
            "the set of scoped panels with no scoped probe has changed. It may \
             only shrink. `observatory.loops` needs the loop model walked per \
             agent, which it cannot do yet — every stage query counts rows \
             platform-wide. `observatory.anomalies` is platform-SILENT against \
             1,418 opportunities, which is a fault at every scope and needs no \
             narrowing to be actionable."
        );
    }

    /// Every probe must be parameterised and read-only.
    #[test]
    fn scoped_probes_are_read_only_and_take_a_subject() {
        let mut ids = std::collections::HashSet::new();
        for (id, probe) in SCOPED_PROBES {
            assert!(ids.insert(id), "duplicate scoped probe for `{id}`");
            assert!(panel(id).is_some(), "`{id}` is not a declared panel");
            assert!(
                probe.opportunity_is.len() > 10,
                "{id}: say what one opportunity is"
            );
            // A named platform contract must exist, so a renamed sink surfaces
            // here rather than as a scoped probe quietly keeping its own,
            // looser definition of an opportunity.
            if let Some(sink) = probe.inherits_opportunity_from {
                assert!(
                    crate::liveness_trust::LIVENESS_CONTRACTS
                        .iter()
                        .any(|c| c.sink == sink),
                    "{id} inherits its opportunity definition from `{sink}`, which no \
                     liveness contract declares"
                );
            }
            for (label, sql) in [
                ("writes", probe.writes_sql),
                ("opportunities", probe.opportunities_sql),
            ] {
                let q = sql.to_ascii_lowercase();
                assert!(q.trim_start().starts_with("select"), "{id}.{label}");
                assert!(
                    sql.contains("$1"),
                    "{id}.{label}: a scoped probe that ignores its subject is a \
                     platform probe wearing a scope"
                );
                assert!(
                    q.contains(&format!("as {label}")),
                    "{id}.{label}: the runner reads the `{label}` alias"
                );
                for w in ["insert", "update ", "delete", "drop", "alter", "truncate"] {
                    assert!(!q.contains(w), "{id}.{label} contains `{w}`");
                }
            }
        }
    }

    /// A platform-wide healthy verdict must not answer an agent-scoped panel.
    ///
    /// The live report exposed this: `observatory.learned` read "the write path
    /// has run (253 rows); … consolidation has produced no rules" — both halves
    /// true, the conjunction nonsense, and the reading `idle`.
    #[test]
    fn a_platform_contract_does_not_answer_an_agent_scoped_panel() {
        let p = panel("observatory.learned").expect("declared");
        assert_eq!(p.scope, Scope::Agent);

        let mut o = empty_observation();
        o.liveness = Some(report_with("semantic_rules", "OK", 253, 400));

        let a = resolve(p, &o);
        assert_eq!(
            a.reading,
            Reading::Unknown,
            "a healthy platform-wide writer is not evidence about one agent"
        );
        assert_eq!(a.token, "out_of_scope");
        assert!(
            !a.detail.contains("produced no rules"),
            "the panel's if_empty contradicts a writer that demonstrably ran: {}",
            a.detail
        );

        // A SILENT writer is still a fault at any scope: nothing anywhere.
        o.liveness = Some(report_with("semantic_rules", "SILENT", 0, 400));
        assert_eq!(resolve(p, &o).reading, Reading::Fault);
    }

    /// Every reason `loop_model` can produce must have a deliberate reading.
    ///
    /// The ratchet that would have caught the `unobserved` bug: it was added
    /// upstream, fell through a `_ => Idle` arm here, and turned "nothing has
    /// been watched" into "the system is idle" on every panel backed by a loop.
    #[test]
    fn every_stall_reason_is_classified() {
        for r in loop_model::STALL_REASONS {
            assert!(
                classify_reason(r).is_some(),
                "`{r}` has no deliberate classification, so it is falling through \
                 to the default. Decide whether it is a fault, an idle state, or \
                 genuinely unknowable, and give it an arm in `classify_reason`."
            );
        }
    }

    /// ...except the three that are genuinely unknowable, pinned explicitly.
    #[test]
    fn the_unknowable_reasons_are_the_ones_we_meant() {
        assert_eq!(reading_for_reason("unobserved"), Reading::Unknown);
        assert_eq!(reading_for_reason("probe_failed"), Reading::Unknown);
        assert_eq!(reading_for_reason("upstream_unmeasured"), Reading::Unknown);
        assert_eq!(reading_for_reason("no_input"), Reading::Idle);
        assert_eq!(reading_for_reason("no_trigger"), Reading::Fault);
        assert_eq!(
            reading_for_reason("a_token_nobody_declared"),
            Reading::Unknown,
            "an unrecognised reason must not be optimistic"
        );
    }

    /// A dropped decision is a hole in the ledger and must outrank everything.
    #[test]
    fn a_dropped_decision_is_a_fault_even_though_the_gates_look_fine() {
        let p = panel("gates.decisions").expect("declared");
        let mut o = empty_observation();
        o.gate_ledger = Some(crate::gate_trust::LedgerStatus {
            pending: 0,
            dropped: 3,
            recorded_gates: vec!["coherence", "admission"],
            counted_only_gates: vec!["grounding"],
        });

        let a = resolve(p, &o);
        assert_eq!(a.reading, Reading::Fault);
        assert_eq!(a.token, "dropped");
        assert!(a.remediation.is_some());

        // Nothing dropped and nothing asked is idle, and must name the tiers.
        o.gate_ledger = Some(crate::gate_trust::LedgerStatus {
            pending: 0,
            dropped: 0,
            recorded_gates: vec!["coherence", "admission"],
            counted_only_gates: vec!["grounding"],
        });
        let a = resolve(p, &o);
        assert_eq!(a.reading, Reading::Idle);
        assert_eq!(a.token, "never_asked");
        assert!(
            a.detail.contains("memory only"),
            "the reader must learn that five gates never reach this panel: {}",
            a.detail
        );
    }

    /// A gate refusing everything empties its queue while looking like consent.
    #[test]
    fn a_gate_refusing_everything_is_a_fault_not_an_empty_queue() {
        let p = panel("hitl.consensus").expect("declared");
        let mut o = empty_observation();
        o.gates = vec![gate_account("coherence", 0, 47)];

        let a = resolve(p, &o);
        assert_eq!(a.reading, Reading::Fault);
        assert_eq!(a.token, "refuses_everything");
        assert!(a.detail.contains("47"));

        // ...and the benign case stays benign.
        o.gates = vec![gate_account("coherence", 0, 0)];
        assert_eq!(resolve(p, &o).reading, Reading::Idle);
    }

    // ── helpers ──────────────────────────────────────────────────────────

    fn report_with(
        sink: &'static str,
        status: &'static str,
        writes: i64,
        opportunities: i64,
    ) -> LivenessReport {
        LivenessReport {
            ran_at: "1970-01-01T00:00:00Z".into(),
            ok: 0,
            silent: 0,
            inert: 0,
            unrunnable: 0,
            undocumented_silent: vec![],
            rejected: vec![],
            outcomes: vec![crate::liveness_trust::ContractOutcome {
                sink,
                writer: "a::b",
                status,
                writes,
                opportunities,
                known_silent_reason: None,
                accounting: None,
                diagnosis: None,
                why: "something stops being knowable",
                remediation: "do the thing",
            }],
        }
    }

    fn gate_account(
        id: &'static str,
        approved: u64,
        refused: u64,
    ) -> crate::gate_trust::GateAccount {
        crate::gate_trust::GateAccount {
            id,
            clock: crate::gate_trust::Clock::Invocation,
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
}
