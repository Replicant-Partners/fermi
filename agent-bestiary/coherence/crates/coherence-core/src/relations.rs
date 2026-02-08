//! Coherence (R⁺) and Incoherence (R⁻) relations between utterances.
//!
//! In Thagard's TEC, propositions are linked by symmetric weighted relations:
//! - **Coherence relations (R⁺):** explanation links, acknowledgments, analogies
//! - **Incoherence relations (R⁻):** contradictions, unresolved competition
//!
//! These relations form the edges of the connectionist constraint-satisfaction
//! network. The settling algorithm propagates activation through positive
//! (coherence) edges and suppression through negative (incoherence) edges.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::{CoreError, CoreResult};
use crate::types::UtteranceId;

// ─── Relation Strength ─────────────────────────────────────────────────────

/// The strength (weight magnitude) of a relation, in the range (0, 1].
///
/// This is always positive — the sign is determined by the relation type
/// (coherence = positive weight, incoherence = negative weight).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RelationStrength(f64);

impl RelationStrength {
    /// The default strength for relations when no specific value is given.
    pub const DEFAULT: Self = Self(0.5);

    /// The maximum strength.
    pub const MAX: Self = Self(1.0);

    /// The minimum (weakest non-zero) strength.
    pub const MIN: Self = Self(0.01);

    /// Create a new relation strength, validating that it is in (0, 1].
    pub fn new(value: f64) -> CoreResult<Self> {
        if value <= 0.0 || value > 1.0 {
            return Err(CoreError::WeightOutOfRange(value));
        }
        Ok(Self(value))
    }

    /// Create a new relation strength, clamping to the valid range.
    /// Values <= 0 are clamped to MIN; values > 1 are clamped to 1.0.
    pub fn new_clamped(value: f64) -> Self {
        if value <= 0.0 {
            Self::MIN
        } else if value > 1.0 {
            Self::MAX
        } else {
            Self(value)
        }
    }

    /// Get the raw f64 value.
    pub fn value(&self) -> f64 {
        self.0
    }

    /// Returns the weight for use in the coherence network.
    /// For coherence relations, this is positive; for incoherence, negative.
    pub fn as_coherence_weight(&self) -> f64 {
        self.0
    }

    /// Returns the negative weight for use in the incoherence network.
    pub fn as_incoherence_weight(&self) -> f64 {
        -self.0
    }
}

impl Default for RelationStrength {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for RelationStrength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}

// ─── Coherence Relation Kinds ──────────────────────────────────────────────

/// The specific kind of coherence (R⁺) relation between two utterances.
///
/// Each kind corresponds to a mechanism by which two utterances positively
/// reinforce each other's acceptability in the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoherenceKind {
    /// One utterance explains the other (or they jointly explain some data).
    /// Maps to Thagard's Principle P2 (Explanation).
    Explains,

    /// One utterance provides evidence that supports the other.
    /// A specialized form of explanation.
    Supports,

    /// One utterance draws an analogy that illuminates the other.
    /// Maps to Thagard's Principle P3 (Analogy).
    Analogizes,

    /// One participant explicitly acknowledges or agrees with another's utterance.
    /// Creates bidirectional coherence (reinforces P1: Symmetry).
    Acknowledges,

    /// One utterance elaborates on, extends, or refines the other.
    Elaborates,

    /// One utterance provides a concrete example or instance of the other.
    Exemplifies,

    /// The two utterances are semantically similar or make the same point
    /// in different words.
    Restates,

    /// A generic coherence link when the specific kind is not determined.
    General,
}

