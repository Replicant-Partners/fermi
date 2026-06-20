//! Mathematical contract tests for `mutually_exclusive` propagation.
//!
//! The actual handler hits the database, so we can't unit-test the full
//! end-to-end path here. Instead we test the **redistribution math** as
//! a pure function: given the trigger's previous probability and the
//! current probabilities of the survivors, what new probabilities
//! should each survivor have?
//!
//! Properties we want:
//!   1. Mass conservation: sum of new-probabilities equals
//!      sum-of-old-probabilities (within ε for floating point) when
//!      starting from a valid mutex (sum ≈ 1.0).
//!   2. Proportional redistribution: stronger survivors absorb more of
//!      the eliminated mass. Specifically, the increase Δ_i for
//!      survivor i is `trigger_prev * (p_i / Σ p_j)`.
//!   3. Resolve YES: every sibling drops to ~0, trigger goes to 1.
//!   4. Resolve NO with single-survivor pool: that survivor gets ALL
//!      the eliminated mass.
//!   5. Degenerate (sum-to-zero survivor pool): no movement, returns
//!      empty deltas + a note.

/// Simulates the same redistribution math the server-side
/// `propagate_mutex` function applies, in pure-Rust form so we can
/// assert against expected values without database setup.
///
/// Returns `(deltas, note)` where deltas is `Vec<(id, prev, new)>` for
/// each forecast that moved.
fn redistribute_on_resolve_no(
    trigger_id: &str,
    trigger_prev: f64,
    survivors: &[(&str, f64)],
) -> (Vec<(String, f64, f64)>, Option<String>) {
    let survivor_total: f64 = survivors.iter().map(|(_, p)| *p).sum();
    if survivor_total < 1e-9 {
        return (Vec::new(), Some("survivor pool sums to zero".into()));
    }
    let mut out: Vec<(String, f64, f64)> = Vec::new();
    for (id, prev) in survivors {
        let share = prev / survivor_total;
        let absorbed = trigger_prev * share;
        let new_p = (prev + absorbed).clamp(0.001, 0.999);
        if (new_p - prev).abs() > 1e-5 {
            out.push(((*id).to_string(), *prev, new_p));
        }
    }
    if trigger_prev > 0.001 {
        out.push((trigger_id.to_string(), trigger_prev, 0.001));
    }
    (out, None)
}

#[test]
fn mass_conservation_resolve_no() {
    // 4-team mutex summing to 1.0. Eliminate the 0.20 team — its mass
    // should redistribute proportionally to the other three.
    let trigger_id = "team_d";
    let survivors = vec![
        ("team_a", 0.40),
        ("team_b", 0.25),
        ("team_c", 0.15),
    ];
    let (deltas, note) = redistribute_on_resolve_no(trigger_id, 0.20, &survivors);
    assert!(note.is_none(), "expected no note, got: {:?}", note);

    // Mass before = 1.00. Mass after: trigger drops to 0.001, survivors
    // sum to 0.80 + 0.20 = 1.00. So total = 1.001 (the floor). Close
    // enough to mass conservation given the floor clamp.
    let total_after: f64 = deltas.iter().map(|(_, _, new_p)| new_p).sum();
    assert!(
        (total_after - 1.001).abs() < 0.001,
        "mass conservation broken: total after = {}",
        total_after
    );

    // team_a gets the largest absorption (it has 50% of the survivor
    // pool, so it absorbs 50% of the 0.20 = 0.10 → goes from 0.40 to 0.50).
    let team_a = deltas.iter().find(|(id, _, _)| id == "team_a").unwrap();
    assert!(
        (team_a.2 - 0.50).abs() < 0.001,
        "team_a expected to absorb 0.10 (40/80 of 0.20), got new={:.4}",
        team_a.2
    );

    // team_c gets the smallest absorption (15/80 of 0.20 = 0.0375 →
    // 0.15 + 0.0375 = 0.1875).
    let team_c = deltas.iter().find(|(id, _, _)| id == "team_c").unwrap();
    assert!(
        (team_c.2 - 0.1875).abs() < 0.001,
        "team_c expected new=0.1875, got {:.4}",
        team_c.2
    );
}

#[test]
fn single_survivor_absorbs_all() {
    let (deltas, _) = redistribute_on_resolve_no(
        "trigger",
        0.30,
        &[("only_survivor", 0.70)],
    );
    let s = deltas
        .iter()
        .find(|(id, _, _)| id == "only_survivor")
        .unwrap();
    // The single survivor's share is 1.0; absorbs the entire 0.30 →
    // goes from 0.70 to 1.0, but clamped to 0.999.
    assert!(
        (s.2 - 0.999).abs() < 0.0001,
        "single survivor should absorb all and clamp to 0.999, got {}",
        s.2
    );
}

#[test]
fn degenerate_zero_survivor_pool_returns_note() {
    let (deltas, note) = redistribute_on_resolve_no(
        "trigger",
        0.30,
        &[("dead1", 0.0), ("dead2", 0.0)],
    );
    assert!(deltas.is_empty(), "no movement when survivor pool is zero");
    assert!(note.is_some(), "expected explanatory note");
}

#[test]
fn proportional_redistribution_preserves_relative_ranking() {
    // Stronger survivors stay stronger after the cascade.
    let survivors = vec![
        ("strong", 0.30),
        ("medium", 0.20),
        ("weak", 0.10),
    ];
    let (deltas, _) = redistribute_on_resolve_no("trigger", 0.40, &survivors);
    let new = |id: &str| -> f64 {
        deltas.iter().find(|(d, _, _)| d == id).unwrap().2
    };
    assert!(new("strong") > new("medium"));
    assert!(new("medium") > new("weak"));

    // The relative ratio strong:medium:weak (3:2:1) should be preserved
    // (each absorbs proportionally to its current p).
    let s = new("strong");
    let m = new("medium");
    let w = new("weak");
    let ratio_sm = s / m;
    let ratio_mw = m / w;
    // Old ratios: 0.30/0.20 = 1.5, 0.20/0.10 = 2.0
    // New ratios: each grew by the same proportional factor (1 + 0.40/0.60)
    // so ratios stay 1.5 and 2.0.
    assert!(
        (ratio_sm - 1.5).abs() < 0.01,
        "strong/medium ratio should stay 1.5, got {}",
        ratio_sm
    );
    assert!(
        (ratio_mw - 2.0).abs() < 0.01,
        "medium/weak ratio should stay 2.0, got {}",
        ratio_mw
    );
}
