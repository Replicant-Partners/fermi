//! Heuristic utterance classification.
//!
//! Classifies a message's text into an [`UtteranceKind`] based on keyword
//! patterns. This is a pragmatic starting point — production use should
//! swap in an LLM-based classifier for higher accuracy.

use coherence_core::types::UtteranceKind;
use regex::Regex;
use std::sync::LazyLock;

/// Classifies raw text into an utterance kind with a confidence score.
pub struct UtteranceClassifier;

/// Classification result.
#[derive(Debug, Clone)]
pub struct Classification {
    pub kind: UtteranceKind,
    pub confidence: f64,
}

// Compiled regex patterns
static RE_QUESTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(what|why|how|when|where|who|which|is |are |do |does |can |could |should |would |will |shall |has |have |\?)").unwrap()
});
static RE_QUESTION_MARK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\?\s*$").unwrap());
static RE_EVIDENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(data\s+shows?|according\s+to|evidence|measured|observed|survey|study\s+(found|shows?)|statistic|percent|%|\d+\.\d+|results?\s+(indicate|show|suggest))").unwrap()
});
static RE_EXPLANATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(because|therefore|thus|hence|since|this\s+(means|implies|suggests)|the\s+reason|explains?\s+why|due\s+to|as\s+a\s+result|consequently)").unwrap()
});
static RE_ANALOGY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(similar\s+to|like\s+when|just\s+as|analogous|reminds?\s+me\s+of|compared?\s+to|in\s+the\s+same\s+way)").unwrap()
});
static RE_ACKNOWLEDGMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(I\s+agree|yes|exactly|good\s+point|that'?s?\s+right|fair\s+point|makes?\s+sense|right|true|correct|absolutely|indeed)").unwrap()
});
static RE_PROCEDURAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(let'?s?\s+(move|start|continue|take|wrap)|next\s+(step|topic)|agenda|action\s+item|moving\s+on|time\s+check|shall\s+we|let me\s+share\s+my\s+screen)").unwrap()
});

impl UtteranceClassifier {
    /// Classify a text string into an utterance kind.
    pub fn classify(text: &str) -> Classification {
        let trimmed = text.trim();

        if trimmed.is_empty() {
            return Classification {
                kind: UtteranceKind::Procedural,
                confidence: 0.5,
            };
        }

        // Check in order of specificity

        // Procedural (meeting logistics) — check early since these are clear
        if RE_PROCEDURAL.is_match(trimmed) {
            return Classification {
                kind: UtteranceKind::Procedural,
                confidence: 0.8,
            };
        }

        // Questions
        if RE_QUESTION_MARK.is_match(trimmed) || RE_QUESTION.is_match(trimmed) {
            return Classification {
                kind: UtteranceKind::Question,
                confidence: if RE_QUESTION_MARK.is_match(trimmed) {
                    0.9
                } else {
                    0.7
                },
            };
        }

        // Acknowledgment
        if RE_ACKNOWLEDGMENT.is_match(trimmed) {
            return Classification {
                kind: UtteranceKind::Acknowledgment,
                confidence: 0.8,
            };
        }

        // Evidence
        if RE_EVIDENCE.is_match(trimmed) {
            return Classification {
                kind: UtteranceKind::Evidence,
                confidence: 0.7,
            };
        }

        // Analogy
        if RE_ANALOGY.is_match(trimmed) {
            return Classification {
                kind: UtteranceKind::Analogy,
                confidence: 0.7,
            };
        }

        // Explanation
        if RE_EXPLANATION.is_match(trimmed) {
            return Classification {
                kind: UtteranceKind::Explanation,
                confidence: 0.7,
            };
        }

        // Default: treat as a claim
        Classification {
            kind: UtteranceKind::Claim,
            confidence: 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_questions() {
        assert_eq!(
            UtteranceClassifier::classify("What do you think?").kind,
            UtteranceKind::Question
        );
        assert_eq!(
            UtteranceClassifier::classify("Is that correct?").kind,
            UtteranceKind::Question
        );
        assert_eq!(
            UtteranceClassifier::classify("Why would that happen").kind,
            UtteranceKind::Question
        );
    }

    #[test]
    fn classifies_evidence() {
        assert_eq!(
            UtteranceClassifier::classify("Data shows that 85% of users prefer the new UI").kind,
            UtteranceKind::Evidence
        );
        assert_eq!(
            UtteranceClassifier::classify("According to the latest survey, engagement is up").kind,
            UtteranceKind::Evidence
        );
    }

    #[test]
    fn classifies_explanations() {
        assert_eq!(
            UtteranceClassifier::classify("This is the case because the API changed").kind,
            UtteranceKind::Explanation
        );
        assert_eq!(
            UtteranceClassifier::classify("Therefore we should update the schema").kind,
            UtteranceKind::Explanation
        );
    }

    #[test]
    fn classifies_analogies() {
        assert_eq!(
            UtteranceClassifier::classify("This is similar to what happened with project X").kind,
            UtteranceKind::Analogy
        );
    }

    #[test]
    fn classifies_acknowledgments() {
        assert_eq!(
            UtteranceClassifier::classify("I agree, that makes sense").kind,
            UtteranceKind::Acknowledgment
        );
        assert_eq!(
            UtteranceClassifier::classify("Good point about the timeline").kind,
            UtteranceKind::Acknowledgment
        );
    }

    #[test]
    fn classifies_procedural() {
        assert_eq!(
            UtteranceClassifier::classify("Let's move on to the next topic").kind,
            UtteranceKind::Procedural
        );
    }

    #[test]
    fn defaults_to_claim() {
        assert_eq!(
            UtteranceClassifier::classify("We should redesign the entire module").kind,
            UtteranceKind::Claim
        );
    }
}