impl CoherenceKind {
    /// Returns a human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            CoherenceKind::Explains => "explains",
            CoherenceKind::Supports => "supports",
            CoherenceKind::Analogizes => "analogizes",
            CoherenceKind::Acknowledges => "acknowledges",
            CoherenceKind::Elaborates => "elaborates",
            CoherenceKind::Exemplifies => "exemplifies",
            CoherenceKind::Restates => "restates",
            CoherenceKind::General => "general coherence",
        }
    }

    /// Returns the default strength for this kind of coherence relation.
    pub fn default_strength(&self) -> RelationStrength {
        match self {
            CoherenceKind::Explains => RelationStrength(0.7),
            CoherenceKind::Supports => RelationStrength(0.8),
            CoherenceKind::Analogizes => RelationStrength(0.5),
            CoherenceKind::Acknowledges => RelationStrength(0.6),
            CoherenceKind::Elaborates => RelationStrength(0.5),
            CoherenceKind::Exemplifies => RelationStrength(0.6),
            CoherenceKind::Restates => RelationStrength(0.4),
            CoherenceKind::General => RelationStrength(0.3),
        }
    }

    /// Returns which Thagard principle this kind primarily maps to, if any.
    pub fn primary_principle(&self) -> Option<crate::Principle> {
        use crate::Principle;
        match self {
            CoherenceKind::Explains => Some(Principle::Explanation),
            CoherenceKind::Supports => Some(Principle::Explanation),
            CoherenceKind::Analogizes => Some(Principle::Analogy),
            CoherenceKind::Acknowledges => Some(Principle::Symmetry),
            CoherenceKind::Elaborates => Some(Principle::Explanation),
            CoherenceKind::Exemplifies => Some(Principle::DataPriority),
            CoherenceKind::Restates => None,
            CoherenceKind::General => None,
        }
    }
}

impl fmt::Display for CoherenceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

// ─── Incoherence Relation Kinds ────────────────────────────────────────────

/// The specific kind of incoherence (R⁻) relation between two utterances.
///
/// Each kind corresponds to a mechanism by which two utterances suppress
/// each other's acceptability in the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncoherenceKind {
    /// Direct logical or factual contradiction.
    /// Maps to Thagard's Principle P5 (Contradiction).
    Contradicts,

    /// Two explanations compete to account for the same data, and neither
    /// subsumes the other.
    /// Maps to Thagard's Principle P6 (Competition).
    Competes,

    /// One utterance undermines or weakens the other without direct
    /// contradiction (e.g. casting doubt on methodology).
    Undermines,

    /// The two utterances are logically or semantically inconsistent but
    /// not directly contradictory (tension without opposition).
    Tensions,

    /// One utterance explicitly rejects or dismisses the other.
    Rejects,

    /// A generic incoherence link when the specific kind is not determined.
    General,
}

impl IncoherenceKind {
    /// Returns a human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            IncoherenceKind::Contradicts => "contradicts",
            IncoherenceKind::Competes => "competes",
            IncoherenceKind::Undermines => "undermines",
            IncoherenceKind::Tensions => "tensions",
            IncoherenceKind::Rejects => "rejects",
            IncoherenceKind::General => "general incoherence",
        }
    }

    /// Returns the default strength for this kind of incoherence relation.
    pub fn default_strength(&self) -> RelationStrength {
        match self {
            IncoherenceKind::Contradicts => RelationStrength(0.9),
            IncoherenceKind::Competes => RelationStrength(0.6),
            IncoherenceKind::Undermines => RelationStrength(0.5),
            IncoherenceKind::Tensions => RelationStrength(0.3),
            IncoherenceKind::Rejects => RelationStrength(0.8),
            IncoherenceKind::General => RelationStrength(0.3),
        }
    }

    /// Returns which Thagard principle this kind primarily maps to, if any.
    pub fn primary_principle(&self) -> Option<crate::Principle> {
        use crate::Principle;
        match self {
            IncoherenceKind::Contradicts => Some(Principle::Contradiction),
            IncoherenceKind::Competes => Some(Principle::Competition),
            IncoherenceKind::Undermines => Some(Principle::Contradiction),
            IncoherenceKind::Tensions => None,
            IncoherenceKind::Rejects => Some(Principle::Contradiction),
            IncoherenceKind::General => None,
        }
    }
}

impl fmt::Display for IncoherenceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

// ─── Coherence Relation (R⁺) ──────────────────────────────────────────────

