//! What *kind* of disagreement is this?
//!
//! Loop 3's signal is not Γ(C). It is Γ(C) **plus** a classification of the
//! incoherence underneath it, and the architecture calls that taxonomy
//! "essential" — because without it, optimising coherence optimises a team into
//! agreement, and agreement is not the goal.
//!
//! A composition whose members share a base model will converge readily and
//! cheaply. Some of that convergence is corroboration and some is an artefact
//! of the sharing, and a global score cannot tell them apart. Worse, a system
//! tuned to raise Γ will actively suppress the contrarian and the devil's
//! advocate — the two contributions that most improve group calibration — and
//! it will report the resulting monoculture as an improvement.
//!
//! # The distinction that does the work
//!
//! **Productive incoherence is characterised by high engagement with evidence
//! (P4) despite low scores on other principles. Destructive incoherence shows
//! low evidence engagement alongside low coherence.**
//!
//! Same Γ, opposite verdicts. Two agents arguing hard about what a dataset
//! implies and two agents talking past each other can score identically; the
//! first is the composition working and the second is it failing.
//!
//! | Type | Formal signature | Epistemic value |
//! |---|---|---|
//! | Destructive | low σ(P2), low σ(P7) | negative — reduces group performance |
//! | Productive-Competitive | low σ(P6), moderate σ(P2) | positive — forces evidence evaluation |
//! | Productive-Analogical | low σ(P3), high σ(P2) | positive — reveals hidden assumptions |
//! | Productive-Contradictory | low σ(P5), high σ(P4) | positive — sharpens hypothesis space |
//!
//! # Why this lives here
//!
//! The seven principle scores were already computed and persisted; the
//! taxonomy over them existed only as English inside an LLM prompt, on a path
//! that ran when someone paid for `depth=recommendations`. A classification
//! that only exists in a prompt cannot be asserted on, cannot be trended, and
//! cannot stop a ratchet from rewarding homophily. This makes it a function
//! with a test.

use serde::{Deserialize, Serialize};

use crate::principles::{Principle, PrincipleScores};

/// Below this, a principle counts as depressed.
pub const LOW: f64 = 0.5;
/// At or above this, a principle counts as strong — used for the "despite"
/// half of each productive signature.
pub const HIGH: f64 = 0.6;

/// The optimal tension range for Γ(C).
///
/// A **range**, not a maximum. Above it is its own failure mode, and naming it
/// as one is the entire point of the taxonomy: a team optimised into agreement
/// has stopped generating the friction that improves its calibration.
pub const OPTIMAL_TENSION: (f64, f64) = (0.45, 0.75);

/// Where Γ(C) sits relative to the optimal range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TensionBand {
    /// Not enough shared ground for the disagreement to be about anything.
    Below,
    /// The target.
    Inside,
    /// The homophily trap. Not a better score.
    Above,
}

impl TensionBand {
    pub fn classify(gamma: f64) -> Self {
        if gamma < OPTIMAL_TENSION.0 {
            TensionBand::Below
        } else if gamma > OPTIMAL_TENSION.1 {
            TensionBand::Above
        } else {
            TensionBand::Inside
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TensionBand::Below => "below_band",
            TensionBand::Inside => "inside_band",
            TensionBand::Above => "above_band",
        }
    }
}

/// The four kinds, plus the case where nothing is depressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncoherenceType {
    /// No principle is depressed. Not automatically good — read the band.
    Coherent,
    /// Low σ(P2) and low σ(P7): utterances are not engaging with each other.
    Destructive,
    /// Low σ(P6) with moderate σ(P2): competing explanations, both engaged
    /// with the evidence.
    ProductiveCompetitive,
    /// Low σ(P3) with high σ(P2): different frameworks over shared data.
    ProductiveAnalogical,
    /// Low σ(P5) with high σ(P4): direct contradiction, grounded in evidence.
    ProductiveContradictory,
}

impl IncoherenceType {
    /// Is this worth preserving?
    ///
    /// The load-bearing method. Anything that acts on Loop 3's output — a
    /// coordination brief, a composition proposal, a burn-down metric — must
    /// branch on this rather than on the score, or it will suppress the three
    /// positive kinds along with the negative one.
    pub fn is_productive(self) -> bool {
        matches!(
            self,
            IncoherenceType::ProductiveCompetitive
                | IncoherenceType::ProductiveAnalogical
                | IncoherenceType::ProductiveContradictory
        )
    }

