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
}
