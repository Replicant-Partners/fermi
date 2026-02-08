//! The top-level coherence system that assembles the formal tuple C = ⟨U, E, R⁺, R⁻, A, σ⟩.
//!
//! This module provides:
//! - [`CoherenceSystem`]: the complete state of a coherence evaluation for one conversation
//! - [`Activation`]: a single utterance's activation value in [-1, 1]
//! - [`GlobalCoherence`]: the computed Γ(C) score with metadata
//! - [`CoherenceSnapshot`]: a point-in-time snapshot of the full evaluation state

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

use crate::error::{CoreError, CoreResult};
use crate::principles::{PrincipleConfig, PrincipleScores};
use crate::relations::RelationSet;
use crate::types::{ConversationId, Utterance, UtteranceId, UtteranceKind};

// ─── Activation ────────────────────────────────────────────────────────────

/// The activation value for a single utterance node in the connectionist network.
///
/// Activation is bounded to [-1, 1]:
/// - **Positive activation** means the utterance is currently *accepted* by the network
/// - **Negative activation** means the utterance is currently *rejected*
/// - **Zero** means the utterance is undecided
///
/// Evidence nodes (Data Priority) start with positive intrinsic activation;
/// all other nodes start at a configurable initial value (typically 0.01).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Activation {
    /// The utterance this activation belongs to.
    pub utterance_id: UtteranceId,

    /// The current activation value in [-1, 1].
    pub value: f64,

    /// The activation value from the previous settling cycle.
    /// Used to detect convergence.
    pub previous: f64,
}

impl Activation {
    /// The default initial activation for non-evidence utterances.
    pub const DEFAULT_INITIAL: f64 = 0.01;

    /// The intrinsic activation for evidence utterances (Data Priority).
    pub const EVIDENCE_INITIAL: f64 = 0.5;

    /// Create a new activation for the given utterance, choosing the initial
    /// value based on whether the utterance is evidence.
    pub fn new(utterance_id: UtteranceId, is_evidence: bool) -> Self {
        let initial = if is_evidence {
            Self::EVIDENCE_INITIAL
        } else {
            Self::DEFAULT_INITIAL
        };
        Self {
            utterance_id,
            value: initial,
            previous: initial,
        }
    }

    /// Create a new activation with an explicit initial value.
    pub fn with_value(utterance_id: UtteranceId, value: f64) -> CoreResult<Self> {
        if !(-1.0..=1.0).contains(&value) {
            return Err(CoreError::ActivationOutOfRange(value));
        }
        Ok(Self {
            utterance_id,
            value,
            previous: value,
        })
    }

    /// Update the activation value for a new settling cycle.
    /// The previous value is saved, and the new value is clipped to [-1, 1].
    pub fn update(&mut self, new_value: f64) {
        self.previous = self.value;
        self.value = new_value.clamp(-1.0, 1.0);
    }

    /// Returns the absolute change from the previous cycle.
    /// Used to detect convergence.
    pub fn delta(&self) -> f64 {
        (self.value - self.previous).abs()
    }

    /// Returns `true` if this node has positive activation (accepted).
    pub fn is_accepted(&self) -> bool {
        self.value > 0.0
    }

    /// Returns `true` if this node has negative activation (rejected).
    pub fn is_rejected(&self) -> bool {
        self.value < 0.0
    }

    /// Returns the positive contribution of this activation to the global
    /// coherence score: max(0, value).
    pub fn positive_contribution(&self) -> f64 {
        self.value.max(0.0)
    }
}

impl fmt::Display for Activation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = if self.is_accepted() {
            "✓"
        } else if self.is_rejected() {
            "✗"
        } else {
            "?"
        };
        write!(f, "A({})={:.3} {status}", self.utterance_id, self.value)
    }
}

// ─── Global Coherence ──────────────────────────────────────────────────────

