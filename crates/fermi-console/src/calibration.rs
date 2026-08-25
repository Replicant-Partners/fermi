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

/// The empirical counterpart to the structural checks below.
///
/// # Why this module gained a semantic check after all
///
/// The doc above says these checks are deliberately structural — they ask
/// questions with defensible answers rather than judging whether a reference
/// class is wise. That still holds for `critique_base_rate`. This is different:
/// it does not judge the claim, it compares the claim against a MEASUREMENT of
/// the same quantity that the platform computed itself.
///
/// `weather_climatology` counts ERA5 observations over a calendar window and
/// applies an OLS warming trend, and `weather_oracle` reports the result as
/// `stages.calibration.climatology_base_rate`. The FPL separately declares
/// `historical_frequency`. Nothing had ever compared the two, even though a
/// disagreement means one of them is wrong about a question of fact.
///
/// Measured across the live weather forecasts at the time this was written:
///
/// | question              | declared | measured | gap    | relative |
/// |-----------------------|----------|----------|--------|----------|
/// | Chicago 78-79F        | 8.3%     | 13.5%    | 5.2pp  | 38%      |
/// | Miami 92-93F          | 12.0%    | 5.9%     | 6.1pp  | 103%     |
/// | Houston 74-75F low    | 12.0%    | 10.0%    | 2.0pp  | 20%      |
/// | London 32C            | 0.8%     | 1.04%    | 0.24pp | 23%      |
#[derive(Debug, Clone, PartialEq)]
pub enum BaseRateAgreement {
    /// No measurement to compare against. Not a pass.
    Unmeasured,
    /// Close enough that the difference is not worth an operator's attention.
    Agrees { declared: f64, measured: f64 },
    /// The two numbers are claims about the same frequency and they differ.
    Disagrees {
        declared: f64,
        measured: f64,
        /// Absolute difference in percentage points.
        gap_pp: f64,
        /// Difference relative to the measurement.
        relative: f64,
    },
}

/// Minimum absolute gap before a disagreement is worth raising, in pp.
///
/// Without a floor, two estimates of a rare event that round to "about 1%"
/// (London: 0.8% against 1.04%) would be reported as a 23% error. True, and not
/// useful: at that magnitude the reference classes differ by a day or two of
/// window and neither number is doing damage.
pub const AGREEMENT_MIN_GAP_PP: f64 = 1.0;

/// Minimum relative difference, against the measurement.
///
/// A pp threshold alone cannot work across base rates that span 0.8% to 33%:
/// 2pp is a rounding difference on one and a doubling on the other. Calibrated
/// against the four live forecasts so that the two which are materially wrong
/// fire and the two which are close stay quiet — Houston at 20% is the nearest
/// miss, and it is a genuine near-miss rather than a tuned exclusion: both its
/// numbers share the same bucket-width error, so they agree with each other
/// while both being wrong. This check cannot see that, and should not pretend to.
pub const AGREEMENT_MIN_RELATIVE: f64 = 0.25;

/// Does the declared base rate agree with the one the platform measured?
///
/// `measured` is the reference, so the relative difference is taken against it:
/// it came from counting observations, and the declared value is what someone
/// or something asserted.
pub fn base_rate_agreement(declared: f64, measured: Option<f64>) -> BaseRateAgreement {
    let Some(measured) = measured.filter(|m| m.is_finite() && *m > 0.0) else {
        return BaseRateAgreement::Unmeasured;
    };
    if !declared.is_finite() {
        return BaseRateAgreement::Unmeasured;
    }

    let gap_pp = (declared - measured).abs() * 100.0;
    let relative = (declared - measured).abs() / measured;

    if gap_pp >= AGREEMENT_MIN_GAP_PP && relative >= AGREEMENT_MIN_RELATIVE {
        BaseRateAgreement::Disagrees {
            declared,
            measured,
            gap_pp,
            relative,
        }
    } else {
        BaseRateAgreement::Agrees { declared, measured }
    }
}

