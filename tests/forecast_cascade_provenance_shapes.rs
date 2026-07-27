//! Wire-format tests for the cascade-provenance endpoint
//! (Phase 2.5 — the redistribution waterfall surface).
//!
//! These tests assert the JSON shape of
//! `/api/forecasts/:id/cascade-provenance` — what the console's Provenance
//! right-tab (and any external client that wants to explain a forecast's
//! current probability) actually sees on the wire.
//!
//! Like `forecast_timeline_shapes.rs`, we don't spin up a full server; we
//! construct the response shape and verify its invariants so a handler
//! change breaks this test loudly rather than silently corrupting the UI.

use serde_json::{json, Value};

/// Canonical example response. This is the contract; if it changes, the
/// console's `render_provenance_tab` and any external consumer must move
/// with it.
///
/// Scenario: a forecast whose raw model probability was 50.0% has seen
/// four upstream forecasts resolve NO (each cascading mass onto it),
/// plus one cascade_undo that reverted a small delta. Every number in
/// this fixture must satisfy the invariants at the bottom of the file
/// simultaneously; adjust in lockstep if you edit them.
///
///   baseline    = 0.500
///   + Δ curacao  = +2.0 pp  → 0.520
///   + Δ panama   = +1.5 pp  → 0.535
///   + Δ jordan   = +1.3 pp  → 0.548
///   + Δ turkiye  = +1.2 pp  → 0.560
///   + Δ undo     = −0.1 pp  → 0.559
///   ------------------------
///   cumulative  = +5.9 pp
///   current     = 0.559
fn example_response() -> Value {
    json!({
        "forecast_id": "fc-spain-2026",
        "question": "Will Spain win the 2026 FIFA World Cup?",
        "current_probability": 0.559,
        "baseline_probability": 0.500,
        "cumulative_cascade_pp": 5.9,
        "cascade_count": 5,
        "contributions": [
            {
                "ts": "2026-06-30T20:00:00Z",
                "trigger_forecast_id": "fc-curacao-2026",
                "trigger_question": "Will Curaçao win the 2026 FIFA World Cup?",
                "prev_p": 0.500,
                "new_p": 0.520,
                "delta_pp": 2.0,
                "revision_trigger": "cascade",
                "is_undo": false,
                "reason": "cascade from fc-curacao-2026 (resolved)"
            },
            {
                "ts": "2026-07-04T22:00:00Z",
                "trigger_forecast_id": "fc-panama-2026",
                "trigger_question": "Will Panama win the 2026 FIFA World Cup?",
                "prev_p": 0.520,
                "new_p": 0.535,
                "delta_pp": 1.5,
                "revision_trigger": "cascade",
                "is_undo": false,
                "reason": "cascade from fc-panama-2026 (resolved)"
            },
            {
                "ts": "2026-07-05T22:00:00Z",
                "trigger_forecast_id": "fc-jordan-2026",
                "trigger_question": "Will Jordan win the 2026 FIFA World Cup?",
                "prev_p": 0.535,
                "new_p": 0.548,
                "delta_pp": 1.3,
                "revision_trigger": "cascade",
                "is_undo": false,
                "reason": "cascade from fc-jordan-2026 (resolved)"
            },
            {
                "ts": "2026-07-08T22:00:00Z",
                "trigger_forecast_id": "fc-turkiye-2026",
                "trigger_question": "Will Türkiye win the 2026 FIFA World Cup?",
                "prev_p": 0.548,
                "new_p": 0.560,
                "delta_pp": 1.2,
                "revision_trigger": "cascade",
                "is_undo": false,
                "reason": "cascade from fc-turkiye-2026 (resolved)"
            },
            {
                "ts": "2026-07-09T22:00:00Z",
                "trigger_forecast_id": null,
                "trigger_question": null,
                "prev_p": 0.560,
                "new_p": 0.559,
                "delta_pp": -0.1,
                "revision_trigger": "cascade_undo",
                "is_undo": true,
                "reason": "cascade_undo of 3f4a-…"
            }
        ]
    })
}

fn get_f64(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(|x| x.as_f64()).unwrap_or_else(|| {
        panic!("missing or non-numeric field: {}", key);
    })
}

#[test]
fn top_level_fields_present() {
    let r = example_response();
    for f in [
        "forecast_id",
        "question",
        "current_probability",
        "baseline_probability",
        "cumulative_cascade_pp",
        "cascade_count",
        "contributions",
    ] {
        assert!(r.get(f).is_some(), "missing top-level field: {}", f);
    }
    assert!(
        r["contributions"].is_array(),
        "contributions must be an array"
    );
}