/// The global coherence score Γ(C) for the entire system, along with metadata.
///
/// Computed as:
/// ```text
/// Γ(C) = (1 / |U|) · Σᵢ max(0, A(uᵢ))
/// ```
///
/// This is the mean positive activation across all scorable utterances,
/// normalized to [0, 1].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalCoherence {
    /// The global coherence score in [0, 1].
    pub score: f64,

    /// The number of utterances that contributed to the score.
    pub utterance_count: usize,

    /// The number of utterances with positive activation (accepted).
    pub accepted_count: usize,

    /// The number of utterances with negative activation (rejected).
    pub rejected_count: usize,

    /// The number of settling cycles that were performed.
    pub settling_cycles: usize,

    /// Whether the network converged (max delta < epsilon).
    pub converged: bool,

    /// The maximum activation delta in the final cycle.
    pub final_max_delta: f64,

    /// When this score was computed.
    pub computed_at: DateTime<Utc>,
}

impl GlobalCoherence {
    /// Compute the global coherence from a set of activations.
    ///
    /// Only scorable utterances (not questions or procedural) are included.
    /// The `settling_cycles`, `converged`, and `final_max_delta` fields must
    /// be provided by the caller (the settling engine).
    pub fn compute(
        activations: &[Activation],
        settling_cycles: usize,
        converged: bool,
        final_max_delta: f64,
    ) -> Self {
        if activations.is_empty() {
            return Self {
                score: 0.0,
                utterance_count: 0,
                accepted_count: 0,
                rejected_count: 0,
                settling_cycles,
                converged,
                final_max_delta,
                computed_at: Utc::now(),
            };
        }

        let utterance_count = activations.len();
        let accepted_count = activations.iter().filter(|a| a.is_accepted()).count();
        let rejected_count = activations.iter().filter(|a| a.is_rejected()).count();

        let positive_sum: f64 = activations.iter().map(|a| a.positive_contribution()).sum();

        let score = positive_sum / utterance_count as f64;

        Self {
            score: score.clamp(0.0, 1.0),
            utterance_count,
            accepted_count,
            rejected_count,
            settling_cycles,
            converged,
            final_max_delta,
            computed_at: Utc::now(),
        }
    }

    /// Returns a qualitative label for this coherence level.
    pub fn quality_label(&self) -> &'static str {
        match self.score {
            s if s >= 0.8 => "excellent",
            s if s >= 0.6 => "good",
            s if s >= 0.4 => "moderate",
            s if s >= 0.2 => "weak",
            _ => "critical",
        }
    }

    /// Returns `true` if this score is below the critical threshold.
    pub fn is_critical(&self, threshold: f64) -> bool {
        self.score < threshold
    }

    /// Returns `true` if this score is above the "good" threshold.
    pub fn is_good(&self, threshold: f64) -> bool {
        self.score >= threshold
    }
}

impl fmt::Display for GlobalCoherence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Γ(C) = {:.3} ({}) | {}/{} accepted | {} cycles{}",
            self.score,
            self.quality_label(),
            self.accepted_count,
            self.utterance_count,
            self.settling_cycles,
            if self.converged {
                " [converged]"
            } else {
                " [max cycles reached]"
            }
        )
    }
}

// ─── Feedback Action ───────────────────────────────────────────────────────

/// The kind of feedback action the agent should take based on the evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackAction {
    /// Full intervention — systemic incoherence detected.
    /// Triggered when Γ(C) < θ_critical.
    FullIntervention,

    /// Targeted feedback for specific principles that are below threshold.
    /// Triggered when any σ(Pₖ) < θ_principle.
    TargetedFeedback {
        /// The principles that need attention.
        principles: Vec<String>,
    },

    /// Positive reinforcement — the conversation is coherent.
    /// Triggered when Γ(C) ≥ θ_good.
    PositiveReinforcement,

    /// Alert — coherence is degrading rapidly.
    /// Triggered when Γ(C) drops significantly between evaluations.
    CoherenceDeclining {
        /// The magnitude of the decline.
        decline: f64,
    },

    /// No action needed — coherence is in the acceptable middle range.
    NoAction,
}

