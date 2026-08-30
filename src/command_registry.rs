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
    /// Typed, persisted and exposed, and never compared against anything.
    Declared,
}

impl Enforcement {
    /// Can this refuse the verb?
    pub fn refuses(self) -> bool {
        matches!(self, Enforcement::Control)
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
            metric(
                Gate::Grounding,
                "episode_boundary::Pulse::grade, from handlers::execution",
                "`enforce` mutates a document the handler keeps only to check a \
                 schema against; the persisted response_text and the rendered \
                 body are both un-stripped. Retention is deliberate — a digest \
                 is not a record — and it means the endpoint a third party \
                 calls reports fabrication rather than preventing it.",
            ),
            metric(
                Gate::InputBinding,
                "port_trust::bind_input, from handlers::execution",
                "Declared advisory: `is_mismatch()` guards a warning and control \
                 flow is identical either way. Here so the mismatch RATE is \
                 visible, which is the number that would justify making it fatal.",
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
                "The same un-stripped-body shape as agent.execute. It also \
                 raised no anomaly, so the metric was not even recorded — that \
                 half is fixed: both routes reach the grade and the raise \
                 through the one boundary, and neither can drift from the other \
                 again without the other going with it.",
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
pub fn gates_computed_and_discarded() -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for c in COMMANDS {
        for g in c.gates {
            if !g.enforcement.refuses() {
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
    #[test]
    fn the_discarded_gate_verdicts_are_the_ones_we_know_about() {
        assert_eq!(
            gates_computed_and_discarded(),
            vec![
                ("agent.execute", "grounding"),
                ("agent.execute", "input_binding"),
                ("agent.execute_stream", "grounding"),
            ],
            "the set of verbs whose gate verdict is thrown away has changed. It \
             may only shrink. Promoting one to Control is the fix; adding one is \
             a regression that needs its cost written into `why_not_control`."
        );
    }

    /// The two general-purpose execute endpoints are the ungrounded ones.
    ///
    /// Pinned on its own because it is the audit's sharpest finding and the
    /// easiest to lose in a refactor: grounding is a control on the creature and
    /// wild handlers and a metric on the endpoints a third party actually calls.
    #[test]
    fn grounding_is_not_a_control_on_the_public_execute_paths() {
        for id in ["agent.execute", "agent.execute_stream"] {
            let c = command(id).expect("declared");
            let g = c
                .gates
                .iter()
                .find(|g| g.gate == Gate::Grounding)
                .unwrap_or_else(|| panic!("{id} no longer declares a grounding gate at all"));
            assert_eq!(
                g.enforcement,
                Enforcement::Metric,
                "{id}: grounding has changed enforcement. If it is now a Control, \
                 that is the fix the audit asked for — delete this test and \
                 update `the_discarded_gate_verdicts_are_the_ones_we_know_about`."
            );
        }
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
