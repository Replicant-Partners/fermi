//! Driver readiness predicates — GPUI-free, therefore testable.
//!
//! Lives in the lib target for the reason given in the crate docs: logic
//! inside the binary cannot be tested at all, and "is this driver ready to
//! spend money on?" is a question whose wrong answer costs the user real
//! credits in both directions.

use fermi::ast::{Distribution, DriverStmt, Expression};

/// Read a driver parameter as a number.
///
/// FPL driver parameters are `Expression`s because they may reference other
/// drivers. Anything that isn't a literal reads as 0.0 — matching the
/// console's own evaluation — which is why [`driver_is_unspecified`]
/// requires *all three* percentiles to be zero before it concludes
/// anything: a single zero is indistinguishable from "not a literal".
fn as_number(expr: &Expression) -> f64 {
    match expr {
        Expression::Number(n) => *n,
        _ => 0.0,
    }
}

/// Whether `p5`/`p50`/`p95` describe an unfilled placeholder rather than an
/// estimate.
///
/// A `triangular(0, 0, 0)` is degenerate: it has zero width, so it carries
/// no uncertainty, and it is zero-valued, so as a multiplier it annihilates
/// the whole model. Nobody enters it on purpose.
pub fn triangular_is_unspecified(p5: f64, p50: f64, p95: f64) -> bool {
    p5 == 0.0 && p50 == 0.0 && p95 == 0.0
}

/// Whether a driver is still the unfilled placeholder that
/// `add_manual_driver` seeds.
///
/// `add_manual_driver` creates a continuous driver named `Driver N` with
/// `triangular(0, 0, 0)` and a "describe this driver and set your
/// estimates" rationale, on the expectation the user fills it in. Every
/// generated template driver ships real values (0.7/1.0/1.4 and friends),
/// so the all-zero triangular is an unambiguous marker.
///
/// # Why this is checked before dispatch
///
/// Decomposition hires one research agent per driver. A forgotten
/// placeholder used to be dispatched like any other, producing a query the
/// agent cannot answer:
///
/// ```text
/// Research evidence for the 'Driver 3' driver.
/// Current estimate: p5=0.00, p50=0.00, p95=0.00
/// Context:
/// ```
///
/// The agent then spends its entire iteration budget searching for a
/// subject that was never named, returns no final text, and the run is
/// billed as a failure. Skipping is strictly better than asking.
///
/// Deliberately conservative: only the fully degenerate triangular counts.
/// A false positive silently starves a real driver of research, which is a
/// worse failure than the one being prevented.
pub fn driver_is_unspecified(driver: &DriverStmt) -> bool {
    match driver.distribution.as_ref() {
        Some(Distribution::Triangular { p5, p50, p95 }) => {
            triangular_is_unspecified(as_number(p5), as_number(p50), as_number(p95))
        }
        _ => false,
    }
}

/// Which driver an agent's stated judgement belongs to.
///
/// See [`bind_judgement_to_driver`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JudgementBinding {
    /// Unambiguous: the agent named the driver, or it covers only one.
    Bound(String),
    /// The agent stated an adjustment and covers several drivers without
    /// saying which it meant. Carries the candidates so the operator can pick.
    Ambiguous(Vec<String>),
    /// The agent is bound to no driver at all.
    NoDriver,
}

