//! Prose for the cockpit's Trajectory pane.
//!
//! The Trajectory pane exists to answer one question: *did this system
//! event have anything to do with the rate moving?* For a long time it
//! could not, because it rendered events as bare kind names — rows that
//! read `market_observation · market_observation`, repeated 17 times,
//! with a microsecond timestamp and nothing else. An operator looking at
//! that has to hold two time series in their head and correlate by eye,
//! which is precisely the work the pane was built to do for them.
//!
//! This module turns an annotated timeline event into text with enough
//! semantic content to be read in context. Three jobs:
//!
//!   1. **Name the event** ([`humanize_event_kind`]) — never print a
//!      snake_case identifier at a human, and never print it twice.
//!   2. **Describe what it observed** ([`market_tick_text`]) — a market
//!      tick is a price, a move, and a reason to trust the quote.
//!   3. **Relate it to the rate trace** ([`build_correlation_line`],
//!      [`build_phase_summary`]) — where the model and crowd stood at
//!      that instant, and which revision (if any) followed.
//!
//! On causation: these functions report temporal adjacency *as*
//! adjacency. "12m before the +4.6pp revision" is a statement about
//! clocks, not mechanism. The operator draws the causal conclusion; the
//! UI's job is to supply the timing and the gap so that they can, and to
//! avoid implying more than the data supports.
//!
//! Inputs are the JSON events from `GET /api/forecasts/:id/timeline`,
//! which the server annotates with `model_rate_at_pct`,
//! `crowd_price_at_pct`, `divergence_at_pp`, `next_revision_*` and
//! `since_prev_revision_secs`. Every field is read defensively — a
//! missing annotation degrades the sentence, it never panics and never
//! fabricates a number.
//!
//! Lives in the lib target because the binary's `#[cfg(test)]` modules
//! are unrunnable (see the crate docs on rustc's stack overflow when
//! expanding the GPUI element tree under `--test`). These are pure
//! `JsonValue -> String` functions; they are exactly what belongs here.

use serde_json::Value as JsonValue;

/// Smallest movement worth reporting, in percentage points. Below this a
/// tick is "unchanged" rather than a spurious `+0.0pp`.
const EPSILON_PP: f64 = 0.05;

/// Turn a snake_case system event kind into something an operator can
/// actually read: `bayesops_fit_accepted` → `"BayesOps fit accepted"`.
///
/// Known initialisms are preserved in their conventional casing;
/// everything else is sentence case, because these appear as row
/// headlines rather than titles.
pub fn humanize_event_kind(kind: &str) -> String {
    let mut out = String::with_capacity(kind.len());
    for (i, word) in kind.split('_').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        match word {
            "bayesops" => out.push_str("BayesOps"),
            "fpl" => out.push_str("FPL"),
            "pm" => out.push_str("PM"),
            "ci" => out.push_str("CI"),
            _ if i == 0 => {
                let mut chars = word.chars();
                if let Some(first) = chars.next() {
                    out.extend(first.to_uppercase());
                    out.push_str(chars.as_str());
                }
            }
            _ => out.push_str(word),
        }
    }
    out
}

/// Compact elapsed time for correlation phrasing: `45s`, `12m`,
/// `3h 10m`, `2d`.
///
/// Deliberately coarse. The operator needs "about how long ago", and a
/// lag printed to the millisecond implies a precision the correlation
/// itself does not have.
pub fn format_lag_secs(secs: f64) -> String {
    let s = secs.abs();
    if s < 90.0 {
        return format!("{:.0}s", s);
    }
    // Round to whole minutes first, then carry into hours from that
    // rounded value. Deriving hours and minutes independently from the
    // raw seconds lets 3599s round to "0h 60m", and dropping into the
    // hours branch only above 90 minutes lets 3600s read "60m".
    let mins = (s / 60.0).round();
    if mins < 60.0 {
        return format!("{:.0}m", mins);
    }
    if mins >= 2880.0 {
        return format!("{:.0}d", mins / 1440.0);
    }
    let h = (mins / 60.0).floor();
    let m = mins - h * 60.0;
    if m >= 1.0 {
        format!("{:.0}h {:.0}m", h, m)
    } else {
        format!("{:.0}h", h)
    }
}

