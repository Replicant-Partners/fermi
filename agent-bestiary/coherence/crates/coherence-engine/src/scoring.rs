//! Principle scoring — compute σ(Pₖ) for each of Thagard's seven principles.
//!
//! Each principle score is in [0, 1] where higher is always better.
//! For "positive" principles (Symmetry, Explanation, Analogy, DataPriority,
//! Acceptability), 1.0 means fully satisfied. For "negative" principles
//! (Contradiction, Competition), 1.0 means the problematic pattern is absent.

use coherence_core::{
    relations::{CoherenceKind, IncoherenceKind},
    CoherenceSystem, Principle, PrincipleScore, PrincipleScores,
};

/// Computes principle-level scores from a settled [`CoherenceSystem`].
pub struct PrincipleScorer;

impl PrincipleScorer {
    /// Score all seven principles and return a [`PrincipleScores`] collection.
    pub fn score(system: &CoherenceSystem) -> PrincipleScores {
        let mut scores = PrincipleScores::new();
        for p in Principle::ALL {
            if let Ok(ps) = Self::score_principle(system, p) {
                scores.add(ps);
            }
        }
        scores
    }

    fn score_principle(
        system: &CoherenceSystem,
        principle: Principle,
    ) -> Result<PrincipleScore, coherence_core::CoreError> {
        let (value, diagnostic) = match principle {
            Principle::Symmetry => Self::score_symmetry(system),
            Principle::Explanation => Self::score_explanation(system),
            Principle::Analogy => Self::score_analogy(system),
            Principle::DataPriority => Self::score_data_priority(system),
            Principle::Contradiction => Self::score_contradiction(system),
            Principle::Competition => Self::score_competition(system),
            Principle::Acceptability => Self::score_acceptability(system),
        };

        let mut ps = PrincipleScore::new(principle, value)?;
        if let Some(msg) = diagnostic {
            ps = ps.with_diagnostic(msg);
        }
        Ok(ps)
    }

    /// P1 — Symmetry: Measures bidirectional engagement.
    ///
    /// Score = fraction of utterance pairs that have at least one
    /// acknowledgment-type coherence relation between them. Higher means
    /// participants are engaging with each other's contributions.
    fn score_symmetry(system: &CoherenceSystem) -> (f64, Option<String>) {
        let ack_count = system
            .relations
            .coherence_of_kind(CoherenceKind::Acknowledges)
            .len();
        let scorable = system.scorable_count();

        if scorable <= 1 {
            return (
                1.0,
                Some("Single or no utterances — symmetry trivially satisfied".into()),
            );
        }

        // Ratio of acknowledgments to scorable utterances, capped at 1.0
        let ratio = (ack_count as f64 / scorable as f64).min(1.0);

        let diag = if ratio < 0.2 {
            Some("Low mutual acknowledgment between participants".into())
        } else if ratio >= 0.6 {
            Some("Strong bidirectional engagement".into())
        } else {
            None
        };

        (ratio, diag)
    }

    /// P2 — Explanation: Measures reasoning chain density.
    ///
    /// Score = fraction of scorable utterances that participate in at least
    /// one Explains/Supports/Elaborates relation.
    fn score_explanation(system: &CoherenceSystem) -> (f64, Option<String>) {
        let scorable = system.scorable_utterances();
        if scorable.is_empty() {
            return (0.0, Some("No scorable utterances".into()));
        }

        let explanation_kinds = [
            CoherenceKind::Explains,
            CoherenceKind::Supports,
            CoherenceKind::Elaborates,
        ];

        let participating: usize = scorable
            .iter()
            .filter(|u| {
                let rels = system.relations.coherence_for(u.id);
                rels.iter().any(|r| explanation_kinds.contains(&r.kind))
            })
            .count();

        let ratio = participating as f64 / scorable.len() as f64;

        let diag = if ratio < 0.3 {
            Some("Few utterances connected by reasoning chains".into())
        } else if ratio >= 0.7 {
            Some("Rich explanation structure".into())
        } else {
            None
        };

        (ratio, diag)
    }