impl BaseRateAgreement {
    /// Operator-facing sentence, or `None` when there is nothing to say.
    ///
    /// Phrased so the action is obvious: the two numbers are named, and so is
    /// the fact that they are answers to the same question. Deliberately does
    /// not say which is right — the measurement has a reference class too, and
    /// a bucket-width error puts both of them wrong together.
    pub fn message(&self) -> Option<String> {
        match self {
            BaseRateAgreement::Disagrees {
                declared,
                measured,
                gap_pp,
                relative,
            } => Some(format!(
                "Base rate disagreement: this forecast declares {:.1}% but the \
                 agent measured {:.1}% from climatology — {:.1}pp apart ({:.0}% \
                 relative). Both are estimates of the same frequency, so one of \
                 them is wrong. Check the reference class and the bucket bounds \
                 before treating any gap to the market as an edge.",
                declared * 100.0,
                measured * 100.0,
                gap_pp,
                relative * 100.0
            )),
            _ => None,
        }
    }
}

/// Pull the measured climatology base rate out of an agent response.
///
/// # Why the declared path is tried first and is not the only path
///
/// `grounding_trust::FIELD_CONTRACTS` declares this field at
/// `stages.calibration.climatology_base_rate`, sourced from the
/// `weather_climatology` tool's `base_rates.trend_adjusted_base_rate`. That is
/// the shape to honour.
///
/// A bare recursive search is kept as a fallback for the same reason the
/// weather cross-checks had to stop requiring the response to BE a JSON object:
/// the model wraps a correct document in prose and a fence, and nests it one
/// level deeper than asked, often enough that a strict reader returns nothing
/// and reports clean. An inert check is not a passing one.
///
/// This is the field `apply_base_rate_only`'s extractor could not read at all —
/// it accepts only `{"base_rate": {...}}` or a bare `historical_frequency`,
/// neither of which `weather_oracle`'s card ever emits. So the specialist was
/// routed to, ran, measured the number, and had its answer discarded.
pub fn extract_measured_base_rate(response: &serde_json::Value) -> Option<f64> {
    fn as_rate(v: &serde_json::Value) -> Option<f64> {
        v.as_f64()
            .filter(|f| f.is_finite() && (0.0..=1.0).contains(f))
    }

    /// The path `grounding_trust::FIELD_CONTRACTS` declares for this value.
    fn declared_path(v: &serde_json::Value) -> Option<f64> {
        v.get("stages")
            .and_then(|s| s.get("calibration"))
            .and_then(|c| c.get("climatology_base_rate"))
            .and_then(as_rate)
    }

    /// A JSON document carried inside a string.
    ///
    /// This is how the agent's answer actually arrives. The console reads
    /// `metadata.reasoning`, and the agent's whole document sits in there as
    /// TEXT — which is why `apply_base_rate_only` has always used
    /// `serde_json::from_str` on that field rather than walking it. Tolerates a
    /// ```json fence and surrounding prose by taking the outermost braces, the
    /// same allowance the weather cross-checks needed in 2d0ed9f6.
    fn embedded(s: &str) -> Option<serde_json::Value> {
        let t = s.trim();
        let (lo, hi) = (t.find('{')?, t.rfind('}')?);
        if hi <= lo {
            return None;
        }
        serde_json::from_str(&t[lo..=hi]).ok()
    }

    /// `depth` bounds the descent through nested embedded documents. Two is
    /// already more nesting than has ever been observed; the bound exists so a
    /// pathological response cannot recurse without end.
    fn find(v: &serde_json::Value, depth: u8) -> Option<f64> {
        if let Some(hit) = declared_path(v) {
            return Some(hit);
        }
        match v {
            serde_json::Value::Object(map) => {
                if let Some(hit) = map.get("climatology_base_rate").and_then(as_rate) {
                    return Some(hit);
                }
                map.values().find_map(|x| find(x, depth))
            }
            serde_json::Value::Array(items) => items.iter().find_map(|x| find(x, depth)),
            serde_json::Value::String(s) if depth > 0 => {
                embedded(s).and_then(|inner| find(&inner, depth - 1))
            }
            _ => None,
        }
    }

    find(response, 2)
}

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

    // ── Base-rate agreement against a measurement ────────────────────────

    fn gap(a: &BaseRateAgreement) -> f64 {
        match a {
            BaseRateAgreement::Disagrees { gap_pp, .. } => *gap_pp,
            other => panic!("expected Disagrees, got {other:?}"),
        }
    }

    /// The four live weather forecasts, with the numbers they actually carried.
    ///
    /// Two are materially wrong and two are close. A check that fires on all
    /// four is wallpaper; one that fires on none is inert. This pins which.
    #[test]
    fn the_two_forecasts_that_were_wrong_fire_and_the_two_that_were_close_do_not() {
        // Chicago 78-79F: declared 8.3%, ERA5 measured 13.5%.
        let chicago = base_rate_agreement(0.083, Some(0.135));
        assert!(
            matches!(chicago, BaseRateAgreement::Disagrees { .. }),
            "38% low on the most load-bearing number in the model: {chicago:?}"
        );
        assert!((gap(&chicago) - 5.2).abs() < 0.05, "{chicago:?}");

        // Miami 92-93F: declared 12.0%, measured 5.9% — declared is 2x.
        let miami = base_rate_agreement(0.12, Some(0.059));
        assert!(
            matches!(miami, BaseRateAgreement::Disagrees { .. }),
            "a doubled base rate doubles every driver's effect: {miami:?}"
        );

        // Houston 74-75F low: 12.0% vs 10.0%. A genuine near-miss at 20%
        // relative — and both numbers share the same bucket-width error, so
        // they agree with each other while both being wrong. This check cannot
        // see that and must not pretend to.
        assert!(matches!(
            base_rate_agreement(0.12, Some(0.10)),
            BaseRateAgreement::Agrees { .. }
        ));

        // London 32C: 0.8% vs 1.04%. 23% relative but a quarter of a point
        // absolute — at this magnitude the reference classes differ by a day of
        // window and neither number is doing damage.
        assert!(matches!(
            base_rate_agreement(0.008, Some(0.0104)),
            BaseRateAgreement::Agrees { .. }
        ));
    }

    /// A missing measurement is not a pass.
    ///
    /// The failure this whole line of work keeps finding: a check whose input is
    /// absent reporting clean. `Unmeasured` is a distinct state so a caller
    /// cannot mistake "nothing to compare" for "compared and agreed".
    #[test]
    fn no_measurement_is_unmeasured_rather_than_agreement() {
        assert_eq!(
            base_rate_agreement(0.083, None),
            BaseRateAgreement::Unmeasured
        );
        assert_eq!(
            base_rate_agreement(0.083, Some(f64::NAN)),
            BaseRateAgreement::Unmeasured
        );
        assert_eq!(
            base_rate_agreement(0.083, Some(0.0)),
            BaseRateAgreement::Unmeasured,
            "a measured zero is a degenerate reference class, not a frequency"
        );
    }

    #[test]
    fn only_a_disagreement_produces_a_message() {
        assert!(base_rate_agreement(0.12, Some(0.059)).message().is_some());
        assert!(base_rate_agreement(0.12, Some(0.10)).message().is_none());
        assert!(base_rate_agreement(0.12, None).message().is_none());
    }

    /// The message names both numbers, so it can be acted on without a query.
    #[test]
    fn the_message_names_both_numbers_and_neither_as_correct() {
        let m = base_rate_agreement(0.12, Some(0.059)).message().unwrap();
        assert!(m.contains("12.0%"), "{m}");
        assert!(m.contains("5.9%"), "{m}");
        assert!(
            m.contains("bucket bounds"),
            "the measurement has a reference class too — a bucket-width error \
             puts both numbers wrong together, and the message must point there: {m}"
        );
    }

    // ── Reading the measurement out of the response ──────────────────────

    /// The shape the contract declares, and the shape the agent actually sends.
    #[test]
    fn the_declared_contract_path_is_read() {
        let r = serde_json::json!({
            "stages": {
                "calibration": {
                    "agent": "weather_calibrator",
                    "predictive_sd": 5.0,
                    "climatology_base_rate": 0.0305,
                    "calibrated_probability": 0.031
                }
            }
        });
        assert_eq!(extract_measured_base_rate(&r), Some(0.0305));
    }

    /// Nested one level deeper than asked, which is how it often arrives.
    ///
    /// The weather cross-checks were permanently inert for exactly this reason:
    /// they required the response to BE the document, and the model wraps it.
    /// A strict reader here would return None and the comparison would silently
    /// never run — an inert check reporting clean, which is the failure mode
    /// this whole effort exists to remove.
    #[test]
    fn a_measurement_nested_deeper_than_declared_is_still_found() {
        let r = serde_json::json!({
            "result": { "output": { "stages": {
                "calibration": { "climatology_base_rate": 0.135 }
            }}}
        });
        assert_eq!(extract_measured_base_rate(&r), Some(0.135));
    }

    #[test]
    fn an_out_of_range_measurement_is_not_a_base_rate() {
        let r = serde_json::json!({ "climatology_base_rate": 13.5 });
        assert_eq!(
            extract_measured_base_rate(&r),
            None,
            "13.5 is a percentage written as a number; treating it as a \
             probability would make every comparison fire"
        );
    }

    /// The shape the console actually receives, which defeated the first version.
    ///
    /// The agent's document does not arrive as a nested JSON object. It arrives
    /// as TEXT in `metadata.reasoning` — which is why `apply_base_rate_only` has
    /// always used `serde_json::from_str` on that field. The original recursive
    /// search walked `Value::Object` and `Value::Array` and never descended into
    /// a `Value::String`, so it returned `None` on every real response and the
    /// comparison silently never ran.
    ///
    /// Measured consequence, from the live Houston forecast: `weather_oracle`
    /// reported `climatology_base_rate = 0.32` while the forecast carried 12.0%,
    /// a 20pp disagreement in the term that IS the forecast, and the check that
    /// exists to catch exactly that reported nothing.
    ///
    /// An inert check is not a passing one. This is that failure, one level up
    /// from the one the check was written for.
    #[test]
    fn a_document_delivered_as_a_string_is_still_read() {
        let doc = serde_json::json!({
            "settlement_target": { "station": "KHOU", "bucket_lo": 76, "bucket_hi": 77 },
            "stages": {
                "calibration": {
                    "agent": "weather_calibrator",
                    "predictive_sd": 2.25,
                    "calibrated_probability": 0.32,
                    "climatology_base_rate": 0.32,
                    "sd_was_measured": true
                }
            },
            "final_probability": 0.32
        });

        let response = serde_json::json!({
            "agent_id": "weather_oracle",
            "status": "success",
            "metadata": { "reasoning": serde_json::to_string(&doc).unwrap() },
            "evidence": [{ "source": "weather_oracle", "summary": "ERA5 climatology" }]
        });

        assert_eq!(
            extract_measured_base_rate(&response),
            Some(0.32),
            "the measurement is in the response, one string deep"
        );
    }

    /// Fenced and wrapped in prose, which is how models actually reply.
    #[test]
    fn a_fenced_document_surrounded_by_prose_is_still_read() {
        let response = serde_json::json!({
            "metadata": {
                "reasoning": "Here is my analysis.\n\n```json\n{\"stages\":                     {\"calibration\": {\"climatology_base_rate\": 0.135}}}\n```\n                    Let me know if you need more."
            }
        });
        assert_eq!(extract_measured_base_rate(&response), Some(0.135));
    }

    /// The live disagreement this was built to surface.
    ///
    /// Houston, 2026-08-21: the forecast declared 12.0%; `weather_oracle`
    /// measured 32% from 330 ERA5 observations in the Aug 16-26 window at KHOU,
    /// trend-adjusted to 2025. The crowd was at 27.5%. The forecast was wrong by
    /// a factor of ~2.7 and reported the resulting 15.5pp gap as a possible edge.
    #[test]
    fn the_houston_disagreement_is_caught_end_to_end() {
        let response = serde_json::json!({
            "metadata": {
                "reasoning": "{\"stages\": {\"calibration\":                     {\"climatology_base_rate\": 0.32}}}"
            }
        });
        let measured = extract_measured_base_rate(&response);
        assert_eq!(measured, Some(0.32));

        let verdict = base_rate_agreement(0.12, measured);
        assert!(
            matches!(verdict, BaseRateAgreement::Disagrees { .. }),
            "{verdict:?}"
        );
        let msg = verdict.message().expect("a disagreement speaks");
        assert!(msg.contains("12.0%"), "{msg}");
        assert!(msg.contains("32.0%"), "{msg}");
    }

    #[test]
    fn a_response_with_no_measurement_yields_none() {
        let r = serde_json::json!({ "stages": { "calibration": { "predictive_sd": 5.0 } } });
        assert_eq!(extract_measured_base_rate(&r), None);
    }
}
