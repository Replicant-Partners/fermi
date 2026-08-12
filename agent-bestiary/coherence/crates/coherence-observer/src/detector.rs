//! Heuristic relation detection between utterances.
//!
//! Detects coherence (R⁺) and incoherence (R⁻) relations between pairs
//! of utterances based on textual cues and structural patterns.

use coherence_core::{
    relations::{CoherenceKind, IncoherenceKind},
    types::{Utterance, UtteranceKind},
    CoherenceRelation, IncoherenceRelation,
};
use regex::Regex;
use std::sync::LazyLock;

/// Detects relations between pairs of utterances.
pub struct RelationDetector;

/// A detected relation (either coherence or incoherence).
#[derive(Debug)]
pub enum DetectedRelation {
    Coherence(CoherenceRelation),
    Incoherence(IncoherenceRelation),
}

// Patterns for contradiction detection
static RE_NEGATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(I\s+disagree|that'?s?\s+(not|wrong|incorrect)|no,?\s|not\s+true|actually,?\s+no|on\s+the\s+contrary|but\s+that'?s?\s+not)").unwrap()
});

// Patterns for support/explanation detection
static RE_BUILDING_ON: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(building\s+on|to\s+add\s+to|expanding\s+on|adding\s+to\s+that|furthermore|moreover|also|in\s+addition)").unwrap()
});

impl RelationDetector {
    /// Detect all relations between a set of utterances.
    ///
    /// Examines every pair (i, j) where j comes after i in the list.
    /// Returns detected relations with confidence scores.
    pub fn detect_all(utterances: &[Utterance]) -> Vec<DetectedRelation> {
        let mut relations = Vec::new();

        for (i, u1) in utterances.iter().enumerate() {
            for (offset, u2) in utterances.iter().skip(i + 1).enumerate() {
                let distance = offset + 1;
                // Topical gate. Without it the cue-based rules below, which
                // inspect only the later utterance, relate it to every
                // utterance that precedes it — producing an exact
                // claims×evidence cross-product and an edge density above
                // 50%. See [`crate::relevance`].
                if !coherence_core::relevance::is_relevant(u1, u2, distance) {
                    continue;
                }
                relations.extend(Self::detect_pair(u1, u2));
            }
        }

        relations
    }

