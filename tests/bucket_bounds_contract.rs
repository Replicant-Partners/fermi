//! A bucket label is an integer set, and the cards that count them say so.
//!
//! # The error
//!
//! `"76-77"` on a prediction market denotes the integer SET `{76, 77}`, which is
//! the half-open real interval `[75.5, 77.5)` and is **two** units wide. Reading
//! it as a threshold, or as a band one unit narrower than it is, roughly halves
//! the probability.
//!
//! This is not hypothetical and it is not small. On a live Houston market,
//! 2026-08-21:
//!
//! | source | P(bucket) |
//! |---|---|
//! | the forecast's declared base rate | **12.0%** |
//! | the same climatology it stated (mean 75F, sd 2.5F), integrated over [75.5, 77.5) | **26.4%** |
//! | `weather_oracle`, 330 ERA5 observations at KHOU, trend-adjusted | **32%** |
//! | Polymarket crowd | 27.5% |
//!
//! The agent's own reasoning was internally contradictory — it said "a 2F band
//! (76-77F)" and returned the ONE-degree answer. Every driver is a multiplier on
//! the base rate, so the error scaled through the entire model, and the console
//! reported the resulting 15.5pp gap to the market as a possible edge. The
//! market was right.
//!
//! The same shape had already been recorded for Houston 74-75 and never fixed,
//! because it lived in a prompt and nothing tested prompts.
//!
//! # Why a test over card text
//!
//! The rule is stated correctly in `examples/reference_bucket_indicator_kord.fpl`,
//! in `examples/weather_spawn_plan.rs` and in two test files — and was stated in
//! **no agent card at all**, which is where the agents doing the counting
//! actually read their instructions. Nine curated cards declare a `[BASE RATE]`
//! finding label; none mentioned bucket bounds.
//!
//! Asserting on prompt text is a blunt instrument and it is the right one here.
//! The failure was never a wrong value in a field — it was an absent instruction,
//! and only the text can show whether the instruction is present.

use std::path::Path;

/// Cards that must state the rule, and why each one.
///
/// Kept short deliberately. A rule pasted into every card is noise that trains
/// readers to skim; these are the two agents that actually meet bucket
/// questions.
const MUST_STATE: &[(&str, &str)] = &[
    (
        "fermi",
        "decomposes every question including bucket markets, and produces the \
         base rate while doing it. This is the agent that returned 12%.",
    ),
    (
        "weather_oracle",
        "sees bucket ladders constantly. It already derives the bounds \
         correctly, which is exactly why the rule is written down — so that \
         stays true rather than being rediscovered each run.",
    ),
];

/// Base-rate-producing cards that do NOT yet state the rule, and why not.
///
/// The honest half of the claim, in the shape `CROSS_CHECK_EXEMPTIONS` uses. A
/// card that produces base rates and is neither covered above nor listed here
/// fails the test below, so the set cannot quietly grow.
///
/// Every one of these handles event questions ("will X happen") rather than
/// range-valued outcomes, so the bounds ambiguity does not arise for them today.
/// `equity_analyst` is the one to watch: an EPS or revenue band has exactly the
/// same shape as a temperature band, and the day it is asked for one, it belongs
/// in `MUST_STATE`.
const NOT_YET: &[(&str, &str)] = &[
    (
        "biotech_analyst",
        "trial outcomes and approvals, not ranges",
    ),
    ("entity_investigator", "entity behaviour, not ranges"),
    (
        "equity_analyst",
        "event questions today; an EPS or revenue BAND has the same shape and \
         would move it into MUST_STATE",
    ),
    (
        "football_analyst",
        "match and tournament outcomes, not ranges",
    ),
    (
        "macro_forecaster",
        "policy and geopolitical events; produces base rates but has not been \
         asked a bucket question",
    ),
    ("market_research", "adoption and share, not ranges"),
    ("nba_analyst", "game and series outcomes, not ranges"),
    ("sentiment_analyzer", "narrative scoring, not ranges"),
];