/// Compact USD for volume and liquidity: `$642`, `$18.4k`, `$2.1M`.
pub fn format_usd_compact(v: f64) -> String {
    let a = v.abs();
    if a >= 1_000_000.0 {
        format!("${:.1}M", v / 1_000_000.0)
    } else if a >= 1_000.0 {
        format!("${:.1}k", v / 1_000.0)
    } else {
        format!("${:.0}", v)
    }
}

/// Compact timestamp for an event row: relative age plus wall clock to
/// the minute, e.g. `"5h ago · 22:03"`.
///
/// The raw `2026-08-14T04:09:26.138076+00:00` used to render verbatim,
/// so every row ended in 26 characters of microsecond noise that pushed
/// the meaningful text out of the viewport.
pub fn format_event_timestamp(ts: &str, relative: &str) -> String {
    if ts.is_empty() {
        return String::new();
    }
    let clock = ts
        .split('T')
        .nth(1)
        .map(|t| t.chars().take(5).collect::<String>())
        .unwrap_or_default();
    if clock.is_empty() || relative.is_empty() {
        if clock.is_empty() {
            relative.to_string()
        } else {
            clock
        }
    } else {
        format!("{} · {}", relative, clock)
    }
}

/// Signed tick-to-tick move of a market observation, in percentage
/// points, or `None` for the first tick in a series.
///
/// Exposed separately from [`market_tick_text`] so the UI can pick a
/// direction colour and glyph from the same number the prose reports.
pub fn market_tick_delta_pp(ev: &JsonValue) -> Option<f64> {
    ev.get("tick_delta_pp").and_then(|v| v.as_f64())
}

/// Crowd price of a market observation as a percentage.
///
/// Prefers `market_price` and falls back to `predicted_probability`,
/// which is where the timeline endpoint also stashes the price so the
/// chart's generic event-y lookup can place the dot on the crowd worm.
pub fn market_price_pct(ev: &JsonValue) -> Option<f64> {
    ev.get("market_price")
        .and_then(|v| v.as_f64())
        .or_else(|| ev.get("predicted_probability").and_then(|v| v.as_f64()))
        .map(|p| p * 100.0)
}

