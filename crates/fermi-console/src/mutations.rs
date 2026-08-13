//! Validating the model edits Fermi proposes in chat.
//!
//! This is the symbolic half of the loop. Conversation with Fermi is
//! neuro; the FPL program is symbolic; research agents are neuro again;
//! the Brier score closes it. Until now the chat could only *navigate*
//! the console — `open_forecast`, `open_panel`, `run_simulation` — so the
//! loop was open: Fermi could say "your manager_continuity p50 should be
//! 0.65, Guardiola has left" and the operator had to go and type it.
//!
//! These are writes to the forecast, proposed by a language model. Two
//! properties matter more than convenience:
//!
//!   * **Every field is validated before it touches the AST.** An LLM
//!     will eventually emit `p5=1.2, p50=0.8`, a probability of 1.4, or
//!     the string `"0.9"` where a number belongs. A backwards triangular
//!     distribution does not fail loudly — it silently produces a
//!     nonsense forecast, which is the failure mode this codebase has
//!     already been bitten by more than once.
//!   * **Parsing is separated from applying.** Validation is pure and
//!     tested here; the caller does the mutation. That keeps "is this
//!     edit legal?" answerable without a GPUI context.
//!
//! Approval stays with the operator: chat actions render as chips that
//! must be clicked. Nothing here bypasses that, and nothing here
//! bypasses `refuse_write()`.

use serde_json::Value as JsonValue;

/// A validated triangular distribution for a continuous driver.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DistributionEdit {
    pub p5: f64,
    pub p50: f64,
    pub p95: f64,
}

/// A validated binary-driver edit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BinaryEdit {
    pub probability: f64,
    pub impact_multiplier: f64,
}

/// A validated base-rate edit.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseRateEdit {
    pub historical_frequency: f64,
    pub reference_class: String,
    pub sample_size: Option<usize>,
    pub reasoning: Option<String>,
}

/// Read a required finite number, tolerating a numeric string.
///
/// LLMs quote numbers. Rejecting `"0.9"` when the intent is unambiguous
/// would make the feature feel broken for a reason the operator cannot
/// see or fix.
fn num(args: &JsonValue, key: &str) -> Result<f64, String> {
    let v = args.get(key).ok_or_else(|| format!("missing `{key}`"))?;
    let n = v
        .as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<f64>().ok()))
        .ok_or_else(|| format!("`{key}` is not a number: {v}"))?;
    if !n.is_finite() {
        return Err(format!("`{key}` is not finite"));
    }
    Ok(n)
}

fn opt_num(args: &JsonValue, key: &str) -> Result<Option<f64>, String> {
    if args.get(key).map(|v| v.is_null()).unwrap_or(true) {
        return Ok(None);
    }
    num(args, key).map(Some)
}

