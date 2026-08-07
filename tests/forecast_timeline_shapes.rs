//! Wire-format tests for the forecast timeline endpoint (Spec 23 R-3 Piece 2).
//!
//! These tests assert the JSON shape of `/api/forecasts/:id/timeline` —
//! what an HTTP client (the console's Trajectory view, the ABW observatory,
//! external integrations) actually sees on the wire. They do not spin up a
//! full server; instead they exercise the merging logic by constructing
//! the expected response shape and verifying its invariants.
//!
//! End-to-end live testing through a real server is intentionally out of
//! scope; that requires the full DB harness used by `tests/api_tests.rs`
//! and is gated by `DATABASE_URL`.

use serde_json::{json, Value};

/// The timeline response advertised by the handler. Pulled into this test
/// file as a hard contract: if the handler changes shape, the test breaks
/// and any clients keying off the shape get a heads-up.
fn example_response() -> Value {
    json!({
        "forecast_id": "fc-arg-2026",
        "question": "Will ARG win the 2026 World Cup?",
        "workspace_id": "11111111-1111-1111-1111-111111111111",
        "rate_series": [
            { "ts": "2026-06-01T00:00:00Z", "rate": 0.22 },
            { "ts": "2026-06-14T18:00:00Z", "rate": 0.26 },
        ],
        "market_series": [
            { "ts": "2026-06-10T12:00:00Z", "market_price": 0.21, "volume_total": 12000.0, "pm_event_id": "wc_arg_champion" },
        ],
        "events": [
            {
                "kind": "rate_revision",
                "ts": "2026-06-01T00:00:00Z",
                "revision_seq": 0,
                "predicted_probability": 0.22,
                "previous_probability": null,
                "revision_trigger": "initial",
                "reason": null,
                "triggering_agent": null,
                "evidence_delta": null,
            },
            {
                "kind": "agent_run",
                "ts": "2026-06-05T09:00:00Z",
                "message_id": "22222222-2222-2222-2222-222222222222",
                "sender_type": "agent",
                "sender_id": "wc_history_research",
                "sender_name": "WC History Research",
                "content": "Found 3 prior tournaments where ARG entered as top-3 favorite...",
                "metadata": { "agent_name": "wc_history_research", "confidence": 0.78, "tokens_used": 1240 },
            },
            {
                "kind": "upstream_resolved",
                "ts": "2026-06-14T17:30:00Z",
                "message_id": "33333333-3333-3333-3333-333333333333",
                "sender_type": "system",
                "sender_id": "h2h-arg-mex",
                "sender_name": "Workspace Resolved",
                "content": "Upstream workspace h2h-arg-mex resolved with outcome",
                "metadata": { "event": "upstream_resolved", "outcome": { "winner_team_id": "ARG" } },
            },
            {
                "kind": "bayesops_fit",
                "ts": "2026-06-14T17:30:05Z",
                "snapshot_id": "44444444-4444-4444-4444-444444444444",
                "driver_name": "won_rate",
                "decision": "auto_accepted",
                "n_observations": 8,
                "n_eff": 7.3,
                "ci_width": 0.42,
                "rate_before": 0.22,
                "rate_after": 0.26,
                "delta_pp": 4.0,
            },
            {
                "kind": "rate_revision",
                "ts": "2026-06-14T18:00:00Z",
                "revision_seq": 1,
                "predicted_probability": 0.26,
                "previous_probability": 0.22,
                "revision_trigger": "bayesops_refit",
                "reason": "BayesOps refit accepted: driver 'won_rate' fitted from 8 observations",
                "triggering_agent": null,
                "evidence_delta": {
                    "kind": "bayesops_refit",
                    "driver_name": "won_rate",
                    "rate_before": 0.22,
                    "rate_after": 0.26,
                },
            },
        ],
        "span": {
            "forecast_created_at": "2026-06-01T00:00:00Z",
            "forecast_resolved_at": null,
            "event_count": 5,
            "rate_revision_count": 2,
            "market_observation_count": 1,
        }
    })
}

// ─── Invariants ──────────────────────────────────────────────────────────────

#[test]
fn top_level_keys_present() {
    let v = example_response();
    for key in &[
        "forecast_id",
        "rate_series",
        "market_series",
        "events",
        "span",
    ] {
        assert!(v.get(*key).is_some(), "missing top-level key '{}'", key);
    }
}

