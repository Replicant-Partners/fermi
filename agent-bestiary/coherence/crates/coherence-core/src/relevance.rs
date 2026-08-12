//! Topical relevance gating for relation detection.
//!
//! ## Why this exists
//!
//! `RelationDetector::detect_all` (in `coherence-observer`)
//! considers every ordered pair of utterances, and the cue-based rules key
//! only on the *later* utterance. Without a relatedness test that means a
//! relation is created between an utterance and **every** utterance before
//! it. Measured on a real 64-episode agent↔human history:
//!
//! ```text
//!   27 claims x 15 evidence  = 405 "supports" edges   (exact cross-product)
//!    1 analogy utterance     =  34 "analogizes" edges (to everything prior)
//!   edge density             = 55.3% of all scorable pairs
//! ```
//!
//! Thagard's TEC takes the relation set as *given* by domain analysis. When
//! relations are instead inferred heuristically, an explicit relatedness
//! prior is what stops the graph degenerating into "everything coheres with
//! everything", which drives Γ(C) to a meaningless ~1.0.
//!
//! ## The prior
//!
//! Two signals, deliberately cheap and deterministic (no embeddings, no LLM,
//! so this stays replayable and auditable):
//!
//! - **Lexical overlap** — containment over content words, which is robust to
//!   the large length asymmetry between a 60-char question and a 3,000-char
//!   answer.
//! - **Conversational distance** — adjacent turns are directly responsive and
//!   need no lexical evidence; the further apart two utterances are, the
//!   stronger the topical match required to link them.
//!
//! Distance-scaled thresholds are what let a genuine cross-session
//! contradiction still register while suppressing the incidental ones.
//!
//! Lives in `coherence-core` rather than the observer because the settling
//! engine's Symmetry scorer needs the same overlap measure to decide whether
//! one participant's turn actually takes up the other's.

use std::collections::HashSet;

use crate::types::Utterance;

/// Words carrying no topical information. Kept small and obvious rather than
/// exhaustive — the goal is to stop function words from manufacturing
/// overlap, not to do real linguistics.
const STOPWORDS: &[&str] = &[
    "about", "after", "again", "against", "along", "also", "although", "always", "among",
    "another", "around", "because", "been", "before", "being", "below", "between", "both",
    "cannot", "could", "does", "doing", "done", "down", "during", "each", "either", "else",
    "enough", "even", "ever", "every", "from", "further", "give", "given", "goes", "going", "gone",
    "have", "having", "here", "hers", "high", "him", "his", "how", "however", "into", "its",
    "itself", "just", "keep", "kind", "know", "known", "large", "last", "later", "least", "less",
    "let", "like", "long", "look", "made", "make", "makes", "making", "many", "may", "maybe",
    "mean", "means", "might", "more", "most", "much", "must", "need", "needs", "never", "next",
    "not", "now", "off", "often", "once", "one", "only", "onto", "other", "others", "our", "ours",
    "out", "over", "own", "part", "per", "perhaps", "put", "quite", "rather", "really", "right",
    "said", "same", "say", "says", "see", "seem", "seems", "seen", "several", "shall", "she",
    "should", "since", "some", "still", "such", "sure", "take", "taken", "than", "that", "the",
    "their", "them", "then", "there", "these", "they", "thing", "things", "this", "those",
    "though", "through", "thus", "time", "too", "took", "toward", "try", "under", "until", "upon",
    "use", "used", "uses", "using", "very", "want", "was", "way", "well", "were", "what", "when",
    "where", "whether", "which", "while", "who", "whom", "why", "will", "with", "within",
    "without", "would", "yes", "yet", "you", "your", "yours",
];

/// Minimum token length to count as a content word.
const MIN_TOKEN_LEN: usize = 4;

/// Utterances this close are treated as directly responsive.
pub const ADJACENT_DISTANCE: usize = 1;

/// Upper bound of the "local context" band.
pub const LOCAL_WINDOW: usize = 10;

/// Overlap required to relate two utterances inside [`LOCAL_WINDOW`].
pub const LOCAL_THRESHOLD: f64 = 0.12;

/// Overlap required to relate two utterances beyond [`LOCAL_WINDOW`].
///
/// Higher because a distant pair needs to earn its edge: this is what
/// separates a real callback to an earlier topic from coincidental
/// vocabulary reuse.
pub const DISTANT_THRESHOLD: f64 = 0.30;

