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
/// produce materially different raw model outputs. The cockpit applies a
/// base-rate normalization on top (P = base_rate × sim_mean / baseline_mean)
/// so we test the relative magnitudes here rather than the post-norm rate
/// — that's what makes the template a useful per-team prior.
#[test]
fn team_prior_simulates_team_differentiated_rates() {
    // Argentina-class: Elo 1850, modest positive trend, mid GDP, mid pop, high HDI.
    let arg_raw = simulate_team_rate("Argentina", 1850.0, 0.10, 9.5, 17.4, 2.10);
    // Panama-class: Elo 1500, modest trend, lower GDP, smaller pop, mid HDI.
    let pan_raw = simulate_team_rate("Panama", 1500.0, -0.05, 9.0, 15.1, 1.40);

    // Sanity: both are finite positive (the multiplicative model can't
    // produce zero or negative values given Triangular/Normal priors with
    // positive support, but a misconfigured prior would).
    assert!(
        arg_raw.is_finite() && arg_raw > 0.0,
        "Argentina raw output {} not finite-positive",
        arg_raw
    );
    assert!(
        pan_raw.is_finite() && pan_raw > 0.0,
        "Panama raw output {} not finite-positive",
        pan_raw
    );

    // Strong teams MUST produce higher raw outputs than weak teams. If this
    // fires, the per-team params aren't reaching the driver distributions
    // and the cockpit's normalization will produce the same rate for every
    // team.
    assert!(
        arg_raw > pan_raw * 1.3,
        "expected Argentina raw ({:.4}) to be at least 1.3x Panama raw ({:.4}); the per-team params aren't flowing into the driver distributions",
        arg_raw,
        pan_raw
    );

    // Simulate the cockpit's normalization explicitly so the calibration
    // print reflects what the user will actually see. Baseline = run with
    // all drivers fixed at their p50. For the Triangular drivers in this
    // template the p50 is the second argument; for Normal it's the mean
    // expression. Both evaluate to known numbers given the bound params.
    //
    // Reuse simulate_team_rate to get the per-team mean; baseline is
    // computed by running the same template with a "neutral" param set
    // (Elo 1700, mid socio inputs that sum to 7.8 — the offset we centered
    // around in the socio_capital distribution). The resulting product is
    // the natural baseline.
    let baseline_raw = simulate_team_rate("Baseline", 1700.0, 0.0, 2.6, 2.6, 2.6);
    let base_rate = 0.0208_f64;
    let arg_final = (base_rate * arg_raw / baseline_raw).clamp(0.01, 0.99);
    let pan_final = (base_rate * pan_raw / baseline_raw).clamp(0.01, 0.99);

    eprintln!(
        "team_prior calibration: \n  raw arg={:.3}, pan={:.3}, baseline={:.3}\n  normalized arg={:.2}%, pan={:.2}%",
        arg_raw, pan_raw, baseline_raw,
        arg_final * 100.0, pan_final * 100.0
    );

    // Normalized rates land in plausible bookmaker territory (sub-30%).
    // Pin the upper bound loosely — the calibration constants may evolve.
    assert!(
        arg_final < 0.30,
        "Argentina normalized rate {:.4} > 30%; calibration is off",
        arg_final
    );
    assert!(
        arg_final > pan_final,
        "Argentina normalized rate {:.4} should exceed Panama {:.4}",
        arg_final, pan_final
    );
}
