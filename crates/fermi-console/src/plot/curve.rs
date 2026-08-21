//! A driver's shape, drawn from the distribution the simulation will sample.
//!
//! # What was wrong
//!
//! `render_driver_card` could only draw a Triangular driver. The gate read
//!
//! ```ignore
//! if let Some(Distribution::Triangular { p5, p50, p95 }) = driver.distribution {
//!     ... sparkline ...
//! } else { el }          // <- normal, lognormal, uniform, beta: nothing
//! ```
//!
//! and the `else` arms returned the element unchanged, so four of the five
//! distribution types the engine samples rendered no curve at all — silently,
//! while the text summary beside them read "no distribution" for a driver that
//! had one. A `discrete` driver rendered `—`, and its `values`/`weights` were
//! printed nowhere in the console.
//!
//! That is not a cosmetic gap. `examples/reference_bucket_indicator_kord.fpl`
//! carries `predictive_error_f` as `normal(0.0, 2.796)` and `model_cluster` as a
//! two-point discrete whose bimodality the file's own commentary calls "the
//! dominant uncertainty". Neither was visible.
//!
//! # Why sampling rather than quantiles
//!
//! The one curve that did render went through `Density::from_quantiles`, which
//! the density module itself labels a sketch: `shape_is_real()` returns false
//! for it, because a two-sided Gaussian through p5/p50/p95 cannot show
//! skew, bimodality, or a bound. `Density::from_samples` — a Gaussian KDE over
//! real draws — was written, tested, and never called from any render path.
//!
//! Drawing from [`fermi::distributions::sample_literal`] means the picture comes
//! from the same per-family samplers the executor runs, so the curve is a
//! picture of what the simulation will actually draw rather than a second
//! opinion about it.

use fermi::ast::{DriverStmt, DriverType};
use rand::SeedableRng;

use super::density::Density;

/// Draws taken per curve.
///
/// Enough for the KDE to show a second mode — the case a quantile sketch
/// structurally cannot represent, and the one the reference forecast turns on.
/// The cost is a few hundred microseconds, paid when the driver changes rather
/// than per frame.
pub const CURVE_SAMPLES: usize = 4000;

/// Fixed so a driver's curve does not shimmer between redraws.
///
/// A visibly different shape on every repaint would read as the model changing
/// when nothing had. Sampling noise is not information.
const CURVE_SEED: u64 = 0x5EED_C0FFEE;

/// Representative draws for a driver, or `None` when it cannot be sampled.
///
/// `None` is returned rather than an empty or defaulted curve for:
///   * a **parameterised** distribution (`triangular(socio_p5, …)`) whose values
///     live in `workspace_params` and are not in the AST — drawing zeros would
///     be a confident picture of a distribution nobody declared;
///   * a **binary** driver, which is a probability and a multiplier rather than
///     a shape;
///   * a **discrete** driver with no values, or weights that do not match.
pub fn driver_samples(driver: &DriverStmt) -> Option<Vec<f64>> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(CURVE_SEED);

    match driver.driver_type {
        DriverType::Continuous => {
            let dist = driver.distribution.as_ref()?;
            let draws: Vec<f64> = (0..CURVE_SAMPLES)
                .filter_map(|_| fermi::distributions::sample_literal(&mut rng, dist))
                .collect();
            // A single unresolvable parameter yields nothing at all, not a
            // partial cloud: `sample_literal` fails identically every draw.
            (draws.len() == CURVE_SAMPLES).then_some(draws)
        }
        DriverType::Discrete => {
            let values = driver.values.as_ref()?;
            let weights = driver.weights.as_ref()?;
            let draws: Vec<f64> = (0..CURVE_SAMPLES)
                .filter_map(|_| fermi::distributions::sample_categorical(&mut rng, values, weights))
                .collect();
            (draws.len() == CURVE_SAMPLES).then_some(draws)
        }
        DriverType::Binary => None,
    }
}

