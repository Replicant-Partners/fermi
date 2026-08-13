//! Structural critique of a forecast's base rate.
//!
//! A base rate is the single most load-bearing number in a Fermi
//! decomposition: every driver is a multiplier on it, so an error here
//! scales through the entire model. Nothing in the console used to check
//! it. A real run anchored on
//!
//! ```text
//! reference_class: "Manchester City EPL title wins in Guardiola era"
//! historical_frequency: 0.60
//! sample_size: 10
//! ```
//!
//! and the console accepted it silently, then reported the resulting
//! +33.5pp gap to the market as possible alpha.
//!
//! The checks here are deliberately **structural** rather than
//! semantic — they ask questions with defensible answers ("how wide is
//! the sampling interval on this frequency?") rather than trying to
//! judge whether a reference class is *wise*. The prompt-side guidance
//! in `agents/curated/fermi/agent_card.json` handles the judgement; this
//! catches the cases where guidance was ignored.
//!
//! GPUI-free so it can be tested in seconds. See the crate docs.

/// How loudly to complain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Worth knowing; the forecast is still usable.
    Note,
    /// The base rate is likely to be materially wrong.
    Warn,
}

/// One finding about a base rate.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseRateFinding {
    pub severity: Severity,
    pub message: String,
}

impl BaseRateFinding {
    fn note(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Note,
            message: message.into(),
        }
    }
    fn warn(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warn,
            message: message.into(),
        }
    }
}

/// 95% Wilson score interval for a binomial proportion.
///
/// Wilson rather than the normal approximation because the cases that
/// matter here are exactly the ones the normal approximation handles
/// worst: small `n`, or `p` near 0 or 1. The normal interval for
/// 0 successes in 10 trials is the single point 0.0, which would let a
/// degenerate base rate pass as certain.
///
/// Returns `(low, high)`, both clamped to `[0, 1]`. `n == 0` yields the
/// vacuous interval `(0, 1)`.
pub fn wilson_interval(frequency: f64, n: u32) -> (f64, f64) {
    if n == 0 {
        return (0.0, 1.0);
    }
    const Z: f64 = 1.959_963_985; // 97.5th percentile of the standard normal
    let n_f = f64::from(n);
    let p = frequency.clamp(0.0, 1.0);
    let z2 = Z * Z;

    let denom = 1.0 + z2 / n_f;
    let centre = (p + z2 / (2.0 * n_f)) / denom;
    let margin = (Z / denom) * (p * (1.0 - p) / n_f + z2 / (4.0 * n_f * n_f)).sqrt();

    ((centre - margin).max(0.0), (centre + margin).min(1.0))
}

/// The entity a question is *about*.
///
/// Heuristic but narrow: the first capitalised token run in the
/// question, skipping the leading interrogative and a small stoplist.
/// "Will Manchester City win the 2026-27 English Premier League?" gives
/// `Manchester City`.
///
/// Used only to detect a reference class built out of the subject
/// itself, and only in combination with a small sample — see
/// [`critique_base_rate`] for why that pairing matters.
pub fn subject_entity(question: &str) -> Option<String> {
    // Words that are capitalised for position or grammar, not identity.
    const SKIP: &[&str] = &[
        "will", "is", "are", "was", "were", "do", "does", "did", "can", "could", "should", "would",
        "the", "a", "an", "by", "in", "on", "at", "if", "when", "what", "which", "who", "how",
        "there", "this", "that",
    ];

    let mut run: Vec<&str> = Vec::new();
    for raw in question.split_whitespace() {
        let token = raw.trim_matches(|c: char| !c.is_alphanumeric());
        if token.is_empty() {
            continue;
        }
        let is_cap = token
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false);
        // A capitalised token that isn't a positional artefact extends
        // the current run.
        if is_cap && !SKIP.contains(&token.to_lowercase().as_str()) {
            run.push(token);
        } else if !run.is_empty() {
            break; // first complete run wins
        }
    }
    if run.is_empty() {
        None
    } else {
        Some(run.join(" "))
    }
}