impl fmt::Display for FeedbackAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FeedbackAction::FullIntervention => write!(f, "⚠ FULL INTERVENTION NEEDED"),
            FeedbackAction::TargetedFeedback { principles } => {
                write!(f, "⚡ Targeted feedback for: {}", principles.join(", "))
            }
            FeedbackAction::PositiveReinforcement => write!(f, "✅ Conversation is coherent"),
            FeedbackAction::CoherenceDeclining { decline } => {
                write!(f, "📉 Coherence declining by {decline:.2}")
            }
            FeedbackAction::NoAction => write!(f, "— No action needed"),
        }
    }
}

// ─── Coherence Snapshot ────────────────────────────────────────────────────

/// A point-in-time snapshot of the complete coherence evaluation state.
///
/// This is what gets serialized and sent over the API / protocol layer.
/// It captures everything needed to understand the current state of the
/// coherence evaluation for a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoherenceSnapshot {
    /// Unique identifier for this snapshot.
    pub id: Uuid,

    /// The conversation being evaluated.
    pub conversation_id: ConversationId,

    /// The global coherence score.
    pub global_coherence: GlobalCoherence,

    /// Per-principle scores.
    pub principle_scores: PrincipleScores,

    /// The recommended feedback action.
    pub feedback_action: FeedbackAction,

    /// Summary statistics about the utterance population.
    pub utterance_stats: UtteranceStats,

    /// Summary statistics about the relation population.
    pub relation_stats: RelationStats,

    /// Individual activation values (optional, can be large).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub activations: Vec<Activation>,

    /// When this snapshot was created.
    pub created_at: DateTime<Utc>,

    /// The previous global coherence score, if available (for trend detection).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_score: Option<f64>,
}

/// Summary statistics about utterances in the system.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UtteranceStats {
    pub total: usize,
    pub claims: usize,
    pub evidence: usize,
    pub explanations: usize,
    pub analogies: usize,
    pub questions: usize,
    pub acknowledgments: usize,
    pub procedural: usize,
    pub participant_count: usize,
}

impl UtteranceStats {
    /// Compute statistics from a slice of utterances.
    pub fn from_utterances(utterances: &[Utterance], participant_count: usize) -> Self {
        let mut stats = Self {
            total: utterances.len(),
            participant_count,
            ..Default::default()
        };
        for u in utterances {
            match u.kind {
                UtteranceKind::Claim => stats.claims += 1,
                UtteranceKind::Evidence => stats.evidence += 1,
                UtteranceKind::Explanation => stats.explanations += 1,
                UtteranceKind::Analogy => stats.analogies += 1,
                UtteranceKind::Question => stats.questions += 1,
                UtteranceKind::Acknowledgment => stats.acknowledgments += 1,
                UtteranceKind::Procedural => stats.procedural += 1,
            }
        }
        stats
    }

    /// Returns the number of scorable utterances (those that participate in
    /// coherence evaluation).
    pub fn scorable(&self) -> usize {
        self.total - self.questions - self.procedural
    }

    /// Returns the evidence density: fraction of scorable utterances that are evidence.
    pub fn evidence_density(&self) -> f64 {
        let scorable = self.scorable();
        if scorable == 0 {
            return 0.0;
        }
        self.evidence as f64 / scorable as f64
    }

    /// Returns the explanation density: fraction of scorable utterances that are explanations.
    pub fn explanation_density(&self) -> f64 {
        let scorable = self.scorable();
        if scorable == 0 {
            return 0.0;
        }
        self.explanations as f64 / scorable as f64
    }
}

/// Summary statistics about relations in the system.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelationStats {
    pub coherence_count: usize,
    pub incoherence_count: usize,
    pub unresolved_incoherence: usize,
    pub coherence_ratio: f64,
}

impl RelationStats {
    /// Compute statistics from a relation set.
    pub fn from_relations(relations: &RelationSet) -> Self {
        Self {
            coherence_count: relations.coherence.len(),
            incoherence_count: relations.incoherence.len(),
            unresolved_incoherence: relations.unresolved_incoherence_count(),
            coherence_ratio: relations.coherence_ratio(),
        }
    }
}

// ─── Coherence System ──────────────────────────────────────────────────────

