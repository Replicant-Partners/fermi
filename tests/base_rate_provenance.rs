//! Who produced the base rate survives, and is shown.
//!
//! # The defect
//!
//! `BaseRate.generated_by` is required by the parser, emitted by the console's
//! FPL writer, and persisted into `forecasts.metadata`. Its only two reads in
//! the entire codebase were those two serialisation sites. No render function,
//! no report, no validation rule.
//!
//! That is what made a *false* provenance claim undetectable rather than merely
//! unhelpful, and there were two of them:
//!
//! * `apply_base_rate_only` wrote `source: "fermi"` and
//!   `generated_by: Agent("fermi")` as string literals, even though
//!   `update_outside_rate` had just routed the work to whichever agent declares
//!   the question's domain. A base rate measured by `weather_oracle` from 525
//!   station-days of ERA5 was recorded as the generalist's.
//! * the local `state.json` restore set `generated_by: Agent("fermi")`
//!   unconditionally, so every open/close cycle rewrote the provenance of every
//!   base rate. `examples/reference_bucket_indicator_kord.fpl` declares
//!   `generated_by: weather_oracle` and did not survive one reload.
//!
//! The operator requirement is "I should know why the base rate is the base
//! rate". Reference class, sample size and reasoning say what was counted. This
//! field says who is answerable for it, and it was the one nobody could see.
//!
//! # What is covered here, and what is not
//!
//! The parse and the report are in the `fermi` lib and are tested directly. The
//! two write sites and the console panel are in `cockpit.rs`, which has no test
//! coverage at all because rustc segfaults expanding GPUI element chains under
//! `--test`. Those are verified by review, and recorded here as a known gap
//! rather than implied to be covered.

use fermi::ast::{GeneratedBy, Statement};
use fermi::{Lexer, Parser};

fn parse(src: &str) -> fermi::ast::Program {
    let tokens = Lexer::new(src)
        .tokenize()
        .unwrap_or_else(|e| panic!("tokenize:\n{src}\n\n{e:?}"));
    Parser::new(tokens)
        .parse()
        .unwrap_or_else(|e| panic!("parse:\n{src}\n\n{e:?}"))
}

fn base_rate_of(p: &fermi::ast::Program) -> fermi::ast::BaseRate {
    p.statements
        .iter()
        .find_map(|s| match s {
            Statement::Question(q) => q.base_rate.clone(),
            _ => None,
        })
        .expect("question carries a base rate")
}

const WEATHER: &str = r#"question "Will KORD hit 78-79F on 2026-08-20?" {
    base_rate {
        reference_class: "ERA5 KORD daily maximum, Aug 13-27, 1990-2024"
        historical_frequency: 11.1%
        sample_size: 525
        source: "ERA5 reanalysis via weather_oracle"
        generated_by: weather_oracle
    }
}

driver d continuous {
    distribution: triangular(0.8, 1.0, 1.2)
    applies_to: probability
}

model: d
"#;

/// The specialist that measured it is the one recorded.
#[test]
fn a_specialists_attribution_parses_as_that_specialist() {
    let br = base_rate_of(&parse(WEATHER));

    assert_eq!(
        br.generated_by,
        GeneratedBy::Agent("weather_oracle".into()),
        "the reference forecast names the agent that counted the observations"
    );
    assert_ne!(
        br.generated_by,
        GeneratedBy::Agent("fermi".into()),
        "'fermi' is what two write paths substituted regardless of routing"
    );
}

/// A human-authored base rate is distinguishable from an agent-authored one.
///
/// The distinction is the whole point of the field: it separates a number
/// somebody stands behind from one a model asserted.
#[test]
fn a_human_authored_base_rate_is_not_attributed_to_an_agent() {
    let src = WEATHER.replace("generated_by: weather_oracle", "generated_by: human");
    assert_eq!(base_rate_of(&parse(&src)).generated_by, GeneratedBy::Human);
}

/// The field is required, so no path can produce a base rate without one.
///
/// This is why the console's fallback on restore can prefer the value already
/// parsed from the FPL rather than a literal: there is always one to prefer.
#[test]
fn a_base_rate_without_an_attribution_does_not_parse() {
    let src = WEATHER
        .lines()
        .filter(|l| !l.contains("generated_by"))
        .collect::<Vec<_>>()
        .join("\n");

    let tokens = Lexer::new(&src).tokenize().expect("tokenize");
    let err = Parser::new(tokens)
        .parse()
        .expect_err("a base rate with no stated producer must not parse");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("generated_by"),
        "the error should name the missing field, got: {msg}"
    );
}

/// The markdown report states who produced the base rate.
///
/// `extract_base_rate_info` returned a 4-tuple that structurally could not carry
/// `generated_by`, so the artefact read outside the console stated a frequency,
/// a reference class and a sample size while never saying whose they were.
#[test]
fn the_markdown_report_names_the_producer() {
    use fermi::{Executor, SemanticAnalyzer};

    let program = parse(WEATHER);
    assert!(
        SemanticAnalyzer::new().analyze(&program).errors.is_empty(),
        "fixture must be a valid program"
    );

    let results = Executor::new(200)
        .execute(&program)
        .expect("fixture simulates");
    let sensitivity = fermi::sensitivity::full_sensitivity_analysis(&program, 200)
        .expect("sensitivity runs");
    let dir = std::env::temp_dir().join("fermi_provenance_report_test");
    std::fs::create_dir_all(&dir).expect("temp dir");

    let md = fermi::report::markdown::generate(
        &program,
        &results,
        &sensitivity,
        &chrono::Utc::now(),
        &dir,
    )
    .expect("report generates");

    assert!(
        md.contains("weather_oracle"),
        "the report never names the agent that produced the base rate:\n{}",
        &md[..md.len().min(1200)]
    );
    assert!(
        md.contains("**Generated by:**"),
        "the producer needs its own labelled line, not a mention buried in a \
         reference class string"
    );
}
