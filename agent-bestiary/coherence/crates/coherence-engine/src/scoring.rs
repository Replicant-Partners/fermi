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

/// Topical overlap at which a reply counts as taking up the previous turn.
///
/// Deliberately lower than the relation-detection gate: a reply need only
/// engage with the other's point, not restate it.
const UPTAKE_THRESHOLD: f64 = 0.10;

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

    /// P1 — Symmetry: mutual acknowledgment and bidirectional engagement.
    ///
    /// Measured as **uptake**: when one participant speaks and the other
    /// replies, does the reply actually engage with what was said?
    ///
    /// The previous definition was `acknowledgments / scorable utterances`,
    /// where an acknowledgment required the turn to *begin* with a phrase like
    /// "I agree" / "exactly" / "good point". That is a human-meeting-transcript
    /// idiom. In an agent↔human dyad the human's turn is a question (excluded
    /// from `scorable` entirely) and the agent's answer never opens that way,
    /// so the count was structurally pinned at zero — confirmed across every
    /// stored evaluation and on a real 64-episode history.
    ///
    /// Uptake is the property that definition was reaching for, and it is
    /// measurable in Q&A: a companion that answers the question you actually
    /// asked is bidirectionally engaged; one that responds with boilerplate is
    /// not. Questions are deliberately included here even though they are
    /// unscorable as *propositions* — in a dyad the question is the human's
    /// entire contribution, and excluding it makes symmetry unmeasurable.
    ///
    /// An explicit acknowledgment still counts as full uptake.
    fn score_symmetry(system: &CoherenceSystem) -> (f64, Option<String>) {
        let utterances = &system.utterances;
        if utterances.len() <= 1 {
            return (
                1.0,
                Some("Single or no utterances — symmetry trivially satisfied".into()),
            );
        }

        // Explicit acknowledgments short-circuit to full uptake regardless of
        // vocabulary overlap ("Correct." shares no content words).
        let ack_ids: std::collections::HashSet<_> = system
            .relations
            .coherence_of_kind(CoherenceKind::Acknowledges)
            .iter()
            .map(|r| r.target)
            .collect();

        let mut exchanges = 0usize;
        let mut engaged = 0usize;
        for pair in utterances.windows(2) {
            let (prev, next) = (&pair[0], &pair[1]);
            // Only turn-taking across participants counts; a participant
            // continuing their own point is not bidirectional engagement.
            if prev.participant_id == next.participant_id {
                continue;
            }
            exchanges += 1;

            if ack_ids.contains(&next.id) {
                engaged += 1;
                continue;
            }
            // Adjacent turns are `distance = 1`, which the relevance gate
            // passes unconditionally, so compare overlap directly instead.
            let o = coherence_core::relevance::overlap(
                &coherence_core::relevance::content_tokens(&prev.content),
                &coherence_core::relevance::content_tokens(&next.content),
            );
            if o >= UPTAKE_THRESHOLD {
                engaged += 1;
            }
        }

        if exchanges == 0 {
            return (
                1.0,
                Some("No turn-taking between participants — symmetry not applicable".into()),
            );
        }

        let ratio = engaged as f64 / exchanges as f64;
        let diag = if ratio < 0.2 {
            Some(format!(
                "Low uptake — {}/{} replies engage with what the other said",
                engaged, exchanges
            ))
        } else if ratio >= 0.6 {
            Some(format!(
                "Strong bidirectional engagement — {}/{} replies take up the other's content",
                engaged, exchanges
            ))
        } else {
            Some(format!(
                "Partial uptake — {}/{} replies engage with what the other said",
                engaged, exchanges
            ))
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

    fn utt_from(p: ParticipantId, kind: UtteranceKind, content: &str) -> Utterance {
        Utterance::new(p, MessageId::new(), kind, content)
    }

    fn symmetry_of(sys: &CoherenceSystem) -> f64 {
        PrincipleScorer::score(sys)
            .scores
            .iter()
            .find(|s| s.principle == Principle::Symmetry)
            .map(|s| s.score)
            .expect("symmetry scored")
    }

    /// A Q&A dyad where the agent answers the question that was asked.
    ///
    /// Regression: under the old prefix-regex definition this scored exactly
    /// 0.0, because the human's turns are questions (unscorable) and the
    /// agent never opens with "I agree".
    #[test]
    fn symmetry_rewards_on_topic_answers_without_acknowledgment_phrases() {
        let human = ParticipantId::new();
        let agent = ParticipantId::new();
        let mut sys = CoherenceSystem::new(ConversationId::new());
        for (q, a) in [
            (
                "Which agents help with social media marketing?",
                "These marketing agents handle social media scheduling and campaigns.",
            ),
            (
                "How does the deployment pipeline handle rollback?",
                "The deployment pipeline performs rollback via blue-green cutover.",
            ),
        ] {
            sys.add_utterance(utt_from(human, UtteranceKind::Question, q));
            sys.add_utterance(utt_from(agent, UtteranceKind::Claim, a));
        }
        let s = symmetry_of(&sys);
        assert!(
            s > 0.6,
            "on-topic answers should register as engagement, got {s}"
        );
    }

    /// The discriminating case: the agent replies, but about something else.
    /// If symmetry cannot separate this from the case above it carries no
    /// information.
    #[test]
    fn symmetry_penalises_boilerplate_that_ignores_the_question() {
        let human = ParticipantId::new();
        let agent = ParticipantId::new();
        let mut sys = CoherenceSystem::new(ConversationId::new());
        for (q, a) in [
            (
                "Which agents help with social media marketing?",
                "Thank you for reaching out. Please consult the documentation portal.",
            ),
            (
                "How does the deployment pipeline handle rollback?",
                "Thank you for reaching out. Please consult the documentation portal.",
            ),
        ] {
            sys.add_utterance(utt_from(human, UtteranceKind::Question, q));
            sys.add_utterance(utt_from(agent, UtteranceKind::Claim, a));
        }
        let s = symmetry_of(&sys);
        assert!(
            s < 0.34,
            "off-topic boilerplate should not read as engagement, got {s}"
        );
    }

    /// A participant continuing their own point is not bidirectional.
    #[test]
    fn symmetry_ignores_same_speaker_runs() {
        let solo = ParticipantId::new();
        let mut sys = CoherenceSystem::new(ConversationId::new());
        for c in [
            "deployment pipeline notes",
            "more deployment pipeline notes",
        ] {
            sys.add_utterance(utt_from(solo, UtteranceKind::Claim, c));
        }
        // No cross-participant exchange exists, so symmetry is not applicable
        // rather than zero.
        assert_eq!(symmetry_of(&sys), 1.0);
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