/// Extract lowercase content tokens from raw utterance text.
pub fn content_tokens(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= MIN_TOKEN_LEN)
        .map(|w| w.to_lowercase())
        .filter(|w| !STOPWORDS.contains(&w.as_str()))
        .filter(|w| !w.chars().all(|c| c.is_numeric()))
        .collect()
}

/// Topical overlap in `[0, 1]`.
///
/// Containment (`|A ∩ B| / min(|A|, |B|)`) rather than Jaccard: a short
/// question fully covered by a long answer should score ~1.0, whereas Jaccard
/// would report ~0.02 purely because the answer is longer.
pub fn overlap(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let shared = a.intersection(b).count();
    shared as f64 / a.len().min(b.len()) as f64
}

/// Whether two utterances are related enough to carry a relation.
///
/// `distance` is their separation in the conversation ordering.
pub fn is_relevant(earlier: &Utterance, later: &Utterance, distance: usize) -> bool {
    relevance(earlier, later, distance).is_some()
}

/// Relevance score for a pair, or `None` when the pair fails the gate.
///
/// Adjacent turns pass unconditionally with a nominal score: a direct reply is
/// responsive to what precedes it even when it shares no vocabulary
/// ("Correct." after a detailed claim).
pub fn relevance(earlier: &Utterance, later: &Utterance, distance: usize) -> Option<f64> {
    if distance <= ADJACENT_DISTANCE {
        let o = overlap(
            &content_tokens(&earlier.content),
            &content_tokens(&later.content),
        );
        return Some(o.max(0.5));
    }

    let o = overlap(
        &content_tokens(&earlier.content),
        &content_tokens(&later.content),
    );
    let threshold = if distance <= LOCAL_WINDOW {
        LOCAL_THRESHOLD
    } else {
        DISTANT_THRESHOLD
    };

    if o >= threshold {
        Some(o)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MessageId, ParticipantId, UtteranceKind};

    fn utt(content: &str) -> Utterance {
        Utterance::new(
            ParticipantId::new(),
            MessageId::new(),
            UtteranceKind::Claim,
            content,
        )
    }

    #[test]
    fn stopwords_do_not_manufacture_overlap() {
        let a = content_tokens("This is the thing that we should have");
        let b = content_tokens("That was the other thing they would take");
        // "thing" is a stopword here; nothing topical is shared.
        assert_eq!(overlap(&a, &b), 0.0);
    }

    #[test]
    fn containment_handles_length_asymmetry() {
        let q = content_tokens("What agents help with social media marketing?");
        let long = content_tokens(
            "Several agents help with social media marketing. The marketing pipeline \
             includes scheduling, analytics, audience research, campaign design, \
             copywriting, and reporting across many other unrelated capabilities.",
        );
        // Short side is well covered, so containment should be high even though
        // Jaccard would be small.
        assert!(overlap(&q, &long) > 0.6, "got {}", overlap(&q, &long));
    }

    #[test]
    fn adjacent_pairs_always_pass() {
        let a = utt("The deployment uses blue-green cutover");
        let b = utt("Correct.");
        assert!(is_relevant(&a, &b, 1));
    }

    #[test]
    fn distant_unrelated_pairs_are_gated_out() {
        let a = utt("The deployment pipeline uses blue-green cutover strategies");
        let b = utt("Penguins huddle together for warmth during Antarctic winters");
        assert!(!is_relevant(&a, &b, 40));
    }

    #[test]
    fn distant_but_strongly_matching_pairs_pass() {
        let a = utt("The deployment pipeline uses blue-green cutover strategies");
        let b = utt("Blue-green cutover deployment pipeline strategies were revisited");
        assert!(
            is_relevant(&a, &b, 40),
            "a genuine callback to an earlier topic must still register"
        );
    }

    /// The gate must be stricter far away than nearby, or cross-session
    /// contradiction detection degenerates back into the cross-product.
    #[test]
    fn threshold_tightens_with_distance() {
        let a = utt("latency budget affects checkout conversion metrics");
        let b = utt("checkout redesign shipped without measuring anything else at all here");
        let near = relevance(&a, &b, 5);
        let far = relevance(&a, &b, 50);
        assert!(near.is_some(), "should pass nearby");
        assert!(far.is_none(), "same pair should fail at distance");
    }

    #[test]
    fn empty_content_never_relates() {
        let a = utt("");
        let b = utt("something substantive about deployment");
        assert!(!is_relevant(&a, &b, 5));
    }
}