/// Headline and detail for a market observation.
///
/// Returns `(headline, detail)`:
///
/// ```text
/// Crowd 5.0% (−0.3pp)
/// scheduled poll · low confidence · bid 4.0% / ask 6.0% · $0/24h · $642 liquidity
/// ```
///
/// Market ticks are the highest-volume event on the timeline and used to
/// be the least informative — they had no renderer at all and fell
/// through to a fallback that printed the kind name twice. What a tick
/// is actually worth is the price, the move since the last one, and
/// whether the quote is deep enough to believe: an unchanged price on
/// `$0` of 24h volume is a very different signal from the same price on
/// real depth.
pub fn market_tick_text(ev: &JsonValue) -> (String, String) {
    let price_pct = market_price_pct(ev);
    let tick = market_tick_delta_pp(ev);

    let headline = match (price_pct, tick) {
        (Some(p), Some(d)) if d.abs() >= EPSILON_PP => format!("Crowd {:.1}% ({:+.1}pp)", p, d),
        (Some(p), Some(_)) => format!("Crowd {:.1}% (unchanged)", p),
        (Some(p), None) => format!("Crowd {:.1}% (first tick)", p),
        (None, _) => "Crowd tick · no price".to_string(),
    };

    // Detail fragments, most decision-relevant first: how the tick was
    // sampled, then quote quality, then depth, then the market's own
    // recent trend, then resolution.
    let mut frags: Vec<String> = Vec::new();

    if let Some(t) = ev.get("observation_type").and_then(|v| v.as_str()) {
        frags.push(observation_type_phrase(t).to_string());
    }
    if let Some(c) = ev.get("confidence_signal").and_then(|v| v.as_str()) {
        frags.push(format!("{} confidence", c.replace('_', " ")));
    }

    let bid = ev.get("bid_price").and_then(|v| v.as_f64());
    let ask = ev.get("ask_price").and_then(|v| v.as_f64());
    match (bid, ask) {
        (Some(b), Some(a)) => frags.push(format!("bid {:.1}% / ask {:.1}%", b * 100.0, a * 100.0)),
        _ => {
            if let Some(s) = ev.get("spread").and_then(|v| v.as_f64()) {
                frags.push(format!("spread {:.1}pp", s * 100.0));
            }
        }
    }

    if let Some(v) = ev.get("volume_24h").and_then(|v| v.as_f64()) {
        frags.push(format!("{}/24h", format_usd_compact(v)));
    }
    if let Some(l) = ev.get("liquidity").and_then(|v| v.as_f64()) {
        frags.push(format!("{} liquidity", format_usd_compact(l)));
    }

    let trend: Vec<String> = [("1h", "price_change_1h"), ("1d", "price_change_1d")]
        .iter()
        .filter_map(|(label, key)| {
            ev.get(*key)
                .and_then(|v| v.as_f64())
                .filter(|d| d.abs() * 100.0 >= EPSILON_PP)
                .map(|d| format!("{} {:+.1}pp", label, d * 100.0))
        })
        .collect();
    if !trend.is_empty() {
        frags.push(trend.join(" · "));
    }

    if ev.get("pm_resolved").and_then(|v| v.as_bool()) == Some(true) {
        let outcome = ev
            .get("pm_outcome")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        frags.push(format!("market RESOLVED → {}", outcome));
    }

    (headline, frags.join(" · "))
}

/// How an observation was sampled, in words. Mirrors the
/// `observation_type` CHECK constraint on `fermi_market_observations`;
/// unknown values pass through so a new type is never swallowed.
fn observation_type_phrase(t: &str) -> &str {
    match t {
        "scheduled" => "scheduled poll",
        "refresh" => "manual refresh",
        "manual_link" => "market linked",
        "agent_research" => "agent research",
        "resolution_check" => "resolution check",
        "import" => "import",
        "search" => "search",
        other => other,
    }
}