/// A coherence relation (R⁺) between two utterances.
///
/// Coherence relations are **symmetric**: if A coheres with B, then B coheres
/// with A (Thagard's Principle 1). The `source` and `target` fields are
/// ordered only for provenance (which utterance prompted the detection of
/// the relation); the relation applies equally in both directions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoherenceRelation {
    /// The first utterance in the pair (typically the earlier one).
    pub source: UtteranceId,

    /// The second utterance in the pair (typically the later one).
    pub target: UtteranceId,

    /// The kind of coherence relation.
    pub kind: CoherenceKind,

    /// The strength of the relation.
    pub strength: RelationStrength,

    /// Confidence that this relation was correctly detected, in [0, 1].
    pub confidence: f64,

    /// Optional textual justification for why this relation was detected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub justification: Option<String>,
}

impl CoherenceRelation {
    /// Create a new coherence relation with the default strength for its kind.
    pub fn new(source: UtteranceId, target: UtteranceId, kind: CoherenceKind) -> CoreResult<Self> {
        if source == target {
            return Err(CoreError::SelfRelation(source.0));
        }
        Ok(Self {
            source,
            target,
            kind,
            strength: kind.default_strength(),
            confidence: 1.0,
            justification: None,
        })
    }

    /// Create a new coherence relation with explicit strength.
    pub fn with_strength(
        source: UtteranceId,
        target: UtteranceId,
        kind: CoherenceKind,
        strength: RelationStrength,
    ) -> CoreResult<Self> {
        if source == target {
            return Err(CoreError::SelfRelation(source.0));
        }
        Ok(Self {
            source,
            target,
            kind,
            strength,
            confidence: 1.0,
            justification: None,
        })
    }

    /// Set the confidence.
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set the justification.
    pub fn with_justification(mut self, justification: impl Into<String>) -> Self {
        self.justification = Some(justification.into());
        self
    }

    /// Returns the network weight for this relation (positive).
    /// Scaled by confidence: effective_weight = strength * confidence.
    pub fn network_weight(&self) -> f64 {
        self.strength.as_coherence_weight() * self.confidence
    }

    /// Returns `true` if this relation involves the given utterance.
    pub fn involves(&self, utterance_id: UtteranceId) -> bool {
        self.source == utterance_id || self.target == utterance_id
    }

    /// Returns the other utterance in the pair, given one of them.
    /// Returns `None` if the given utterance is not part of this relation.
    pub fn other(&self, utterance_id: UtteranceId) -> Option<UtteranceId> {
        if self.source == utterance_id {
            Some(self.target)
        } else if self.target == utterance_id {
            Some(self.source)
        } else {
            None
        }
    }

    /// Returns the canonical (sorted) pair of utterance IDs for deduplication.
    /// The pair is ordered by UUID bytes so that (A, B) and (B, A) produce
    /// the same canonical form.
    pub fn canonical_pair(&self) -> (UtteranceId, UtteranceId) {
        if self.source.0 <= self.target.0 {
            (self.source, self.target)
        } else {
            (self.target, self.source)
        }
    }
}

impl fmt::Display for CoherenceRelation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "R⁺({} ←{}→ {}, w={:.2})",
            self.source, self.kind, self.target, self.strength
        )
    }
}

// ─── Incoherence Relation (R⁻) ────────────────────────────────────────────

/// An incoherence relation (R⁻) between two utterances.
///
/// Like coherence relations, incoherence relations are **symmetric**.
/// The `source` and `target` fields are ordered for provenance only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncoherenceRelation {
    /// The first utterance in the pair.
    pub source: UtteranceId,

    /// The second utterance in the pair.
    pub target: UtteranceId,

    /// The kind of incoherence relation.
    pub kind: IncoherenceKind,

    /// The strength of the relation.
    pub strength: RelationStrength,

    /// Confidence that this relation was correctly detected, in [0, 1].
    pub confidence: f64,

    /// Whether this incoherence has been acknowledged or addressed by
    /// the participants. Resolved incoherence may have reduced impact.
    pub resolved: bool,

    /// Optional textual justification for why this relation was detected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub justification: Option<String>,
}

