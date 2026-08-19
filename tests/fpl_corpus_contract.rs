//! FPL corpus contract — does the stored corpus satisfy the rules that already exist?
//!
//! # Why this exists
//!
//! `SemanticAnalyzer` implements real rules — driver/model presence, distribution
//! wellformedness, unused drivers, type inference on the model expression — and
//! **the two paths that keep the corpus alive never call it**:
//!
//! | path | pipeline |
//! | --- | --- |
//! | console re-simulate (`cockpit.rs:8185`) | lex → parse → execute |
//! | BayesOps refit (`refit.rs`) | lex → parse |
//! | `fermi_execute_fpl` (agents) | lex → parse → **analyse** |
//!
//! So a forecast an analyst authored and simulated has never been analysed, while
//! the same source run by an agent would be. Two answers to the question "is this
//! program valid", and the one the humans use is the permissive one.
//!
//! That was discovered while planning a *new* semantic rule — a dimensional check
//! to stop a temperature ratio being multiplied into a probability. Adding it to
//! `semantic.rs` alone would have produced a correct rule that never runs on the
//! path that needs it, which is the same defect as `type_env` (constructed, never
//! read) and `DriverStmt.constraints` (parsed, never enforced).
//!
//! # What it measures
//!
//! Runs the existing analyser over every `fpl_source` in `fermi_forecasts` and
//! reports what the corpus would say if anyone asked it. This is a MEASUREMENT,
//! not a gate: it prints and asserts only that the measurement itself ran. The
//! numbers are the input to a keep-or-rebuild decision about legacy forecasts, and
//! guessing at them is exactly what this repository keeps getting wrong.
//!
//! # Running it
//!
//! ```sh
//! set -a; . ./.env; set +a
//! cargo test --test fpl_corpus_contract -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;

use fermi::{Lexer, Parser, SemanticAnalyzer};
use sqlx::Row;

/// Offline: the analyser must actually be reachable from a test.
///
/// Trivial, and it exists because the whole point of this file is that a rule is
/// worthless if nothing calls it. A compile-time reference here means the import
/// cannot rot silently.
#[test]
fn the_analyser_is_callable_and_reports_a_known_defect() {
    // A program with a driver that is never used in the model: an existing rule,
    // and one an analyst has never been shown.
    let src = r#"
question "Will it rain?" {
    base_rate {
        reference_class: "days"
        historical_frequency: 0.3
        sample_size: 100
        source: "fixture"
        generated_by: macro_forecaster
    }
}

driver used continuous {
    distribution: triangular(0.9, 1.0, 1.1)
    unit: "multiplier"
}

driver ignored continuous {
    distribution: triangular(0.9, 1.0, 1.1)
    unit: "multiplier"
}

model: 0.3 * used

simulate 1000 iterations
"#;
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let program = Parser::new(tokens).parse().expect("parse");
    let analysis = SemanticAnalyzer::new().analyze(&program);

    assert!(
        analysis.warnings.iter().any(|w| w.contains("ignored")),
        "the unused-driver rule should fire; warnings were {:?}",
        analysis.warnings
    );
    // The rule is a WARNING, so a program carrying it is still "valid" — which is
    // why an unused driver has never stopped anything.
    assert!(
        analysis.is_valid(),
        "an unused driver is a warning, not an error: {:?}",
        analysis.errors
    );
}

/// A declared `param` is a defined symbol.
///
/// The regression for the finding this file was written to measure. Before the
/// fix, `symbol_table.rs` registered `Driver`, `Evidence`, `Agent` and `Model` and
/// not `Param`, so every reference to a param resolved to `UndefinedSymbol` — **48
/// of 78 stored programs**, at twelve params each. The programs were correct.
///
/// The fixture mirrors the real shape rather than a minimal one: the World Cup
/// family declares `param socio_p50: real` and then consumes it inside a
/// distribution, `triangular(socio_p5, socio_p50, socio_p95)`, which is the case
/// that broke. A param used in a bare arithmetic expression would have passed for
/// the wrong reason.
#[test]
fn a_declared_param_is_not_an_undefined_symbol() {
    let src = r#"
question "Will Spain win?" {
    base_rate {
        reference_class: "48-team field"
        historical_frequency: 2.08%
        sample_size: 48
        source: "fixture"
        generated_by: macro_forecaster
    }
}

param socio_p5: real
param socio_p50: real
param socio_p95: real

driver socio_capital continuous {
    distribution: triangular(socio_p5, socio_p50, socio_p95)
    unit: "multiplier"
}

model: 0.0208 * socio_capital

simulate 1000 iterations
"#;
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let program = Parser::new(tokens).parse().expect("parse");
    let analysis = SemanticAnalyzer::new().analyze(&program);

    let undefined: Vec<String> = analysis
        .errors
        .iter()
        .map(|e| e.to_string())
        .filter(|s| s.contains("Undefined symbol"))
        .collect();
    assert!(
        undefined.is_empty(),
        "a declared param must resolve; got {undefined:?}"
    );
    assert!(
        analysis.is_valid(),
        "program should be valid; errors were {:?}",
        analysis
            .errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
    );

    // ...and an UNDECLARED identifier must still fail, or the fix has simply
    // switched the check off.
    let bad = src.replace("param socio_p95: real", "");
    let tokens = Lexer::new(&bad).tokenize().expect("tokenize");
    let program = Parser::new(tokens).parse().expect("parse");
    let analysis = SemanticAnalyzer::new().analyze(&program);
    assert!(
        analysis
            .errors
            .iter()
            .any(|e| e.to_string().contains("socio_p95")),
        "removing a param declaration must still be an error, else nothing is \
         being checked: {:?}",
        analysis
            .errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
    );
}

