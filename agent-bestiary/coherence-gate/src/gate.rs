//! Step 3 — Coherence gate.
//!
//! Runs the TEC formal model against the proposed corrective update and
//! returns a [`GateOutcome`].
//!
//! Decision D10 (OBSERVABILITY_IMPL.md):
//!   - `AgentWide` scope → synchronous gate: Γ(C) < threshold ⟹ `Blocked`
//!   - `Episode` / `Dyad` scope → settler mode: gate advises but always
//!     returns `Ok(GateOutcome { verdict: Settled })`.
//!
//! Decision OQ-5: initial threshold = 0.5.
//!
//! ## Model
//!
//! The gate settles the correction **against the agent's actual world model**:
//! the episodes it has observed and the semantic rules it has distilled from
//! them, with `Explains` relations from each rule to the episodes it rests on.
//! The correction `Contradicts` the one node being corrected. Settling then
//! propagates that contradiction along the explanation links, so correcting a
//! load-bearing observation drags down every rule built on it, while correcting
//! a peripheral one does not.
//!
//! A low Γ(C) means the correction cannot be absorbed without wrecking a large
//! part of what the agent believes — blocked for `AgentWide`. A high Γ(C) means
//! the world model can take it.
//!
//! ### The bug this replaces
//!
//! Until this was rewritten the gate built a throwaway two-node system — the
//! literal string `"existing agent response"` and the correction, joined by one
//! `Contradicts` edge — and never loaded anything about the agent at all. Two
//! non-evidence utterances start at `Activation::DEFAULT_INITIAL` (0.01), Γ is
//! the mean positive activation, and the only edge was negative, so Γ could not
//! exceed 0.01 against a threshold of 0.5. **Every `AgentWide` intervention was
//! rejected, for arithmetic reasons, regardless of its content.**
//!
//! The engine's own test asserts that exact shape scores below 0.5
//! (`coherence-engine`, `contradicting_utterances_reduce_coherence`), and the
//! gate's tests did not catch it because neither used the production threshold:
//! one passed `0.0`, the other `1.0` with the comment "no real settling will
//! ever reach this".
//!
//! Two consequences beyond the wrong verdict. The two-reviewer consensus path
//! sits downstream of the gate, so the strongest control in the loop — two
//! independent humans — was unreachable. And `persona_version` only bumps on
//! `AgentWide`, so it never bumped.
//!
//! A correction contradicting the response being corrected is *definitionally*
//! what a correction is. Asking whether it contradicts that response can only
//! ever return yes. The question worth asking is whether it contradicts
//! everything *else* the agent believes, which is what this now does.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use coherence_core::{
    relations::{CoherenceKind, CoherenceRelation, IncoherenceKind, IncoherenceRelation},
    CoherenceSystem, ConversationId, MessageId, ParticipantId, Utterance, UtteranceKind,
};
use coherence_engine::SettlingEngine;

use crate::encoder::EncodedIntervention;
use crate::error::GateError;

/// Default Γ(C) threshold below which `AgentWide` writes are blocked.
pub const DEFAULT_GATE_THRESHOLD: f64 = 0.5;

/// One proposition the agent currently holds.
///
/// Either something it observed (an episode) or something it concluded (a
/// semantic rule distilled from episodes). The distinction is load-bearing:
/// only observations get [`UtteranceKind::Evidence`] and therefore Thagard's
/// Data Priority (P4). A distilled rule is a [`UtteranceKind::Claim`] however
/// well-sourced its inputs were — the same extraction ceiling the provenance
/// oracle enforces, because judgement does not inherit retrieval.
#[derive(Debug, Clone)]
pub struct WorldNode {
    /// Stable identifier, used to label the minimum update set.
    pub id: String,
    pub text: String,
    /// `true` for an observed episode, `false` for a distilled rule.
    pub grounded: bool,
    /// Indices of the nodes this one is derived from / explains.
    ///
    /// For a semantic rule these are the episodes it was distilled from. This
    /// is what makes the settling propagate: contradict an episode and every
    /// rule resting on it loses support too.
    pub derived_from: Vec<usize>,
}

/// The agent's current beliefs, as supplied by the caller.
///
/// Assembled outside this crate so the gate stays pure and testable: the
/// server reads episodes and semantic rules, this decides what they imply.
#[derive(Debug, Clone, Default)]
pub struct WorldModel {
    pub nodes: Vec<WorldNode>,
    /// Index into `nodes` of the proposition the correction overturns.
    ///
    /// `None` when the episode under review could not be located, which is not
    /// the same as there being nothing to overturn — see [`GateVerdict::Undetermined`].
    pub target: Option<usize>,
}

impl WorldModel {
    /// Is there enough here to say anything?
    ///
    /// One node and a target is the minimum: a correction needs something to
    /// contradict and something to be judged against. Below that the gate has
    /// no opinion, and must say so rather than defaulting either way.
    pub fn is_sufficient(&self) -> bool {
        self.target.is_some() && self.nodes.len() >= 2
    }
}