impl IncoherenceRelation {
    /// Create a new incoherence relation with the default strength for its kind.
    pub fn new(
        source: UtteranceId,
        target: UtteranceId,
        kind: IncoherenceKind,
    ) -> CoreResult<Self> {
        if source == target {
            return Err(CoreError::SelfRelation(source.0));
        }
        Ok(Self {
            source,
            target,
            kind,
            strength: kind.default_strength(),
            confidence: 1.0,
            resolved: false,
            justification: None,
        })
    }

    /// Create a new incoherence relation with explicit strength.
    pub fn with_strength(
        source: UtteranceId,
        target: UtteranceId,
        kind: IncoherenceKind,
        strength: RelationStrength,
    ) -> CoreResult<Self> {
        if source == target {
            return Err(CoreError::SelfRelation(source.0));
        }
        Ok(Self {
            source,
            target,
            kind,
            strength,
            confidence: 1.0,
            resolved: false,
            justification: None,
        })
    }

    /// Set the confidence.
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set the justification.
    pub fn with_justification(mut self, justification: impl Into<String>) -> Self {
        self.justification = Some(justification.into());
        self
    }

    /// Mark this incoherence as resolved.
    pub fn resolve(&mut self) {
        self.resolved = true;
    }

    /// Returns the network weight for this relation (negative).
    /// Scaled by confidence, and attenuated if resolved.
    ///
    /// Resolved incoherence has its weight reduced by 80% — it still exists
    /// but has much less impact on the coherence score.
    pub fn network_weight(&self) -> f64 {
        let base = self.strength.as_incoherence_weight() * self.confidence;
        if self.resolved {
            base * 0.2 // 80% attenuation when resolved
        } else {
            base
        }
    }

    /// Returns `true` if this relation involves the given utterance.
    pub fn involves(&self, utterance_id: UtteranceId) -> bool {
        self.source == utterance_id || self.target == utterance_id
    }

    /// Returns the other utterance in the pair, given one of them.
    pub fn other(&self, utterance_id: UtteranceId) -> Option<UtteranceId> {
        if self.source == utterance_id {
            Some(self.target)
        } else if self.target == utterance_id {
            Some(self.source)
        } else {
            None
        }
    }

    /// Returns the canonical (sorted) pair of utterance IDs.
    pub fn canonical_pair(&self) -> (UtteranceId, UtteranceId) {
        if self.source.0 <= self.target.0 {
            (self.source, self.target)
        } else {
            (self.target, self.source)
        }
    }
}

impl fmt::Display for IncoherenceRelation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "R⁻({} ←{}→ {}, w={:.2}{})",
            self.source,
            self.kind,
            self.target,
            self.strength,
            if self.resolved { ", resolved" } else { "" }
        )
    }
}

// ─── Relation Set ──────────────────────────────────────────────────────────

/// A collection of all relations in a coherence system, providing indexed
/// access and aggregate statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelationSet {
    /// All coherence (R⁺) relations.
    pub coherence: Vec<CoherenceRelation>,

    /// All incoherence (R⁻) relations.
    pub incoherence: Vec<IncoherenceRelation>,
}

impl RelationSet {
    /// Create an empty relation set.
    pub fn new() -> Self {
        Self {
            coherence: Vec::new(),
            incoherence: Vec::new(),
        }
    }

    /// Add a coherence relation.
    pub fn add_coherence(&mut self, relation: CoherenceRelation) {
        self.coherence.push(relation);
    }

    /// Add an incoherence relation.
    pub fn add_incoherence(&mut self, relation: IncoherenceRelation) {
        self.incoherence.push(relation);
    }

    /// Returns all coherence relations involving the given utterance.
    pub fn coherence_for(&self, utterance_id: UtteranceId) -> Vec<&CoherenceRelation> {
        self.coherence
            .iter()
            .filter(|r| r.involves(utterance_id))
            .collect()
    }