/// Live: what does the existing analyser say about the stored corpus?
#[tokio::test]
#[ignore = "needs DATABASE_URL; measures the stored FPL corpus"]
async fn the_stored_corpus_is_measured_against_the_rules_that_exist() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect");

    let rows = sqlx::query(
        "SELECT id::text AS id, left(question_text, 60) AS q, fpl_source \
           FROM fermi_forecasts \
          WHERE fpl_source IS NOT NULL AND fpl_source <> '' \
          ORDER BY updated_at DESC",
    )
    .fetch_all(&pool)
    .await
    .expect("read corpus");

    assert!(
        !rows.is_empty(),
        "no FPL in the corpus — this test would pass by measuring nothing"
    );

    let mut tokenize_failed = 0usize;
    let mut parse_failed = 0usize;
    let mut analysed = 0usize;
    let mut with_errors = 0usize;
    let mut with_warnings = 0usize;
    let mut error_kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut warning_kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: Vec<String> = Vec::new();

    for r in &rows {
        let q: String = r.get("q");
        let src: String = r.get("fpl_source");

        let tokens = match Lexer::new(&src).tokenize() {
            Ok(t) => t,
            Err(_) => {
                tokenize_failed += 1;
                continue;
            }
        };
        let program = match Parser::new(tokens).parse() {
            Ok(p) => p,
            Err(_) => {
                parse_failed += 1;
                continue;
            }
        };

        let analysis = SemanticAnalyzer::new().analyze(&program);
        analysed += 1;

        if !analysis.errors.is_empty() {
            with_errors += 1;
            if examples.len() < 6 {
                examples.push(format!("{q} — {}", analysis.errors[0]));
            }
        }
        if !analysis.warnings.is_empty() {
            with_warnings += 1;
        }

        // Bucket by rule rather than by message, so counts are readable. The
        // Display impl embeds names and values, which would make every row unique.
        for e in &analysis.errors {
            let s = e.to_string();
            let key = s.split(':').next().unwrap_or("?").trim().to_string();
            *error_kinds.entry(key).or_default() += 1;
        }
        for w in &analysis.warnings {
            let key = if w.contains("not used in the model") {
                "driver defined but not used in the model".to_string()
            } else {
                w.split(':').next().unwrap_or("?").trim().to_string()
            };
            *warning_kinds.entry(key).or_default() += 1;
        }
    }

    println!("\n  FPL corpus: {} stored program(s)", rows.len());
    println!("    {tokenize_failed} failed to tokenize");
    println!("    {parse_failed} failed to parse");
    println!("    {analysed} analysed");
    println!("      {with_errors} with semantic ERRORS");
    println!("      {with_warnings} with WARNINGS");

    if !error_kinds.is_empty() {
        println!("\n  errors by rule:");
        for (k, n) in &error_kinds {
            println!("    {n:4}  {k}");
        }
    }
    if !warning_kinds.is_empty() {
        println!("\n  warnings by rule:");
        for (k, n) in &warning_kinds {
            println!("    {n:4}  {k}");
        }
    }
    if !examples.is_empty() {
        println!("\n  first few programs with errors:");
        for e in &examples {
            println!("    · {e}");
        }
    }

    // ── What is asserted, and what deliberately is not ──────────────────
    //
    // Asserted: the measurement ran over real programs. If tokenize or parse
    // failed for EVERYTHING then the numbers above are about the harness, not the
    // corpus, and a reader would take "0 errors" as good news.
    //
    // Not asserted: that the corpus is clean. It is not this test's job to decide
    // whether legacy forecasts should be repaired or discarded — that is a
    // judgement about a product, and the numbers above are its input. Failing here
    // would only teach people to pass `--skip`.
    assert!(
        analysed > 0,
        "every program failed before analysis ({tokenize_failed} tokenize, \
         {parse_failed} parse) — the counts above describe the harness, not the corpus"
    );
    println!(
        "\n  Measurement only: this test does not fail on a dirty corpus. The \
         counts are the input to a keep-or-rebuild decision.\n"
    );
}
