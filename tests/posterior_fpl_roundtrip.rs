//! BayesOps Phase 1 acceptance criterion: `FittedDistribution::to_fpl_params()`
//! emits a string that is directly parseable as a `distribution:` clause inside
//! an FPL `driver` declaration.
//!
//! This test lives in `tests/` rather than `crates/posterior/tests/` because
//! Spec 14 §9 forbids `posterior` from depending on `fermi`. The round-trip is
//! validated here by having `fermi` (which already exposes its Lexer + Parser)
//! consume `posterior`'s output.

use fermi::{Distribution, Lexer, Parser, Statement};
use posterior::FittedDistribution;

/// Construct a minimal FPL program with a single driver whose distribution
/// clause is sourced from `fitted.to_fpl_params()`, then lex + parse it and
/// return the parsed `Distribution`.
fn parse_driver_with_distribution(fitted: &FittedDistribution) -> Distribution {
    let source = format!(
        "question \"smoke test\"\n\
         driver fitted_driver continuous {{\n\
         \x20\x20\x20\x20distribution: {}\n\
         }}\n\
         model: fitted_driver\n",
        fitted.to_fpl_params()
    );

    let tokens = Lexer::new(&source).tokenize().unwrap_or_else(|errs| {
        panic!(
            "Lexer failed on {:?}: {:?}\nsource:\n{}",
            fitted, errs, source
        )
    });

    let program = Parser::new(tokens)
        .parse()
        .unwrap_or_else(|e| panic!("Parser failed on {:?}: {:?}\nsource:\n{}", fitted, e, source));

    // Find the driver statement and return its distribution.
    for stmt in program.statements {
        if let Statement::Driver(d) = stmt {
            if d.name == "fitted_driver" {
                return d
                    .distribution
                    .expect("driver had no distribution clause");
            }
        }
    }
    panic!("no fitted_driver found in parsed program");
}

#[test]
fn beta_round_trips_through_fpl_parser() {
    let fd = FittedDistribution::Beta {
        alpha: 9.4,
        beta: 13.6,
        ci_low: 0.2,
        ci_high: 0.65,
        n_eff: 23.0,
    };
    let parsed = parse_driver_with_distribution(&fd);
    assert!(
        matches!(parsed, Distribution::Beta { .. }),
        "expected Distribution::Beta, got {:?}",
        parsed
    );
}

#[test]
fn normal_round_trips_through_fpl_parser() {
    let fd = FittedDistribution::Normal {
        mean: 4.8,
        std_dev: 0.7,
        ci_low: 3.7,
        ci_high: 5.9,
        n_eff: 12.0,
    };
    let parsed = parse_driver_with_distribution(&fd);
    assert!(
        matches!(parsed, Distribution::Normal { .. }),
        "expected Distribution::Normal, got {:?}",
        parsed
    );
}

#[test]
fn lognormal_round_trips_through_fpl_parser() {
    let fd = FittedDistribution::Lognormal {
        median: 100.0,
        sigma: 0.3,
        ci_low: 60.0,
        ci_high: 160.0,
        n_eff: 14.0,
    };
    let parsed = parse_driver_with_distribution(&fd);
    assert!(
        matches!(parsed, Distribution::Lognormal { .. }),
        "expected Distribution::Lognormal, got {:?}",
        parsed
    );
}

#[test]
fn triangular_round_trips_through_fpl_parser() {
    let fd = FittedDistribution::Triangular {
        p5: 3.1,
        p50: 4.8,
        p95: 6.9,
        n: 11,
    };
    let parsed = parse_driver_with_distribution(&fd);
    assert!(
        matches!(parsed, Distribution::Triangular { .. }),
        "expected Distribution::Triangular, got {:?}",
        parsed
    );
}

/// End-to-end: fit a Beta from synthetic data → emit FPL → round-trip parse.
/// Validates the entire fit_marginal → to_fpl_params → Lexer → Parser chain.
#[test]
fn fit_marginal_output_round_trips() {
    use posterior::{fit_marginal, DistFamily};
    // 30 synthetic observations from a Beta-like distribution in (0, 1)
    let obs: Vec<f64> = (1..=30).map(|i| i as f64 / 31.0).collect();
    let (fitted, meta) =
        fit_marginal(&obs, None, DistFamily::Beta).expect("fit_marginal should succeed");
    assert_eq!(meta.n_observations, 30);
    let parsed = parse_driver_with_distribution(&fitted);
    assert!(matches!(parsed, Distribution::Beta { .. }));
}