/// True when `haystack` mentions `entity`, case-insensitively.
fn mentions(haystack: &str, entity: &str) -> bool {
    haystack.to_lowercase().contains(&entity.to_lowercase())
}

/// Structural findings about a base rate. Empty means nothing to say.
///
/// The two thresholds below are the load-bearing choices:
///
/// * **n < 30** for the circularity check. A reference class naming the
///   subject is not automatically wrong — "days in London reaching 32C
///   during August" (n=744) names London and is a perfectly good class,
///   because the subject is one draw from a large unselected
///   population. It becomes circular when the class is *also* small,
///   i.e. a handful of the subject's own outcomes over a chosen window.
///   Requiring both conditions is what keeps this check quiet on the
///   London case and loud on the Manchester City one.
/// * **n < 20** for the small-sample note, reported with the Wilson
///   interval so the operator sees the actual sampling uncertainty
///   rather than a bare adjective.
pub fn critique_base_rate(
    question: &str,
    reference_class: &str,
    frequency: f64,
    sample_size: Option<u32>,
) -> Vec<BaseRateFinding> {
    let mut out = Vec::new();

    if !(0.0..=1.0).contains(&frequency) {
        out.push(BaseRateFinding::warn(format!(
            "Base rate {frequency} is not a probability."
        )));
        return out;
    }

    let Some(n) = sample_size.filter(|n| *n > 0) else {
        out.push(BaseRateFinding::warn(
            "Base rate has no sample size. A frequency without an n is an \
             assertion, not a reference class — there is no way to tell 6 of \
             10 from 600 of 1000."
                .to_string(),
        ));
        return out;
    };

    let (lo, hi) = wilson_interval(frequency, n);
    let width_pp = (hi - lo) * 100.0;

    if n < 20 {
        out.push(BaseRateFinding::warn(format!(
            "Base rate is {:.0}% from only n={}. The 95% interval on that \
             frequency alone is {:.0}%–{:.0}% ({:.0}pp wide) — before any \
             driver is applied. Widen the reference class or treat this \
             anchor as weak.",
            frequency * 100.0,
            n,
            lo * 100.0,
            hi * 100.0,
            width_pp,
        )));
    } else if width_pp > 30.0 {
        out.push(BaseRateFinding::note(format!(
            "Base rate {:.0}% carries a 95% interval of {:.0}%–{:.0}% at n={}.",
            frequency * 100.0,
            lo * 100.0,
            hi * 100.0,
            n,
        )));
    }

    // Circularity: the class is built from the subject's own outcomes.
    if n < 30 {
        if let Some(entity) = subject_entity(question) {
            if mentions(reference_class, &entity) {
                out.push(BaseRateFinding::warn(format!(
                    "Reference class \"{reference_class}\" is about {entity} — the \
                     subject of the question — and has only n={n}. That is the \
                     inside view, not a base rate: it is circular, and picking the \
                     window is picking the answer. Anchor on the broader class \
                     ({entity} is one member of it) and let the drivers narrow it."
                )));
            }
        }
    }

    // Degenerate anchors multiply to zero (or saturate) whatever the
    // drivers say.
    if frequency <= 0.0 {
        out.push(BaseRateFinding::warn(
            "Base rate is 0%. Every driver is a multiplier on it, so the model \
             can only ever return 0."
                .to_string(),
        ));
    } else if frequency >= 1.0 {
        out.push(BaseRateFinding::warn(
            "Base rate is 100%. No driver can move the forecast below certainty.".to_string(),
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Wilson ──────────────────────────────────────────────────────

    #[test]
    fn wilson_matches_known_values() {
        // 6 of 10 — the Manchester City anchor.
        let (lo, hi) = wilson_interval(0.6, 10);
        assert!((lo - 0.3127).abs() < 0.001, "lo was {lo}");
        assert!((hi - 0.8319).abs() < 0.001, "hi was {hi}");
    }

    #[test]
    fn wilson_never_collapses_at_the_extremes() {
        // The normal approximation gives the degenerate point 0.0 here.
        let (lo, hi) = wilson_interval(0.0, 10);
        assert_eq!(lo, 0.0);
        assert!(hi > 0.25, "upper bound collapsed to {hi}");

        let (lo, hi) = wilson_interval(1.0, 10);
        assert!(lo < 0.75, "lower bound collapsed to {lo}");
        assert_eq!(hi, 1.0);
    }

    #[test]
    fn wilson_tightens_as_n_grows() {
        let narrow = wilson_interval(0.5, 1000);
        let wide = wilson_interval(0.5, 10);
        assert!((narrow.1 - narrow.0) < (wide.1 - wide.0));
    }

    #[test]
    fn wilson_handles_zero_n() {
        assert_eq!(wilson_interval(0.5, 0), (0.0, 1.0));
    }

    // ── Subject extraction ──────────────────────────────────────────

    #[test]
    fn extracts_the_subject_entity() {
        assert_eq!(
            subject_entity(
                "Will Manchester City win the 2026-27 English Premier League (EPL) Championship?"
            )
            .as_deref(),
            Some("Manchester City")
        );
        assert_eq!(
            subject_entity("Will the highest temperature in London be 32C on August 14?")
                .as_deref(),
            Some("London")
        );
        assert_eq!(subject_entity("will it rain tomorrow?"), None);
    }

    // ── The reported case ───────────────────────────────────────────

    const EPL_Q: &str =
        "Will Manchester City win the 2026-27 English Premier League (EPL) Championship?";

    #[test]
    fn flags_the_manchester_city_anchor() {
        let f = critique_base_rate(
            EPL_Q,
            "Manchester City EPL title wins in Guardiola era (2016-17 to 2025-26)",
            0.60,
            Some(10),
        );
        assert_eq!(f.len(), 2, "expected small-n + circularity, got {f:#?}");
        assert!(f.iter().all(|x| x.severity == Severity::Warn));
        assert!(f[0].message.contains("31%–83%"), "got {}", f[0].message);
        assert!(f[1].message.contains("circular"));
    }

    #[test]
    fn accepts_a_broad_class_on_the_same_question() {
        // The honest anchor the agent computed and then discarded.
        let f = critique_base_rate(
            EPL_Q,
            "EPL titles won by the pre-season favourite (1992-2026)",
            0.24,
            Some(34),
        );
        assert!(f.is_empty(), "false positive: {f:#?}");
    }

    #[test]
    fn does_not_flag_a_large_class_that_names_the_subject() {
        // London appears in the class, but the subject is one draw from
        // 744 — that is what a reference class IS.
        let f = critique_base_rate(
            "Will the highest temperature in London be 32C on August 14?",
            "Days in London reaching 32C or higher during August (2000-2023)",
            0.003,
            Some(744),
        );
        assert!(f.is_empty(), "false positive: {f:#?}");
    }

    // ── Degenerate inputs ───────────────────────────────────────────

    #[test]
    fn flags_a_missing_sample_size() {
        let f = critique_base_rate(EPL_Q, "vibes", 0.6, None);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Warn);
        assert!(f[0].message.contains("no sample size"));
    }

    #[test]
    fn flags_a_zero_base_rate() {
        let f = critique_base_rate("Will X happen?", "some large class", 0.0, Some(500));
        assert!(f
            .iter()
            .any(|x| x.message.contains("can only ever return 0")));
    }

    #[test]
    fn flags_an_out_of_range_frequency() {
        let f = critique_base_rate("Will X happen?", "c", 1.4, Some(50));
        assert_eq!(f.len(), 1);
        assert!(f[0].message.contains("not a probability"));
    }

    #[test]
    fn notes_a_wide_interval_at_moderate_n() {
        let f = critique_base_rate("Will X happen?", "some class", 0.5, Some(25));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Note);
        assert!(f[0].message.contains("95% interval"));
    }
}