/// The complete coherence system for a single conversation.
///
/// This is the runtime representation of the formal tuple C = ⟨U, E, R⁺, R⁻, A, σ⟩:
///
/// | Symbol | Field |
/// |--------|-------|
/// | **U** | `utterances` |
/// | **E** | utterances where `kind == Evidence` (subset of U) |
/// | **R⁺** | `relations.coherence` |
/// | **R⁻** | `relations.incoherence` |
/// | **A** | `activations` |
/// | **σ** | `principle_scores` |
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoherenceSystem {
    /// The conversation this system evaluates.
    pub conversation_id: ConversationId,

    /// All utterance-propositions extracted from the conversation (set U).
    pub utterances: Vec<Utterance>,

    /// All coherence and incoherence relations (R⁺ and R⁻).
    pub relations: RelationSet,

    /// Current activation values for each utterance (A).
    /// Indexed by UtteranceId for O(1) lookup during settling.
    pub activations: HashMap<UtteranceId, Activation>,

    /// The most recently computed principle scores (σ).
    pub principle_scores: PrincipleScores,

    /// The most recently computed global coherence score Γ(C).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_coherence: Option<GlobalCoherence>,

    /// Configuration for principle evaluation.
    pub config: PrincipleConfig,

    /// When this system was last evaluated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_evaluated_at: Option<DateTime<Utc>>,

    /// History of global coherence scores (for trend detection).
    /// Most recent is last.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub score_history: Vec<f64>,
}

impl CoherenceSystem {
    /// Create a new empty coherence system for the given conversation.
    pub fn new(conversation_id: ConversationId) -> Self {
        Self {
            conversation_id,
            utterances: Vec::new(),
            relations: RelationSet::new(),
            activations: HashMap::new(),
            principle_scores: PrincipleScores::new(),
            global_coherence: None,
            config: PrincipleConfig::default(),
            last_evaluated_at: None,
            score_history: Vec::new(),
        }
    }

    /// Create a new coherence system with custom configuration.
    pub fn with_config(conversation_id: ConversationId, config: PrincipleConfig) -> Self {
        Self {
            config,
            ..Self::new(conversation_id)
        }
    }

    // ── Utterance management ──

    /// Add an utterance to the system.
    /// Automatically initializes its activation value.
    pub fn add_utterance(&mut self, utterance: Utterance) {
        let id = utterance.id;
        let is_evidence = utterance.is_evidence();
        self.utterances.push(utterance);
        self.activations
            .insert(id, Activation::new(id, is_evidence));
    }

    /// Get an utterance by ID.
    pub fn utterance(&self, id: UtteranceId) -> Option<&Utterance> {
        self.utterances.iter().find(|u| u.id == id)
    }

    /// Returns the evidence subset E ⊆ U.
    pub fn evidence_set(&self) -> Vec<&Utterance> {
        self.utterances.iter().filter(|u| u.is_evidence()).collect()
    }

    /// Returns only the scorable utterances (excluding questions and procedural).
    pub fn scorable_utterances(&self) -> Vec<&Utterance> {
        self.utterances.iter().filter(|u| u.is_scorable()).collect()
    }

    /// Returns the activation for a specific utterance.
    pub fn activation(&self, id: UtteranceId) -> Option<&Activation> {
        self.activations.get(&id)
    }

    /// Returns all activations as a sorted vector (by utterance ID).
    pub fn all_activations(&self) -> Vec<&Activation> {
        let mut activations: Vec<&Activation> = self.activations.values().collect();
        activations.sort_by(|a, b| a.utterance_id.0.cmp(&b.utterance_id.0));
        activations
    }

    /// Returns only activations for scorable utterances.
    pub fn scorable_activations(&self) -> Vec<Activation> {
        let scorable_ids: std::collections::HashSet<UtteranceId> =
            self.scorable_utterances().iter().map(|u| u.id).collect();

        self.activations
            .values()
            .filter(|a| scorable_ids.contains(&a.utterance_id))
            .copied()
            .collect()
    }

    // ── Relation management ──