fn card(agent: &str) -> serde_json::Value {
    let path = format!("agents/curated/{agent}/agent_card.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

fn system_prompt(agent: &str) -> String {
    let c = card(agent);
    c.get("system_prompt")
        .or_else(|| c.get("capabilities").and_then(|x| x.get("system_prompt")))
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("{agent} has no system_prompt"))
        .to_string()
}

fn declares_base_rate(agent: &str) -> bool {
    let c = card(agent);
    let contract = c
        .get("fermi_contract")
        .or_else(|| c.get("capabilities").and_then(|x| x.get("fermi_contract")));
    contract
        .and_then(|f| f.get("finding_labels"))
        .and_then(|v| v.as_array())
        .map(|labels| {
            labels
                .iter()
                .filter_map(|l| l.as_str())
                .any(|l| l.to_ascii_uppercase().contains("BASE RATE"))
        })
        .unwrap_or(false)
}

/// The two cards that meet bucket questions state the rule.
#[test]
fn the_cards_that_count_buckets_say_what_a_bucket_is() {
    for (agent, why) in MUST_STATE {
        let p = system_prompt(agent);

        assert!(
            p.contains("integer SET"),
            "{agent} must state that a bucket label is an integer set, not a \
             threshold — {why}"
        );
        assert!(
            p.contains("[75.5, 77.5)") || p.contains("[31.5, 32.5)"),
            "{agent} must give the bounds concretely. \"two units wide\" is \
             advice; \"[75.5, 77.5)\" is a specification, and the failure was an \
             agent that said the former and computed as though it meant a \
             one-unit band."
        );
    }
}

/// The 12% is named, so the instruction carries its own evidence.
///
/// A rule with a number attached is one an agent can check itself against. The
/// bare rule had been true and unwritten for months; what was missing was the
/// demonstration that ignoring it costs a factor of two.
#[test]
fn the_rule_carries_the_measurement_that_justifies_it() {
    let p = system_prompt("fermi");
    assert!(p.contains("12%"), "the wrong answer must be named");
    assert!(
        p.contains("26%") || p.contains("32%"),
        "and so must the right one"
    );
}

/// Every base-rate-producing card is either covered or explicitly deferred.
///
/// The point is not that all nine state the rule. It is that none of them can be
/// silently missing it — the same standard `CROSS_CHECK_EXEMPTIONS` sets for
/// unverifiable grounding claims.
#[test]
fn no_base_rate_card_is_silently_uncovered() {
    let dir = Path::new("agents/curated");
    let mut uncovered = Vec::new();

    for entry in std::fs::read_dir(dir).expect("read agents/curated") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if !entry.path().join("agent_card.json").exists() {
            continue;
        }
        if !declares_base_rate(&name) {
            continue;
        }
        let covered =
            MUST_STATE.iter().any(|(a, _)| *a == name) || NOT_YET.iter().any(|(a, _)| *a == name);
        if !covered {
            uncovered.push(name);
        }
    }

    assert!(
        uncovered.is_empty(),
        "these cards declare a [BASE RATE] finding label and are neither \
         required to state the bucket rule nor listed as deferred: {uncovered:?}. \
         Add each to MUST_STATE or to NOT_YET with a reason. A base-rate producer \
         that is silent about what a bucket means is how the Houston 12% \
         happened."
    );
}

/// A deferral has to say why, so the list cannot become a dumping ground.
#[test]
fn every_deferral_gives_a_reason() {
    for (agent, why) in NOT_YET {
        assert!(
            why.len() >= 20,
            "{agent}: \"{why}\" is not a reason for deferring"
        );
        assert!(
            !MUST_STATE.iter().any(|(a, _)| a == agent),
            "{agent} is in both lists"
        );
    }
}

/// The cards still parse and still carry a prompt.
///
/// The rule was spliced into the JSON as raw text rather than via
/// `json.load`/`dump`, deliberately — a reserialise rewrites all 140 lines and
/// buries a one-paragraph change in an unreviewable diff. The tradeoff is that
/// a bad escape produces invalid JSON, so that is checked here.
#[test]
fn the_spliced_cards_are_still_valid_json_with_intact_prompts() {
    for (agent, _) in MUST_STATE {
        let p = system_prompt(agent);
        assert!(
            p.len() > 4000,
            "{agent} prompt is {} chars — suspiciously short, as though a \
             splice truncated it",
            p.len()
        );
        assert!(
            !p.contains("\\n"),
            "{agent} prompt contains a literal backslash-n, \
             which means an escape was double-written and the agent will read it \
             as text"
        );
    }
}