/// Outcome of the coherence gate check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateOutcome {
    pub verdict: GateVerdict,
    /// Γ(C) from the settling run (`None` when system had no scorable utterances).
    pub gamma: Option<f64>,
    /// Per-principle scores as `{ principle_name: score }`.
    pub principle_scores: HashMap<String, f64>,
    /// Principle dimensions where the proposed update creates tension
    /// (score < 0.5 on incoherence-sensitive principles: Contradiction, Competition).
    pub tensions: Vec<String>,
    /// The minimal set of world-model nodes that must change for the
    /// correction to be accepted (nodes with negative activation after settling).
    pub minimum_update_set: Vec<MinimumUpdateNode>,
    /// The correction's own activation after settling.
    ///
    /// Negative means the agent's beliefs **rejected** it. This, not Γ, is what
    /// answers "does the correction cohere with the world model" — see the
    /// note on [`CoherenceGate::check_against`].
    pub correction_activation: Option<f64>,
}

/// Single node in the minimum update set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimumUpdateNode {
    pub node: String,
    pub delta: f64,
}

/// Whether the gate approved or blocked the write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateVerdict {
    /// Gate approved — write may proceed.
    Approved,
    /// Gate blocked — write must not proceed (only for synchronous mode,
    /// i.e. `AgentWide` scope).
    Blocked,
    /// Settler mode — gate ran but verdict is advisory only.
    Settled,
    /// The gate could not form an opinion: the agent has too little world model
    /// to settle a correction against.
    ///
    /// Deliberately not `Approved` and deliberately not `Blocked`. Silence is
    /// not a verdict — an agent with no recorded beliefs has neither passed a
    /// coherence check nor failed one, and collapsing that into either answer
    /// is how a check starts reporting on things it never examined. The caller
    /// must fall back to the human controls.
    Undetermined,
}

/// Coherence gate — step 3 of the intervention feedback loop.
pub struct CoherenceGate {
    threshold: f64,
}

impl Default for CoherenceGate {
    fn default() -> Self {
        Self::new(DEFAULT_GATE_THRESHOLD)
    }
}

impl CoherenceGate {
    pub fn new(threshold: f64) -> Self {
        Self { threshold }
    }

    /// Run the gate check for an encoded intervention.
    ///
    /// For `AgentWide` scope (`gate_is_synchronous = true`):
    ///   Returns `Err(GateError::Blocked)` when Γ(C) < threshold.
    ///
    /// For `Episode` / `Dyad` scope:
    ///   Always returns `Ok(GateOutcome { verdict: Settled })`.
    pub fn check(&self, intervention: &EncodedIntervention) -> Result<GateOutcome, GateError> {
        self.check_against(intervention, &WorldModel::default())
    }

    /// Run the gate against the agent's actual beliefs.
    ///
    /// This is the real entry point; [`Self::check`] is the no-world-model case
    /// and now returns [`GateVerdict::Undetermined`] rather than a verdict it
    /// has not earned.
    pub fn check_against(
        &self,
        intervention: &EncodedIntervention,
        world: &WorldModel,
    ) -> Result<GateOutcome, GateError> {
        if !world.is_sufficient() {
            return Ok(GateOutcome {
                verdict: GateVerdict::Undetermined,
                gamma: None,
                principle_scores: HashMap::new(),
                tensions: Vec::new(),
                minimum_update_set: Vec::new(),
                correction_activation: None,
            });
        }

        let mut outcome = self.run_settling(intervention, world)?;

        if intervention.gate_is_synchronous {
            // gamma is None only when nothing scorable survived settling.
            // After the sufficiency check above that means the run failed, and
            // a failed check is not a pass.
            let Some(gamma) = outcome.gamma else {
                outcome.verdict = GateVerdict::Undetermined;
                return Ok(outcome);
            };
            // Primary condition: did the world model reject the correction?
            //
            // Γ alone cannot answer this, and we measured that rather than
            // assuming it. A settled system that *rejects* a contradicting
            // proposition stays perfectly coherent — Γ is identical (0.632) for
            // a correction the agent absorbs and one it throws out. What differs
            // is the correction's own activation: 0 nodes rejected in the first
            // case, the correction itself rejected with mass 0.89 in the second.
            if outcome.correction_activation.is_some_and(|a| a < 0.0) {
                return Err(GateError::Blocked {
                    gamma,
                    threshold: self.threshold,
                    tensions: outcome.tensions.clone(),
                });
            }
            if gamma < self.threshold {
                return Err(GateError::Blocked {
                    gamma,
                    threshold: self.threshold,
                    tensions: outcome.tensions.clone(),
                });
            }
            outcome.verdict = GateVerdict::Approved;
        } else {
            outcome.verdict = GateVerdict::Settled;
        }

        Ok(outcome)
    }

    // ── Internal ────────────────────────────────────────────────────