    /// Add a coherence relation.
    pub fn add_coherence(
        &mut self,
        relation: crate::relations::CoherenceRelation,
    ) -> CoreResult<()> {
        // Validate that both utterances exist
        if self.utterance(relation.source).is_none() {
            return Err(CoreError::DanglingRelation(relation.source.0));
        }
        if self.utterance(relation.target).is_none() {
            return Err(CoreError::DanglingRelation(relation.target.0));
        }
        self.relations.add_coherence(relation);
        Ok(())
    }

    /// Add an incoherence relation.
    pub fn add_incoherence(
        &mut self,
        relation: crate::relations::IncoherenceRelation,
    ) -> CoreResult<()> {
        // Validate that both utterances exist
        if self.utterance(relation.source).is_none() {
            return Err(CoreError::DanglingRelation(relation.source.0));
        }
        if self.utterance(relation.target).is_none() {
            return Err(CoreError::DanglingRelation(relation.target.0));
        }
        self.relations.add_incoherence(relation);
        Ok(())
    }

    // ── Evaluation state ──

    /// Update the principle scores after a settling run.
    pub fn set_principle_scores(&mut self, scores: PrincipleScores) {
        self.principle_scores = scores;
    }

    /// Update the global coherence score after a settling run.
    pub fn set_global_coherence(&mut self, coherence: GlobalCoherence) {
        // Save to history for trend detection
        self.score_history.push(coherence.score);

        // Keep history bounded (last 100 evaluations)
        if self.score_history.len() > 100 {
            self.score_history.remove(0);
        }

        self.global_coherence = Some(coherence);
        self.last_evaluated_at = Some(Utc::now());
    }

    /// Determine the feedback action based on current scores and configuration.
    pub fn determine_feedback_action(&self) -> FeedbackAction {
        let Some(ref gc) = self.global_coherence else {
            return FeedbackAction::NoAction;
        };

        // Check for rapid decline
        if self.score_history.len() >= 2 {
            let prev = self.score_history[self.score_history.len() - 2];
            let decline = prev - gc.score;
            if decline > 0.15 {
                return FeedbackAction::CoherenceDeclining { decline };
            }
        }

        // Check global thresholds
        if gc.is_critical(self.config.critical_threshold) {
            return FeedbackAction::FullIntervention;
        }

        if gc.is_good(self.config.good_threshold) {
            return FeedbackAction::PositiveReinforcement;
        }

        // Check individual principle thresholds
        let weak_principles = self
            .principle_scores
            .below_threshold(self.config.principle_threshold);

        if !weak_principles.is_empty() {
            let principle_names: Vec<String> = weak_principles
                .iter()
                .map(|s| s.principle.label())
                .collect();
            return FeedbackAction::TargetedFeedback {
                principles: principle_names,
            };
        }

        FeedbackAction::NoAction
    }

    /// Create a snapshot of the current evaluation state.
    pub fn snapshot(&self) -> CoherenceSnapshot {
        let global_coherence = self
            .global_coherence
            .clone()
            .unwrap_or_else(|| GlobalCoherence::compute(&[], 0, false, 0.0));

        let previous_score = if self.score_history.len() >= 2 {
            Some(self.score_history[self.score_history.len() - 2])
        } else {
            None
        };

        CoherenceSnapshot {
            id: Uuid::new_v4(),
            conversation_id: self.conversation_id,
            global_coherence,
            principle_scores: self.principle_scores.clone(),
            feedback_action: self.determine_feedback_action(),
            utterance_stats: UtteranceStats::from_utterances(
                &self.utterances,
                self.utterances
                    .iter()
                    .map(|u| u.participant_id)
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
            ),
            relation_stats: RelationStats::from_relations(&self.relations),
            activations: self.scorable_activations(),
            created_at: Utc::now(),
            previous_score,
        }
    }

    // ── Statistics ──

    /// Returns `true` if the system has been evaluated at least once.
    pub fn is_evaluated(&self) -> bool {
        self.global_coherence.is_some()
    }

    /// Returns `true` if the system is empty (no utterances).
    pub fn is_empty(&self) -> bool {
        self.utterances.is_empty()
    }