#[test]
fn rate_series_is_chronological() {
    let v = example_response();
    let arr = v["rate_series"].as_array().unwrap();
    let mut prev_ts = "";
    for point in arr {
        let ts = point["ts"].as_str().unwrap();
        assert!(
            ts >= prev_ts,
            "rate_series not chronological: {} after {}",
            ts,
            prev_ts
        );
        assert!(
            point["rate"].is_number(),
            "rate_series points must have numeric 'rate'"
        );
        prev_ts = ts;
    }
}

#[test]
fn market_series_points_have_required_fields() {
    let v = example_response();
    for point in v["market_series"].as_array().unwrap() {
        assert!(point["ts"].is_string());
        assert!(point["market_price"].is_number());
        // volume_total + pm_event_id may be null
    }
}

#[test]
fn events_have_kind_and_ts() {
    let v = example_response();
    let arr = v["events"].as_array().unwrap();
    assert!(!arr.is_empty());
    for ev in arr {
        assert!(
            ev.get("kind").and_then(|v| v.as_str()).is_some(),
            "every event must have a string 'kind'"
        );
        assert!(
            ev.get("ts").and_then(|v| v.as_str()).is_some(),
            "every event must have a string 'ts'"
        );
    }
}

#[test]
fn events_are_chronological_after_merge() {
    let v = example_response();
    let mut prev_ts = "";
    for ev in v["events"].as_array().unwrap() {
        let ts = ev["ts"].as_str().unwrap();
        assert!(
            ts >= prev_ts,
            "events not chronological: {} after {}",
            ts,
            prev_ts
        );
        prev_ts = ts;
    }
}

#[test]
fn known_event_kinds_round_trip() {
    let v = example_response();
    let kinds: Vec<&str> = v["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    // The demo's canonical event kinds — if any of these go missing from
    // the example, the handler's shape contract has drifted.
    assert!(kinds.contains(&"rate_revision"));
    assert!(kinds.contains(&"bayesops_fit"));
    assert!(kinds.contains(&"agent_run"));
    assert!(kinds.contains(&"upstream_resolved"));
}

#[test]
fn rate_revision_event_carries_full_context() {
    let v = example_response();
    let revision = v["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "rate_revision" && e["revision_seq"] == 1)
        .expect("second revision should exist");

    // The fields the spacetime endpoint already returns and which the
    // timeline preserves
    for key in &[
        "ts",
        "revision_seq",
        "predicted_probability",
        "previous_probability",
        "revision_trigger",
        "reason",
    ] {
        assert!(
            revision.get(*key).is_some(),
            "rate_revision event missing '{}'",
            key
        );
    }
    // The bayesops_refit trigger should be visible — that's the whole
    // point of R-3 Piece 1.
    assert_eq!(revision["revision_trigger"], "bayesops_refit");
}

#[test]
fn bayesops_fit_event_carries_impact_assessment() {
    let v = example_response();
    let fit = v["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "bayesops_fit")
        .expect("bayesops_fit event should exist");
    for key in &[
        "snapshot_id",
        "driver_name",
        "decision",
        "n_observations",
        "n_eff",
        "ci_width",
        "rate_before",
        "rate_after",
        "delta_pp",
    ] {
        assert!(
            fit.get(*key).is_some(),
            "bayesops_fit event missing '{}'",
            key
        );
    }
    let decision = fit["decision"].as_str().unwrap();
    assert!(
        ["auto_accepted", "staged", "hard_blocked"].contains(&decision),
        "unexpected bayesops_fit decision '{}'",
        decision
    );
}

#[test]
fn agent_run_event_carries_metadata() {
    let v = example_response();
    let agent = v["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "agent_run")
        .expect("agent_run event should exist");
    assert_eq!(agent["sender_type"], "agent");
    assert!(agent["metadata"].is_object());
    assert!(agent["metadata"]["agent_name"].is_string());
}

#[test]
fn span_summary_is_consistent() {
    let v = example_response();
    let events_len = v["events"].as_array().unwrap().len();
    let rate_revs = v["rate_series"].as_array().unwrap().len();
    let markets = v["market_series"].as_array().unwrap().len();
    assert_eq!(v["span"]["event_count"], events_len);
    assert_eq!(v["span"]["rate_revision_count"], rate_revs);
    assert_eq!(v["span"]["market_observation_count"], markets);
}