/// The line that makes the Trajectory pane a *correlation* view rather
/// than a log: where this event sits relative to the rate trace.
///
/// ```text
/// ↳ 12m before the +4.6pp revision · model 3.4% vs crowd 5.3% (model −1.9pp)
/// ```
///
/// Two clauses. First the temporal relation to the nearest rate movement
/// — and when no revision followed, that absence is itself the finding
/// ("no revision since"), because an event that did *not* move the rate
/// is information the operator needs just as much as one that did.
/// Second the state of both worms at that instant, so the reader never
/// has to look up at the chart to know what the numbers were.
///
/// Returns an empty string when the event carries no annotations at all,
/// so callers can omit the row rather than render a stray arrow.
pub fn build_correlation_line(ev: &JsonValue) -> String {
    let f = |k: &str| ev.get(k).and_then(|v| v.as_f64());
    let kind = ev.get("kind").and_then(|v| v.as_str()).unwrap_or("");

    let mut parts: Vec<String> = Vec::new();

    // 1. Temporal relation to the nearest rate movement.
    if kind == "rate_revision" {
        if let Some(s) = f("since_prev_revision_secs") {
            parts.push(format!(
                "{} after the previous revision",
                format_lag_secs(s)
            ));
        }
    } else {
        // Naming the trigger distinguishes "this event was followed by a
        // revision someone typed by hand" from "...by a cascade the
        // system applied" — a difference that decides whether the
        // adjacency is worth a second look at all.
        let noun = match ev.get("next_revision_trigger").and_then(|v| v.as_str()) {
            Some(t) if !t.is_empty() => format!("{} revision", t.replace('_', " ")),
            _ => "revision".to_string(),
        };
        match (
            f("next_revision_lag_secs"),
            f("next_revision_delta_pp"),
            f("next_revision_to_pct"),
        ) {
            (Some(lag), Some(d), _) if d.abs() >= EPSILON_PP => parts.push(format!(
                "{} before the {:+.1}pp {}",
                format_lag_secs(lag),
                d,
                noun
            )),
            (Some(lag), _, Some(to)) => parts.push(format!(
                "{} before the {} to {:.1}%",
                format_lag_secs(lag),
                noun,
                to
            )),
            (Some(lag), _, None) => {
                parts.push(format!("{} before the next {}", format_lag_secs(lag), noun))
            }
            // No revision after this event is itself the finding: the
            // event did not move the rate. Say so rather than leaving a
            // gap the operator has to interpret.
            _ => parts.push("no revision since".to_string()),
        }
    }

    // 2. State of both worms at that instant.
    match (
        f("model_rate_at_pct"),
        f("crowd_price_at_pct"),
        f("divergence_at_pp"),
    ) {
        (Some(m), Some(c), Some(d)) => parts.push(format!(
            "model {:.1}% vs crowd {:.1}% (model {:+.1}pp)",
            m, c, d
        )),
        (Some(m), Some(c), None) => parts.push(format!("model {:.1}% vs crowd {:.1}%", m, c)),
        (Some(m), None, _) => parts.push(format!("model {:.1}%", m)),
        (None, Some(c), _) => parts.push(format!("crowd {:.1}%", c)),
        _ => {}
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!("↳ {}", parts.join(" · "))
    }
}

/// Context clause for a rate-revision phase header.
///
/// `3.4% → 8.0%` states a number change with no frame of reference. What
/// an operator decides on is whether the move went *toward* or *away
/// from* the crowd, where the gap now stands, and how long the phase
/// that produced it ran:
///
/// ```text
/// 1.9pp away from the crowd · crowd 5.0% · gap now +3.0pp · 6h since previous revision
/// ```
pub fn build_revision_context(ev: &JsonValue) -> String {
    let f = |k: &str| ev.get(k).and_then(|v| v.as_f64());
    let to = f("predicted_probability");
    let from = f("previous_probability");
    let crowd = f("crowd_price_at_pct");

    let mut ctx: Vec<String> = Vec::new();

    if let (Some(p), Some(t), Some(c)) = (from, to, crowd) {
        let before = (p * 100.0 - c).abs();
        let after = (t * 100.0 - c).abs();
        if (before - after).abs() >= EPSILON_PP {
            let direction = if after < before {
                "toward"
            } else {
                "away from"
            };
            ctx.push(format!(
                "{:.1}pp {} the crowd",
                (before - after).abs(),
                direction
            ));
        }
    }

    match (crowd, f("divergence_at_pp")) {
        (Some(c), Some(d)) => ctx.push(format!("crowd {:.1}% · gap now {:+.1}pp", c, d)),
        (Some(c), None) => ctx.push(format!("crowd {:.1}%", c)),
        _ => {}
    }

    if let Some(s) = f("since_prev_revision_secs") {
        ctx.push(format!("{} since previous revision", format_lag_secs(s)));
    }

    ctx.join(" · ")
}

