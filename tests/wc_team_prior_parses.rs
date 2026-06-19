//! Verify the WC team_prior FPL parses, has the expected structure,
//! and produces team-differentiated rates from realistic per-team triples.
//!
//! Pinned design contract (post Option-2 redesign):
//!   - Six factor-derived drivers, each declared as
//!     `triangular(<driver>_p5, <driver>_p50, <driver>_p95)` with the
//!     triple bound from workspace_outputs.params at sim time.
//!   - Three drivers are `learnable: true` with feeds_from for R-1 refit.
//!   - Four curated agents, one per factor family; every driver is
//!     referenced by at least one agent's `driver_refs`.
//!   - The model expression carries the base rate (0.0208) inline and
//!     is the multiplicative product of the six drivers — Option 2 of
//!     the design discussion: model expression IS the forecast quantity,
//!     no normalization downstream.

use fermi::executor::Executor;
use fermi::lexer::Lexer;
use fermi::parser::Parser;

const ARG_TEMPLATE: &str = include_str!("../templates/world_cup/team_prior.fpl");

/// Render the template with a `{team_name}` substitution so tests work
/// against a concrete instantiation.
fn render(team_name: &str) -> String {
    ARG_TEMPLATE.replace("{team_name}", team_name)
}

#[test]
fn team_prior_template_parses() {
    let src = render("Argentina");
    let tokens = Lexer::new(&src)
        .tokenize()
        .expect("template should tokenize");
    let program = Parser::new(tokens).parse().expect("template should parse");

    // Exactly the six factor-derived drivers, no BayesOps-stream impostors.
    let driver_names: Vec<_> = program.drivers().iter().map(|d| d.name.as_str()).collect();
    let expected_drivers = [
        "socio_capital",
        "institutional_capacity",
        "dynamic_performance",
        "squad_quality",
        "tactical_efficiency",
        "fixture_context",
    ];
    for d in &expected_drivers {
        assert!(
            driver_names.contains(d),
            "expected driver {} in {:?}",
            d,
            driver_names
        );
    }
    // No leftover BayesOps-stream pseudo-drivers from earlier templates.
    for bad in &["won_rate", "form_signal"] {
        assert!(
            !driver_names.contains(bad),
            "driver {} was kept by mistake; it's an observation stream, not a driver",
            bad
        );
    }

    // Exactly three learnable drivers — dynamic_performance, squad_quality,
    // tactical_efficiency. Each must have a feeds_from extractor for R-1.
    let drivers_vec = program.drivers();
    let learnable_drivers: Vec<_> = drivers_vec.iter().filter(|d| d.learnable).collect();
    assert_eq!(
        learnable_drivers.len(),
        3,
        "expected exactly 3 learnable drivers (got names: {:?})",
        learnable_drivers
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>()
    );
    for d in &learnable_drivers {
        assert!(
            d.feeds_from.is_some(),
            "learnable driver {} must declare feeds_from for the R-1 refit hook",
            d.name
        );
    }

    // Four curated agents, each referencing the drivers it researches.
    let agent_names: Vec<_> = program.agents().iter().map(|a| a.name.as_str()).collect();
    for expected in &[
        "macro_data_agent",
        "football_institution_agent",
        "football_analyst",
        "fixture_context_agent",
    ] {
        assert!(
            agent_names.contains(expected),
            "expected agent {} in {:?}",
            expected,
            agent_names
        );
    }

    // Every driver should be referenced by at least one agent.
    for d in program.drivers() {
        let referenced = program
            .agents()
            .iter()
            .any(|a| a.driver_refs.iter().any(|r| r == &d.name));
        assert!(
            referenced,
            "driver {} has no agent referencing it via driver_refs",
            d.name
        );
    }

    // Outside view: the question must carry a base_rate.
    let q = program
        .question()
        .expect("template must declare a question");
    let br = q
        .base_rate
        .as_ref()
        .expect("question must declare a base_rate for the outside view");
    assert!(
        (br.historical_frequency - 0.0208).abs() < 0.001,
        "expected base_rate.historical_frequency ≈ 0.0208 (1/48), got {}",
        br.historical_frequency
    );

    // The model expression must reference all six drivers and contain the
    // 0.0208 base rate scalar (Option 2: model owns the rate, no
    // post-processing).
    let model = program
        .model()
        .expect("template must have a `model:` expression");
    let model_text = format!("{:?}", model);
    for d in &expected_drivers {
        assert!(
            model_text.contains(d),
            "model expression must reference driver `{}` (model dump: {})",
            d,
            model_text
        );
    }
    assert!(
        model_text.contains("0.0208"),
        "model expression must contain the 0.0208 base rate scalar; \
         the cockpit no longer multiplies it in post-hoc (model dump: {})",
        model_text
    );
}

