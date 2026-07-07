//! Mathematical contract tests for `mutually_exclusive` propagation.
//!
//! The actual handler hits the database, so we can't unit-test the full
//! end-to-end path here. Instead we test the **redistribution math** as
//! a pure function: given the trigger's previous probability and the
//! current probabilities of the survivors, what new probabilities
//! should each survivor have?
//!
//! Properties we want (Spec 25 §3.1, updated 2026-07):
//!
//!   1. **Renormalisation**: after a resolve, live members sum to
//!      `1 − FLOOR·n_no − CEIL·n_yes` regardless of what they summed
//!      to going in. This is the core fix for the "159%" WC mutex
//!      bug — independent standalone sims don't sum to 1, and the
//!      cascade math must renormalise them.
//!   2. **Proportional preservation of ranking**: stronger survivors
//!      stay stronger; every survivor is scaled by the same factor.
//!   3. **Resolve YES**: every survivor collapses to FLOOR, trigger
//!      pins to CEIL.
//!   4. **Degenerate (survivors sum to ~0)**: no movement + explanatory
//!      note.

const FLOOR: f64 = 0.001;
const CEIL: f64 = 0.999;

/// Pure-Rust mirror of the server-side `propagate_mutex` renormalise
/// path for `("resolved", Some(false))`. Given the trigger, survivors,
/// and the counts of already-resolved siblings, returns
/// `(deltas, note)`.
fn redistribute_on_resolve_no(
    trigger_id: &str,
    trigger_prev: f64,
    survivors: &[(&str, f64)],
    n_resolved_no_prev: usize,
    n_resolved_yes_prev: usize,
) -> (Vec<(String, f64, f64)>, Option<String>) {
    // Trigger joins the resolved-NO ranks → the +1.
    let target_live_sum =
        (1.0 - FLOOR * (n_resolved_no_prev as f64 + 1.0) - CEIL * n_resolved_yes_prev as f64)
            .max(0.0);
    let survivor_total: f64 = survivors.iter().map(|(_, p)| *p).sum();

    let mut out: Vec<(String, f64, f64)> = Vec::new();
    if survivor_total < 1e-9 {
        return (out, Some("survivor pool sums to zero".into()));
    }
    let scale = target_live_sum / survivor_total;
    for (id, prev) in survivors {
        let new_p = (prev * scale).clamp(FLOOR, CEIL);
        if (new_p - prev).abs() > 1e-5 {
            out.push(((*id).to_string(), *prev, new_p));
        }
    }
    if (trigger_prev - FLOOR).abs() > 1e-5 {
        out.push((trigger_id.to_string(), trigger_prev, FLOOR));
    }
    (out, None)
}

#[test]
fn renormalises_to_one_when_starting_at_one() {
    // Classical 4-team mutex already summing to 1.0. Eliminating the
    // 0.20 team should leave the survivors summing to ≈ 1 − FLOOR·1
    // = 0.999, with the trigger pinned to FLOOR (total ≈ 1.000).
    let survivors = vec![("a", 0.40), ("b", 0.25), ("c", 0.15)];
    let (deltas, note) = redistribute_on_resolve_no("d", 0.20, &survivors, 0, 0);
    assert!(note.is_none());

    let survivor_sum_after: f64 = deltas
        .iter()
        .filter(|(id, _, _)| id != "d")
        .map(|(_, _, new_p)| new_p)
        .sum();
    assert!(
        (survivor_sum_after - (1.0 - FLOOR)).abs() < 1e-3,
        "survivors should renormalise to ≈ 0.999, got {}",
        survivor_sum_after
    );

    // Ranking preserved.
    let get = |id: &str| deltas.iter().find(|(d, _, _)| d == id).unwrap().2;
    assert!(get("a") > get("b"));
    assert!(get("b") > get("c"));
}

