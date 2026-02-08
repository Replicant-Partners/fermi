//! Thagard's Seven Principles of Explanatory Coherence, adapted for collaboration.
//!
//! Each principle captures a different dimension of how well a multi-party
//! conversation "hangs together." The coherence engine scores each principle
//! independently, then combines them into a global coherence score.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::{CoreError, CoreResult};

// ─── Principle Enum ────────────────────────────────────────────────────────

/// The seven principles of explanatory coherence from Thagard (1989),
/// adapted for collaborative discourse evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Principle {
    /// **P1 — Symmetry:** Coherence and incoherence are symmetric relations.
    /// If utterance A coheres with B, then B coheres with A.
    ///
    /// In collaboration: mutual acknowledgment and bidirectional engagement
    /// between participants.
    Symmetry,

    /// **P2 — Explanation:** Utterances that jointly explain data cohere with
    /// each other and with the data they explain.
    ///
    /// In collaboration: reasoning chains that connect claims to evidence,
    /// building shared understanding.
    Explanation,

    /// **P3 — Analogy:** Analogical mappings between utterances create coherence.
    ///
    /// In collaboration: participants drawing structural parallels across
    /// domains to illuminate the topic.
    Analogy,

    /// **P4 — Data Priority:** Evidence has a degree of intrinsic acceptability
    /// independent of its relations to other utterances.
    ///
    /// In collaboration: shared data, measurements, and agreed-upon facts
    /// anchor the discourse.
    DataPriority,

    /// **P5 — Contradiction:** Contradictory utterances are incoherent with
    /// each other.
    ///
    /// In collaboration: conflicting claims that have not been resolved or
    /// acknowledged reduce overall coherence.
    Contradiction,

    /// **P6 — Competition:** When two explanations both account for the same
    /// data, they compete (incohere) unless one subsumes the other.
    ///
    /// In collaboration: rival hypotheses that are not synthesized or
    /// explicitly compared.
    Competition,

    /// **P7 — Acceptability:** The acceptability of an utterance depends on
    /// its coherence with the overall system.
    ///
    /// In collaboration: how well each contribution fits with the group's
    /// evolving shared understanding.
    Acceptability,
}

impl Principle {
    /// Returns all seven principles in canonical order (P1–P7).
    pub const ALL: [Principle; 7] = [
        Principle::Symmetry,
        Principle::Explanation,
        Principle::Analogy,
        Principle::DataPriority,
        Principle::Contradiction,
        Principle::Competition,
        Principle::Acceptability,
    ];

    /// Returns the principle number (1–7).
    pub fn number(&self) -> u8 {
        match self {
            Principle::Symmetry => 1,
            Principle::Explanation => 2,
            Principle::Analogy => 3,
            Principle::DataPriority => 4,
            Principle::Contradiction => 5,
            Principle::Competition => 6,
            Principle::Acceptability => 7,
        }
    }

    /// Returns the short label for this principle (e.g. "P1: Symmetry").
    pub fn label(&self) -> String {
        format!("P{}: {}", self.number(), self.name())
    }

