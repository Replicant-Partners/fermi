//! Every verb the platform offers, and what governs it.
//!
//! # Why a registry and not a router
//!
//! The router already knows every route. What it cannot say is **which of them
//! change something, and which gate stands in front of that change** — and that
//! is the question the gate audit kept having to answer by reading code.
//!
//! `docs/AUDIT_loops_and_gates.md` §3 is a table of gates that run and are
//! thrown away. It was assembled by hand, it was correct on the day, and
//! nothing kept it correct. The findings it recorded are the shape this module
//! exists to make permanent:
//!
//! * grounding is a **control** on the creature handlers and a **metric** on
//!   the two general-purpose execute endpoints — the ones a third party calls;
//! * `delegate_to_agent` has no grounding gate at all;
//! * `min_tier` and `capability_gates` are typed, persisted, exposed, and never
//!   compared against anything.
//!
//! Each is a property of a *verb*, not of a gate and not of a route, and there
//! was nowhere to write it down.
//!
//! # One chokepoint, eventually
//!
//! `docs/DESIGN_UX_PANEL_ARCHITECTURE.md` §3.5 wants a single verb path so that
//! every surface — palette, radial, gaze — invokes the same named command, and
//! so that gate checks and receipts have exactly one place to happen. Today the
//! gate call sites are scattered across four handlers and the surfaces each
//! build their own buttons.
//!
//! This module is the first half: **the declaration**. Routing every surface
//! through it is the second, and it is not attempted here, because a registry
//! that lies about what governs a verb would be worse than none.
//!
//! # What is asserted
//!
//! A **write** must either name a gate that can refuse it, or say why it needs
//! none. [`ungoverned_writes`] lists the ones that do neither, and the test that
//! pins that list may only shrink — the same ratchet as
//! `liveness_trust::KNOWN_SILENT` and `loop_model::STALL_REASONS`.
//!
//! It is deliberately seeded **non-empty**, with the audit's real findings. A
//! governance registry whose first run is green has not been pointed at
//! anything.

use crate::gate_trust::Gate;
use crate::panel_absence::Scope;

/// Does invoking this change anything?
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    /// Answers a question. Nothing persists.
    Read,
    /// Changes state, spends money, or emits something a third party sees.
    Write,
}

/// How a gate is actually applied on this verb's path.
///
/// The mechanism axis from `DESIGN_UX_PANEL_ARCHITECTURE.md` §2.1, which had no
/// implementation until now. The paper's sentence is the whole distinction:
///
/// > The check that runs after the write is a metric; the check that runs
/// > before it is a control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Enforcement {
    /// It runs before the effect and can refuse it.
    Control,
    /// It runs, and the verdict is discarded or only logged.
    ///
    /// Not a lesser control. On the surface a caller sees, a metric and an
    /// absent gate are the same thing.
    Metric,
    /// It runs after the effect, cannot refuse it, and **repairs the artifact
    /// before it leaves**.
    ///
    /// The rung between `Metric` and `Control`, and the one the vocabulary was
    /// missing. A post-hoc check can never be a `Control` in the strict sense:
    /// you cannot know a field is ungrounded until the model has written it, so
    /// there is no moment at which refusing the *effect* is available. What is
    /// available is refusing to let the bad part **travel**.
    ///
    /// `Gate::Grounding` on the execute routes is the case. It nulls fields no
    /// tool of the agent's could have supplied, and the caller receives the
    /// document without them. That is not a metric — a metric changes nothing a
    /// caller sees — and it is not a control, because the run still happened and
    /// still cost credits.
    ///
    /// Calling it `Metric` for the life of the feature is what let the execute
    /// route return fabricated values while the trace drew a checkpoint over it.
    Amend,
    /// It runs after the effect, changes nothing about the artifact, and **the
    /// verdict is returned to the caller**.
    ///
    /// `Gate::Completeness` is the case: there is nothing to strip from a field
    /// the agent left empty, and refusing would deny the caller fourteen good
    /// fields because of one missing one. The remedy is the agent. So the honest
    /// enforcement is a report — and a report a caller receives is not a
    /// discarded verdict, which is the only thing `Metric` means.
    ///
    /// The distinction earns its place by keeping the ratchet honest. Declaring
    /// completeness a `Metric` would have grown
    /// [`gates_computed_and_discarded`] from two entries to three, reporting a
    /// brand-new visible check as a regression.
    Report,
    /// Typed, persisted and exposed, and never compared against anything.
    Declared,
}