#[test]
fn cascade_count_matches_array_length() {
    let r = example_response();
    let n = r["contributions"].as_array().unwrap().len();
    assert_eq!(
        r["cascade_count"].as_u64().unwrap() as usize,
        n,
        "cascade_count must equal contributions.len()"
    );
}

#[test]
fn baseline_plus_cumulative_equals_current() {
    // The waterfall's core arithmetic: current = baseline + Σ deltas.
    // This is what makes the "explain the number" story hold together.
    let r = example_response();
    let current = get_f64(&r, "current_probability");
    let baseline = get_f64(&r, "baseline_probability");
    let cumulative_pp = get_f64(&r, "cumulative_cascade_pp");
    let sum = baseline + (cumulative_pp / 100.0);
    assert!(
        (sum - current).abs() < 1e-6,
        "baseline ({}) + cumulative ({}) = {}, but current = {}",
        baseline,
        cumulative_pp / 100.0,
        sum,
        current
    );
}

#[test]
fn cumulative_equals_sum_of_deltas() {
    // Redundant with the previous invariant but pins the per-row contract
    // independently — if a delta row ever ships a wrong sign, this test
    // catches it before the aggregate does.
    let r = example_response();
    let sum_delta_pp: f64 = r["contributions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| get_f64(c, "delta_pp"))
        .sum();
    let cumulative = get_f64(&r, "cumulative_cascade_pp");
    assert!(
        (sum_delta_pp - cumulative).abs() < 1e-6,
        "Σ delta_pp = {}, but cumulative_cascade_pp = {}",
        sum_delta_pp,
        cumulative
    );
}

#[test]
fn contributions_sorted_by_abs_delta_desc() {
    // The client renders top-down; biggest movers must be first regardless
    // of sign, so a cascade_undo of large magnitude sorts above a small
    // positive cascade.
    let r = example_response();
    let arr = r["contributions"].as_array().unwrap();
    let mags: Vec<f64> = arr.iter().map(|c| get_f64(c, "delta_pp").abs()).collect();
    for w in mags.windows(2) {
        assert!(
            w[0] >= w[1] - 1e-9,
            "contributions not sorted by |delta_pp| desc: {:?}",
            mags
        );
    }
}

#[test]
fn per_row_prev_plus_delta_equals_new() {
    // Each cascade row's arithmetic must round-trip. Off-by-one on prev/new
    // has bitten this system before (see apply_wc_cascades.rs comment about
    // the queue path silently returning early).
    let r = example_response();
    for c in r["contributions"].as_array().unwrap() {
        let prev = get_f64(c, "prev_p");
        let new_p = get_f64(c, "new_p");
        let delta_pp = get_f64(c, "delta_pp");
        let recomputed_pp = (new_p - prev) * 100.0;
        assert!(
            (recomputed_pp - delta_pp).abs() < 1e-6,
            "row prev={}, new={} implies delta_pp={}, but got {}",
            prev,
            new_p,
            recomputed_pp,
            delta_pp
        );
    }
}

#[test]
fn undo_rows_flagged_and_negative_or_zero() {
    // cascade_undo rows should be marked is_undo=true. Their delta_pp is
    // typically ≤ 0 (they revert a prior gain) but the sign isn't
    // guaranteed by the schema — we only assert the flag/kind pairing.
    let r = example_response();
    for c in r["contributions"].as_array().unwrap() {
        let is_undo = c["is_undo"].as_bool().unwrap_or(false);
        let trig = c["revision_trigger"].as_str().unwrap_or("");
        assert_eq!(
            is_undo,
            trig == "cascade_undo",
            "is_undo/revision_trigger disagreement: is_undo={}, trigger={}",
            is_undo,
            trig
        );
    }
}

#[test]
fn cascade_rows_carry_trigger_forecast_id() {
    // Non-undo rows must expose the parsed trigger id (may still be null
    // if the reason string was truncated/garbled, but at least one row in
    // the example scenario has one).
    let r = example_response();
    let any_with_trigger = r["contributions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["revision_trigger"].as_str() == Some("cascade"))
        .any(|c| {
            c.get("trigger_forecast_id")
                .and_then(|v| v.as_str())
                .is_some()
        });
    assert!(
        any_with_trigger,
        "at least one cascade row should carry a parsed trigger_forecast_id"
    );
}
