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
//! We build a minimal two-utterance CoherenceSystem:
//!   U0 = the existing agent response (the belief/behaviour being questioned)
//!   U1 = the proposed corrected response
//! with a `Contradicts` incoherence relation between them.
//!
//! After settling, Γ(C) quantifies the resistance to change.
//!
//! A low Γ(C) means the existing system strongly resists the proposed
//! correction — the gate blocks `AgentWide` writes in this case.
//! A high Γ(C) means the system can absorb the correction — the gate
//! approves (or advises for non-AgentWide scope).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use coherence_core::{
    relations::{IncoherenceKind, IncoherenceRelation},
    CoherenceSystem, ConversationId, MessageId, ParticipantId, Utterance, UtteranceKind,
};
use coherence_engine::SettlingEngine;

use crate::encoder::EncodedIntervention;
use crate::error::GateError;

/// Default Γ(C) threshold below which `AgentWide` writes are blocked.
pub const DEFAULT_GATE_THRESHOLD: f64 = 0.5;

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
        let mut outcome = self.run_settling(intervention)?;

        if intervention.gate_is_synchronous {
            if let Some(gamma) = outcome.gamma {
                if gamma < self.threshold {
                    return Err(GateError::Blocked {
                        gamma,
                        threshold: self.threshold,
                        tensions: outcome.tensions.clone(),
                    });
                }
            }
            outcome.verdict = GateVerdict::Approved;
        } else {
            outcome.verdict = GateVerdict::Settled;
        }

        Ok(outcome)
    }

    // ── Internal ────────────────────────────────────────────────────

    fn run_settling(&self, intervention: &EncodedIntervention) -> Result<GateOutcome, GateError> {
        let conv_id = ConversationId::new();
        let mut system = CoherenceSystem::new(conv_id);

        // ── Participants ──────────────────────────────────────────
        let agent_pid = ParticipantId::new();
        let reviewer_pid = ParticipantId::new();

        // ── Messages ──────────────────────────────────────────────
        let msg0_id = MessageId::new();
        let msg1_id = MessageId::new();

        // ── Utterances ────────────────────────────────────────────
        // U0 — existing agent response (the belief/behaviour being questioned)
        let u0 = Utterance::new(
            agent_pid,
            msg0_id,
            UtteranceKind::Claim,
            "existing agent response",
        );
        let u0_id = u0.id;

        // U1 — proposed corrected response
        let correction_text = intervention
            .correction_text
            .as_deref()
            .unwrap_or("proposed correction");
        let u1 = Utterance::new(reviewer_pid, msg1_id, UtteranceKind::Claim, correction_text);
        let u1_id = u1.id;

        system.add_utterance(u0);
        system.add_utterance(u1);

        // ── Incoherence relation ──────────────────────────────────
        // U0 and U1 are mutually exclusive.
        let incoherence = IncoherenceRelation::new(u0_id, u1_id, IncoherenceKind::Contradicts)
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

        // ── Minimum update set ────────────────────────────────────
        // Utterances with negative activation after settling are the
        // nodes that must change for the correction to be absorbed.
        let mut minimum_update_set = Vec::new();
        for activation in system.all_activations() {
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
        })
    }
}