impl Enforcement {
    /// Can this refuse the verb?
    pub fn refuses(self) -> bool {
        matches!(self, Enforcement::Control)
    }

    /// Does this change the artifact itself?
    ///
    /// The question `refuses()` could not answer. An `Amend` does not refuse and
    /// emphatically does change what is delivered.
    pub fn alters_the_artifact(self) -> bool {
        matches!(self, Enforcement::Control | Enforcement::Amend)
    }

    /// Can the caller tell this gate ran?
    ///
    /// The predicate `gates_computed_and_discarded` actually wants, and it is
    /// wider than `alters_the_artifact`: a refusal is visible, an amendment is
    /// visible in the document, and a `Report`'s verdict is visible because it
    /// is in the response body. A `Metric` is not visible at all — which is the
    /// entire content of the sentence that list is built around, *"on the
    /// surface a caller sees, one of these is indistinguishable from having no
    /// gate at all."*
    pub fn reaches_the_caller(self) -> bool {
        matches!(
            self,
            Enforcement::Control | Enforcement::Amend | Enforcement::Report
        )
    }
}

/// One gate, as applied to one verb.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct GateApplication {
    #[serde(skip)]
    pub gate: Gate,
    pub enforcement: Enforcement,
    /// Where the check happens. Named so a reader can go and look.
    pub site: &'static str,
    /// Required when the enforcement is not [`Enforcement::Control`].
    ///
    /// A gate demoted to a metric is a decision somebody made, and the reason
    /// is what tells a later reader whether it was deliberate or drift. The
    /// grounding demotion on `/execute` is documented in the code as
    /// intentional; that it was intentional is exactly what makes it worth
    /// recording here rather than treating as a bug.
    pub why_not_control: Option<&'static str>,
}

/// A named verb.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Command {
    /// Stable, addressable id. `subject.verb`.
    pub id: &'static str,
    /// What a palette, a radial menu or a screen reader says.
    pub label: &'static str,
    /// What invoking it does, for a human.
    pub does: &'static str,
    pub scope: Scope,
    pub effect: Effect,
    /// The route or tool this verb resolves to today.
    pub route: &'static str,
    pub gates: &'static [GateApplication],
    /// Why a write needs no gate that can refuse it.
    ///
    /// `None` on a write means nobody has answered that question, which is what
    /// [`ungoverned_writes`] reports.
    pub ungated_because: Option<&'static str>,
}

impl Command {
    /// Does at least one gate stand in front of this verb?
    pub fn is_governed(&self) -> bool {
        self.gates.iter().any(|g| g.enforcement.refuses())
    }
}

const fn control(gate: Gate, site: &'static str) -> GateApplication {
    GateApplication {
        gate,
        enforcement: Enforcement::Control,
        site,
        why_not_control: None,
    }
}

const fn metric(gate: Gate, site: &'static str, why: &'static str) -> GateApplication {
    GateApplication {
        gate,
        enforcement: Enforcement::Metric,
        site,
        why_not_control: Some(why),
    }
}

/// Cannot refuse the verb; does repair the artifact before it is returned.
const fn amend(gate: Gate, site: &'static str, why: &'static str) -> GateApplication {
    GateApplication {
        gate,
        enforcement: Enforcement::Amend,
        site,
        why_not_control: Some(why),
    }
}

/// Cannot refuse or repair; the verdict is returned to the caller.
const fn report(gate: Gate, site: &'static str, why: &'static str) -> GateApplication {
    GateApplication {
        gate,
        enforcement: Enforcement::Report,
        site,
        why_not_control: Some(why),
    }
}