    /// Returns the number of utterances.
    pub fn utterance_count(&self) -> usize {
        self.utterances.len()
    }

    /// Returns the number of scorable utterances.
    pub fn scorable_count(&self) -> usize {
        self.scorable_utterances().len()
    }

    /// Returns the total number of relations.
    pub fn relation_count(&self) -> usize {
        self.relations.total_count()
    }

    /// Returns the coherence trend over the last N evaluations.
    /// Positive means improving, negative means degrading.
    pub fn trend(&self, window: usize) -> Option<f64> {
        if self.score_history.len() < 2 {
            return None;
        }
        let history = &self.score_history;
        let start = if history.len() > window {
            history.len() - window
        } else {
            0
        };
        let slice = &history[start..];
        if slice.len() < 2 {
            return None;
        }
        let first = slice[0];
        let last = slice[slice.len() - 1];
        Some(last - first)
    }
}

impl fmt::Display for CoherenceSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "CoherenceSystem for {}", self.conversation_id)?;
        writeln!(
            f,
            "  Utterances: {} ({} scorable)",
            self.utterance_count(),
            self.scorable_count()
        )?;
        writeln!(f, "  Relations:  {}", self.relations)?;
        if let Some(ref gc) = self.global_coherence {
            writeln!(f, "  {gc}")?;
        } else {
            writeln!(f, "  (not yet evaluated)")?;
        }
        if !self.principle_scores.is_empty() {
            writeln!(f, "  Principle scores:")?;
            write!(f, "{}", self.principle_scores)?;
        }
        Ok(())
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    // ── Activation ──

    #[test]
    fn activation_new_non_evidence() {
        let uid = UtteranceId::new();
        let a = Activation::new(uid, false);
        assert!((a.value - Activation::DEFAULT_INITIAL).abs() < f64::EPSILON);
        assert!(a.is_accepted()); // 0.01 > 0
    }

    #[test]
    fn activation_new_evidence() {
        let uid = UtteranceId::new();
        let a = Activation::new(uid, true);
        assert!((a.value - Activation::EVIDENCE_INITIAL).abs() < f64::EPSILON);
    }

    #[test]
    fn activation_with_value_validation() {
        let uid = UtteranceId::new();
        assert!(Activation::with_value(uid, 0.5).is_ok());
        assert!(Activation::with_value(uid, -1.0).is_ok());
        assert!(Activation::with_value(uid, 1.0).is_ok());
        assert!(Activation::with_value(uid, 1.1).is_err());
        assert!(Activation::with_value(uid, -1.1).is_err());
    }

    #[test]
    fn activation_update_and_delta() {
        let uid = UtteranceId::new();
        let mut a = Activation::new(uid, false);

        a.update(0.5);
        assert!((a.value - 0.5).abs() < f64::EPSILON);
        assert!((a.previous - Activation::DEFAULT_INITIAL).abs() < f64::EPSILON);
        assert!((a.delta() - (0.5 - Activation::DEFAULT_INITIAL)).abs() < f64::EPSILON);

        a.update(0.8);
        assert!((a.value - 0.8).abs() < f64::EPSILON);
        assert!((a.previous - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn activation_update_clips() {
        let uid = UtteranceId::new();
        let mut a = Activation::new(uid, false);

        a.update(5.0);
        assert!((a.value - 1.0).abs() < f64::EPSILON);

        a.update(-5.0);
        assert!((a.value - (-1.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn activation_positive_contribution() {
        let uid = UtteranceId::new();
        let mut a = Activation::new(uid, false);

        a.update(0.7);
        assert!((a.positive_contribution() - 0.7).abs() < f64::EPSILON);

        a.update(-0.3);
        assert!(a.positive_contribution().abs() < f64::EPSILON);
    }

    #[test]
    fn activation_display() {
        let uid = UtteranceId::new();
        let a = Activation::new(uid, false);
        let display = format!("{a}");
        assert!(display.contains("0.010"));
        assert!(display.contains("✓"));
    }
}