/// Bind a per-team triple set into the executor and return the Monte
/// Carlo mean. Mirrors what the cockpit's load_workspace_params does at
/// runtime — the params come from workspace_outputs.params, which the
/// backfill script populates.
fn simulate_team_rate(
    team_name: &str,
    socio: (f64, f64, f64),
    institutional: (f64, f64, f64),
    dynamic: (f64, f64, f64),
    squad: (f64, f64, f64),
    tactical: (f64, f64, f64),
    fixture: (f64, f64, f64),
) -> f64 {
    let src = render(team_name);
    let tokens = Lexer::new(&src).tokenize().expect("tokenize");
    let program = Parser::new(tokens).parse().expect("parse");

    let mut exec = Executor::new(10_000);
    // Driver-distribution triples — the design's whole point: every
    // driver's prior comes from the per-team backfill, not a hardcoded
    // template literal.
    exec.set_param("socio_p5", socio.0);
    exec.set_param("socio_p50", socio.1);
    exec.set_param("socio_p95", socio.2);
    exec.set_param("institutional_p5", institutional.0);
    exec.set_param("institutional_p50", institutional.1);
    exec.set_param("institutional_p95", institutional.2);
    exec.set_param("dynamic_p5", dynamic.0);
    exec.set_param("dynamic_p50", dynamic.1);
    exec.set_param("dynamic_p95", dynamic.2);
    exec.set_param("squad_p5", squad.0);
    exec.set_param("squad_p50", squad.1);
    exec.set_param("squad_p95", squad.2);
    exec.set_param("tactical_p5", tactical.0);
    exec.set_param("tactical_p50", tactical.1);
    exec.set_param("tactical_p95", tactical.2);
    exec.set_param("fixture_p5", fixture.0);
    exec.set_param("fixture_p50", fixture.1);
    exec.set_param("fixture_p95", fixture.2);
    // The other params (elo_current, gdp_per_capita_log, etc.) aren't
    // referenced by any driver distribution any more — they're kept in
    // the FPL only as metadata. We don't need to bind them for the sim
    // to succeed.

    let results = exec.execute(&program).expect("execute");
    results.mean
}

/// Argentina (top-tier, defending champion) and Panama (CONCACAF mid-tier)
/// must produce materially different rates. With Option 2 the rate IS
/// the simulation mean, so this test asserts directly on the rate range
/// instead of the raw product.
#[test]
fn team_prior_simulates_team_differentiated_rates() {
    // ARG: triples derived from the agent reports + public data
    // (Transfermarkt, Elo, World Bank) in the design discussion.
    let arg = simulate_team_rate(
        "Argentina",
        (1.23, 1.43, 1.63),  // socio (high HDI, mid GDP)
        (0.75, 1.05, 1.35),  // institutional (CONMEBOL strong, league weak)
        (1.09, 1.27, 1.45),  // dynamic (Elo 2115, defending champion)
        (1.10, 1.30, 1.50),  // squad (Transfermarkt €807M, top-7)
        (1.05, 1.25, 1.45),  // tactical (recent xG +1.2, trophy form)
        (0.90, 1.05, 1.20),  // fixture (favourable Group J)
    );
    // PAN: low across the board, wider spreads (less data).
    let pan = simulate_team_rate(
        "Panama",
        (0.80, 1.00, 1.20),  // socio
        (0.50, 0.80, 1.10),  // institutional (small federation)
        (0.55, 0.75, 0.95),  // dynamic (Elo ~1730)
        (0.45, 0.65, 0.85),  // squad (limited Big-5 presence)
        (0.55, 0.75, 0.95),  // tactical
        (0.85, 1.00, 1.15),  // fixture
    );

    eprintln!();
    eprintln!("─── team_prior calibration ────────────────");
    eprintln!("  Argentina rate: {:.2}%", arg * 100.0);
    eprintln!("  Panama rate:    {:.2}%", pan * 100.0);
    eprintln!("───────────────────────────────────────────");
    eprintln!();

    // ARG should land in a realistic top-team range (3–15%) — bracketing
    // the Polymarket 11.6% from below since the model is conservative
    // without elasticities.
    assert!(
        (0.03..0.15).contains(&arg),
        "Argentina rate {:.4} outside [3%, 15%] — recalibrate seed",
        arg
    );

    // PAN should land in the lower mid-tier (1–4%).
    assert!(
        (0.005..0.04).contains(&pan),
        "Panama rate {:.4} outside [0.5%, 4%]",
        pan
    );

    // Strong > weak: ARG should be at least 2× PAN.
    assert!(
        arg > pan * 2.0,
        "Argentina rate ({:.4}) should be >2x Panama ({:.4})",
        arg, pan
    );
}