    /// Returns the name of this principle.
    pub fn name(&self) -> &'static str {
        match self {
            Principle::Symmetry => "Symmetry",
            Principle::Explanation => "Explanation",
            Principle::Analogy => "Analogy",
            Principle::DataPriority => "Data Priority",
            Principle::Contradiction => "Contradiction",
            Principle::Competition => "Competition",
            Principle::Acceptability => "Acceptability",
        }
    }

    /// Returns a brief description of what this principle measures
    /// in the context of collaborative discourse.
    pub fn description(&self) -> &'static str {
        match self {
            Principle::Symmetry => {
                "Mutual acknowledgment and bidirectional engagement between participants"
            }
            Principle::Explanation => {
                "Reasoning chains connecting claims to evidence and building shared understanding"
            }
            Principle::Analogy => {
                "Structural parallels drawn across domains to illuminate the topic"
            }
            Principle::DataPriority => {
                "Shared data, measurements, and agreed-upon facts anchoring the discourse"
            }
            Principle::Contradiction => {
                "Conflicting claims that have not been resolved or acknowledged"
            }
            Principle::Competition => {
                "Rival hypotheses that are not synthesized or explicitly compared"
            }
            Principle::Acceptability => {
                "How well each contribution fits with the group's evolving shared understanding"
            }
        }
    }

    /// Returns the default weight this principle contributes to the global
    /// coherence score. Weights are normalized so they sum to 1.0 across
    /// all seven principles.
    ///
    /// Default distribution emphasizes Explanation and Acceptability as the
    /// most impactful for collaborative quality.
    pub fn default_weight(&self) -> f64 {
        match self {
            Principle::Symmetry => 0.10,
            Principle::Explanation => 0.20,
            Principle::Analogy => 0.10,
            Principle::DataPriority => 0.15,
            Principle::Contradiction => 0.15,
            Principle::Competition => 0.10,
            Principle::Acceptability => 0.20,
        }
    }

    /// Returns `true` if this principle measures a positive coherence dimension.
    /// Returns `false` if it measures an incoherence dimension (where lower
    /// raw scores indicate *more* of the problematic pattern, but the final
    /// σ score is inverted so higher is always better).
    pub fn is_positive(&self) -> bool {
        match self {
            Principle::Symmetry => true,
            Principle::Explanation => true,
            Principle::Analogy => true,
            Principle::DataPriority => true,
            Principle::Contradiction => false,
            Principle::Competition => false,
            Principle::Acceptability => true,
        }
    }

    /// Try to parse a principle from its number (1–7).
    pub fn from_number(n: u8) -> Option<Self> {
        match n {
            1 => Some(Principle::Symmetry),
            2 => Some(Principle::Explanation),
            3 => Some(Principle::Analogy),
            4 => Some(Principle::DataPriority),
            5 => Some(Principle::Contradiction),
            6 => Some(Principle::Competition),
            7 => Some(Principle::Acceptability),
            _ => None,
        }
    }
}

impl fmt::Display for Principle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

// ─── Principle Score ───────────────────────────────────────────────────────

/// A score for a single principle, in the range [0, 1].
///
/// Higher is always better:
/// - For positive principles (Symmetry, Explanation, etc.), 1.0 means the
///   principle is fully satisfied.
/// - For incoherence principles (Contradiction, Competition), 1.0 means
///   the problematic pattern is *absent*.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrincipleScore {
    /// Which principle this score is for.
    pub principle: Principle,

    /// The score value in [0, 1]. Higher is always better.
    pub score: f64,

    /// Optional diagnostic message explaining the score.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

impl PrincipleScore {
    /// Create a new principle score, validating the range.
    pub fn new(principle: Principle, score: f64) -> CoreResult<Self> {
        if !(0.0..=1.0).contains(&score) {
            return Err(CoreError::ScoreOutOfRange(score));
        }
        Ok(Self {
            principle,
            score,
            diagnostic: None,
        })
    }

    /// Create a new principle score, clamping to [0, 1] instead of erroring.
    pub fn new_clamped(principle: Principle, score: f64) -> Self {
        Self {
            principle,
            score: score.clamp(0.0, 1.0),
            diagnostic: None,
        }
    }

    /// Attach a diagnostic message.
    pub fn with_diagnostic(mut self, diagnostic: impl Into<String>) -> Self {
        self.diagnostic = Some(diagnostic.into());
        self
    }

    /// Returns `true` if this score is below the given threshold.
    pub fn is_below_threshold(&self, threshold: f64) -> bool {
        self.score < threshold
    }

    /// Returns the weighted contribution of this score to the global coherence.
    pub fn weighted(&self) -> f64 {
        self.score * self.principle.default_weight()
    }

    /// Returns a qualitative label for this score.
    pub fn quality_label(&self) -> &'static str {
        match self.score {
            s if s >= 0.8 => "excellent",
            s if s >= 0.6 => "good",
            s if s >= 0.4 => "moderate",
            s if s >= 0.2 => "weak",
            _ => "critical",
        }
    }
}

impl fmt::Display for PrincipleScore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {:.2} ({})",
            self.principle,
            self.score,
            self.quality_label()
        )
    }
}

// ─── Principle Scores (collection) ─────────────────────────────────────────

/// The complete set of principle scores for a coherence evaluation.
///
/// This corresponds to the **σ** function in the formal model:
/// σ : {P₁, …, P₇} → [0, 1]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrincipleScores {
    /// The individual principle scores, one per principle.
    pub scores: Vec<PrincipleScore>,
}