    /// Detect relations between two specific utterances.
    fn detect_pair(earlier: &Utterance, later: &Utterance) -> Vec<DetectedRelation> {
        let mut results = Vec::new();

        // Skip non-scorable utterances as relation endpoints
        if !earlier.is_scorable() || !later.is_scorable() {
            return results;
        }

        // Acknowledgment: later utterance acknowledges earlier
        if later.kind == UtteranceKind::Acknowledgment {
            if let Ok(rel) =
                CoherenceRelation::new(earlier.id, later.id, CoherenceKind::Acknowledges)
            {
                results.push(DetectedRelation::Coherence(rel.with_confidence(0.8)));
            }
        }

        // Explanation → Evidence: explanation supports evidence
        if earlier.kind == UtteranceKind::Evidence && later.kind == UtteranceKind::Explanation {
            if let Ok(rel) = CoherenceRelation::new(earlier.id, later.id, CoherenceKind::Explains) {
                results.push(DetectedRelation::Coherence(rel.with_confidence(0.7)));
            }
        }

        // Evidence → Claim: evidence supports claim
        if earlier.kind == UtteranceKind::Claim && later.kind == UtteranceKind::Evidence {
            if let Ok(rel) = CoherenceRelation::new(earlier.id, later.id, CoherenceKind::Supports) {
                results.push(DetectedRelation::Coherence(rel.with_confidence(0.6)));
            }
        }
        if earlier.kind == UtteranceKind::Evidence && later.kind == UtteranceKind::Claim {
            if let Ok(rel) = CoherenceRelation::new(earlier.id, later.id, CoherenceKind::Supports) {
                results.push(DetectedRelation::Coherence(rel.with_confidence(0.6)));
            }
        }

        // Analogy creates coherence with anything nearby
        if later.kind == UtteranceKind::Analogy {
            if let Ok(rel) = CoherenceRelation::new(earlier.id, later.id, CoherenceKind::Analogizes)
            {
                results.push(DetectedRelation::Coherence(rel.with_confidence(0.7)));
            }
        }

        // Building-on text pattern → Elaborates
        if RE_BUILDING_ON.is_match(&later.content) {
            if let Ok(rel) = CoherenceRelation::new(earlier.id, later.id, CoherenceKind::Elaborates)
            {
                results.push(DetectedRelation::Coherence(rel.with_confidence(0.6)));
            }
        }

        // Contradiction: negation patterns in the later utterance
        if RE_NEGATION.is_match(&later.content) {
            // Check if both are claims (most likely to actually contradict)
            if earlier.kind == UtteranceKind::Claim && later.kind == UtteranceKind::Claim {
                if let Ok(rel) =
                    IncoherenceRelation::new(earlier.id, later.id, IncoherenceKind::Contradicts)
                {
                    results.push(DetectedRelation::Incoherence(rel.with_confidence(0.5)));
                }
            }
        }

        // Competing explanations: two explanations for what might be the same thing
        if earlier.kind == UtteranceKind::Explanation && later.kind == UtteranceKind::Explanation {
            // Only flag as competing if the later one has negation cues
            if RE_NEGATION.is_match(&later.content) {
                if let Ok(rel) =
                    IncoherenceRelation::new(earlier.id, later.id, IncoherenceKind::Competes)
                {
                    results.push(DetectedRelation::Incoherence(rel.with_confidence(0.4)));
                }
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coherence_core::types::{MessageId, ParticipantId};

    fn make_utterance(kind: UtteranceKind, content: &str) -> Utterance {
        Utterance::new(ParticipantId::new(), MessageId::new(), kind, content)
    }

    #[test]
    fn detects_acknowledgment_relation() {
        let u1 = make_utterance(UtteranceKind::Claim, "We should use Rust");
        let u2 = make_utterance(UtteranceKind::Acknowledgment, "I agree with that");

        let rels = RelationDetector::detect_pair(&u1, &u2);
        assert!(!rels.is_empty());
        assert!(matches!(rels[0], DetectedRelation::Coherence(_)));
    }

    #[test]
    fn detects_evidence_supports_claim() {
        let u1 = make_utterance(UtteranceKind::Claim, "Performance is critical");
        let u2 = make_utterance(UtteranceKind::Evidence, "Data shows 50ms latency");

        let rels = RelationDetector::detect_pair(&u1, &u2);
        assert!(!rels.is_empty());
        assert!(matches!(rels[0], DetectedRelation::Coherence(_)));
    }

    #[test]
    fn detects_contradiction() {
        let u1 = make_utterance(UtteranceKind::Claim, "We should use microservices");
        let u2 = make_utterance(UtteranceKind::Claim, "I disagree, monolith is better");

        let rels = RelationDetector::detect_pair(&u1, &u2);
        let has_incoherence = rels
            .iter()
            .any(|r| matches!(r, DetectedRelation::Incoherence(_)));
        assert!(has_incoherence, "should detect contradiction");
    }

    #[test]
    fn skips_non_scorable() {
        let u1 = make_utterance(UtteranceKind::Question, "What do you think?");
        let u2 = make_utterance(UtteranceKind::Claim, "I think X");

        let rels = RelationDetector::detect_pair(&u1, &u2);
        assert!(
            rels.is_empty(),
            "questions should not be relation endpoints"
        );
    }

    #[test]
    fn detects_elaboration() {
        let u1 = make_utterance(UtteranceKind::Claim, "We need better caching");
        let u2 = make_utterance(
            UtteranceKind::Claim,
            "Adding to that, we should also use CDN",
        );

        let rels = RelationDetector::detect_pair(&u1, &u2);
        let has_elaboration = rels.iter().any(|r| {
            if let DetectedRelation::Coherence(cr) = r {
                cr.kind == CoherenceKind::Elaborates
            } else {
                false
            }
        });
        assert!(has_elaboration, "should detect elaboration");
    }
}