    /// Should anything try to reduce this?
    pub fn should_remedy(self) -> bool {
        matches!(self, IncoherenceType::Destructive)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            IncoherenceType::Coherent => "coherent",
            IncoherenceType::Destructive => "destructive",
            IncoherenceType::ProductiveCompetitive => "productive_competitive",
            IncoherenceType::ProductiveAnalogical => "productive_analogical",
            IncoherenceType::ProductiveContradictory => "productive_contradictory",
        }
    }

    /// The principles whose values produced this classification.
    ///
    /// Returned so a brief can name its own reasoning. A verdict that cannot
    /// say which measurement produced it is one nobody can argue with, and
    /// therefore one nobody can correct.
    pub fn signature(self) -> &'static [Principle] {
        match self {
            IncoherenceType::Coherent => &[],
            IncoherenceType::Destructive => &[Principle::Explanation, Principle::Acceptability],
            IncoherenceType::ProductiveCompetitive => {
                &[Principle::Competition, Principle::Explanation]
            }
            IncoherenceType::ProductiveAnalogical => &[Principle::Analogy, Principle::Explanation],
            IncoherenceType::ProductiveContradictory => {
                &[Principle::Contradiction, Principle::DataPriority]
            }
        }
    }
}

/// A full Loop 3 verdict: the type, the band, and why.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncoherenceAssessment {
    pub incoherence_type: IncoherenceType,
    pub band: TensionBand,
    pub gamma: f64,
    /// Human-readable, and specific enough to argue with.
    pub rationale: String,
    /// True when the group is *above* the optimal band, i.e. agreeing more than
    /// is good for it.
    pub homophily_risk: bool,
}

fn score_of(scores: &PrincipleScores, p: Principle) -> f64 {
    scores.get(p).map(|s| s.score).unwrap_or(0.0)
}