impl PrincipleScores {
    /// Create a new empty score set.
    pub fn new() -> Self {
        Self { scores: Vec::new() }
    }

    /// Create a score set with all principles initialized to zero.
    pub fn zeros() -> Self {
        Self {
            scores: Principle::ALL
                .iter()
                .map(|&p| PrincipleScore::new_clamped(p, 0.0))
                .collect(),
        }
    }

    /// Create a score set from a list of (Principle, f64) pairs.
    pub fn from_pairs(pairs: impl IntoIterator<Item = (Principle, f64)>) -> Self {
        Self {
            scores: pairs
                .into_iter()
                .map(|(p, s)| PrincipleScore::new_clamped(p, s))
                .collect(),
        }
    }

    /// Add a score.
    pub fn add(&mut self, score: PrincipleScore) {
        // Replace existing score for the same principle, if any
        if let Some(existing) = self
            .scores
            .iter_mut()
            .find(|s| s.principle == score.principle)
        {
            *existing = score;
        } else {
            self.scores.push(score);
        }
    }

    /// Get the score for a specific principle.
    pub fn get(&self, principle: Principle) -> Option<&PrincipleScore> {
        self.scores.iter().find(|s| s.principle == principle)
    }

    /// Get the score value for a specific principle, or 0.0 if not present.
    pub fn value(&self, principle: Principle) -> f64 {
        self.get(principle).map(|s| s.score).unwrap_or(0.0)
    }

    /// Returns all principles that are below the given threshold.
    pub fn below_threshold(&self, threshold: f64) -> Vec<&PrincipleScore> {
        self.scores
            .iter()
            .filter(|s| s.is_below_threshold(threshold))
            .collect()
    }

    /// Returns the weakest principle (lowest score).
    pub fn weakest(&self) -> Option<&PrincipleScore> {
        self.scores.iter().min_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Returns the strongest principle (highest score).
    pub fn strongest(&self) -> Option<&PrincipleScore> {
        self.scores.iter().max_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Compute the weighted average across all principle scores, using each
    /// principle's default weight.
    ///
    /// This produces a value in [0, 1] that summarizes the overall principle-level
    /// coherence. It is one input to the global coherence score Γ(C).
    pub fn weighted_average(&self) -> f64 {
        if self.scores.is_empty() {
            return 0.0;
        }

        let total_weight: f64 = self
            .scores
            .iter()
            .map(|s| s.principle.default_weight())
            .sum();
        if total_weight == 0.0 {
            return 0.0;
        }

        let weighted_sum: f64 = self.scores.iter().map(|s| s.weighted()).sum();
        weighted_sum / total_weight
    }

    /// Compute the simple (unweighted) mean of all principle scores.
    pub fn mean(&self) -> f64 {
        if self.scores.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.scores.iter().map(|s| s.score).sum();
        sum / self.scores.len() as f64
    }

    /// Returns the number of scored principles.
    pub fn len(&self) -> usize {
        self.scores.len()
    }

    /// Returns `true` if no principles have been scored.
    pub fn is_empty(&self) -> bool {
        self.scores.is_empty()
    }

    /// Returns an iterator over the scores in canonical principle order (P1–P7).
    pub fn in_order(&self) -> Vec<&PrincipleScore> {
        let mut ordered: Vec<&PrincipleScore> = self.scores.iter().collect();
        ordered.sort_by_key(|s| s.principle.number());
        ordered
    }
}

impl Default for PrincipleScores {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PrincipleScores {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for score in self.in_order() {
            writeln!(f, "  {score}")?;
        }
        writeln!(f, "  ─────────────────")?;
        write!(f, "  Weighted avg: {:.2}", self.weighted_average())
    }
}

// ─── Principle Config ──────────────────────────────────────────────────────

/// Configuration for principle evaluation, including custom weights and thresholds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrincipleConfig {
    /// Custom weights per principle. If a principle is not present, its
    /// `default_weight()` is used. Weights are normalized to sum to 1.0
    /// before use.
    #[serde(default)]
    pub weights: std::collections::HashMap<Principle, f64>,

    /// The threshold below which a principle score triggers targeted feedback.
    /// Default: 0.4
    #[serde(default = "default_principle_threshold")]
    pub principle_threshold: f64,

    /// The threshold below which the global coherence score triggers a
    /// full intervention. Default: 0.3
    #[serde(default = "default_critical_threshold")]
    pub critical_threshold: f64,

    /// The threshold above which the global coherence score triggers
    /// positive reinforcement. Default: 0.7
    #[serde(default = "default_good_threshold")]
    pub good_threshold: f64,
}

fn default_principle_threshold() -> f64 {
    0.4
}

fn default_critical_threshold() -> f64 {
    0.3
}

fn default_good_threshold() -> f64 {
    0.7
}

impl Default for PrincipleConfig {
    fn default() -> Self {
        Self {
            weights: std::collections::HashMap::new(),
            principle_threshold: default_principle_threshold(),
            critical_threshold: default_critical_threshold(),
            good_threshold: default_good_threshold(),
        }
    }
}

impl PrincipleConfig {
    /// Get the weight for a principle, using the custom weight if configured,
    /// otherwise falling back to the default.
    pub fn weight(&self, principle: Principle) -> f64 {
        self.weights
            .get(&principle)
            .copied()
            .unwrap_or_else(|| principle.default_weight())
    }

