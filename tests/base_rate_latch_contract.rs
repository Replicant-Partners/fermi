//! # A latch that is only released on success is released by the wrong run
//!
//! `CockpitState::base_rate_producer` is set by `update_outside_rate` before it
//! fires the scoped "Update base rate" run, and it diverts a completing agent
//! into `apply_base_rate_only` instead of the normal evidence path. It is a
//! latch: one flag, global to the console, read by every completion.
//!
//! ## The failure
//!
//! Two independent defects, and the second is only reachable because of the
//! first:
//!
//!   1. **The guard asked the wrong question.** It read
//!      `base_rate_producer.is_some()` — "is a base-rate refresh outstanding
//!      somewhere" — where the question is "is *this* completion that refresh".
//!   2. **Only success released it.** `apply_base_rate_only` `take()`s the
//!      latch. Nothing on the failure path did, so a base-rate run that never
//!      ran left it set forever.
//!
//! Observed on a San Francisco temperature forecast:
//!
//! ```text
//! 12:05:15  ✗ weather_oracle_base_rate failed: 429, LLM rate limit (10/min)
//! 12:05:18  ℹ Base-rate update: no parseable base rate in response.
//! 12:05:18  ✓ entity_investigator_synoptic_pattern_aug29 complete
//! ```
//!
//! The base-rate refusal at 12:05:15 left the latch on. Three seconds later a
//! driver run completed, was diverted into the base-rate extractor, and
//! reported that it had not answered a question nobody asked it.
//!
//! The confusing message is the benign symptom. `apply_base_rate_only` *writes*
//! when it can parse: any diverted run whose response happened to carry
//! `historical_frequency` would have overwritten the forecast's outside view —
//! the term every driver multiplies — with a number measured for one driver,
//! and stamped the wrong agent's name on its provenance.
//!
//! ## Why this is a source scan
//!
//! The property is a lifecycle across three methods of a 29k-line GPUI type
//! that needs a window, an async executor and a live ABW session to
//! instantiate. `cockpit.rs` has no test harness. Scanning the source is the
//! established pattern here — see `tests/execute_path_parity.rs` and
//! `tests/gate_trust_coverage.rs`.
//!
//! It is a weaker instrument than a behavioural test and is chosen knowingly.
//! What it can do is stop the latch acquiring a fourth read site that asks
//! `is_some()`, which is the shape the defect had and the shape it would
//! regrow in.

use std::path::Path;

const COCKPIT: &str = "crates/fermi-console/src/cockpit.rs";

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// The body of an `impl`-level method, from its signature to its closing
/// brace.
///
/// Terminates on the first line that is exactly four spaces and `}`, which is
/// rustfmt's invariant for a method close inside an `impl` block — rather than
/// counting braces, which would have to model string literals, and this file
/// has format strings full of them.
fn method_body(src: &str, signature: &str) -> String {
    let start = src.find(signature).unwrap_or_else(|| {
        panic!("`{signature}` not found in {COCKPIT} — this test has lost its subject")
    });
    let rest = &src[start..];
    let end = rest
        .find("\n    }\n")
        .unwrap_or_else(|| panic!("no method close found after `{signature}`"));
    rest[..end].to_string()
}

/// Lines mentioning `needle` that are not comments.
///
/// Comment-aware because this codebase documents its own defects in prose next
/// to the fix. A scan that counted `// the old code did X` as X would fail on
/// the very comment explaining why X is gone.
fn live_lines<'a>(src: &'a str, needle: &str) -> Vec<&'a str> {
    src.lines()
        .filter(|l| l.contains(needle))
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect()
}

// ─── the contract ──────────────────────────────────────────────────────

/// **The one that stops drift.** No control flow branches on whether the latch
/// is merely set.
///
/// `is_some()` is the question that cannot distinguish the base-rate run from
/// the next run to finish. Every read for control flow must go through
/// `base_rate_tracking_id`, which compares identities.
#[test]
fn the_latch_is_never_read_as_a_bare_boolean() {
    let src = read(COCKPIT);
    let offenders: Vec<&str> = live_lines(&src, "base_rate_producer")
        .into_iter()
        .filter(|l| l.contains("is_some()") || l.contains("is_none()"))
        .collect();
    assert!(
        offenders.is_empty(),
        "control flow in {COCKPIT} branches on whether `base_rate_producer` is \
         set, rather than on whether the completing run IS the base-rate run. \
         Use `base_rate_tracking_id()` and compare against the tracking id. A \
         set latch and the run it refers to are the same thing only when \
         nothing goes wrong, and the base-rate run is the one most likely to be \
         rate-limited — it is fired on its own, after a decomposition has just \
         emptied the per-minute bucket. Offenders: {offenders:#?}"
    );
}

/// There is exactly one definition of the base-rate tracking id.
///
/// The launch, the completion guard and the failure release all need
/// `"{producer}_base_rate"`. When each spelled it out, they disagreed: an
/// earlier repair looked the run row up as `"fermi_base_rate"` while the launch
/// pushed `"{producer}_base_rate"`, and every specialist's row spun forever.
#[test]
fn the_tracking_id_has_one_definition() {
    let src = read(COCKPIT);
    let inline = live_lines(&src, "_base_rate\")")
        .into_iter()
        .filter(|l| l.contains("format!"))
        .count();
    assert!(
        inline <= 2,
        "{inline} sites in {COCKPIT} build the base-rate tracking id inline. \
         Expected at most two: `update_outside_rate` (which pushes the run row) \
         and `base_rate_tracking_id` (which everything else asks). A third is \
         how the launch and the lookup came to disagree the first time."
    );
    assert!(
        src.contains("fn base_rate_tracking_id"),
        "`base_rate_tracking_id` is gone; the three sites that need the id are \
         back to spelling it out"
    );
}

/// The failure path releases the latch.
///
/// Without this, a refused base-rate run poisons every subsequent completion
/// for the life of the session.
#[test]
fn a_failed_run_releases_the_latch() {
    let src = read(COCKPIT);
    let body = method_body(&src, "fn mark_agent_failed(");
    assert!(
        body.contains("base_rate_producer.take()"),
        "`mark_agent_failed` does not release `base_rate_producer`. A base-rate \
         run that failed is a base-rate refresh that is over; leaving the latch \
         set hands the next agent's result to the base-rate extractor, which \
         WRITES the forecast's outside view when it can parse one."
    );
    assert!(
        body.contains("Base rate NOT updated"),
        "`mark_agent_failed` releases the latch silently. The failure line names \
         the agent and the error; it does not say that the anchor every driver \
         multiplies is still the old one, which is the fact the operator needs \
         in order to press the button again."
    );
}

/// The base-rate run row is found by exact identity, never by agent id.
///
/// A declared specialist is routinely on drivers as well as on the base rate —
/// `weather_oracle` held three rows on one forecast. `find` takes the first
/// match, so a `base_agent_id` fallback marks a *driver* run completed and
/// leaves the base-rate row spinning: the bug it was written to fix, one row
/// over.
#[test]
fn the_run_row_is_matched_by_tracking_id_not_by_agent_id() {
    let src = read(COCKPIT);
    let body = method_body(&src, "fn apply_base_rate_only(");
    assert!(
        live_lines(&body, "r.base_agent_id == producer").is_empty(),
        "`apply_base_rate_only` falls back to matching the run row on \
         `base_agent_id`. That matches the specialist's DRIVER runs too, and \
         `find` returns the first. `update_outside_rate` is the only site that \
         sets the latch and it always pushes `\"{{producer}}_base_rate\"`, so the \
         exact match is total and the fallback can only be wrong."
    );
}