/// The density to paint for a driver, or `None` when there is nothing honest to
/// paint.
pub fn driver_density(driver: &DriverStmt, grid_n: usize) -> Option<Density> {
    driver_curve(driver, grid_n).map(|(d, _)| d)
}

/// The curve and its `(p5, p50, p95)` tick positions.
///
/// The percentiles come from the same draws as the curve, so a tick always
/// lands on the shape it annotates. Reading them off the AST instead would put
/// the ticks in the right place only for Triangular — which is exactly the
/// coupling that limited the card to one distribution type.
pub fn driver_curve(driver: &DriverStmt, grid_n: usize) -> Option<(Density, (f64, f64, f64))> {
    let samples = driver_samples(driver)?;
    let d = Density::from_samples(&samples, grid_n);
    if d.is_empty() {
        return None;
    }
    Some((d, sample_quantiles(&samples)))
}

/// `(p5, p50, p95)` by nearest-rank over a copy of the draws.
pub fn sample_quantiles(samples: &[f64]) -> (f64, f64, f64) {
    let mut xs: Vec<f64> = samples.iter().copied().filter(|v| v.is_finite()).collect();
    if xs.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let at = |q: f64| {
        let i = ((xs.len() as f64 - 1.0) * q).round() as usize;
        xs[i.min(xs.len() - 1)]
    };
    (at(0.05), at(0.50), at(0.95))
}