/// The driver an action targets.
pub fn driver_name(args: &JsonValue) -> Result<String, String> {
    let name = args
        .get("driver")
        .or_else(|| args.get("driver_name"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("missing `driver`")?;
    Ok(name.to_string())
}

/// Validate a `set_driver_distribution` payload.
///
/// Enforces `p5 <= p50 <= p95`, because a backwards triangular is not a
/// distribution and the executor will happily sample from it anyway.
/// Also rejects non-positive bounds: drivers are multipliers on a base
/// rate, and a negative or zero multiplier is either a sign error or a
/// claim that the outcome is impossible — neither should arrive silently
/// from a chat reply.
pub fn parse_distribution(args: &JsonValue) -> Result<DistributionEdit, String> {
    let p5 = num(args, "p5")?;
    let p50 = num(args, "p50")?;
    let p95 = num(args, "p95")?;

    if !(p5 <= p50 && p50 <= p95) {
        return Err(format!(
            "p5 ≤ p50 ≤ p95 violated: got p5={p5}, p50={p50}, p95={p95}"
        ));
    }
    if p5 <= 0.0 {
        return Err(format!(
            "p5={p5} is not positive; drivers are multipliers on the base rate"
        ));
    }
    // A multiplier this large is almost always a confusion between
    // "multiplier" and "percent" (e.g. 65 meaning 0.65).
    if p95 > 100.0 {
        return Err(format!(
            "p95={p95} is implausible for a multiplier — did you mean {}?",
            p95 / 100.0
        ));
    }

    Ok(DistributionEdit { p5, p50, p95 })
}

/// Validate a `set_driver_probability` payload for a binary driver.
pub fn parse_binary(args: &JsonValue) -> Result<BinaryEdit, String> {
    let probability = num(args, "probability")?;
    if !(0.0..=1.0).contains(&probability) {
        return Err(format!(
            "probability={probability} is outside [0,1]{}",
            if probability > 1.0 && probability <= 100.0 {
                format!(" — did you mean {}?", probability / 100.0)
            } else {
                String::new()
            }
        ));
    }
    // Default 1.0 = "the event happens but changes nothing", which is
    // the safe reading of an omitted impact.
    let impact_multiplier = opt_num(args, "impact_multiplier")?.unwrap_or(1.0);
    if impact_multiplier < 0.0 {
        return Err(format!("impact_multiplier={impact_multiplier} is negative"));
    }
    Ok(BinaryEdit {
        probability,
        impact_multiplier,
    })
}

/// Validate a `set_base_rate` payload.
///
/// The reference class is required, not decorative. A frequency with no
/// class is the exact failure `calibration` exists to catch, and letting
/// chat write one without it would route around that check.
pub fn parse_base_rate(args: &JsonValue) -> Result<BaseRateEdit, String> {
    let f = num(args, "historical_frequency")?;
    let historical_frequency = if f > 1.0 && f <= 100.0 {
        // Percent given where a fraction was asked for. Unambiguous
        // enough to accept, since a base rate above 1.0 is meaningless.
        f / 100.0
    } else {
        f
    };
    if !(0.0..=1.0).contains(&historical_frequency) {
        return Err(format!("historical_frequency={f} is not a probability"));
    }

    let reference_class = args
        .get("reference_class")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("missing `reference_class` — a frequency without a class is an assertion, not a base rate")?
        .to_string();

    let sample_size = match opt_num(args, "sample_size")? {
        Some(n) if n >= 1.0 => Some(n as usize),
        Some(n) => return Err(format!("sample_size={n} must be at least 1")),
        None => None,
    };

    let reasoning = args
        .get("reasoning")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    Ok(BaseRateEdit {
        historical_frequency,
        reference_class,
        sample_size,
        reasoning,
    })
}

/// Validate an `assign_agent` payload. Returns `(driver, agent_id)`.
pub fn parse_assign_agent(args: &JsonValue) -> Result<(String, String), String> {
    let driver = driver_name(args)?;
    let agent = args
        .get("agent_id")
        .or_else(|| args.get("agent"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("missing `agent_id`")?
        .to_string();
    Ok((driver, agent))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Distributions ───────────────────────────────────────────────

    #[test]
    fn accepts_a_well_formed_distribution() {
        let e = parse_distribution(&json!({"p5": 0.8, "p50": 1.0, "p95": 1.3})).unwrap();
        assert_eq!(
            e,
            DistributionEdit {
                p5: 0.8,
                p50: 1.0,
                p95: 1.3
            }
        );
    }

    #[test]
    fn rejects_a_backwards_distribution() {
        // The dangerous case: it does not fail loudly downstream, it just
        // produces a nonsense forecast.
        let err = parse_distribution(&json!({"p5": 1.2, "p50": 0.8, "p95": 1.5})).unwrap_err();
        assert!(err.contains("p5 ≤ p50 ≤ p95"), "got: {err}");
    }

    #[test]
    fn rejects_p50_above_p95() {
        assert!(parse_distribution(&json!({"p5": 0.5, "p50": 1.9, "p95": 1.1})).is_err());
    }

    #[test]
    fn allows_a_degenerate_but_ordered_distribution() {
        // p5 == p50 == p95 is a point mass: unusual, not illegal, and a
        // legitimate way to pin a driver while investigating another.
        assert!(parse_distribution(&json!({"p5": 1.0, "p50": 1.0, "p95": 1.0})).is_ok());
    }

    #[test]
    fn rejects_non_positive_multipliers() {
        let err = parse_distribution(&json!({"p5": 0.0, "p50": 0.5, "p95": 1.0})).unwrap_err();
        assert!(err.contains("not positive"), "got: {err}");
        assert!(parse_distribution(&json!({"p5": -0.5, "p50": 0.5, "p95": 1.0})).is_err());
    }

    #[test]
    fn catches_percent_confusion_in_a_multiplier() {
        let err = parse_distribution(&json!({"p5": 50, "p50": 65, "p95": 800})).unwrap_err();
        assert!(err.contains("did you mean 8"), "got: {err}");
    }

    #[test]
    fn tolerates_quoted_numbers() {
        // LLMs quote numbers. The intent is unambiguous.
        let e = parse_distribution(&json!({"p5": "0.8", "p50": "1.0", "p95": "1.3"})).unwrap();
        assert_eq!(e.p50, 1.0);
    }

    #[test]
    fn rejects_missing_and_non_numeric_fields() {
        assert!(parse_distribution(&json!({"p5": 0.8, "p50": 1.0})).is_err());
        assert!(parse_distribution(&json!({"p5": 0.8, "p50": "high", "p95": 1.3})).is_err());
    }

    // ── Binary drivers ──────────────────────────────────────────────

    #[test]
    fn accepts_a_binary_edit_and_defaults_the_impact() {
        let e = parse_binary(&json!({"probability": 0.15})).unwrap();
        assert_eq!(e.probability, 0.15);
        // Omitted impact means "happens, changes nothing".
        assert_eq!(e.impact_multiplier, 1.0);
    }

    #[test]
    fn rejects_a_probability_outside_zero_one() {
        let err = parse_binary(&json!({"probability": 15})).unwrap_err();
        assert!(err.contains("did you mean 0.15"), "got: {err}");
        assert!(parse_binary(&json!({"probability": -0.1})).is_err());
    }

    #[test]
    fn rejects_a_negative_impact() {
        assert!(parse_binary(&json!({"probability": 0.2, "impact_multiplier": -1.0})).is_err());
    }

    // ── Base rate ───────────────────────────────────────────────────

    #[test]
    fn accepts_a_base_rate_with_a_class() {
        let e = parse_base_rate(&json!({
            "historical_frequency": 0.24,
            "reference_class": "EPL titles won by the pre-season favourite",
            "sample_size": 34,
            "reasoning": "8 of 34 since 1992"
        }))
        .unwrap();
        assert_eq!(e.historical_frequency, 0.24);
        assert_eq!(e.sample_size, Some(34));
        assert!(e.reasoning.is_some());
    }

    #[test]
    fn refuses_a_base_rate_with_no_reference_class() {
        // Routing around `calibration` is not allowed just because the
        // edit arrived from chat instead of the UI.
        let err = parse_base_rate(&json!({"historical_frequency": 0.6})).unwrap_err();
        assert!(err.contains("reference_class"), "got: {err}");
        // Blank is the same as absent.
        assert!(parse_base_rate(&json!({
            "historical_frequency": 0.6,
            "reference_class": "   "
        }))
        .is_err());
    }

    #[test]
    fn normalises_a_percentage_base_rate() {
        // 58 can only mean 58% — a base rate above 1.0 is meaningless.
        let e = parse_base_rate(&json!({
            "historical_frequency": 58,
            "reference_class": "pre-season favourites"
        }))
        .unwrap();
        assert!((e.historical_frequency - 0.58).abs() < 1e-9);
    }

    #[test]
    fn rejects_an_impossible_base_rate() {
        assert!(parse_base_rate(&json!({
            "historical_frequency": 140,
            "reference_class": "c"
        }))
        .is_err());
    }

    #[test]
    fn rejects_a_zero_sample_size() {
        assert!(parse_base_rate(&json!({
            "historical_frequency": 0.3,
            "reference_class": "c",
            "sample_size": 0
        }))
        .is_err());
    }

    // ── Targeting ───────────────────────────────────────────────────

    #[test]
    fn accepts_either_driver_key() {
        assert_eq!(driver_name(&json!({"driver": "squad"})).unwrap(), "squad");
        assert_eq!(
            driver_name(&json!({"driver_name": "squad"})).unwrap(),
            "squad"
        );
        assert!(driver_name(&json!({"d": "squad"})).is_err());
        assert!(driver_name(&json!({"driver": "  "})).is_err());
    }

    #[test]
    fn parses_an_agent_assignment() {
        let (d, a) =
            parse_assign_agent(&json!({"driver": "squad", "agent_id": "football_analyst"}))
                .unwrap();
        assert_eq!(d, "squad");
        assert_eq!(a, "football_analyst");
        // `agent` is accepted as an alias.
        assert!(parse_assign_agent(&json!({"driver": "s", "agent": "nba_analyst"})).is_ok());
        assert!(parse_assign_agent(&json!({"driver": "s"})).is_err());
    }
}