    /// P3 — Analogy: Measures use of structural parallels.
    ///
    /// Score is based on the presence of analogy relations. Even a few
    /// analogies are valuable, so we use a saturating function.
    fn score_analogy(system: &CoherenceSystem) -> (f64, Option<String>) {
        let scorable = system.scorable_count();
        if scorable == 0 {
            return (0.5, Some("No utterances to evaluate".into()));
        }

        let analogy_count = system
            .relations
            .coherence_of_kind(CoherenceKind::Analogizes)
            .len();

        if analogy_count == 0 {
            return (0.5, Some("No analogies detected (neutral)".into()));
        }

        // Saturating: 1 analogy per 3 utterances → full score
        let ratio = (analogy_count as f64 / (scorable as f64 / 3.0)).min(1.0);

        let diag = Some(format!(
            "{analogy_count} analog{} detected",
            if analogy_count == 1 { "y" } else { "ies" }
        ));

        (ratio, diag)
    }

    /// P4 — Data Priority: Measures evidence anchoring.
    ///
    /// Score = fraction of evidence utterances that have positive activation
    /// (are accepted by the network). Evidence should be inherently credible.
    fn score_data_priority(system: &CoherenceSystem) -> (f64, Option<String>) {
        let evidence: Vec<_> = system.evidence_set();
        if evidence.is_empty() {
            return (0.3, Some("No evidence utterances in the discourse".into()));
        }

        let accepted_evidence = evidence
            .iter()
            .filter(|e| {
                system
                    .activation(e.id)
                    .map(|a| a.is_accepted())
                    .unwrap_or(false)
            })
            .count();

        let ratio = accepted_evidence as f64 / evidence.len() as f64;

        let diag = if ratio < 0.5 {
            Some("Evidence is not well-anchored in the discourse".into())
        } else if ratio >= 0.9 {
            Some("Evidence strongly anchors the discourse".into())
        } else {
            None
        };

        (ratio, diag)
    }

    /// P5 — Contradiction: Measures absence of unresolved contradictions.
    ///
    /// Score = 1.0 means no contradictions. Lower means more unresolved
    /// contradictions relative to the system size.
    fn score_contradiction(system: &CoherenceSystem) -> (f64, Option<String>) {
        let scorable = system.scorable_count();
        if scorable == 0 {
            return (1.0, Some("No utterances to evaluate".into()));
        }

        let contradiction_count = system
            .relations
            .incoherence_of_kind(IncoherenceKind::Contradicts)
            .iter()
            .filter(|r| !r.resolved)
            .count();

        if contradiction_count == 0 {
            return (1.0, Some("No unresolved contradictions".into()));
        }

        // Each contradiction penalizes the score. Max possible contradictions
        // is n*(n-1)/2 for n utterances.
        let max_possible = scorable * (scorable - 1) / 2;
        let max_possible = max_possible.max(1);
        let ratio = 1.0 - (contradiction_count as f64 / max_possible as f64).min(1.0);

        let diag = Some(format!(
            "{contradiction_count} unresolved contradiction{}",
            if contradiction_count == 1 { "" } else { "s" }
        ));

        (ratio, diag)
    }

    /// P6 — Competition: Measures absence of unresolved rivalry.
    ///
    /// Score = 1.0 means no competing explanations. Lower means more
    /// unresolved competition.
    fn score_competition(system: &CoherenceSystem) -> (f64, Option<String>) {
        let scorable = system.scorable_count();
        if scorable == 0 {
            return (1.0, Some("No utterances to evaluate".into()));
        }

        let competition_count = system
            .relations
            .incoherence_of_kind(IncoherenceKind::Competes)
            .iter()
            .filter(|r| !r.resolved)
            .count();

        if competition_count == 0 {
            return (1.0, Some("No unresolved competing explanations".into()));
        }

        let max_possible = scorable * (scorable - 1) / 2;
        let max_possible = max_possible.max(1);
        let ratio = 1.0 - (competition_count as f64 / max_possible as f64).min(1.0);

        let diag = Some(format!(
            "{competition_count} competing explanation pair{}",
            if competition_count == 1 { "" } else { "s" }
        ));

        (ratio, diag)
    }

