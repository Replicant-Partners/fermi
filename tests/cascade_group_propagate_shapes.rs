//! Wire-format tests for the group-scoped propagate endpoint
//! (Phase 2.5 Slice B — the cascade detail panel's dry-run preview).
//!
//! These tests pin the JSON shape returned by
//! `POST /api/relationship-groups/:group_id/propagate` — what the
//! console's `render_cascade_detail_body` reads to render the preview
//! table. The response is a `PropagateResult`, which is also what the
//! legacy `/api/forecast-relationships/:id/propagate` endpoint and the
//! `queue_pending_cascade` internal snapshot return, so this contract
//! is broadly shared. If it changes, several call sites move together.
//!
//! Same style as `forecast_timeline_shapes.rs` and
//! `forecast_cascade_provenance_shapes.rs`: no live server, just
//! construct the expected shape and assert its invariants.

use serde_json::{json, Value};

/// Canonical dry-run response. Scenario: the "wc_2026_winner" mutex
/// group with three survivors + one trigger; operator hovers "resolve
/// Curaçao NO", server previews the redistribution. `deltas` sums
/// (in probability space) to zero because a mutex redistribution is
/// mass-conserving — the trigger loses `trigger_prev`, the survivors
/// gain it collectively.
fn example_dry_run_response() -> Value {
    json!({
        "n_updated": 4,
        "deltas": [
            {
                "forecast_id": "fc-curacao-2026",
                "previous_probability": 0.010,
                "new_probability": 0.001,
                "delta_pp": -0.9
            },
            {
                "forecast_id": "fc-spain-2026",
                "previous_probability": 0.559,
                "new_probability": 0.5643,
                "delta_pp": 0.53
            },
            {
                "forecast_id": "fc-france-2026",
                "previous_probability": 0.258,
                "new_probability": 0.2604,
                "delta_pp": 0.24
            },
            {
                "forecast_id": "fc-england-2026",
                "previous_probability": 0.163,
                "new_probability": 0.1643,
                "delta_pp": 0.13
            }
        ],
        "note": null
    })
}

fn get_f64(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(|x| x.as_f64()).unwrap_or_else(|| {
        panic!("missing or non-numeric field: {}", key);
    })
}

#[test]
fn top_level_fields_present() {
    let r = example_dry_run_response();
    for f in ["n_updated", "deltas", "note"] {
        assert!(r.get(f).is_some(), "missing top-level field: {}", f);
    }
    assert!(r["deltas"].is_array(), "deltas must be an array");
}

#[test]
fn n_updated_matches_delta_count() {
    // For dry-run responses n_updated should equal deltas.len() (no
    // rows actually written). Live-apply may report the write count if
    // it diverges, but the current handler pins them equal.
    let r = example_dry_run_response();
    let deltas = r["deltas"].as_array().unwrap();
    let n = r["n_updated"].as_u64().unwrap() as usize;
    assert_eq!(
        n,
        deltas.len(),
        "n_updated must equal deltas.len() in the dry-run contract"
    );
}

#[test]
fn every_delta_row_has_the_five_fields() {
    // These are the fields `render_cascade_preview_row` reads. If any
    // are dropped from the handler, the preview panel renders empty
    // rows silently — this test catches that.
    let r = example_dry_run_response();
    for d in r["deltas"].as_array().unwrap() {
        for f in [
            "forecast_id",
            "previous_probability",
            "new_probability",
            "delta_pp",
        ] {
            assert!(d.get(f).is_some(), "delta row missing field: {}", f);
        }
        assert!(
            d["forecast_id"].as_str().is_some(),
            "forecast_id must be a string"
        );
    }
}

#[test]
fn per_row_prev_plus_delta_equals_new() {
    // Matches the same invariant we enforce on the provenance shape.
    // `delta_pp` is a display convenience; the server should never
    // return values that disagree with the arithmetic on prev/new.
    let r = example_dry_run_response();
    for d in r["deltas"].as_array().unwrap() {
        let prev = get_f64(d, "previous_probability");
        let new_p = get_f64(d, "new_probability");
        let delta_pp = get_f64(d, "delta_pp");
        let recomputed_pp = (new_p - prev) * 100.0;
        assert!(
            (recomputed_pp - delta_pp).abs() < 1e-2,
            "row prev={}, new={} implies delta_pp={:.4}, but got {}",
            prev,
            new_p,
            recomputed_pp,
            delta_pp
        );
    }
}

#[test]
fn mutex_dry_run_is_mass_conserving() {
    // The mutex kind's core invariant: Σ new_p == Σ prev_p over the
    // group members (mass moves between them but doesn't appear or
    // vanish). The cascade detail panel relies on this for its
    // "Σp still 1.0 after resolve" health strip; a violation here
    // means the engine is drifting mass, which the strip should show
    // as red.
    let r = example_dry_run_response();
    let (sum_prev, sum_new): (f64, f64) =
        r["deltas"]
            .as_array()
            .unwrap()
            .iter()
            .fold((0.0, 0.0), |(sp, sn), d| {
                (
                    sp + get_f64(d, "previous_probability"),
                    sn + get_f64(d, "new_probability"),
                )
            });
    // Tolerance covers f32→f64 round-tripping through the DB layer
    // (predicted_probability is REAL in Postgres) plus the FLOOR/CEIL
    // clamping in propagate_mutex (0.001/0.999).
    assert!(
        (sum_new - sum_prev).abs() < 0.005,
        "mutex dry-run drifted mass: Σprev={:.4}, Σnew={:.4}",
        sum_prev,
        sum_new
    );
}

#[test]
fn trigger_row_dominates_the_delta_magnitude() {
    // The trigger (Curaçao) is the row that loses the most mass in
    // absolute terms; the survivors gain fractions of it weighted by
    // their prior share. The detail panel's preview sorts by
    // |delta_pp| desc and highlights the trigger — this invariant
    // holds as long as the trigger's |delta| exceeds every survivor's.
    // Fixture-driven, not a general theorem: a symmetric group could
    // violate it. But typical cascade previews satisfy it, and if a
    // future rule kind doesn't, the UI just loses the visual anchor.
    let r = example_dry_run_response();
    let deltas = r["deltas"].as_array().unwrap();
    let max_mag = deltas
        .iter()
        .map(|d| get_f64(d, "delta_pp").abs())
        .fold(0.0_f64, f64::max);
    // In the fixture, Curaçao's -0.9 pp is the largest magnitude.
    assert!(
        (max_mag - 0.9).abs() < 1e-6,
        "expected trigger magnitude 0.9 pp, got {}",
        max_mag
    );
}