/// Decide which driver a single stated judgement applies to.
///
/// # The case this exists for
///
/// An agent bound to N drivers states ONE `[MULTIPLIER]`. That is not an
/// oversight in the cards — it is what they specify: `weather_oracle`'s card
/// says "the last two key_findings MUST use these exact formats" and names one
/// `[MULTIPLIER]` line. Measured over the production episode table: 31 episodes
/// carrying a multiplier, **31 distinct triples**, and not one episode holding
/// two different ones.
///
/// So the broker pattern — one complex agent responsible for several drivers,
/// resolving the relationships between them internally — produces one number
/// for many drivers, and the number cannot honestly be split. Applying it to
/// every ref compounds it (a 1.25 across five drivers is 3.05, not 1.25); that
/// is what `agent_params_hook` does. Applying it to `driver_refs.first()` picks
/// one arbitrarily and says nothing; that is what the console did, and it is
/// why four of five drivers on a broker-driven forecast never moved while the
/// first one did.
///
/// Neither is defensible, so neither is offered. When the target cannot be
/// determined the judgement is not bound and the candidates are returned, which
/// leaves a human to make the choice that was always being made silently.
///
/// # Resolution order
///
/// 1. `hint` matching a ref wins — the agent said so. Compared with `-`/`_`
///    folded and case ignored, because cards spell driver names both ways.
/// 2. A single ref is unambiguous whether or not anything was hinted.
/// 3. Otherwise ambiguous.
///
/// No agent populates `target_hint` today (0 of 64 stored assertions), so arm 1
/// is currently unreachable. It is the arm that makes the broker pattern work
/// properly once an agent names its target, which is the point: the fix is for
/// the agent to say, and this is what will listen when it does.
pub fn bind_judgement_to_driver(hint: Option<&str>, driver_refs: &[String]) -> JudgementBinding {
    let fold = |s: &str| s.trim().to_ascii_lowercase().replace('-', "_");

    if driver_refs.is_empty() {
        return JudgementBinding::NoDriver;
    }

    if let Some(h) = hint.map(fold).filter(|h| !h.is_empty()) {
        if let Some(hit) = driver_refs.iter().find(|d| fold(d) == h) {
            return JudgementBinding::Bound(hit.clone());
        }
    }

    if driver_refs.len() == 1 {
        return JudgementBinding::Bound(driver_refs[0].clone());
    }

    JudgementBinding::Ambiguous(driver_refs.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fermi::ast::DriverType;

    fn continuous(name: &str, p5: f64, p50: f64, p95: f64) -> DriverStmt {
        DriverStmt {
            name: name.to_string(),
            driver_type: DriverType::Continuous,
            distribution: Some(Distribution::Triangular {
                p5: Expression::Number(p5),
                p50: Expression::Number(p50),
                p95: Expression::Number(p95),
            }),
            ..default_driver(name)
        }
    }

    fn default_driver(name: &str) -> DriverStmt {
        DriverStmt {
            name: name.to_string(),
            driver_type: DriverType::Continuous,
            distribution: None,
            display_name: None,
            description: None,
            unit: None,
            probability: None,
            impact_multiplier: None,
            values: None,
            weights: None,
            evidence_refs: vec![],
            rationale: None,
            constraints: vec![],
            learnable: false,
            feeds_from: None,
            applies_to: None,
        }
    }

    #[test]
    fn all_zero_triangular_is_the_placeholder() {
        assert!(driver_is_unspecified(&continuous(
            "driver_3", 0.0, 0.0, 0.0
        )));
    }

    #[test]
    fn a_filled_in_driver_is_specified() {
        assert!(!driver_is_unspecified(&continuous(
            "ai_product_execution",
            0.8,
            1.05,
            1.3
        )));
    }

    #[test]
    fn a_zero_p5_alone_is_not_a_placeholder() {
        // A legitimate estimate may bottom out at zero. Only the fully
        // degenerate case counts.
        assert!(!driver_is_unspecified(&continuous("x", 0.0, 1.0, 2.0)));
        assert!(!driver_is_unspecified(&continuous("y", 0.0, 0.0, 1.0)));
    }

    #[test]
    fn template_driver_values_are_never_placeholders() {
        // The literal percentile triples `generate_decomposition` ships for
        // every domain. A false positive here would skip research on a real
        // driver, so these are the cases that must stay negative.
        for (p5, p50, p95) in [
            (0.7, 1.0, 1.4), // finance / fundamentals
            (0.6, 1.0, 1.5), // finance / market_conditions, tech / adoption
            (0.8, 1.0, 1.3), // finance / momentum
            (0.5, 1.0, 1.3), // technology / feasibility
            (0.75, 0.95, 1.2),
            (0.7, 1.0, 1.15),
            (0.8, 1.05, 1.3),
        ] {
            assert!(
                !triangular_is_unspecified(p5, p50, p95),
                "template triple ({p5}, {p50}, {p95}) read as an unfilled placeholder"
            );
        }
    }

    #[test]
    fn a_binary_driver_is_never_a_continuous_placeholder() {
        // Binary drivers carry probability/impact, not a triangular, and
        // `add_manual_driver` seeds them with real values (0.5 / 1.3).
        let mut d = default_driver("event_2");
        d.driver_type = DriverType::Binary;
        d.probability = Some(0.5);
        d.impact_multiplier = Some(1.3);
        assert!(!driver_is_unspecified(&d));
    }

    #[test]
    fn a_non_triangular_distribution_is_left_alone() {
        let mut d = default_driver("n");
        d.distribution = Some(Distribution::Normal {
            mean: Expression::Number(0.0),
            stddev: Expression::Number(0.0),
        });
        assert!(!driver_is_unspecified(&d));
    }

    // ── Binding a judgement to a driver ──────────────────────────────────

    fn refs(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_single_driver_ref_needs_no_hint() {
        assert_eq!(
            bind_judgement_to_driver(None, &refs(&["ensemble_spread"])),
            JudgementBinding::Bound("ensemble_spread".into())
        );
    }

    /// The weather broker: one agent, five drivers, one stated multiplier.
    ///
    /// The console used to take `driver_refs.first()`, so `ensemble_spread`
    /// moved and the other four silently did not. The server hook applies the
    /// same triple to all five, which compounds a 1.25 into 3.05. Neither is
    /// offered; the candidates come back for a human to choose from.
    #[test]
    fn one_judgement_across_several_drivers_is_ambiguous_not_the_first_one() {
        let drivers = refs(&[
            "ensemble_spread",
            "model_cluster",
            "urban_heat_island",
            "synoptic_pattern",
            "climate_trend",
        ]);
        let bound = bind_judgement_to_driver(None, &drivers);

        assert_eq!(
            bound,
            JudgementBinding::Ambiguous(drivers.clone()),
            "picking the first driver is a choice being made silently on the \
             operator's behalf, and it is wrong four times out of five"
        );
        match bound {
            JudgementBinding::Ambiguous(c) => assert_eq!(
                c.len(),
                5,
                "every candidate must be offered, or the operator cannot choose"
            ),
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn a_named_target_resolves_the_ambiguity() {
        let drivers = refs(&["ensemble_spread", "model_cluster", "climate_trend"]);
        assert_eq!(
            bind_judgement_to_driver(Some("model_cluster"), &drivers),
            JudgementBinding::Bound("model_cluster".into())
        );
    }

    /// Cards spell driver names with hyphens and with underscores.
    #[test]
    fn a_hint_matches_across_hyphen_and_case_spelling() {
        let drivers = refs(&["urban_heat_island", "climate_trend"]);
        assert_eq!(
            bind_judgement_to_driver(Some("Urban-Heat-Island"), &drivers),
            JudgementBinding::Bound("urban_heat_island".into())
        );
    }

    /// A hint naming something the agent is not bound to does not silently win.
    #[test]
    fn a_hint_for_an_unbound_driver_falls_back_rather_than_inventing_a_target() {
        let drivers = refs(&["ensemble_spread", "model_cluster"]);
        assert_eq!(
            bind_judgement_to_driver(Some("sea_surface_temp"), &drivers),
            JudgementBinding::Ambiguous(drivers),
            "an unrecognised hint is no information, not permission to guess"
        );
    }

    #[test]
    fn an_agent_bound_to_nothing_binds_nothing() {
        assert_eq!(
            bind_judgement_to_driver(Some("anything"), &[]),
            JudgementBinding::NoDriver
        );
    }
}