/// Every verb, with what governs it.
///
/// Rule for adding one: **if a person can invoke it and it changes something,
/// it belongs here.** Reads are included where a surface offers them as an
/// explicit action, because the palette needs them and because a read that
/// costs credits is not really a read.
pub const COMMANDS: &[Command] = &[
    // ── Agent execution ──────────────────────────────────────────────────
    Command {
        id: "agent.execute",
        label: "Run agent",
        does: "Send a query to an agent and get its answer",
        scope: Scope::Agent,
        effect: Effect::Write,
        route: "POST /api/agents/:agent_id/execute",
        gates: &[
            control(Gate::Credit, "handlers::execution, gas::charge_gas"),
            control(Gate::Attachment, "attachments::ensure_deliverable"),
            amend(
                Gate::Grounding,
                "episode_boundary::Pulse::grade + envelope::amend_document, \
                 from handlers::execution",
                "Cannot refuse: a field's grounding is unknowable until the model \
                 has written it, so there is no moment at which refusing the run \
                 is available. It amends instead — the response body carries the \
                 enforced document and a `grounding.stripped` list naming what \
                 was removed. The persisted response_text stays raw on purpose: \
                 it is the only evidence of what was claimed. Previously a plain \
                 Metric, and the reason recorded here was that `enforce` mutated \
                 a copy the handler kept only to check a schema against, so the \
                 endpoint a third party calls reported fabrication rather than \
                 preventing it. That is now closed for the body and remains true \
                 of the run itself.",
            ),
            metric(
                Gate::InputBinding,
                "port_trust::bind_input, from handlers::execution",
                "Declared advisory: `is_mismatch()` guards a warning and control \
                 flow is identical either way. Here so the mismatch RATE is \
                 visible, which is the number that would justify making it fatal. \
                 Also the one place on this route where genuine PREVENTION is \
                 available and unused — a malformed input can be refused before a \
                 credit is spent, which protects the payer rather than the reader.",
            ),
            report(
                Gate::Completeness,
                "episode_boundary::Pulse::assess_completeness, from \
                 handlers::execution",
                "Cannot refuse and cannot amend: there is nothing to strip from a \
                 field the agent left empty, and refusing would deny the caller \
                 fourteen good fields because of one missing one. The remedy is \
                 the agent, so the honest enforcement is a report. It exists at \
                 all because this was the one question on the artifact trace that \
                 no checkpoint stood behind — grounding asks whether a tool COULD \
                 supply a field and never whether the agent DID.",
            ),
        ],
        ungated_because: None,
    },
    Command {
        id: "agent.execute_stream",
        label: "Run agent (streaming)",
        does: "Same as Run agent, delivered as a stream",
        scope: Scope::Agent,
        effect: Effect::Write,
        route: "POST /api/agents/:agent_id/execute/stream",
        gates: &[
            control(Gate::Credit, "handlers::execution_stream"),
            metric(
                Gate::Grounding,
                "episode_boundary::Pulse::grade, from handlers::execution_stream",
                "Still un-amended, and the asymmetry is now with its own sibling \
                 rather than with the delegation hop: `agent.execute` returns the \
                 enforced document and this route does not. A stream has already \
                 sent its tokens by the time the document is gradeable, so \
                 amending the body is not the same edit — it needs a terminal \
                 frame carrying the enforced document and a consumer that prefers \
                 it over the concatenated deltas. Named here so the gap is a \
                 declared to-do rather than a difference nobody noticed.",
            ),
        ],
        ungated_because: None,
    },
    // ── Lifecycle ────────────────────────────────────────────────────────
    Command {
        id: "agent.publish",
        label: "Publish agent",
        does: "Make an agent visible and callable by others",
        scope: Scope::Agent,
        effect: Effect::Write,
        route: "POST /api/agents/:agent_id/publish",
        gates: &[control(
            Gate::Admission,
            "workflows::publish_pipeline, card_contract::validate",
        )],
        ungated_because: None,
    },
    Command {
        id: "agent.archive",
        label: "Archive agent",
        does: "Withdraw an agent from use without deleting it",
        scope: Scope::Agent,
        effect: Effect::Write,
        route: "POST /api/agents/:agent_id/archive",
        gates: &[],
        ungated_because: Some(
            "Withdrawal is the safe direction. An owner removing their own agent \
             from circulation needs no permission from a gate, and a gate that \
             could refuse it would be a gate that traps an owner with a bad \
             agent in production.",
        ),
    },
    Command {
        id: "agent.fork",
        label: "Fork agent",
        does: "Copy a published agent, optionally with its ontology and embeddings",
        scope: Scope::Agent,
        effect: Effect::Write,
        route: "POST /api/agents/:agent_id/fork",
        gates: &[control(Gate::Credit, "workflows::fork")],
        ungated_because: None,
    },
    // ── Learning ─────────────────────────────────────────────────────────
    Command {
        id: "agent.consolidate",
        label: "Run dream cycle",
        does: "Cluster this agent's episodes into semantic rules (Loop 1)",
        scope: Scope::Agent,
        effect: Effect::Write,
        route: "POST /api/agents/:agent_id/consolidate",
        gates: &[control(
            Gate::Credit,
            "handlers::consolidation, gas::charge_gas",
        )],
        ungated_because: None,
    },
    Command {
        id: "agent.eval_run",
        label: "Run eval",
        does: "Score an agent against its test cases",
        scope: Scope::Agent,
        effect: Effect::Write,
        route: "POST /api/agents/:agent_id/eval/run",
        gates: &[control(Gate::Credit, "handlers::eval")],
        ungated_because: None,
    },
    // ── Human-gated correction ───────────────────────────────────────────
    Command {
        id: "hitl.intervene",
        label: "Intervene on anomaly",
        does: "Write a human correction into an agent's memory (Loop 2)",
        scope: Scope::Agent,
        effect: Effect::Write,
        route: "POST /api/observatory/hitl/:event_id/action",
        gates: &[control(
            Gate::Coherence,
            "coherence-gate::CoherenceGate::check_against, via handlers::observatory",
        )],
        ungated_because: None,
    },
    Command {
        id: "hitl.confirm_consensus",
        label: "Confirm as second reviewer",
        does: "Supply the second approval an agent-wide correction requires",
        scope: Scope::Agent,
        effect: Effect::Write,
        route: "POST /api/observatory/hitl/consensus/:request_id",
        gates: &[control(
            Gate::Coherence,
            "coherence-gate::two_write, via handlers::observatory",
        )],
        ungated_because: None,
    },
    // ── Composition ──────────────────────────────────────────────────────
    Command {
        id: "workspace.create",
        label: "New composition",
        does: "Create a workspace agents can be hired into",
        scope: Scope::Workspace,
        effect: Effect::Write,
        route: "POST /api/teams",
        gates: &[],
        ungated_because: Some(
            "Creating an empty workspace spends nothing and exposes nothing. The \
             gates that matter apply to hiring into it and to executing in it, \
             which are separate verbs with their own entries.",
        ),
    },
    Command {
        id: "workspace.hire",
        label: "Hire agent",
        does: "Add an agent to a composition",
        scope: Scope::Workspace,
        effect: Effect::Write,
        route: "POST /api/workspaces/:workspace_id/add",
        gates: &[control(Gate::Credit, "handlers::rabble_workspace")],
        ungated_because: None,
    },
    // ── Money ────────────────────────────────────────────────────────────
    Command {
        id: "billing.checkout",
        label: "Buy credits",
        does: "Start a Stripe checkout for a credit bundle",
        scope: Scope::Account,
        effect: Effect::Write,
        route: "POST /api/billing/checkout",
        gates: &[],
        ungated_because: Some(
            "The gate is Stripe's. Adding a credit gate in front of buying \
             credits would be the deadlock it sounds like.",
        ),
    },
    // ── Reads offered as actions ─────────────────────────────────────────
    Command {
        id: "agent.publish_checks",
        label: "Diagnose agent",
        does: "Run the publish contract without publishing",
        scope: Scope::Agent,
        effect: Effect::Read,
        route: "GET /api/agents/:agent_id/publish-checks",
        gates: &[],
        ungated_because: None,
    },
    Command {
        id: "observatory.fleet_scan",
        label: "Fleet scan",
        does: "Look for provider-correlated decline across all your agents",
        scope: Scope::Account,
        effect: Effect::Read,
        route: "POST /api/observatory/fleet/scan",
        gates: &[],
        ungated_because: None,
    },
];

