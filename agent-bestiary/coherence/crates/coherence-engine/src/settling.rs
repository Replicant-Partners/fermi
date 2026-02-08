//! The connectionist settling engine.
//!
//! Implements Thagard's constraint-satisfaction network settling algorithm
//! to compute activation values for all utterance nodes.

use std::collections::HashMap;

use coherence_core::{types::UtteranceId, CoherenceSystem, GlobalCoherence};

use crate::scoring::PrincipleScorer;

// ─── Configuration ─────────────────────────────────────────────────────────

/// Configuration parameters for the settling algorithm.
#[derive(Debug, Clone)]
pub struct SettlingConfig {
    /// Decay parameter δ — controls how much activation dissipates each cycle.
    /// Typical value: 0.05. Range: (0, 1).
    pub decay: f64,

    /// Learning rate η — controls how strongly neighbor activations propagate.
    /// Typical value: 0.05. Range: (0, 1).
    pub learning_rate: f64,

    /// Convergence threshold ε — the network has converged when the maximum
    /// activation change across all nodes is less than this value.
    /// Typical value: 0.001.
    pub epsilon: f64,

    /// Maximum number of settling cycles before giving up.
    /// Typical value: 200.
    pub max_cycles: usize,
}

impl Default for SettlingConfig {
    fn default() -> Self {
        Self {
            decay: 0.05,
            learning_rate: 0.05,
            epsilon: 0.001,
            max_cycles: 200,
        }
    }
}

// ─── Settling Result ───────────────────────────────────────────────────────

/// The result of running the settling algorithm.
#[derive(Debug, Clone)]
pub struct SettlingResult {
    /// The number of cycles that were run.
    pub cycles: usize,

    /// Whether the network converged (max delta < epsilon).
    pub converged: bool,

    /// The maximum activation delta in the final cycle.
    pub final_max_delta: f64,

    /// Per-cycle max-delta history (for diagnostics).
    pub delta_history: Vec<f64>,
}

// ─── Weight Matrix ─────────────────────────────────────────────────────────

/// Sparse weight matrix for the connectionist network.
///
/// For each node (UtteranceId), stores its weighted neighbors: the other
/// node and the signed weight between them.
type WeightMap = HashMap<UtteranceId, Vec<(UtteranceId, f64)>>;

/// Build the weight map from a CoherenceSystem's relations.
///
/// Relations are symmetric, so each relation adds two entries (one per endpoint).
fn build_weight_map(system: &CoherenceSystem) -> WeightMap {
    let mut map: WeightMap = HashMap::new();

    // Coherence relations → positive weights
    for rel in &system.relations.coherence {
        let w = rel.network_weight();
        map.entry(rel.source).or_default().push((rel.target, w));
        map.entry(rel.target).or_default().push((rel.source, w));
    }

    // Incoherence relations → negative weights
    for rel in &system.relations.incoherence {
        let w = rel.network_weight(); // already negative
        map.entry(rel.source).or_default().push((rel.target, w));
        map.entry(rel.target).or_default().push((rel.source, w));
    }

    map
}

// ─── Engine ────────────────────────────────────────────────────────────────

/// The settling engine runs the connectionist constraint-satisfaction
/// algorithm on a [`CoherenceSystem`].
pub struct SettlingEngine {
    pub config: SettlingConfig,
}

impl SettlingEngine {
    /// Create a new engine with the given configuration.
    pub fn new(config: SettlingConfig) -> Self {
        Self { config }
    }

    /// Create a new engine with default configuration.
    pub fn with_defaults() -> Self {
        Self {
            config: SettlingConfig::default(),
        }
    }