/// A one-line summary of a driver's shape, for the text beside the curve.
///
/// Replaces a `match` whose non-Triangular arm produced the string
/// `"no distribution"` for drivers that had one, and `"—"` for every discrete
/// driver. Each arm now says what the driver actually is, in the units the
/// author wrote.
pub fn driver_summary(driver: &DriverStmt) -> String {
    use fermi::ast::Distribution as D;
    use fermi::ast::Expression;

    let unit = driver
        .unit
        .as_deref()
        .map(|u| format!(" {u}"))
        .unwrap_or_default();

    let n = |e: &Expression| match e {
        Expression::Number(v) => Some(*v),
        _ => None,
    };
    // A parameterised driver names its parameter rather than pretending to a
    // value: the number lives in `workspace_params`, and printing 0.0 here is
    // how a placeholder becomes indistinguishable from an estimate.
    let show = |e: &Expression| match e {
        Expression::Number(v) => format!("{v:.4}"),
        Expression::Identifier(name) => format!("<{name}>"),
        _ => "<expr>".to_string(),
    };

    match driver.driver_type {
        DriverType::Binary => {
            let p = driver.probability.unwrap_or(0.0);
            let m = driver.impact_multiplier.unwrap_or(1.0);
            format!("{:.0}% (\u{d7}{:.2})", p * 100.0, m)
        }
        DriverType::Discrete => match (&driver.values, &driver.weights) {
            (Some(values), Some(weights)) if !values.is_empty() => {
                let parts: Vec<String> = values
                    .iter()
                    .zip(weights.iter())
                    .map(|(v, w)| format!("{v:.4} @ {:.0}%", w * 100.0))
                    .collect();
                format!("{}{}", parts.join(" \u{b7} "), unit)
            }
            _ => "discrete, no values".to_string(),
        },
        DriverType::Continuous => match driver.distribution.as_ref() {
            None => "no distribution".to_string(),
            Some(D::Triangular { p5, p50, p95 }) => {
                format!(
                    "{} \u{2013} {} \u{2013} {}{}",
                    show(p5),
                    show(p50),
                    show(p95),
                    unit
                )
            }
            Some(D::Normal { mean, stddev }) => match (n(mean), n(stddev)) {
                (Some(m), Some(s)) => format!("normal \u{3bc}={m:.4} \u{3c3}={s:.4}{unit}"),
                _ => format!("normal({}, {})", show(mean), show(stddev)),
            },
            Some(D::Lognormal { median, sigma }) => match (n(median), n(sigma)) {
                (Some(m), Some(s)) => {
                    format!("lognormal median={m:.4} \u{3c3}={s:.4}{unit}")
                }
                _ => format!("lognormal({}, {})", show(median), show(sigma)),
            },
            Some(D::Uniform { low, high }) => {
                format!("uniform {} \u{2013} {}{}", show(low), show(high), unit)
            }
            Some(D::Beta { alpha, beta, .. }) => {
                format!("beta \u{3b1}={} \u{3b2}={}", show(alpha), show(beta))
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fermi::ast::{Distribution, Expression};

    fn driver(driver_type: DriverType, distribution: Option<Distribution>) -> DriverStmt {
        DriverStmt {
            name: "d".into(),
            driver_type,
            distribution,
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

    fn num(v: f64) -> Expression {
        Expression::Number(v)
    }

    fn mean_of(s: &[f64]) -> f64 {
        s.iter().sum::<f64>() / s.len() as f64
    }

    /// The four types that rendered nothing now produce a curve.
    ///
    /// This is the whole defect: the engine samples five distribution types and
    /// the console could draw one. The `else` arm was silent, so a `normal`
    /// driver looked like a driver with no distribution at all.
    #[test]
    fn every_continuous_distribution_the_engine_samples_can_be_drawn() {
        let cases = [
            (
                "triangular",
                Distribution::Triangular {
                    p5: num(0.8),
                    p50: num(1.0),
                    p95: num(1.3),
                },
            ),
            (
                "normal",
                Distribution::Normal {
                    mean: num(0.0),
                    stddev: num(2.796),
                },
            ),
            (
                "lognormal",
                Distribution::Lognormal {
                    median: num(1.0),
                    sigma: num(0.4),
                },
            ),
            (
                "uniform",
                Distribution::Uniform {
                    low: num(2.0),
                    high: num(6.0),
                },
            ),
            (
                "beta",
                Distribution::Beta {
                    alpha: num(2.0),
                    beta: num(5.0),
                    min: None,
                    max: None,
                },
            ),
        ];

        for (name, dist) in cases {
            let d = driver(DriverType::Continuous, Some(dist));
            let density = driver_density(&d, 96)
                .unwrap_or_else(|| panic!("{name} produced no curve — this is the defect"));
            assert!(
                density.source.shape_is_real(),
                "{name} must be drawn from draws, not sketched from quantiles"
            );
        }
    }

    /// `normal(0.0, 2.796)` — the reference forecast's `predictive_error_f`.
    #[test]
    fn a_normal_driver_is_sampled_around_its_mean() {
        let d = driver(
            DriverType::Continuous,
            Some(Distribution::Normal {
                mean: num(10.0),
                stddev: num(1.0),
            }),
        );
        let s = driver_samples(&d).expect("normal is samplable");
        assert_eq!(s.len(), CURVE_SAMPLES);
        assert!(
            (mean_of(&s) - 10.0).abs() < 0.1,
            "sample mean {} is not near the declared mean",
            mean_of(&s)
        );
    }

    /// The bimodal discrete driver a quantile sketch structurally cannot show.
    ///
    /// `model_cluster` in the reference forecast is `values: [78.15, 84.2]`,
    /// `weights: [0.699, 0.301]` — two ensemble clusters, and the file's own
    /// commentary calls the split the dominant uncertainty. The console showed
    /// `—`.
    #[test]
    fn a_discrete_driver_reproduces_its_declared_weights() {
        let mut d = driver(DriverType::Discrete, None);
        d.values = Some(vec![78.15, 84.2]);
        d.weights = Some(vec![0.699, 0.301]);

        let s = driver_samples(&d).expect("discrete is samplable");
        let hi = s.iter().filter(|v| **v > 80.0).count() as f64 / s.len() as f64;
        assert!(
            (hi - 0.301).abs() < 0.03,
            "upper cluster drawn {hi:.3} of the time, declared 0.301"
        );
        assert!(driver_density(&d, 96).is_some());
    }

    /// A parameterised driver is not drawn, rather than drawn as zeros.
    ///
    /// `triangular(socio_p5, socio_p50, socio_p95)` has its values in
    /// `workspace_params`. `as_number` elsewhere in the console reads a
    /// non-literal as 0.0, which would render every World Cup driver as an
    /// identical spike at the origin and look authoritative doing it.
    #[test]
    fn a_parameterised_driver_declines_to_be_drawn() {
        let d = driver(
            DriverType::Continuous,
            Some(Distribution::Triangular {
                p5: Expression::Identifier("socio_p5".into()),
                p50: Expression::Identifier("socio_p50".into()),
                p95: Expression::Identifier("socio_p95".into()),
            }),
        );
        assert!(driver_samples(&d).is_none());
        assert!(driver_density(&d, 96).is_none());
        assert_eq!(
            driver_summary(&d),
            "<socio_p5> – <socio_p50> – <socio_p95>",
            "the summary names the parameter rather than printing a fabricated 0"
        );
    }

    /// Ticks are derived from the draws, so they land on the shape they label.
    ///
    /// Reading p5/p50/p95 off the AST works only for Triangular — which is the
    /// coupling that kept the card to one distribution type in the first place.
    #[test]
    fn the_percentile_ticks_come_from_the_same_draws_as_the_curve() {
        let d = driver(
            DriverType::Continuous,
            Some(Distribution::Uniform {
                low: num(0.0),
                high: num(10.0),
            }),
        );
        let (_, (p5, p50, p95)) = driver_curve(&d, 96).expect("uniform is samplable");

        assert!(p5 < p50 && p50 < p95, "{p5} {p50} {p95}");
        assert!((p5 - 0.5).abs() < 0.3, "p5 {p5} should be near 0.5");
        assert!((p50 - 5.0).abs() < 0.3, "p50 {p50} should be near 5.0");
        assert!((p95 - 9.5).abs() < 0.3, "p95 {p95} should be near 9.5");
    }

    #[test]
    fn a_binary_driver_has_no_curve_and_says_so_in_text() {
        let mut d = driver(DriverType::Binary, None);
        d.probability = Some(0.5);
        d.impact_multiplier = Some(1.3);
        assert!(driver_samples(&d).is_none());
        assert_eq!(driver_summary(&d), "50% (×1.30)");
    }

    /// The summary said "no distribution" for drivers that had one.
    #[test]
    fn the_summary_describes_each_shape_instead_of_denying_it() {
        let mut d = driver(
            DriverType::Continuous,
            Some(Distribution::Normal {
                mean: num(0.0),
                stddev: num(2.796),
            }),
        );
        d.unit = Some("degF".into());
        let s = driver_summary(&d);
        assert!(s.contains("normal"), "{s}");
        assert!(s.contains("2.796"), "{s}");
        assert!(s.contains("degF"), "{s}");
        assert!(
            !s.contains("no distribution"),
            "the old text denied the driver had a distribution: {s}"
        );
    }

    #[test]
    fn a_discrete_summary_lists_its_values_and_weights() {
        let mut d = driver(DriverType::Discrete, None);
        d.values = Some(vec![78.15, 84.2]);
        d.weights = Some(vec![0.699, 0.301]);
        d.unit = Some("degF".into());
        let s = driver_summary(&d);
        assert!(s.contains("78.15"), "{s}");
        assert!(s.contains("70%"), "{s}");
        assert!(s.contains("84.2"), "{s}");
        assert!(s.contains("30%"), "{s}");
        assert_ne!(s, "—");
    }

    /// The same driver drawn twice is the same picture.
    #[test]
    fn the_curve_does_not_shimmer_between_redraws() {
        let d = driver(
            DriverType::Continuous,
            Some(Distribution::Normal {
                mean: num(0.0),
                stddev: num(1.0),
            }),
        );
        assert_eq!(driver_samples(&d), driver_samples(&d));
    }

    #[test]
    fn a_discrete_driver_with_mismatched_weights_is_not_drawn() {
        let mut d = driver(DriverType::Discrete, None);
        d.values = Some(vec![1.0, 2.0, 3.0]);
        d.weights = Some(vec![0.5, 0.5]);
        assert!(driver_samples(&d).is_none());
    }
}