/// Two-sentence summary of one phase — the run of activity between two
/// rate revisions.
///
/// ```text
/// During this phase: 8 market ticks (crowd 5.3% → 5.0%, −0.3pp), 1 agent run
/// (macro_forecaster). Model held at 3.4% throughout. Gap to crowd narrowed
/// −1.9pp → −1.6pp.
/// ```
///
/// The first sentence counts what happened; the second says whether any
/// of it corresponded to the rate moving. The second sentence is the
/// point — a count of ticks is activity, not evidence, and the previous
/// version of this summary stopped at the count.
///
/// Rule-based, no LLM: this text sits under a number the operator is
/// about to act on, so it must be derivable from the events and
/// reproducible from them.
pub fn build_phase_summary(events: &[&JsonValue]) -> String {
    let mut agent_runs: usize = 0;
    let mut bayesops_fits: usize = 0;
    let mut market_obs: usize = 0;
    let mut upstream_resolves: usize = 0;
    let mut other: usize = 0;

    // Agent names, up to 3, so the summary reads "5 agent runs (fermi,
    // macro_forecaster)" instead of an anonymous count.
    let mut agent_names: Vec<String> = Vec::new();
    // BayesOps outcomes, so "3 BayesOps fits" can say what they decided.
    let mut fit_decisions: Vec<String> = Vec::new();

    let mut market_start: Option<f64> = None;
    let mut market_end: Option<f64> = None;
    // Model rate and model-vs-crowd gap at the phase's first and last
    // annotated event — the correlation the second sentence reports.
    let mut model_start: Option<f64> = None;
    let mut model_end: Option<f64> = None;
    let mut div_start: Option<f64> = None;
    let mut div_end: Option<f64> = None;

    for ev in events {
        let kind = ev.get("kind").and_then(|v| v.as_str()).unwrap_or("");

        if let Some(m) = ev.get("model_rate_at_pct").and_then(|v| v.as_f64()) {
            if model_start.is_none() {
                model_start = Some(m);
            }
            model_end = Some(m);
        }
        if let Some(d) = ev.get("divergence_at_pp").and_then(|v| v.as_f64()) {
            if div_start.is_none() {
                div_start = Some(d);
            }
            div_end = Some(d);
        }

        match kind {
            "agent_run" => {
                agent_runs += 1;
                if agent_names.len() < 3 {
                    let name = ev
                        .get("sender_name")
                        .and_then(|v| v.as_str())
                        .or_else(|| ev.get("sender_id").and_then(|v| v.as_str()))
                        .unwrap_or("agent")
                        .to_string();
                    if !agent_names.contains(&name) {
                        agent_names.push(name);
                    }
                }
            }
            "bayesops_fit"
            | "bayesops_fit_accepted"
            | "bayesops_fit_pending"
            | "bayesops_fit_failed"
            | "bayesops_fit_decision" => {
                bayesops_fits += 1;
                if let Some(d) = ev
                    .get("decision")
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        ev.get("metadata")
                            .and_then(|m| m.get("decision"))
                            .and_then(|v| v.as_str())
                    })
                    .map(|d| d.replace('_', " "))
                {
                    if !fit_decisions.contains(&d) {
                        fit_decisions.push(d);
                    }
                }
            }
            "market_observation" => {
                market_obs += 1;
                if let Some(p) = ev.get("market_price").and_then(|v| v.as_f64()) {
                    if market_start.is_none() {
                        market_start = Some(p);
                    }
                    market_end = Some(p);
                }
            }
            "upstream_resolved" => {
                upstream_resolves += 1;
            }
            _ => {
                other += 1;
            }
        }
    }

    fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
        if n == 1 {
            one
        } else {
            many
        }
    }

    let mut fragments: Vec<String> = Vec::new();
    if agent_runs > 0 {
        let word = plural(agent_runs, "agent run", "agent runs");
        if agent_names.is_empty() {
            fragments.push(format!("{} {}", agent_runs, word));
        } else {
            fragments.push(format!(
                "{} {} ({})",
                agent_runs,
                word,
                agent_names.join(", ")
            ));
        }
    }
    if bayesops_fits > 0 {
        let word = plural(bayesops_fits, "BayesOps fit", "BayesOps fits");
        if fit_decisions.is_empty() {
            fragments.push(format!("{} {}", bayesops_fits, word));
        } else {
            fragments.push(format!(
                "{} {} ({})",
                bayesops_fits,
                word,
                fit_decisions.join(", ")
            ));
        }
    }
    if market_obs > 0 {
        let word = plural(market_obs, "market tick", "market ticks");
        // The crowd's actual path, not just that ticks occurred. This
        // clause read `market_price`, which the timeline endpoint never
        // emitted on market events — so it was dead code and every phase
        // reported a bare count. The server now sends the field.
        let drift = match (market_start, market_end) {
            (Some(a), Some(b)) if (b - a).abs() * 100.0 >= EPSILON_PP => format!(
                " (crowd {:.1}% → {:.1}%, {:+.1}pp)",
                a * 100.0,
                b * 100.0,
                (b - a) * 100.0
            ),
            (Some(a), Some(_)) => format!(" (crowd flat at {:.1}%)", a * 100.0),
            _ => String::new(),
        };
        fragments.push(format!("{} {}{}", market_obs, word, drift));
    }
    if upstream_resolves > 0 {
        let word = plural(upstream_resolves, "upstream resolve", "upstream resolves");
        fragments.push(format!("{} {}", upstream_resolves, word));
    }
    if other > 0 {
        fragments.push(format!("{} other", other));
    }

    // Second sentence: did the model move while all that was happening,
    // and did the gap to the crowd open or close?
    let mut tail: Vec<String> = Vec::new();
    match (model_start, model_end) {
        (Some(a), Some(b)) if (b - a).abs() >= EPSILON_PP => tail.push(format!(
            "Model drifted {:.1}% → {:.1}% ({:+.1}pp) within the phase.",
            a,
            b,
            b - a
        )),
        (Some(a), Some(_)) => tail.push(format!("Model held at {:.1}% throughout.", a)),
        _ => {}
    }
    match (div_start, div_end) {
        (Some(a), Some(b)) if (a.abs() - b.abs()).abs() >= EPSILON_PP => {
            let word = if b.abs() > a.abs() {
                "widened"
            } else {
                "narrowed"
            };
            tail.push(format!("Gap to crowd {} {:+.1}pp → {:+.1}pp.", word, a, b));
        }
        (Some(a), Some(_)) => tail.push(format!("Gap to crowd steady at {:+.1}pp.", a)),
        _ => {}
    }

    let head = if fragments.is_empty() {
        String::new()
    } else {
        format!("During this phase: {}.", fragments.join(", "))
    };

    match (head.is_empty(), tail.is_empty()) {
        (true, true) => String::new(),
        (true, false) => tail.join(" "),
        (false, true) => head,
        (false, false) => format!("{} {}", head, tail.join(" ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn humanises_kinds_without_repeating_them() {
        assert_eq!(
            humanize_event_kind("market_observation"),
            "Market observation"
        );
        assert_eq!(
            humanize_event_kind("bayesops_fit_accepted"),
            "BayesOps fit accepted"
        );
        assert_eq!(
            humanize_event_kind("upstream_resolved"),
            "Upstream resolved"
        );
        assert_eq!(humanize_event_kind("fpl_edit"), "FPL edit");
        assert_eq!(humanize_event_kind(""), "");
    }

    #[test]
    fn formats_lags_at_human_granularity() {
        assert_eq!(format_lag_secs(45.0), "45s");
        assert_eq!(format_lag_secs(720.0), "12m");
        assert_eq!(format_lag_secs(11_400.0), "3h 10m");
        assert_eq!(format_lag_secs(10_800.0), "3h");
        assert_eq!(format_lag_secs(259_200.0), "3d");
    }

    /// Unit carries must not produce "60m" or "0h 60m".
    #[test]
    fn formats_lags_across_unit_boundaries() {
        assert_eq!(format_lag_secs(3_540.0), "59m");
        assert_eq!(format_lag_secs(3_599.0), "1h");
        assert_eq!(format_lag_secs(3_600.0), "1h");
        assert_eq!(format_lag_secs(4_500.0), "1h 15m");
        assert_eq!(format_lag_secs(172_800.0), "2d");
    }

    #[test]
    fn formats_usd_compactly() {
        assert_eq!(format_usd_compact(0.0), "$0");
        assert_eq!(format_usd_compact(642.0), "$642");
        assert_eq!(format_usd_compact(18_400.0), "$18.4k");
        assert_eq!(format_usd_compact(2_100_000.0), "$2.1M");
    }

    #[test]
    fn timestamp_drops_microsecond_noise() {
        assert_eq!(
            format_event_timestamp("2026-08-14T04:09:26.138076+00:00", "5h ago"),
            "5h ago · 04:09"
        );
        assert_eq!(format_event_timestamp("", "5h ago"), "");
    }

    /// The regression this module exists for: a market tick used to
    /// render as its own kind name, twice.
    #[test]
    fn market_tick_reports_price_move_and_quote_quality() {
        let ev = json!({
            "kind": "market_observation",
            "market_price": 0.05,
            "tick_delta_pp": -0.3,
            "observation_type": "scheduled",
            "confidence_signal": "low",
            "bid_price": 0.04,
            "ask_price": 0.06,
            "volume_24h": 0.0,
            "liquidity": 642.0,
        });
        let (headline, detail) = market_tick_text(&ev);
        assert_eq!(headline, "Crowd 5.0% (-0.3pp)");
        assert!(detail.contains("scheduled poll"), "{}", detail);
        assert!(detail.contains("low confidence"), "{}", detail);
        assert!(detail.contains("bid 4.0% / ask 6.0%"), "{}", detail);
        assert!(detail.contains("$642 liquidity"), "{}", detail);
        assert!(!headline.contains("market_observation"));
        assert!(!detail.contains("market_observation"));
    }

    #[test]
    fn market_tick_distinguishes_first_tick_from_unchanged() {
        let first = json!({ "kind": "market_observation", "market_price": 0.05 });
        assert_eq!(market_tick_text(&first).0, "Crowd 5.0% (first tick)");

        let flat = json!({
            "kind": "market_observation",
            "market_price": 0.05,
            "tick_delta_pp": 0.0,
        });
        assert_eq!(market_tick_text(&flat).0, "Crowd 5.0% (unchanged)");
    }

    #[test]
    fn market_tick_falls_back_to_chart_price_field() {
        // The timeline endpoint also stashes the price under
        // `predicted_probability` for the chart's generic y lookup.
        let ev = json!({ "kind": "market_observation", "predicted_probability": 0.053 });
        assert_eq!(market_tick_text(&ev).0, "Crowd 5.3% (first tick)");
    }

    #[test]
    fn correlation_line_leads_to_the_following_revision() {
        let ev = json!({
            "kind": "market_observation",
            "next_revision_lag_secs": 720.0,
            "next_revision_delta_pp": 4.6,
            "next_revision_to_pct": 8.0,
            "model_rate_at_pct": 3.4,
            "crowd_price_at_pct": 5.3,
            "divergence_at_pp": -1.9,
        });
        assert_eq!(
            build_correlation_line(&ev),
            "↳ 12m before the +4.6pp revision · model 3.4% vs crowd 5.3% (model -1.9pp)"
        );
    }

    #[test]
    fn correlation_line_names_the_revision_trigger() {
        let ev = json!({
            "kind": "agent_run",
            "next_revision_lag_secs": 720.0,
            "next_revision_delta_pp": 4.6,
            "next_revision_trigger": "manual",
        });
        assert_eq!(
            build_correlation_line(&ev),
            "↳ 12m before the +4.6pp manual revision"
        );
    }

    /// An event that did *not* precede a revision is a finding, not a
    /// blank.
    #[test]
    fn correlation_line_states_the_absence_of_a_revision() {
        let ev = json!({
            "kind": "market_observation",
            "model_rate_at_pct": 8.0,
            "crowd_price_at_pct": 5.0,
            "divergence_at_pp": 3.0,
        });
        assert_eq!(
            build_correlation_line(&ev),
            "↳ no revision since · model 8.0% vs crowd 5.0% (model +3.0pp)"
        );
    }

    #[test]
    fn correlation_line_for_a_revision_measures_the_phase() {
        let ev = json!({
            "kind": "rate_revision",
            "since_prev_revision_secs": 3600.0,
            "model_rate_at_pct": 8.0,
            "crowd_price_at_pct": 5.0,
            "divergence_at_pp": 3.0,
        });
        let line = build_correlation_line(&ev);
        assert!(line.contains("1h after the previous revision"), "{}", line);
        assert!(!line.contains("no revision since"), "{}", line);
    }

    #[test]
    fn correlation_line_is_empty_without_annotations() {
        let ev = json!({ "kind": "rate_revision" });
        assert_eq!(build_correlation_line(&ev), "");
    }

    #[test]
    fn revision_context_says_which_way_the_move_went() {
        // 3.4% → 8.0% against a crowd at 5.0%: gap goes -1.6pp to +3.0pp,
        // i.e. 1.4pp further from the crowd in absolute terms.
        let away = json!({
            "kind": "rate_revision",
            "previous_probability": 0.034,
            "predicted_probability": 0.08,
            "crowd_price_at_pct": 5.0,
            "divergence_at_pp": 3.0,
        });
        let ctx = build_revision_context(&away);
        assert!(ctx.contains("away from the crowd"), "{}", ctx);
        assert!(ctx.contains("gap now +3.0pp"), "{}", ctx);

        // 8.0% → 5.5% against the same crowd converges.
        let toward = json!({
            "kind": "rate_revision",
            "previous_probability": 0.08,
            "predicted_probability": 0.055,
            "crowd_price_at_pct": 5.0,
            "divergence_at_pp": 0.5,
        });
        assert!(
            build_revision_context(&toward).contains("toward the crowd"),
            "{}",
            build_revision_context(&toward)
        );
    }

    #[test]
    fn phase_summary_reports_crowd_drift_and_a_held_model() {
        let a = json!({
            "kind": "market_observation",
            "market_price": 0.053,
            "model_rate_at_pct": 3.4,
            "divergence_at_pp": -1.9,
        });
        let b = json!({
            "kind": "market_observation",
            "market_price": 0.05,
            "model_rate_at_pct": 3.4,
            "divergence_at_pp": -1.6,
        });
        let summary = build_phase_summary(&[&a, &b]);
        assert!(
            summary.contains("2 market ticks (crowd 5.3% → 5.0%, -0.3pp)"),
            "{}",
            summary
        );
        assert!(
            summary.contains("Model held at 3.4% throughout."),
            "{}",
            summary
        );
        assert!(
            summary.contains("Gap to crowd narrowed -1.9pp → -1.6pp."),
            "{}",
            summary
        );
    }

    #[test]
    fn phase_summary_names_agents_and_fit_decisions() {
        let run = json!({ "kind": "agent_run", "sender_name": "macro_forecaster" });
        let fit = json!({ "kind": "bayesops_fit", "decision": "auto_accepted" });
        let summary = build_phase_summary(&[&run, &fit]);
        assert!(
            summary.contains("1 agent run (macro_forecaster)"),
            "{}",
            summary
        );
        assert!(
            summary.contains("1 BayesOps fit (auto accepted)"),
            "{}",
            summary
        );
    }

    #[test]
    fn phase_summary_is_empty_for_no_events() {
        assert_eq!(build_phase_summary(&[]), "");
    }

    #[test]
    fn phase_summary_reports_model_drift_when_the_rate_moved() {
        let a = json!({ "kind": "agent_run", "model_rate_at_pct": 3.4 });
        let b = json!({ "kind": "agent_run", "model_rate_at_pct": 8.0 });
        let summary = build_phase_summary(&[&a, &b]);
        assert!(
            summary.contains("Model drifted 3.4% → 8.0% (+4.6pp) within the phase."),
            "{}",
            summary
        );
    }
}