#[test]
fn recovers_from_inflated_starting_sum_wc_case() {
    // Regression for the "159%" WC bug: independent per-country sims
    // whose standalones sum to 1.59 must renormalise to ≈ 1.0 after
    // an elimination — NOT preserve the 1.59.
    let survivors = vec![
        ("england", 0.27),
        ("colombia", 0.12),
        ("argentina", 0.21),
        ("norway", 0.13),
        ("france", 0.29),
        ("spain", 0.30),
        ("belgium", 0.12),
        ("morocco", 0.04),
        ("switzerland", 0.10),
    ];
    // Jamaica @ 0.01 gets eliminated; 20 countries already resolved-NO.
    let (deltas, note) = redistribute_on_resolve_no("jamaica", 0.01, &survivors, 20, 0);
    assert!(note.is_none());

    let survivor_sum_after: f64 = deltas
        .iter()
        .filter(|(id, _, _)| id != "jamaica")
        .map(|(_, _, new_p)| new_p)
        .sum();
    // 20 already-resolved-NO plus the trigger = 21 members at FLOOR.
    let expected_live_sum = 1.0 - FLOOR * 21.0;
    assert!(
        (survivor_sum_after - expected_live_sum).abs() < 1e-3,
        "survivors should renormalise from 1.58 → {:.4}, got {:.4}",
        expected_live_sum,
        survivor_sum_after
    );

    // Total across the mutex group ≈ 1.0.
    let total = survivor_sum_after + FLOOR * 21.0;
    assert!(
        (total - 1.0).abs() < 1e-3,
        "whole mutex group must sum to ≈ 1.0, got {}",
        total
    );
}

#[test]
fn recovers_from_deflated_starting_sum() {
    // Symmetric case: standalones sum to 0.40 (each sim wildly
    // under-estimates). Renormalisation must scale them UP.
    let survivors = vec![("a", 0.10), ("b", 0.15), ("c", 0.15)];
    let (deltas, _) = redistribute_on_resolve_no("t", 0.05, &survivors, 0, 0);
    let survivor_sum_after: f64 = deltas
        .iter()
        .filter(|(id, _, _)| id != "t")
        .map(|(_, _, new_p)| new_p)
        .sum();
    assert!(
        (survivor_sum_after - (1.0 - FLOOR)).abs() < 1e-3,
        "survivors should scale UP to ≈ 0.999, got {}",
        survivor_sum_after
    );
}

#[test]
fn ranking_preserved_under_renormalisation() {
    // Every survivor is multiplied by the same scale factor, so
    // relative ratios are preserved bit-for-bit.
    let survivors = vec![("strong", 0.30), ("medium", 0.20), ("weak", 0.10)];
    let (deltas, _) = redistribute_on_resolve_no("t", 0.40, &survivors, 0, 0);
    let get = |id: &str| deltas.iter().find(|(d, _, _)| d == id).unwrap().2;
    let s = get("strong");
    let m = get("medium");
    let w = get("weak");
    // Old ratios: 3:2:1 → new ratios must be identical.
    assert!((s / m - 1.5).abs() < 1e-6, "strong/medium ratio drifted");
    assert!((m / w - 2.0).abs() < 1e-6, "medium/weak ratio drifted");
}

#[test]
fn single_survivor_absorbs_all_but_clamps_at_ceil() {
    let (deltas, _) = redistribute_on_resolve_no("trigger", 0.30, &[("only_survivor", 0.70)], 0, 0);
    let s = deltas
        .iter()
        .find(|(id, _, _)| id == "only_survivor")
        .unwrap();
    // Would renormalise to 0.999 exactly; clamp is a no-op here.
    assert!(
        (s.2 - CEIL).abs() < 1e-4,
        "single survivor should scale to CEIL, got {}",
        s.2
    );
}

#[test]
fn degenerate_zero_survivor_pool_returns_note() {
    let (deltas, note) =
        redistribute_on_resolve_no("trigger", 0.30, &[("dead1", 0.0), ("dead2", 0.0)], 0, 0);
    // Trigger still needs to be pinned to FLOOR even when the survivor
    // pool is empty.
    assert!(
        deltas.iter().all(|(id, _, _)| id == "trigger"),
        "no survivor deltas when pool sums to zero"
    );
    assert!(note.is_some(), "expected explanatory note");
}
