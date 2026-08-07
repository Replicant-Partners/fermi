//! ARG scenario: realistic per-team distribution triples.
//!
//! Tests the proposed (p5, p50, p95)-per-driver design with values
//! sourced from the agent reports + public data. Compares against:
//!   - Polymarket crowd price (~11.6%)
//!   - Current production rate (~1-3% — too pessimistic)
//!   - The hand-authored expectation of 6-10% for ARG
//!
//! Contract under test ("Option 2", per the TEAM_PRIOR header):
//! the `model:` expression IS the forecast quantity. The base rate
//! (0.0208 ≈ 1/48) lives inside the model, so the Monte Carlo mean is
//! taken directly — no normalization, no rescaling, no second
//! base_rate multiply.

use fermi::executor::Executor;
use fermi::lexer::Lexer;
use fermi::parser::Parser;

// The real production template, not a frozen copy. A snapshot would
// keep passing after the template it is supposed to be guarding had
// drifted, which is the opposite of what this test is for.
const ARG_SCENARIO: &str =
    include_str!("../forecasts/will_argentina_win_the_2026_fifa_world_cup_.fpl");

/// The uniform prior baked into the template's `model:` line — 1/48,
/// one of 48 teams. Kept here so the neutral-team assertion below
/// states the number it is checking rather than hiding it in a literal.
const BASE_RATE: f64 = 0.0208;

/// The six drivers and ARG's p50 for each, in template order. Single
/// source of truth for both fixed-driver runs below — they used to
/// carry independent copies of these names, so a rename in the FPL
/// could desync them silently.
const DRIVERS: [(&str, f64); 6] = [
    ("socio_capital", 1.43),
    ("institutional_capacity", 1.05),
    ("dynamic_performance", 1.27),
    ("squad_quality", 1.30),
    ("tactical_efficiency", 1.25),
    ("fixture_context", 1.05),
];

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

    // ── Deterministic p50 evaluation ──────────────────────────────────
    //
    // Every driver pinned to its own p50 instead of sampled. This is
    // not a normalization denominator (see the contract note up top) —
    // it's a check that the triangular spreads don't bias the mean away
    // from the central estimate. Params still need binding because the
    // distribution expressions reference them at parse time and the
    // executor still walks the program; it just substitutes fixed
    // values for the driver samples.
    let fixed = DRIVERS
        .iter()
        .map(|(name, p50)| ((*name).to_string(), *p50))
        .collect();
    let mut baseline_exec = Executor::with_fixed_drivers(1, fixed);
    for (name, value) in triples {
        baseline_exec.set_param(*name, *value);
    }
    let baseline_results = baseline_exec.execute(&program).expect("baseline execute");
    let p50_point = baseline_results.mean;

    // ── Neutral team: every driver at 1.0 ─────────────────────────────
    //
    // The Cobb-Douglas exponents sum to ≈ 6 with a 1/6 normalizer, so a
    // team that is average on every factor must fall back to the
    // uniform prior. This is the assertion that would have caught the
    // bug this test itself used to have: if anything ever multiplies by
    // base_rate a second time, `neutral` collapses to 0.0208² and this
    // fails loudly instead of the range check below quietly drifting.
    let neutral_drivers = DRIVERS
        .iter()
        .map(|(name, _)| ((*name).to_string(), 1.0))
        .collect();
    let mut neutral_exec = Executor::with_fixed_drivers(1, neutral_drivers);
    for (name, value) in triples {
        neutral_exec.set_param(*name, *value);
    }
    let neutral = neutral_exec
        .execute(&program)
        .expect("neutral execute")
        .mean;

    // ── Diagnostic output ─────────────────────────────────────────────
    eprintln!();
    eprintln!("─── ARG scenario results ────────────────────────────────");
    eprintln!(
        "  Simulation:       mean={:.4}  p5={:.4}  p50={:.4}  p95={:.4}",
        sim_mean, results.p5, results.median, results.p95
    );
    eprintln!("  p50 point est.:   {:.4}", p50_point);
    eprintln!(
        "  Neutral team:     {:.4}  (uniform prior = {:.4})",
        neutral, BASE_RATE
    );
    eprintln!();
    eprintln!("  Comparison:");
    eprintln!("    Simulated:                  {:.1}%", sim_mean * 100.0);
    eprintln!("    Polymarket crowd:           ~11.6%");
    eprintln!("    Hand-authored expectation:  6-10%");
    eprintln!("─────────────────────────────────────────────────────────");
    eprintln!();

    assert!(
        (BASE_RATE * 0.9..BASE_RATE * 1.1).contains(&neutral),
        "A team average on every driver produced {neutral:.4}, not the \
         uniform prior {BASE_RATE:.4}. Either the Cobb-Douglas exponents \
         no longer sum to ~6, or base_rate is being applied twice."
    );

    assert!(
        (sim_mean - p50_point).abs() < 0.15 * p50_point,
        "Monte Carlo mean {sim_mean:.4} is >15% away from the p50 point \
         estimate {p50_point:.4} — the triangular spreads are skewing the \
         answer rather than expressing uncertainty around it."
    );

    // Sanity: must be in a plausible range for a top-3 team.
    assert!(
        (0.04..0.18).contains(&sim_mean),
        "Forecast {:.2}% outside [4%, 18%] — recalibrate the seed triples",
        sim_mean * 100.0
    );
}