    /// Run the settling algorithm on the given system.
    ///
    /// This mutates the system's activations in place, then computes and
    /// sets the global coherence score and principle scores.
    ///
    /// Returns a [`SettlingResult`] with convergence diagnostics.
    pub fn settle(&self, system: &mut CoherenceSystem) -> SettlingResult {
        let weights = build_weight_map(system);
        let scorable_ids: Vec<UtteranceId> =
            system.scorable_utterances().iter().map(|u| u.id).collect();

        if scorable_ids.is_empty() {
            let gc = GlobalCoherence::compute(&[], 0, true, 0.0);
            system.set_global_coherence(gc);
            return SettlingResult {
                cycles: 0,
                converged: true,
                final_max_delta: 0.0,
                delta_history: vec![],
            };
        }

        let mut delta_history = Vec::with_capacity(self.config.max_cycles);
        let mut cycles = 0;
        let mut converged = false;
        let mut final_max_delta = f64::MAX;

        for _ in 0..self.config.max_cycles {
            cycles += 1;

            // Snapshot current activations for synchronous update
            let current: HashMap<UtteranceId, f64> = system
                .activations
                .iter()
                .map(|(&id, a)| (id, a.value))
                .collect();

            let mut max_delta: f64 = 0.0;

            // Update each scorable node
            for &uid in &scorable_ids {
                let old_value = current.get(&uid).copied().unwrap_or(0.0);

                // Compute neighbor influence: Σⱼ wᵢⱼ · Aₜ(uⱼ)
                let neighbor_sum: f64 = weights
                    .get(&uid)
                    .map(|neighbors| {
                        neighbors
                            .iter()
                            .map(|&(nid, w)| {
                                let n_val = current.get(&nid).copied().unwrap_or(0.0);
                                w * n_val
                            })
                            .sum()
                    })
                    .unwrap_or(0.0);

                // Settling rule: A_{t+1} = clip( (1-δ)·Aₜ + η·Σⱼ wᵢⱼ·Aₜ(uⱼ) )
                let new_value = (1.0 - self.config.decay) * old_value
                    + self.config.learning_rate * neighbor_sum;

                if let Some(activation) = system.activations.get_mut(&uid) {
                    activation.update(new_value); // update() clips to [-1, 1]
                }

                let delta = (new_value.clamp(-1.0, 1.0) - old_value).abs();
                if delta > max_delta {
                    max_delta = delta;
                }
            }

            delta_history.push(max_delta);
            final_max_delta = max_delta;

            if max_delta < self.config.epsilon {
                converged = true;
                break;
            }
        }

        // Compute global coherence from final activations
        let scorable_activations = system.scorable_activations();
        let gc =
            GlobalCoherence::compute(&scorable_activations, cycles, converged, final_max_delta);
        system.set_global_coherence(gc);

        // Compute principle scores
        let principle_scores = PrincipleScorer::score(system);
        system.set_principle_scores(principle_scores);

        SettlingResult {
            cycles,
            converged,
            final_max_delta,
            delta_history,
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use coherence_core::{
        relations::{CoherenceKind, IncoherenceKind},
        types::{ConversationId, MessageId, ParticipantId, Utterance, UtteranceKind},
        CoherenceRelation, IncoherenceRelation,
    };

    fn make_utterance(kind: UtteranceKind, content: &str) -> Utterance {
        Utterance::new(ParticipantId::new(), MessageId::new(), kind, content)
    }

    fn make_system_with_two_coherent() -> CoherenceSystem {
        let mut sys = CoherenceSystem::new(ConversationId::new());
        let u1 = make_utterance(UtteranceKind::Evidence, "Data shows X");
        let u2 = make_utterance(UtteranceKind::Explanation, "X because Y");
        let id1 = u1.id;
        let id2 = u2.id;
        sys.add_utterance(u1);
        sys.add_utterance(u2);
        sys.add_coherence(CoherenceRelation::new(id1, id2, CoherenceKind::Explains).unwrap())
            .unwrap();
        sys
    }

    #[test]
    fn empty_system_settles_immediately() {
        let engine = SettlingEngine::with_defaults();
        let mut sys = CoherenceSystem::new(ConversationId::new());
        let result = engine.settle(&mut sys);
        assert!(result.converged);
        assert_eq!(result.cycles, 0);
        assert!(sys.global_coherence.is_some());
        assert!((sys.global_coherence.as_ref().unwrap().score - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn two_coherent_utterances_settle_positively() {
        let engine = SettlingEngine::with_defaults();
        let mut sys = make_system_with_two_coherent();
        let result = engine.settle(&mut sys);

        assert!(result.converged);
        let gc = sys.global_coherence.as_ref().unwrap();
        assert!(
            gc.score > 0.0,
            "coherent pair should have positive score, got {}",
            gc.score
        );
        assert_eq!(gc.accepted_count, 2);
        assert_eq!(gc.rejected_count, 0);
    }

    #[test]
    fn contradicting_utterances_reduce_coherence() {
        let engine = SettlingEngine::with_defaults();
        let mut sys = CoherenceSystem::new(ConversationId::new());
        let u1 = make_utterance(UtteranceKind::Claim, "The sky is blue");
        let u2 = make_utterance(UtteranceKind::Claim, "The sky is green");
        let id1 = u1.id;
        let id2 = u2.id;
        sys.add_utterance(u1);
        sys.add_utterance(u2);
        sys.add_incoherence(
            IncoherenceRelation::new(id1, id2, IncoherenceKind::Contradicts).unwrap(),
        )
        .unwrap();

        let result = engine.settle(&mut sys);
        assert!(result.converged);

        let gc = sys.global_coherence.as_ref().unwrap();
        // With only incoherence and no evidence anchoring, score should be very low
        assert!(
            gc.score < 0.5,
            "contradicting pair should have low score, got {}",
            gc.score
        );
    }

    #[test]
    fn evidence_has_higher_activation() {
        let engine = SettlingEngine::with_defaults();
        let mut sys = CoherenceSystem::new(ConversationId::new());
        let evidence = make_utterance(UtteranceKind::Evidence, "Data: X=5");
        let claim = make_utterance(UtteranceKind::Claim, "Therefore Y");
        let eid = evidence.id;
        let cid = claim.id;
        sys.add_utterance(evidence);
        sys.add_utterance(claim);
        sys.add_coherence(CoherenceRelation::new(eid, cid, CoherenceKind::Supports).unwrap())
            .unwrap();

        engine.settle(&mut sys);

        let ev_act = sys.activation(eid).unwrap().value;
        let cl_act = sys.activation(cid).unwrap().value;

        // Evidence should maintain higher activation (data priority)
        assert!(
            ev_act > cl_act,
            "evidence activation ({ev_act}) should exceed claim ({cl_act})"
        );
    }

    #[test]
    fn mixed_coherence_and_incoherence() {
        let engine = SettlingEngine::with_defaults();
        let mut sys = CoherenceSystem::new(ConversationId::new());

        let e = make_utterance(UtteranceKind::Evidence, "Data shows X");
        let h1 = make_utterance(UtteranceKind::Explanation, "Hypothesis A explains X");
        let h2 = make_utterance(UtteranceKind::Explanation, "Hypothesis B explains X");

        let eid = e.id;
        let h1id = h1.id;
        let h2id = h2.id;
        sys.add_utterance(e);
        sys.add_utterance(h1);
        sys.add_utterance(h2);

        // Both hypotheses explain the evidence
        sys.add_coherence(CoherenceRelation::new(eid, h1id, CoherenceKind::Explains).unwrap())
            .unwrap();
        sys.add_coherence(CoherenceRelation::new(eid, h2id, CoherenceKind::Explains).unwrap())
            .unwrap();

        // But they compete with each other
        sys.add_incoherence(
            IncoherenceRelation::new(h1id, h2id, IncoherenceKind::Competes).unwrap(),
        )
        .unwrap();

        let result = engine.settle(&mut sys);
        assert!(result.converged);

        let gc = sys.global_coherence.as_ref().unwrap();
        // Should have moderate coherence — evidence is anchored but hypotheses compete
        assert!(gc.score > 0.0, "score should be positive, got {}", gc.score);

        // Evidence should be accepted
        assert!(sys.activation(eid).unwrap().is_accepted());
    }

    #[test]
    fn questions_and_procedural_excluded_from_scoring() {
        let engine = SettlingEngine::with_defaults();
        let mut sys = CoherenceSystem::new(ConversationId::new());

        let q = make_utterance(UtteranceKind::Question, "What about X?");
        let p = make_utterance(UtteranceKind::Procedural, "Let's move on");
        let c = make_utterance(UtteranceKind::Claim, "I think Y");

        sys.add_utterance(q);
        sys.add_utterance(p);
        sys.add_utterance(c);

        engine.settle(&mut sys);

        let gc = sys.global_coherence.as_ref().unwrap();
        // Only the claim should be counted
        assert_eq!(gc.utterance_count, 1);
    }

    #[test]
    fn settling_respects_max_cycles() {
        let config = SettlingConfig {
            epsilon: 1e-15, // impossibly tight convergence
            max_cycles: 5,
            ..Default::default()
        };
        let engine = SettlingEngine::new(config);

        // Build a larger system that won't converge in 5 cycles at this epsilon
        let mut sys = CoherenceSystem::new(ConversationId::new());
        let mut ids = Vec::new();
        for i in 0..10 {
            let u = make_utterance(UtteranceKind::Claim, &format!("Claim {i}"));
            ids.push(u.id);
            sys.add_utterance(u);
        }
        // Chain of coherence relations
        for i in 0..9 {
            sys.add_coherence(
                CoherenceRelation::new(ids[i], ids[i + 1], CoherenceKind::Supports).unwrap(),
            )
            .unwrap();
        }

        let result = engine.settle(&mut sys);
        assert_eq!(result.cycles, 5);
        assert!(!result.converged);
    }

    #[test]
    fn resolved_incoherence_has_less_impact() {
        let engine = SettlingEngine::with_defaults();

        // System with unresolved conflict
        let mut sys1 = CoherenceSystem::new(ConversationId::new());
        let u1 = make_utterance(UtteranceKind::Claim, "A");
        let u2 = make_utterance(UtteranceKind::Claim, "Not A");
        let id1 = u1.id;
        let id2 = u2.id;
        sys1.add_utterance(u1);
        sys1.add_utterance(u2);
        let rel = IncoherenceRelation::new(id1, id2, IncoherenceKind::Contradicts).unwrap();
        sys1.add_incoherence(rel).unwrap();

        // System with resolved conflict
        let mut sys2 = CoherenceSystem::new(ConversationId::new());
        let u3 = make_utterance(UtteranceKind::Claim, "A");
        let u4 = make_utterance(UtteranceKind::Claim, "Not A");
        let id3 = u3.id;
        let id4 = u4.id;
        sys2.add_utterance(u3);
        sys2.add_utterance(u4);
        let mut rel2 = IncoherenceRelation::new(id3, id4, IncoherenceKind::Contradicts).unwrap();
        rel2.resolve();
        sys2.add_incoherence(rel2).unwrap();

        engine.settle(&mut sys1);
        engine.settle(&mut sys2);

        let score1 = sys1.global_coherence.as_ref().unwrap().score;
        let score2 = sys2.global_coherence.as_ref().unwrap().score;

        assert!(
            score2 >= score1,
            "resolved conflict ({score2}) should score >= unresolved ({score1})"
        );
    }
}