    /// P7 — Acceptability: Measures overall network fit.
    ///
    /// Score = fraction of scorable utterances with positive activation.
    /// This is essentially a per-node view of global coherence.
    fn score_acceptability(system: &CoherenceSystem) -> (f64, Option<String>) {
        let scorable = system.scorable_utterances();
        if scorable.is_empty() {
            return (0.0, Some("No scorable utterances".into()));
        }

        let accepted = scorable
            .iter()
            .filter(|u| {
                system
                    .activation(u.id)
                    .map(|a| a.is_accepted())
                    .unwrap_or(false)
            })
            .count();

        let ratio = accepted as f64 / scorable.len() as f64;

        let diag = if ratio < 0.5 {
            Some("Many utterances not fitting the overall discourse".into())
        } else if ratio >= 0.8 {
            Some("Strong overall fit — most contributions are accepted".into())
        } else {
            None
        };

        (ratio, diag)
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

    #[test]
    fn empty_system_scores() {
        let sys = CoherenceSystem::new(ConversationId::new());
        let scores = PrincipleScorer::score(&sys);
        // Should produce scores for all 7 principles
        assert_eq!(scores.len(), 7);
    }

    #[test]
    fn explanation_score_increases_with_relations() {
        let mut sys = CoherenceSystem::new(ConversationId::new());
        let u1 = make_utterance(UtteranceKind::Evidence, "Data X");
        let u2 = make_utterance(UtteranceKind::Explanation, "X because Y");
        let u3 = make_utterance(UtteranceKind::Claim, "Therefore Z");
        let id1 = u1.id;
        let id2 = u2.id;
        let id3 = u3.id;
        sys.add_utterance(u1);
        sys.add_utterance(u2);
        sys.add_utterance(u3);

        // Score with no relations
        let scores_before = PrincipleScorer::score(&sys);
        let expl_before = scores_before.get(Principle::Explanation).unwrap().score;

        // Add explanation relations
        sys.add_coherence(CoherenceRelation::new(id1, id2, CoherenceKind::Explains).unwrap())
            .unwrap();
        sys.add_coherence(CoherenceRelation::new(id2, id3, CoherenceKind::Supports).unwrap())
            .unwrap();

        let scores_after = PrincipleScorer::score(&sys);
        let expl_after = scores_after.get(Principle::Explanation).unwrap().score;

        assert!(
            expl_after > expl_before,
            "explanation score should increase: {expl_before} -> {expl_after}"
        );
    }

    #[test]
    fn contradiction_score_decreases_with_conflicts() {
        let mut sys = CoherenceSystem::new(ConversationId::new());
        let u1 = make_utterance(UtteranceKind::Claim, "A is true");
        let u2 = make_utterance(UtteranceKind::Claim, "A is false");
        let id1 = u1.id;
        let id2 = u2.id;
        sys.add_utterance(u1);
        sys.add_utterance(u2);

        let scores_before = PrincipleScorer::score(&sys);
        let contr_before = scores_before.get(Principle::Contradiction).unwrap().score;

        sys.add_incoherence(
            IncoherenceRelation::new(id1, id2, IncoherenceKind::Contradicts).unwrap(),
        )
        .unwrap();

        let scores_after = PrincipleScorer::score(&sys);
        let contr_after = scores_after.get(Principle::Contradiction).unwrap().score;

        assert!(
            contr_after < contr_before,
            "contradiction score should decrease: {contr_before} -> {contr_after}"
        );
    }

    #[test]
    fn data_priority_reflects_evidence_acceptance() {
        let mut sys = CoherenceSystem::new(ConversationId::new());
        let e = make_utterance(UtteranceKind::Evidence, "Data: X=5");
        sys.add_utterance(e);

        // Evidence starts with positive activation (0.5), so should be accepted
        let scores = PrincipleScorer::score(&sys);
        let dp = scores.get(Principle::DataPriority).unwrap().score;
        assert!(
            (dp - 1.0).abs() < f64::EPSILON,
            "evidence with positive activation should give score 1.0, got {dp}"
        );
    }
}
