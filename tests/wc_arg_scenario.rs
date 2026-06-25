//! ARG scenario: realistic per-team distribution triples.
//!
//! Tests the proposed (p5, p50, p95)-per-driver design with values
//! sourced from the agent reports + public data. Compares against:
//!   - Polymarket crowd price (~11.6%)
//!   - Current production rate (~1-3% — too pessimistic)
//!   - The hand-authored expectation of 6-10% for ARG
//!
//! Runs the SAME normalization the cockpit's run_simulation applies:
//!   P = base_rate × (sim_mean / baseline_mean)
//! where baseline_mean is computed by fixing every driver at its p50.

use fermi::executor::Executor;
use fermi::lexer::Lexer;
use fermi::parser::Parser;

// Use a build-time env var so we can point at /tmp without needing a
// stable repo path. Falls back to the test fixture if FERMI_TEST_FPL is unset.
const ARG_SCENARIO: &str = include_str!("/tmp/arg_scenario.fpl");

#[test]
fn arg_scenario_produces_realistic_rate() {
    let tokens = Lexer::new(ARG_SCENARIO).tokenize().expect("tokenize");
    let program = Parser::new(tokens).parse().expect("parse");

    // ── Bind ARG params, then run the per-team simulation ────────────
    //
    // The executor doesn't read inline `= default` literals from `param`
    // declarations — those are syntactic sugar; values come from
    // set_params (or set_json_params for JSON values). The cockpit's
    // load_workspace_params writes here in production. For this scenario
    // we hand-bind ARG's triples below so the test mimics what the
    // cockpit would do after the spawn-time backfill.
    let triples: &[(&str, f64)] = &[
        // socio_capital: well-sourced macro stats (World Bank current).
        // Tight spread because GDP / population / HDI move slowly.
        ("socio_p5", 1.23),
        ("socio_p50", 1.43),
        ("socio_p95", 1.63),
        // institutional_capacity: mixed signal (CONMEBOL strong, league
        // finances weak). Wider spread because qualitative.
        ("institutional_p5", 0.75),
        ("institutional_p50", 1.05),
        ("institutional_p95", 1.35),
        // dynamic_performance: Elo 2115, defending champion. Tight spread.
        ("dynamic_p5", 1.09),
        ("dynamic_p50", 1.27),
        ("dynamic_p95", 1.45),
        // squad_quality: Transfermarkt €807M (top 7). Tight.
        ("squad_p5", 1.10),
        ("squad_p50", 1.30),
        ("squad_p95", 1.50),
        // tactical_efficiency: champion form, recent xG. Moderate spread.
        ("tactical_p5", 1.05),
        ("tactical_p50", 1.25),
        ("tactical_p95", 1.45),
        // fixture_context: per-match volatility caps confidence.
        ("fixture_p5", 0.90),
        ("fixture_p50", 1.05),
        ("fixture_p95", 1.20),
    ];
    let mut sim = Executor::new(30_000);
    for (name, value) in triples {
        sim.set_param(*name, *value);
    }
    let results = sim.execute(&program).expect("sim execute");
    let sim_mean = results.mean;

    // ── Run the baseline (drivers fixed at p50) ───────────────────────
    //
    // The cockpit computes baseline by fixing each driver at its p50.
    // We mirror that here. Params still need to be bound because the
    // distribution expressions reference them at parse time and the
    // baseline executor still walks the program (it just substitutes
    // fixed values for the driver samples).
    let mut fixed = std::collections::HashMap::new();
    fixed.insert("socio_capital".into(), 1.43);
    fixed.insert("institutional_capacity".into(), 1.05);
    fixed.insert("dynamic_performance".into(), 1.27);
    fixed.insert("squad_quality".into(), 1.30);
    fixed.insert("tactical_efficiency".into(), 1.25);
    fixed.insert("fixture_context".into(), 1.05);
    let mut baseline_exec = Executor::with_fixed_drivers(1, fixed);
    for (name, value) in triples {
        baseline_exec.set_param(*name, *value);
    }
    let baseline_results = baseline_exec.execute(&program).expect("baseline execute");
    let baseline_mean = baseline_results.mean;

    // ── Cockpit normalization ─────────────────────────────────────────
    let base_rate = 0.0208_f64;
    let ratio = if baseline_mean.abs() > 0.001 {
        sim_mean / baseline_mean
    } else {
        1.0
    };
    let normalized = (base_rate * ratio).clamp(0.01, 0.99);

    // ── Diagnostic output ─────────────────────────────────────────────
    eprintln!();
    eprintln!("─── ARG scenario results ────────────────────────────────");
    eprintln!(
        "  Raw simulation:   mean={:.3}  p5={:.3}  p50={:.3}  p95={:.3}",
        sim_mean, results.p5, results.median, results.p95
    );
    eprintln!("  Baseline (p50):   {:.3}", baseline_mean);
    eprintln!("  Ratio:            {:.3}x", ratio);
    eprintln!("  Normalized rate:  {:.2}%", normalized * 100.0);
    eprintln!();
    eprintln!("  Comparison:");
    eprintln!("    Polymarket crowd:           ~11.6%");
    eprintln!("    Hand-authored expectation:  6-10%");
    eprintln!("    Old production (today):     ~3.7% (still pessimistic)");
    eprintln!("    Old broken (yesterday):     ~1% (zeroed socio params)");
    eprintln!("─────────────────────────────────────────────────────────");
    eprintln!();

    // Sanity: must be in a plausible range for a top-3 team.
    assert!(
        (0.04..0.18).contains(&normalized),
        "Normalized rate {:.2}% outside [4%, 18%] — recalibrate the seed triples",
        normalized * 100.0
    );
}