    /// Returns all incoherence relations involving the given utterance.
    pub fn incoherence_for(&self, utterance_id: UtteranceId) -> Vec<&IncoherenceRelation> {
        self.incoherence
            .iter()
            .filter(|r| r.involves(utterance_id))
            .collect()
    }

    /// Returns the total number of relations (coherence + incoherence).
    pub fn total_count(&self) -> usize {
        self.coherence.len() + self.incoherence.len()
    }

    /// Returns `true` if there are no relations at all.
    pub fn is_empty(&self) -> bool {
        self.coherence.is_empty() && self.incoherence.is_empty()
    }

    /// Returns the number of unresolved incoherence relations.
    pub fn unresolved_incoherence_count(&self) -> usize {
        self.incoherence.iter().filter(|r| !r.resolved).count()
    }

    /// Returns the coherence-to-incoherence ratio.
    /// Returns `f64::INFINITY` if there are no incoherence relations.
    /// Returns `0.0` if there are no coherence relations.
    pub fn coherence_ratio(&self) -> f64 {
        if self.incoherence.is_empty() {
            if self.coherence.is_empty() {
                0.0
            } else {
                f64::INFINITY
            }
        } else {
            self.coherence.len() as f64 / self.incoherence.len() as f64
        }
    }

    /// Compute the net weight affecting a specific utterance.
    /// Positive values indicate net coherence; negative indicate net incoherence.
    pub fn net_weight_for(&self, utterance_id: UtteranceId) -> f64 {
        let pos: f64 = self
            .coherence_for(utterance_id)
            .iter()
            .map(|r| r.network_weight())
            .sum();
        let neg: f64 = self
            .incoherence_for(utterance_id)
            .iter()
            .map(|r| r.network_weight())
            .sum();
        pos + neg // neg is already negative
    }

    /// Returns all coherence relations of a specific kind.
    pub fn coherence_of_kind(&self, kind: CoherenceKind) -> Vec<&CoherenceRelation> {
        self.coherence.iter().filter(|r| r.kind == kind).collect()
    }

    /// Returns all incoherence relations of a specific kind.
    pub fn incoherence_of_kind(&self, kind: IncoherenceKind) -> Vec<&IncoherenceRelation> {
        self.incoherence.iter().filter(|r| r.kind == kind).collect()
    }

    /// Clear all relations.
    pub fn clear(&mut self) {
        self.coherence.clear();
        self.incoherence.clear();
    }
}