    fn run_settling(
        &self,
        intervention: &EncodedIntervention,
        world: &WorldModel,
    ) -> Result<GateOutcome, GateError> {
        let conv_id = ConversationId::new();
        let mut system = CoherenceSystem::new(conv_id);

        let agent_pid = ParticipantId::new();
        let reviewer_pid = ParticipantId::new();
        let msg_agent = MessageId::new();
        let msg_reviewer = MessageId::new();

        // ── The agent's world model ────────────────────────────────
        // Observations are Evidence and therefore carry Data Priority (P4);
        // distilled rules are Claims, because a conclusion never inherits the
        // standing of what it was concluded from.
        let mut node_ids = Vec::with_capacity(world.nodes.len());
        for node in &world.nodes {
            let kind = if node.grounded {
                UtteranceKind::Evidence
            } else {
                UtteranceKind::Claim
            };
            let u = Utterance::new(agent_pid, msg_agent, kind, node.text.as_str());
            node_ids.push(u.id);
            system.add_utterance(u);
        }

        // ── What rests on what ────────────────────────────────────
        // A rule Explains the episodes it was distilled from. These links are
        // what make the result depend on *which* belief is being corrected:
        // contradict a load-bearing observation and every rule above it loses
        // its support; contradict a peripheral one and the rest is unmoved.
        for (i, node) in world.nodes.iter().enumerate() {
            for &src in &node.derived_from {
                if src >= node_ids.len() || src == i {
                    continue;
                }
                let rel =
                    CoherenceRelation::new(node_ids[i], node_ids[src], CoherenceKind::Explains)
                        .map_err(|e| GateError::SettlingFailed(e.to_string()))?;
                system
                    .add_coherence(rel)
                    .map_err(|e| GateError::SettlingFailed(e.to_string()))?;
            }
        }

        // ── The proposed correction ────────────────────────────────
        let correction_text = intervention
            .correction_text
            .as_deref()
            .unwrap_or("proposed correction");
        // A cited correction is Evidence; an uncited one is a Claim.
        //
        // This is the same rule the verification ladder applies to a human
        // verdict: one that records what it was checked against ranks with a
        // tool call, because someone else can follow the citation to the same
        // source; one that does not ranks with an opinion. Here it decides
        // whether the correction contends on equal footing with the
        // observation it contradicts, or is simply outweighed by it under
        // Data Priority.
        let correction_kind = if intervention.justification.is_some() {
            UtteranceKind::Evidence
        } else {
            UtteranceKind::Claim
        };
        let correction =
            Utterance::new(reviewer_pid, msg_reviewer, correction_kind, correction_text);
        let correction_id = correction.id;
        system.add_utterance(correction);

        // It contradicts exactly the node it overturns — not the whole model.
        // `is_sufficient` guarantees the target is present.
        let target = world.target.ok_or_else(|| {
            GateError::SettlingFailed("world model has no target node".to_string())
        })?;
        let target_id = *node_ids
            .get(target)
            .ok_or_else(|| GateError::SettlingFailed("target index out of range".to_string()))?;

        let incoherence =
            IncoherenceRelation::new(target_id, correction_id, IncoherenceKind::Contradicts)
                .map_err(|e| GateError::SettlingFailed(e.to_string()))?;
        system
            .add_incoherence(incoherence)
            .map_err(|e| GateError::SettlingFailed(e.to_string()))?;

        // ── Settle ────────────────────────────────────────────────
        let engine = SettlingEngine::with_defaults();
        engine.settle(&mut system);

        // Read gamma from system after settling.
        let gamma = system.global_coherence.as_ref().map(|gc| gc.score);

        // ── Principle scores ──────────────────────────────────────
        let principle_scores_obj = &system.principle_scores;
        let mut principle_scores: HashMap<String, f64> = HashMap::new();
        let mut tensions: Vec<String> = Vec::new();

        for ps in principle_scores_obj.scores.iter() {
            let name = format!("{:?}", ps.principle).to_lowercase();
            principle_scores.insert(name.clone(), ps.score);
            // Tension = incoherence-sensitive principles below 0.5
            let is_incoherence_sensitive =
                name.contains("contradiction") || name.contains("competition");
            if is_incoherence_sensitive && ps.score < 0.5 {
                tensions.push(name);
            }
        }

        // ── Minimum update set ──────────────────────────────────
        // Utterances with negative activation after settling are the
        // nodes that must change for the correction to be absorbed.
        let mut minimum_update_set = Vec::new();
        let mut correction_activation = None;
        for activation in system.all_activations() {
            if activation.utterance_id == correction_id {
                correction_activation = Some(activation.value);
            }
            if activation.value < 0.0 {
                // Look up the utterance text for a human-readable label.
                let label = system
                    .utterance(activation.utterance_id)
                    .map(|u| u.content.chars().take(60).collect::<String>())
                    .unwrap_or_else(|| activation.utterance_id.to_string());
                minimum_update_set.push(MinimumUpdateNode {
                    node: label,
                    delta: activation.value.abs(),
                });
            }
        }

        Ok(GateOutcome {
            verdict: GateVerdict::Approved, // overwritten by check()
            gamma,
            principle_scores,
            tensions,
            minimum_update_set,
            correction_activation,
        })
    }
}