    /// Validate that the configuration thresholds are sensible.
    pub fn validate(&self) -> CoreResult<()> {
        if !(0.0..=1.0).contains(&self.principle_threshold) {
            return Err(CoreError::InvalidThreshold {
                name: "principle_threshold".to_string(),
                value: self.principle_threshold,
            });
        }
        if !(0.0..=1.0).contains(&self.critical_threshold) {
            return Err(CoreError::InvalidThreshold {
                name: "critical_threshold".to_string(),
                value: self.critical_threshold,
            });
        }
        if !(0.0..=1.0).contains(&self.good_threshold) {
            return Err(CoreError::InvalidThreshold {
                name: "good_threshold".to_string(),
                value: self.good_threshold,
            });
        }
        if self.critical_threshold >= self.good_threshold {
            return Err(CoreError::Internal(format!(
                "critical_threshold ({}) must be less than good_threshold ({})",
                self.critical_threshold, self.good_threshold
            )));
        }
        for (&principle, &weight) in &self.weights {
            if weight < 0.0 {
                return Err(CoreError::Internal(format!(
                    "negative weight {weight} for principle {principle}"
                )));
            }
        }
        Ok(())
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn principle_all_has_seven_entries() {
        assert_eq!(Principle::ALL.len(), 7);
    }

    #[test]
    fn principle_numbers_are_sequential() {
        for (i, p) in Principle::ALL.iter().enumerate() {
            assert_eq!(p.number(), (i + 1) as u8);
        }
    }

    #[test]
    fn principle_from_number_roundtrip() {
        for p in Principle::ALL {
            let n = p.number();
            assert_eq!(Principle::from_number(n), Some(p));
        }
        assert_eq!(Principle::from_number(0), None);
        assert_eq!(Principle::from_number(8), None);
    }

    #[test]
    fn principle_display() {
        assert_eq!(format!("{}", Principle::Symmetry), "P1: Symmetry");
        assert_eq!(format!("{}", Principle::DataPriority), "P4: Data Priority");
        assert_eq!(format!("{}", Principle::Acceptability), "P7: Acceptability");
    }

    #[test]
    fn principle_positive_vs_negative() {
        assert!(Principle::Symmetry.is_positive());
        assert!(Principle::Explanation.is_positive());
        assert!(Principle::Analogy.is_positive());
        assert!(Principle::DataPriority.is_positive());
        assert!(!Principle::Contradiction.is_positive());
        assert!(!Principle::Competition.is_positive());
        assert!(Principle::Acceptability.is_positive());
    }

    #[test]
    fn default_weights_sum_to_one() {
        let sum: f64 = Principle::ALL.iter().map(|p| p.default_weight()).sum();
        assert!(
            (sum - 1.0).abs() < 1e-10,
            "default weights sum to {sum}, expected 1.0"
        );
    }

    #[test]
    fn principle_score_validation() {
        assert!(PrincipleScore::new(Principle::Symmetry, 0.5).is_ok());
        assert!(PrincipleScore::new(Principle::Symmetry, 0.0).is_ok());
        assert!(PrincipleScore::new(Principle::Symmetry, 1.0).is_ok());
        assert!(PrincipleScore::new(Principle::Symmetry, -0.1).is_err());
        assert!(PrincipleScore::new(Principle::Symmetry, 1.1).is_err());
    }

    #[test]
    fn principle_score_clamped() {
        let s = PrincipleScore::new_clamped(Principle::Symmetry, 5.0);
        assert!((s.score - 1.0).abs() < f64::EPSILON);

        let s = PrincipleScore::new_clamped(Principle::Symmetry, -3.0);
        assert!(s.score.abs() < f64::EPSILON);
    }

    #[test]
    fn principle_score_quality_labels() {
        assert_eq!(
            PrincipleScore::new_clamped(Principle::Symmetry, 0.9).quality_label(),
            "excellent"
        );
        assert_eq!(
            PrincipleScore::new_clamped(Principle::Symmetry, 0.7).quality_label(),
            "good"
        );
        assert_eq!(
            PrincipleScore::new_clamped(Principle::Symmetry, 0.5).quality_label(),
            "moderate"
        );
        assert_eq!(
            PrincipleScore::new_clamped(Principle::Symmetry, 0.3).quality_label(),
            "weak"
        );
        assert_eq!(
            PrincipleScore::new_clamped(Principle::Symmetry, 0.1).quality_label(),
            "critical"
        );
    }

    #[test]
    fn principle_score_display() {
        let s = PrincipleScore::new_clamped(Principle::Explanation, 0.75);
        let display = format!("{s}");
        assert!(display.contains("P2: Explanation"));
        assert!(display.contains("0.75"));
        assert!(display.contains("good"));
    }

    #[test]
    fn principle_scores_collection() {
        let mut scores = PrincipleScores::new();
        assert!(scores.is_empty());
        assert_eq!(scores.len(), 0);
        assert!((scores.mean() - 0.0).abs() < f64::EPSILON);
        assert!((scores.weighted_average() - 0.0).abs() < f64::EPSILON);

        scores.add(PrincipleScore::new_clamped(Principle::Symmetry, 0.8));
        scores.add(PrincipleScore::new_clamped(Principle::Explanation, 0.6));
        scores.add(PrincipleScore::new_clamped(Principle::Contradiction, 0.3));

        assert_eq!(scores.len(), 3);
        assert!(!scores.is_empty());

        assert!((scores.value(Principle::Symmetry) - 0.8).abs() < f64::EPSILON);
        assert!((scores.value(Principle::Explanation) - 0.6).abs() < f64::EPSILON);
        assert!((scores.value(Principle::Analogy) - 0.0).abs() < f64::EPSILON); // not set

        // Check weakest/strongest
        assert_eq!(
            scores.weakest().unwrap().principle,
            Principle::Contradiction
        );
        assert_eq!(scores.strongest().unwrap().principle, Principle::Symmetry);
    }

    #[test]
    fn principle_scores_below_threshold() {
        let scores = PrincipleScores::from_pairs(vec![
            (Principle::Symmetry, 0.8),
            (Principle::Explanation, 0.6),
            (Principle::Contradiction, 0.2),
            (Principle::Competition, 0.35),
        ]);

        let below = scores.below_threshold(0.4);
        assert_eq!(below.len(), 2);

        let principles: Vec<Principle> = below.iter().map(|s| s.principle).collect();
        assert!(principles.contains(&Principle::Contradiction));
        assert!(principles.contains(&Principle::Competition));
    }

    #[test]
    fn principle_scores_replaces_existing() {
        let mut scores = PrincipleScores::new();
        scores.add(PrincipleScore::new_clamped(Principle::Symmetry, 0.5));
        assert!((scores.value(Principle::Symmetry) - 0.5).abs() < f64::EPSILON);

        scores.add(PrincipleScore::new_clamped(Principle::Symmetry, 0.9));
        assert!((scores.value(Principle::Symmetry) - 0.9).abs() < f64::EPSILON);
        assert_eq!(scores.len(), 1); // still one entry, not two
    }

    #[test]
    fn principle_scores_weighted_average() {
        // All principles at 1.0 should give weighted average of 1.0
        let scores = PrincipleScores::from_pairs(Principle::ALL.iter().map(|&p| (p, 1.0)));
        assert!((scores.weighted_average() - 1.0).abs() < 1e-10);

        // All principles at 0.0 should give weighted average of 0.0
        let scores = PrincipleScores::from_pairs(Principle::ALL.iter().map(|&p| (p, 0.0)));
        assert!(scores.weighted_average().abs() < 1e-10);

        // All principles at 0.5 should give weighted average of 0.5
        let scores = PrincipleScores::from_pairs(Principle::ALL.iter().map(|&p| (p, 0.5)));
        assert!((scores.weighted_average() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn principle_scores_mean() {
        let scores = PrincipleScores::from_pairs(vec![
            (Principle::Symmetry, 0.8),
            (Principle::Explanation, 0.6),
        ]);
        assert!((scores.mean() - 0.7).abs() < 1e-10);
    }

    #[test]
    fn principle_scores_in_order() {
        let scores = PrincipleScores::from_pairs(vec![
            (Principle::Acceptability, 0.9),
            (Principle::Symmetry, 0.5),
            (Principle::Explanation, 0.7),
        ]);

        let ordered = scores.in_order();
        assert_eq!(ordered[0].principle, Principle::Symmetry);
        assert_eq!(ordered[1].principle, Principle::Explanation);
        assert_eq!(ordered[2].principle, Principle::Acceptability);
    }

    #[test]
    fn principle_scores_zeros() {
        let scores = PrincipleScores::zeros();
        assert_eq!(scores.len(), 7);
        for score in &scores.scores {
            assert!(score.score.abs() < f64::EPSILON);
        }
    }

    #[test]
    fn principle_config_defaults() {
        let config = PrincipleConfig::default();
        assert!((config.principle_threshold - 0.4).abs() < f64::EPSILON);
        assert!((config.critical_threshold - 0.3).abs() < f64::EPSILON);
        assert!((config.good_threshold - 0.7).abs() < f64::EPSILON);
        assert!(config.weights.is_empty());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn principle_config_custom_weight() {
        let mut config = PrincipleConfig::default();
        config.weights.insert(Principle::Explanation, 0.5);

        assert!((config.weight(Principle::Explanation) - 0.5).abs() < f64::EPSILON);
        assert!(
            (config.weight(Principle::Symmetry) - Principle::Symmetry.default_weight()).abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn principle_config_validation() {
        let mut config = PrincipleConfig::default();

        config.principle_threshold = -0.1;
        assert!(config.validate().is_err());

        config.principle_threshold = 0.4;
        config.critical_threshold = 0.8;
        config.good_threshold = 0.7;
        assert!(config.validate().is_err()); // critical >= good

        config.critical_threshold = 0.3;
        config.good_threshold = 0.7;
        assert!(config.validate().is_ok());

        config.weights.insert(Principle::Symmetry, -0.5);
        assert!(config.validate().is_err()); // negative weight
    }

    #[test]
    fn roundtrip_serialize_principle() {
        let p = Principle::DataPriority;
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(json, "\"data_priority\"");

        let deserialized: Principle = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, p);
    }

    #[test]
    fn roundtrip_serialize_principle_score() {
        let score = PrincipleScore::new(Principle::Explanation, 0.75)
            .unwrap()
            .with_diagnostic("Good reasoning chains observed");

        let json = serde_json::to_string_pretty(&score).unwrap();
        let deserialized: PrincipleScore = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.principle, Principle::Explanation);
        assert!((deserialized.score - 0.75).abs() < f64::EPSILON);
        assert_eq!(
            deserialized.diagnostic.as_deref(),
            Some("Good reasoning chains observed")
        );
    }

    #[test]
    fn roundtrip_serialize_principle_scores() {
        let scores = PrincipleScores::from_pairs(vec![
            (Principle::Symmetry, 0.8),
            (Principle::Explanation, 0.6),
            (Principle::Contradiction, 0.4),
        ]);

        let json = serde_json::to_string_pretty(&scores).unwrap();
        let deserialized: PrincipleScores = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.len(), 3);
        assert!((deserialized.mean() - scores.mean()).abs() < f64::EPSILON);
    }
}
