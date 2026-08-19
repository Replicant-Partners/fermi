//! The known-good reference forecast, held to its claims.
//!
//! `examples/reference_bucket_indicator_kord.fpl` is the artefact a regression is
//! measured against: one temperature-bucket question modelled in the correct shape
//! — drivers in degrees, probability by integrating over the bucket, every ratio
//! declaring what it acts on.
//!
//! A reference nobody executes is a document, not a baseline. These tests run it.
//!
//! # Why each assertion is here
//!
//! The live version of this question produced 5.9% from a probability-ratio chain
//! whose drivers disagreed about what they multiplied, while the agent that read
//! the ensemble concluded 35%. Everything below is a property that version lacked.

use fermi::{Executor, Lexer, Parser, SemanticAnalyzer};

const PATH: &str = "examples/reference_bucket_indicator_kord.fpl";

fn source() -> String {
    std::fs::read_to_string(PATH).unwrap_or_else(|e| panic!("read {PATH}: {e}"))
}

fn program() -> fermi::ast::Program {
    let src = source();
    let tokens = Lexer::new(&src)
        .tokenize()
        .unwrap_or_else(|e| panic!("tokenize: {e:?}"));
    Parser::new(tokens)
        .parse()
        .unwrap_or_else(|e| panic!("parse: {e:?}"))
}

/// It parses and satisfies every rule the analyser has, with no warnings either.
///
/// Warnings matter for a reference specifically. The corpus measurement found 78 of
/// 78 stored programs warning, and the whole point of a baseline is that it starts
/// from zero — otherwise "did my change add a warning?" is unanswerable.
#[test]
fn the_reference_is_clean_under_every_rule_that_exists() {
    let analysis = SemanticAnalyzer::new().analyze(&program());

    let errors: Vec<String> = analysis.errors.iter().map(|e| e.to_string()).collect();
    assert!(
        errors.is_empty(),
        "reference has semantic errors: {errors:?}"
    );

    // `Consider adding evidence` is expected and excluded: this file deliberately
    // carries no `evidence` blocks, because its inputs are cited in prose in the
    // header and inventing evidence statements to silence a warning would be the
    // fabrication this whole workstream exists to prevent.
    let real: Vec<&String> = analysis
        .warnings
        .iter()
        .filter(|w| !w.contains("Consider adding evidence"))
        .collect();
    assert!(real.is_empty(), "reference has warnings: {real:?}");
}

/// Every driver declares the space its value acts on.
///
/// The defect the reference exists to contrast with: `unit: "multiplier"` says a
/// driver is a ratio and never says a ratio of what, and two agents filled one slot
/// from different spaces.
#[test]
fn every_driver_declares_what_it_applies_to() {
    let p = program();
    let undeclared: Vec<&str> = p
        .drivers()
        .iter()
        .filter(|d| d.applies_to.is_none())
        .map(|d| d.name.as_str())
        .collect();
    assert!(
        undeclared.is_empty(),
        "a reference forecast must declare every space: {undeclared:?}"
    );
    // All quantities, because that is the point: the probability comes from the
    // bucket integral, not from scaling anything.
    assert!(
        p.drivers()
            .iter()
            .all(|d| d.applies_to == Some(fermi::ast::AppliesTo::Quantity)),
        "the reference composes quantities, not probability ratios"
    );
}

/// The model is an indicator, so its mean is the bucket probability.
///
/// Asserts the shape rather than just the number: a future edit that reverted to
/// `base_rate * ratios` could still produce a plausible probability, and that is
/// precisely the failure this file is the antidote to.
#[test]
fn the_model_is_an_indicator_over_a_quantity_not_a_scaled_base_rate() {
    let p = program();
    let model = p.model().expect("reference has a model");
    let rendered = format!("{:?}", model.expression);
    assert!(
        rendered.contains("GreaterEqual") && rendered.contains("Less"),
        "the model should compare a quantity against bucket bounds: {rendered}"
    );
    assert!(
        !rendered.contains("Multiply"),
        "a bucket probability must not be a product of ratios: {rendered}"
    );
}

/// It runs, and lands where the header says it does.
///
/// The band is deliberately wide — this is a Monte Carlo over a two-component
/// mixture, and pinning it to a decimal would make the test a hostage to the RNG.
/// It is narrow enough to fail on the thing that matters: the ratio-chain version
/// produced 5.9%, half its own base rate, and any regression toward
/// base-rate-echo lands far below this floor.
#[test]
fn the_reference_simulates_to_the_probability_its_header_claims() {
    let p = program();
    let results = Executor::new(10_000)
        .execute(&p)
        .unwrap_or_else(|e| panic!("execute: {e:?}"));

    let pct = results.mean * 100.0;
    println!("  reference bucket probability: {pct:.1}%");
    assert!(
        (15.0..=26.0).contains(&pct),
        "expected ~20% for the weighted mixture, got {pct:.1}%"
    );

    // A probability, not a scaled quantity that happens to be small.
    assert!(
        (0.0..=1.0).contains(&results.mean),
        "an indicator mean must be a probability: {}",
        results.mean
    );

    // And it must be well clear of the base rate in the direction the ensemble
    // points. The live version's defining symptom was landing ON its base rate
    // (5.9% against 6.7%) while an agent asserted a large correction; a reference
    // that did the same would teach nothing.
    let base = p
        .question()
        .and_then(|q| q.base_rate.as_ref())
        .map(|b| b.historical_frequency)
        .expect("reference declares a base rate");
    assert!(
        results.mean > base * 1.5,
        "the ensemble should move this materially off climatology: model {:.3} \
         vs base {base:.3}",
        results.mean
    );
}

/// The bucket is read as an integer SET, not as thresholds.
///
/// `[78, 79]` in the market's labelling is the half-open real interval
/// [77.5, 79.5). Reading integer labels as thresholds is a documented 6x error on
/// these markets, so the reference pins its own bounds.
#[test]
fn the_bucket_bounds_are_the_half_open_interval_the_labels_mean() {
    let src = source();
    assert!(
        src.contains("77.5") && src.contains("79.5"),
        "the reference must use the half-open real interval for the 78-79 bucket"
    );
}
