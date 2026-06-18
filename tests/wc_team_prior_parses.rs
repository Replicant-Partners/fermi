//! Verify the live team-prior FPL parses cleanly and the simulator
//! recognizes all the variables it references.
//!
//! Cross-checks that
//!   (a) the lexer + parser accept the rendered template, and
//!   (b) the program's variable namespace covers everything the
//!       `estimate tournament_strength as: ...` expression and any
//!       `factor X_n` formulations reference.
//!
//! Catches the regression where executor reports `Undefined variable: foo`
//! because a factor formulation references `goal_difference` but no
//! `param goal_difference` exists.

use fermi::executor::Executor;
use fermi::lexer::Lexer;
use fermi::parser::Parser;

const ARG_TEMPLATE: &str = include_str!("../templates/world_cup/team_prior.fpl");

/// Render the template with a `{team_name}` substitution so tests work
/// against a concrete instantiation.
fn render(team_name: &str) -> String {
    ARG_TEMPLATE.replace("{team_name}", team_name)
}

/// Locks the team-prior FPL template's shape against the spec
/// (docs/specs/23_BAYESOPS_WORLD_CUP_DEMO.md + BAYESOPS_CONTRACT.md):
///
///   - Six factor-derived drivers, each with its own sparkline-able
///     distribution. Four are learnable (refit by R-1), two are static
///     inputs that the agents refresh on a schedule.
///   - Four curated research agents, one per factor family, each owning
///     the drivers it researches via `driver_refs`.
///   - Every learnable driver carries a `feeds_from` extractor declaration
///     so the refit hook knows where to read observations from.
///   - A `model:` expression — simple multiplicative product over the
///     six drivers, scaled by the per-team base rate. (The Cobb-Douglas
///     elasticities the older template carried via `learnable()` are
///     deferred until the cockpit AST supports them.)
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
    // No leftover BayesOps-stream pseudo-drivers from the v0.7.0 template.
    for bad in &["won_rate", "form_signal"] {
        assert!(
            !driver_names.contains(bad),
            "driver {} was kept by mistake; it's an observation stream, not a driver",
            bad
        );
    }

    // Three learnable drivers per spec — dynamic_performance + squad_quality
    // + tactical_efficiency. Each must have a feeds_from extractor for R-1.
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

    // Every driver should be referenced by at least one agent, so the
    // cockpit's "⚠ No agents" warning never fires on a freshly-instantiated
    // workspace.
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

    // Outside view: the question must carry a base_rate. Without it the
    // cockpit's outside-view pane has nothing to render and the inside-view
    // divergence indicator can't compute. Per spec §6 step 1.
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

    // The model expression must exist (so simulate has something to evaluate)
    // and must reference all six drivers — otherwise simulation will report
    // "Undefined variable: foo" or silently drop a factor.
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
}

/// Bind the per-team params that the FPL declares and return the Monte
/// Carlo mean rate. The values are representative of the dataset (Elo /
/// GDP / HDI ranges seen in the WC fleet).
fn simulate_team_rate(
    team_name: &str,
    elo_current: f64,
    elo_trend: f64,
    gdp_per_capita_log: f64,
    population_log: f64,
    hdi_logit: f64,
) -> f64 {
    let src = render(team_name);
    let tokens = Lexer::new(&src).tokenize().expect("tokenize");
    let program = Parser::new(tokens).parse().expect("parse");

    let mut exec = Executor::new(5_000);
    exec.set_param("elo_current", elo_current);
    exec.set_param("elo_trend", elo_trend);
    exec.set_param("gdp_per_capita_log", gdp_per_capita_log);
    exec.set_param("population_log", population_log);
    exec.set_param("hdi_logit", hdi_logit);
    // String params left as defaults — the model expression doesn't touch
    // them. is_host coerces to 0.0/1.0 via the executor; default = 0.0.

    let results = exec.execute(&program).expect("execute");
    results.mean
}

/// Argentina (strong, CONMEBOL) and Panama (CONCACAF qualifier) should
/// produce materially different rates from the same template. If they
/// don't, the per-team params aren't reaching the driver distributions and
/// the demo's "Brazil > Panama" property fails by construction.
#[test]
fn team_prior_simulates_team_differentiated_rates() {
    // Argentina-class: Elo 1850, modest positive trend, mid GDP, mid pop, high HDI.
    let arg_rate = simulate_team_rate("Argentina", 1850.0, 0.10, 9.5, 17.4, 2.10);
    // Panama-class: Elo 1500, modest trend, lower GDP, smaller pop, mid HDI.
    let pan_rate = simulate_team_rate("Panama", 1500.0, -0.05, 9.0, 15.1, 1.40);

    // Sanity: both rates are in [0, 1]. We use a loose bound — the model is
    // calibrated to land in the single-digit percentage range, not anything
    // wild.
    assert!(
        arg_rate.is_finite() && arg_rate > 0.0 && arg_rate < 1.0,
        "Argentina rate {} out of (0, 1)",
        arg_rate
    );
    assert!(
        pan_rate.is_finite() && pan_rate > 0.0 && pan_rate < 1.0,
        "Panama rate {} out of (0, 1)",
        pan_rate
    );

    // Strong teams MUST produce higher rates than weak teams. If this
    // fires, the per-team params aren't reaching the driver distributions
    // and every team will show the same rate in the cockpit.
    assert!(
        arg_rate > pan_rate * 1.5,
        "expected Argentina rate ({:.4}) to be at least 1.5x Panama rate ({:.4}); the per-team params aren't flowing into the driver distributions",
        arg_rate,
        pan_rate
    );

    // Print rates so eyeballing the calibration during dev is easy.
    eprintln!(
        "team_prior calibration: Argentina={:.2}%, Panama={:.2}%",
        arg_rate * 100.0,
        pan_rate * 100.0
    );
}