/// Look one up.
pub fn command(id: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.id == id)
}

/// Writes that no gate can refuse, and that have not said why.
///
/// The assertable finding, shaped like `gate_trust::refusing_everything`: it
/// makes no claim about whether the verb *should* be gated, only that nobody
/// has written down an answer.
pub fn ungoverned_writes() -> Vec<&'static str> {
    COMMANDS
        .iter()
        .filter(|c| c.effect == Effect::Write && !c.is_governed() && c.ungated_because.is_none())
        .map(|c| c.id)
        .collect()
}

/// Verbs where a gate runs and its verdict is thrown away.
///
/// The audit's §3 table, as a live query. On the surface a caller sees, one of
/// these is indistinguishable from having no gate at all.
/// Keyed on `reaches_the_caller`, not on `refuses`.
///
/// Two corrections, both the same shape. An `Amend` changes the document the
/// caller receives and a `Report` puts its verdict in the response body; listing
/// either as *discarded* would report a working check as a dead one — the error
/// this function exists to expose, pointed the other way.
pub fn gates_computed_and_discarded() -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for c in COMMANDS {
        for g in c.gates {
            if !g.enforcement.reaches_the_caller() {
                out.push((c.id, g.gate.id()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_is_distinct_and_says_what_it_does() {
        let mut ids = std::collections::HashSet::new();
        let mut labels = std::collections::HashSet::new();
        for c in COMMANDS {
            assert!(ids.insert(c.id), "duplicate command id `{}`", c.id);
            assert!(
                c.id.contains('.'),
                "{}: ids are `subject.verb`, so a palette can group them",
                c.id
            );
            assert!(
                labels.insert(c.label),
                "{}: two commands share the label `{}`, so a palette or a screen \
                 reader cannot tell them apart",
                c.id,
                c.label
            );
            assert!(c.does.len() > 15, "{}: say what invoking it does", c.id);
            assert!(
                c.route.contains(' '),
                "{}: route is `METHOD /path`, got {:?}",
                c.id,
                c.route
            );
        }
    }

    /// A demoted gate must say why it was demoted.
    #[test]
    fn a_gate_that_cannot_refuse_explains_itself() {
        for c in COMMANDS {
            for g in c.gates {
                assert!(g.site.contains("::"), "{}: gate site must name code", c.id);
                if g.enforcement.refuses() {
                    continue;
                }
                let why = g.why_not_control.unwrap_or_else(|| {
                    panic!(
                        "{}: gate `{}` is {:?} and does not say why. A gate \
                         demoted to a metric is a decision, and an undocumented \
                         one is indistinguishable from drift.",
                        c.id,
                        g.gate.id(),
                        g.enforcement
                    )
                });
                assert!(
                    why.len() > 60,
                    "{}: `{}` — say what it costs",
                    c.id,
                    g.gate.id()
                );
            }
        }
    }

    /// The ratchet: every write is governed or has said why not.
    #[test]
    fn no_write_is_silently_ungoverned() {
        assert_eq!(
            ungoverned_writes(),
            Vec::<&str>::new(),
            "these verbs change something, no gate can refuse them, and nobody \
             has written down why that is acceptable. Either name a Control \
             gate or fill in `ungated_because`."
        );
    }

    /// The audit's §3 table, pinned. It may only shrink.
    ///
    /// Seeded non-empty on purpose: a governance registry whose first run is
    /// green has not been pointed at anything. Every pair here is a verb whose
    /// caller cannot tell the gate from its absence.
    ///
    /// **Three, then two.** `("agent.execute", "grounding")` came off the list
    /// when that route started returning the enforced document instead of the
    /// raw one — the caller can now tell the gate from its absence, because the
    /// fabricated field is gone and `grounding.stripped` names it. That is the
    /// ratchet doing the only thing it is for.
    ///
    /// The two that remain are both real and both named in their
    /// `why_not_control`: the stream cannot amend a body it has already sent,
    /// and `input_binding` is the one place genuine PREVENTION is available and
    /// unused — a malformed input could be refused before a credit is spent.
    #[test]
    fn the_discarded_gate_verdicts_are_the_ones_we_know_about() {
        assert_eq!(
            gates_computed_and_discarded(),
            vec![
                ("agent.execute", "input_binding"),
                ("agent.execute_stream", "grounding"),
            ],
            "the set of verbs whose gate verdict is thrown away has changed. It \
             may only shrink. Promoting one to Control is the fix; adding one is \
             a regression that needs its cost written into `why_not_control`."
        );
    }

    /// What grounding actually does on the two public execute endpoints.
    ///
    /// This test used to assert `Metric` on both, and carried an instruction to
    /// rewrite it if that ever changed. It has. `agent.execute` now **amends**:
    /// the response body carries the enforced document and names what was
    /// stripped, so a fabricated value no longer travels to a third party.
    ///
    /// Not promoted to `Control`, and the distinction is the point. A control
    /// refuses the verb, and grounding cannot — a field's grounding is
    /// unknowable until the model has written it, so the run has already
    /// happened and already cost credits by the time there is anything to judge.
    /// The reachable ceiling for a post-hoc gate is stopping the bad part from
    /// leaving, which is what `Amend` names.
    ///
    /// The streaming sibling is still a plain `Metric` and that is now the
    /// asymmetry worth watching: same platform, same contract, two different
    /// answers depending on which endpoint you call.
    #[test]
    fn grounding_amends_on_execute_and_still_does_not_on_the_stream() {
        let g = |id: &str| {
            command(id)
                .unwrap_or_else(|| panic!("{id} not declared"))
                .gates
                .iter()
                .find(|g| g.gate == Gate::Grounding)
                .unwrap_or_else(|| panic!("{id} no longer declares a grounding gate at all"))
                .enforcement
        };

        assert_eq!(
            g("agent.execute"),
            Enforcement::Amend,
            "grounding stopped amending the execute body. That route returns the \
             document a third party reads; a Metric there means the endpoint \
             reports fabrication rather than preventing it, which is the state \
             this was fixed out of."
        );
        assert!(
            g("agent.execute").alters_the_artifact(),
            "an amend that does not reach the caller is a metric wearing a \
             better name"
        );
        assert!(
            !g("agent.execute").refuses(),
            "grounding cannot refuse: there is no moment before the effect at \
             which the answer is known. If this became a Control, something is \
             claiming to prevent what it can only repair."
        );

        assert_eq!(
            g("agent.execute_stream"),
            Enforcement::Metric,
            "the stream now amends too — good, and this test is stale. Its \
             `why_not_control` says what that change requires (a terminal frame \
             carrying the enforced document); update both together."
        );
    }

    /// Reads must not claim to spend.
    #[test]
    fn a_read_declares_no_credit_gate() {
        for c in COMMANDS {
            if c.effect != Effect::Read {
                continue;
            }
            assert!(
                !c.gates.iter().any(|g| g.gate == Gate::Credit),
                "{}: a verb that charges is a write, whatever it returns",
                c.id
            );
            assert!(
                c.ungated_because.is_none(),
                "{}: `ungated_because` answers a question only writes are asked",
                c.id
            );
        }
    }

    /// Every declared route must exist in the router.
    ///
    /// The fence that keeps this from becoming the thing it replaces. A hand-
    /// maintained table of routes drifts the moment one is renamed, and a
    /// governance registry pointing at a route that no longer exists reads as
    /// current while governing nothing — the same shape as the `fermi_leaderboard`
    /// probe that could never return healthy and was ignored for eight releases.
    ///
    /// Reads the router's source rather than its route table because the table
    /// is built inside a function and is not reachable from a unit test. That is
    /// a weaker check than matching the live `Router`, and it is the strongest
    /// one available without booting the server.
    #[test]
    fn every_declared_route_exists_in_the_router() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/api_server.rs"),
        )
        .expect("api_server.rs is readable");

        let mut missing = Vec::new();
        for c in COMMANDS {
            let path = c.route.split_once(' ').map(|(_, p)| p).unwrap_or(c.route);
            if !src.contains(&format!("\"{path}\"")) {
                missing.push(format!("{} -> {}", c.id, c.route));
            }
        }
        assert!(
            missing.is_empty(),
            "{} command(s) name a route the router does not declare:\n  {}",
            missing.len(),
            missing.join("\n  ")
        );
    }

    /// Every gate a command names must be a declared gate.
    #[test]
    fn every_named_gate_exists() {
        for c in COMMANDS {
            for g in c.gates {
                assert!(
                    crate::gate_trust::GATES.iter().any(|s| s.gate == g.gate),
                    "{}: names a gate that `gate_trust::GATES` does not declare",
                    c.id
                );
            }
        }
    }
}