impl fmt::Display for RelationSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "RelationSet: {} R⁺, {} R⁻ ({} unresolved)",
            self.coherence.len(),
            self.incoherence.len(),
            self.unresolved_incoherence_count()
        )?;
        write!(f, "  Coherence ratio: ")?;
        let ratio = self.coherence_ratio();
        if ratio.is_infinite() {
            writeln!(f, "∞ (no incoherence)")?;
        } else {
            writeln!(f, "{ratio:.2}")?;
        }
        Ok(())
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pair() -> (UtteranceId, UtteranceId) {
        (UtteranceId::new(), UtteranceId::new())
    }

    // ── RelationStrength ──

    #[test]
    fn strength_validation() {
        assert!(RelationStrength::new(0.5).is_ok());
        assert!(RelationStrength::new(1.0).is_ok());
        assert!(RelationStrength::new(0.01).is_ok());
        assert!(RelationStrength::new(0.0).is_err());
        assert!(RelationStrength::new(-0.1).is_err());
        assert!(RelationStrength::new(1.1).is_err());
    }

    #[test]
    fn strength_clamping() {
        let s = RelationStrength::new_clamped(0.0);
        assert!((s.value() - RelationStrength::MIN.value()).abs() < f64::EPSILON);

        let s = RelationStrength::new_clamped(-5.0);
        assert!((s.value() - RelationStrength::MIN.value()).abs() < f64::EPSILON);

        let s = RelationStrength::new_clamped(2.0);
        assert!((s.value() - 1.0).abs() < f64::EPSILON);

        let s = RelationStrength::new_clamped(0.7);
        assert!((s.value() - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn strength_weight_signs() {
        let s = RelationStrength::new(0.6).unwrap();
        assert!(s.as_coherence_weight() > 0.0);
        assert!(s.as_incoherence_weight() < 0.0);
        assert!((s.as_coherence_weight() + s.as_incoherence_weight()).abs() < f64::EPSILON);
    }

    #[test]
    fn strength_default() {
        let s = RelationStrength::default();
        assert!((s.value() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn strength_display() {
        let s = RelationStrength::new(0.75).unwrap();
        assert_eq!(format!("{s}"), "0.75");
    }

    // ── CoherenceKind ──

    #[test]
    fn coherence_kind_default_strengths_are_valid() {
        let kinds = [
            CoherenceKind::Explains,
            CoherenceKind::Supports,
            CoherenceKind::Analogizes,
            CoherenceKind::Acknowledges,
            CoherenceKind::Elaborates,
            CoherenceKind::Exemplifies,
            CoherenceKind::Restates,
            CoherenceKind::General,
        ];
        for kind in kinds {
            let s = kind.default_strength();
            assert!(s.value() > 0.0 && s.value() <= 1.0, "invalid for {kind}");
        }
    }

    #[test]
    fn coherence_kind_labels() {
        assert_eq!(CoherenceKind::Explains.label(), "explains");
        assert_eq!(CoherenceKind::General.label(), "general coherence");
    }

    // ── IncoherenceKind ──

    #[test]
    fn incoherence_kind_default_strengths_are_valid() {
        let kinds = [
            IncoherenceKind::Contradicts,
            IncoherenceKind::Competes,
            IncoherenceKind::Undermines,
            IncoherenceKind::Tensions,
            IncoherenceKind::Rejects,
            IncoherenceKind::General,
        ];
        for kind in kinds {
            let s = kind.default_strength();
            assert!(s.value() > 0.0 && s.value() <= 1.0, "invalid for {kind}");
        }
    }

    #[test]
    fn incoherence_kind_labels() {
        assert_eq!(IncoherenceKind::Contradicts.label(), "contradicts");
        assert_eq!(IncoherenceKind::General.label(), "general incoherence");
    }

    // ── CoherenceRelation ──

    #[test]
    fn coherence_relation_creation() {
        let (a, b) = make_pair();
        let rel = CoherenceRelation::new(a, b, CoherenceKind::Explains).unwrap();

        assert_eq!(rel.source, a);
        assert_eq!(rel.target, b);
        assert_eq!(rel.kind, CoherenceKind::Explains);
        assert!((rel.confidence - 1.0).abs() < f64::EPSILON);
        assert!(rel.justification.is_none());
    }

    #[test]
    fn coherence_relation_self_relation_rejected() {
        let a = UtteranceId::new();
        let result = CoherenceRelation::new(a, a, CoherenceKind::Explains);
        assert!(result.is_err());
    }

    #[test]
    fn coherence_relation_network_weight() {
        let (a, b) = make_pair();
        let rel = CoherenceRelation::new(a, b, CoherenceKind::Supports)
            .unwrap()
            .with_confidence(0.8);

        let expected = CoherenceKind::Supports.default_strength().value() * 0.8;
        assert!((rel.network_weight() - expected).abs() < 1e-10);
        assert!(rel.network_weight() > 0.0);
    }

    #[test]
    fn coherence_relation_involves_and_other() {
        let (a, b) = make_pair();
        let c = UtteranceId::new();
        let rel = CoherenceRelation::new(a, b, CoherenceKind::General).unwrap();

        assert!(rel.involves(a));
        assert!(rel.involves(b));
        assert!(!rel.involves(c));

        assert_eq!(rel.other(a), Some(b));
        assert_eq!(rel.other(b), Some(a));
        assert_eq!(rel.other(c), None);
    }
}