/// Classify the incoherence underneath a settled system.
///
/// Order matters. Destructive is tested first because it is the only negative
/// kind and the only one anything should try to remove; testing a productive
/// signature first would let a disengaged conversation match
/// `ProductiveCompetitive` on a low P6 it earned by nobody explaining anything.
pub fn classify(scores: &PrincipleScores, gamma: f64) -> IncoherenceAssessment {
    let p2 = score_of(scores, Principle::Explanation);
    let p3 = score_of(scores, Principle::Analogy);
    let p4 = score_of(scores, Principle::DataPriority);
    let p5 = score_of(scores, Principle::Contradiction);
    let p6 = score_of(scores, Principle::Competition);
    let p7 = score_of(scores, Principle::Acceptability);

    let band = TensionBand::classify(gamma);
    let homophily_risk = band == TensionBand::Above;

    // Destructive: low explanation AND low acceptability, and crucially the
    // evidence engagement is not there either. P4 is the discriminator the
    // whole taxonomy turns on.
    if p2 < LOW && p7 < LOW && p4 < HIGH {
        return IncoherenceAssessment {
            incoherence_type: IncoherenceType::Destructive,
            band,
            gamma,
            rationale: format!(
                "σ(P2)={p2:.2} and σ(P7)={p7:.2} are both depressed and σ(P4)={p4:.2} \
                 shows little engagement with evidence. Utterances are not \
                 reasoning about each other's data. This is the one kind worth reducing."
            ),
            homophily_risk,
        };
    }

    // Everything below is a productive family: evidence engagement is present,
    // so a depressed principle means live disagreement rather than absence.
    let candidates = [
        (
            p6,
            IncoherenceType::ProductiveCompetitive,
            p2 >= LOW,
            format!("σ(P6)={p6:.2} with σ(P2)={p2:.2}: rival explanations that both engage the evidence"),
        ),
        (
            p3,
            IncoherenceType::ProductiveAnalogical,
            p2 >= HIGH,
            format!("σ(P3)={p3:.2} with σ(P2)={p2:.2}: different frameworks over shared data"),
        ),
        (
            p5,
            IncoherenceType::ProductiveContradictory,
            p4 >= HIGH,
            format!("σ(P5)={p5:.2} with σ(P4)={p4:.2}: direct contradiction, anchored in evidence"),
        ),
    ];

    let best = candidates
        .iter()
        .filter(|(score, _, guard, _)| *score < LOW && *guard)
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    match best {
        Some((_, ty, _, why)) => IncoherenceAssessment {
            incoherence_type: *ty,
            band,
            gamma,
            rationale: format!(
                "{why}. Productive: preserve it. A system tuned to raise Γ would \
                 flatten exactly this."
            ),
            homophily_risk,
        },
        None => IncoherenceAssessment {
            incoherence_type: IncoherenceType::Coherent,
            band,
            gamma,
            rationale: if homophily_risk {
                format!(
                    "No principle is depressed and Γ={gamma:.2} is above the optimal \
                     band ({:.2}–{:.2}). Agreement, but more of it than is good for \
                     the group — and members sharing a base model will agree for \
                     reasons that are not corroboration.",
                    OPTIMAL_TENSION.0, OPTIMAL_TENSION.1
                )
            } else {
                format!("No principle is depressed; Γ={gamma:.2} sits in the optimal band.")
            },
            homophily_risk,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scores(pairs: &[(Principle, f64)]) -> PrincipleScores {
        PrincipleScores::from_pairs(pairs.iter().copied())
    }

    /// A full complement of healthy scores.
    fn healthy() -> Vec<(Principle, f64)> {
        vec![
            (Principle::Symmetry, 0.8),
            (Principle::Explanation, 0.8),
            (Principle::Analogy, 0.8),
            (Principle::DataPriority, 0.8),
            (Principle::Contradiction, 0.8),
            (Principle::Competition, 0.8),
            (Principle::Acceptability, 0.8),
        ]
    }

    fn with(overrides: &[(Principle, f64)]) -> PrincipleScores {
        let mut base = healthy();
        for (p, v) in overrides {
            if let Some(slot) = base.iter_mut().find(|(bp, _)| bp == p) {
                slot.1 = *v;
            }
        }
        scores(&base)
    }

    #[test]
    fn low_explanation_and_acceptability_without_evidence_is_destructive() {
        let s = with(&[
            (Principle::Explanation, 0.2),
            (Principle::Acceptability, 0.2),
            (Principle::DataPriority, 0.2),
        ]);
        let a = classify(&s, 0.3);
        assert_eq!(a.incoherence_type, IncoherenceType::Destructive);
        assert!(a.incoherence_type.should_remedy());
        assert!(!a.incoherence_type.is_productive());
    }

    /// The distinction the whole taxonomy exists for.
    ///
    /// Identical depressed principles, identical Γ. The only difference is
    /// whether anyone is engaging with the evidence. If these ever classify the
    /// same, the framework has collapsed back into "low score = bad".
    #[test]
    fn evidence_engagement_separates_destructive_from_productive() {
        let disengaged = with(&[
            (Principle::Explanation, 0.2),
            (Principle::Acceptability, 0.2),
            (Principle::DataPriority, 0.2),
        ]);
        let engaged = with(&[
            (Principle::Explanation, 0.2),
            (Principle::Acceptability, 0.2),
            (Principle::DataPriority, 0.9),
            (Principle::Contradiction, 0.2),
        ]);

        let a = classify(&disengaged, 0.4);
        let b = classify(&engaged, 0.4);

        assert_eq!(a.incoherence_type, IncoherenceType::Destructive);
        assert!(
            b.incoherence_type.is_productive(),
            "same depressed principles and same gamma, but with evidence \
             engagement present this must not be destructive: got {:?}",
            b.incoherence_type
        );
    }

    #[test]
    fn low_competition_with_engagement_is_competitive() {
        let s = with(&[(Principle::Competition, 0.2)]);
        assert_eq!(
            classify(&s, 0.6).incoherence_type,
            IncoherenceType::ProductiveCompetitive
        );
    }

    #[test]
    fn low_analogy_with_strong_explanation_is_analogical() {
        let s = with(&[(Principle::Analogy, 0.2)]);
        assert_eq!(
            classify(&s, 0.6).incoherence_type,
            IncoherenceType::ProductiveAnalogical
        );
    }

    #[test]
    fn low_contradiction_with_strong_data_is_contradictory() {
        let s = with(&[(Principle::Contradiction, 0.2)]);
        assert_eq!(
            classify(&s, 0.6).incoherence_type,
            IncoherenceType::ProductiveContradictory
        );
    }

    /// Above the band is a finding, not a better score. If this ever passes
    /// silently, a ratchet somewhere will start rewarding homophily.
    #[test]
    fn agreement_above_the_band_is_flagged_not_praised() {
        let a = classify(&with(&[]), 0.92);
        assert_eq!(a.incoherence_type, IncoherenceType::Coherent);
        assert_eq!(a.band, TensionBand::Above);
        assert!(a.homophily_risk);
        assert!(a.rationale.contains("more of it than is good"));

        let b = classify(&with(&[]), 0.6);
        assert_eq!(b.band, TensionBand::Inside);
        assert!(!b.homophily_risk);
    }

    /// Three of the four kinds must be preserved, not remedied. A brief that
    /// treats every low score as a problem is the homophily trap in code.
    #[test]
    fn only_destructive_is_worth_remedying() {
        for ty in [
            IncoherenceType::ProductiveCompetitive,
            IncoherenceType::ProductiveAnalogical,
            IncoherenceType::ProductiveContradictory,
        ] {
            assert!(ty.is_productive());
            assert!(!ty.should_remedy(), "{ty:?} must be preserved");
        }
        assert!(IncoherenceType::Destructive.should_remedy());
        assert!(!IncoherenceType::Coherent.should_remedy());
    }

    #[test]
    fn every_type_names_the_principles_behind_it() {
        for ty in [
            IncoherenceType::Destructive,
            IncoherenceType::ProductiveCompetitive,
            IncoherenceType::ProductiveAnalogical,
            IncoherenceType::ProductiveContradictory,
        ] {
            assert_eq!(
                ty.signature().len(),
                2,
                "{ty:?} must name the two principles that produced it"
            );
        }
    }
}
