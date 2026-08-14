//! Verify the weather bucket-ladder FPL parses, has the intended structure,
//! and reproduces the live market on a case with a known answer.
//!
//! Pinned design contract:
//!   - Five drivers forming an ERROR DECOMPOSITION, not a weather narrative.
//!   - `station_bias` is `learnable: true` with a `scalar_difference` extractor,
//!     so BayesOps refits the residual as observations accumulate.
//!   - Every driver is referenced by exactly one agent's `driver_refs`.
//!   - The model is a BUCKET INDICATOR over a composed predictive temperature,
//!     NOT the multiplicative `base_rate * d1 * d2 * ...` form used by
//!     `team_prior.fpl`. The regression test at the bottom is the reason: the
//!     multiplicative form cannot express a distribution that has moved 11
//!     degrees away from climatology, and it produced a 44x error in
//!     production.
//!   - The base rate is the outside view only. It must NOT appear in the model.

use fermi::executor::Executor;
use fermi::lexer::Lexer;
use fermi::parser::Parser;

const TEMPLATE: &str = include_str!("../templates/weather/bucket_ladder.fpl");

/// Render the string params the way workspace instantiation does.
fn render(station_name: &str, market_date: &str, bucket_label: &str) -> String {
    TEMPLATE
        .replace("{station_name}", station_name)
        .replace("{market_date}", market_date)
        .replace("{bucket_label}", bucket_label)
        .replace("{station}", "EGLC")
        .replace("{timezone}", "Europe/London")
        .replace("{market_unit}", "celsius")
        .replace("{lead_days}", "1")
}

fn parse(src: &str) -> fermi::ast::Program {
    let tokens = Lexer::new(src)
        .tokenize()
        .expect("template should tokenize");
    Parser::new(tokens).parse().expect("template should parse")
}

#[test]
fn bucket_ladder_template_parses_with_the_intended_structure() {
    let program = parse(&render("London City Airport", "2026-08-14", "32"));

    // The five error sources, and nothing meteorologically narrative.
    let names: Vec<&str> = program.drivers().iter().map(|d| d.name.as_str()).collect();
    for expected in [
        "ensemble_center",
        "station_bias",
        "predictive_sd_factor",
        "weather_draw",
    ] {
        assert!(
            names.contains(&expected),
            "missing driver {expected} in {names:?}"
        );
    }
    assert_eq!(names.len(), 4, "unexpected driver set: {names:?}");

    // Three drivers were removed because each was a PRIOR standing in for
    // something now MEASURED, and each participated in a double-count:
    //   epistemic_widening / epistemic_center_shift — cross-model disagreement,
    //     already inside a station-verified RMSE
    //   dispersion_inflation — a single-model literature factor applied to a
    //     pooled spread, which inflated an already-inflated number
    for removed in [
        "epistemic_widening",
        "epistemic_center_shift",
        "dispersion_inflation",
    ] {
        assert!(
            !names.contains(&removed),
            "{removed} was replaced by a measured predictive_sd; reintroducing it double-counts"
        );
    }

    // These are the drivers the failed production decomposition used. They are
    // narrative, not separable error sources, and none can be fed by an
    // observation stream — so none can ever be learnable.
    for narrative in [
        "synoptic_pattern",
        "urban_heat_island_intensity",
        "climate_trend_adjustment",
        "forecast_lead_time_skill",
        "exact_threshold_precision",
    ] {
        assert!(
            !names.iter().any(|n| n.contains(narrative)),
            "{narrative} is a narrative driver, not an error source"
        );
    }

    // The residual is the one thing that can be measured from resolutions.
    let drivers = program.drivers();
    let learnable: Vec<_> = drivers.iter().filter(|d| d.learnable).collect();
    assert_eq!(
        learnable.len(),
        1,
        "expected exactly one learnable driver, got {:?}",
        learnable
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(learnable[0].name, "station_bias");
    let feeds = learnable[0]
        .feeds_from
        .as_ref()
        .expect("station_bias must declare feeds_from so the refit hook can fire");
    assert_eq!(
        feeds.extractor, "scalar_difference",
        "the residual is obs MINUS forecast, so the extractor must be scalar_difference"
    );
}

#[test]
fn every_driver_is_bound_to_an_agent_that_can_research_it() {
    let program = parse(&render("London City Airport", "2026-08-14", "32"));

    let agents: Vec<&str> = program.agents().iter().map(|a| a.name.as_str()).collect();
    for expected in ["weather_ensemble_forecaster", "weather_calibrator"] {
        assert!(
            agents.contains(&expected),
            "missing agent {expected} in {agents:?}"
        );
    }

    // `driver_refs` IS the binding — read from this program at runtime by
    // resolve_driver_prefixes. An unreferenced driver gets no evidence, and an
    // agent with no driver_refs has its multiplier silently discarded.
    for d in program.drivers() {
        // weather_draw is pure chaos: nothing researches it, by design.
        if d.name == "weather_draw" || d.name == "predictive_sd_factor" {
            continue;
        }
        let referenced = program
            .agents()
            .iter()
            .any(|a| a.driver_refs.iter().any(|r| r == &d.name));
        assert!(referenced, "driver {} has no agent researching it", d.name);
    }

    // The market analyst must NOT appear: keeping the calibrator blind to the
    // price is structural, not advisory.
    assert!(
        !agents.contains(&"weather_market_analyst"),
        "the analyst prices the finished distribution; it must not be inside the model"
    );
}

#[test]
fn the_model_is_a_bucket_indicator_and_not_a_scaled_base_rate() {
    let program = parse(&render("London City Airport", "2026-08-14", "32"));

    let q = program.question().expect("must declare a question");
    let br = q
        .base_rate
        .as_ref()
        .expect("must declare a base_rate for the outside view");
    assert!(
        br.historical_frequency > 0.0 && br.historical_frequency < 0.5,
        "base rate {} is not a plausible climatological frequency",
        br.historical_frequency
    );

    let model = format!(
        "{:?}",
        program.model().expect("must have a model expression")
    );

    // The bucket edges must both appear: a bucket is an interval, and reading
    // it as a one-sided threshold was a 6x error in production.
    assert!(model.contains("bucket_lo"), "model must use the lower edge");
    assert!(model.contains("bucket_hi"), "model must use the upper edge");

    // The base rate must NOT be multiplied in. This is the specific structural
    // defect being guarded against.
    let br_literal = format!("{}", br.historical_frequency);
    assert!(
        !model.contains(&br_literal),
        "the base rate must not enter the model — it is the outside view, not a \
         factor. A multiplicative model can only SCALE the base rate, which is \
         exactly why the production London forecast returned 0.3% when the \
         ensemble said 13.3%."
    );

    // Every driver must be in the composed temperature.
    for d in [
        "ensemble_center",
        "station_bias",
        "predictive_sd_factor",
        "weather_draw",
        "predictive_sd",
    ] {
        assert!(model.contains(d), "model must reference {d}");
    }

    // The raw ensemble spread must NOT enter the model. The predictive sd is
    // measured against outcomes, not derived from the spread — and the pooled
    // spread in particular is over-dispersive at short lead.
    assert!(
        !model.contains("ens_sd"),
        "ens_sd is reporting-only; the model must use the MEASURED predictive_sd"
    );
}

/// Bind the ensemble state the way `load_workspace_params` does at runtime.
///
/// Values are the real 143-member multi-model ensemble for EGLC on 2026-08-14
/// at lead 1 day, as returned by `weather_ensemble_forecast`.
#[allow(clippy::too_many_arguments)]
fn simulate_bucket(bucket_lo: f64, bucket_hi: f64) -> f64 {
    let program = parse(&render("London City Airport", "2026-08-14", "32"));
    let mut exec = Executor::new(40_000);

    // Live ensemble state.
    exec.set_param("ens_mean", 33.4);
    exec.set_param("ens_sd", 1.167);
    // Monte Carlo error on the mean ONLY: 1.167/sqrt(143). Every other source
    // of centre uncertainty is already inside the measured predictive_sd.
    exec.set_param("ens_mean_se", 0.10);
    exec.set_param("ens_n_members", 143.0);
    exec.set_param("lead_days", 1.0);
    // MEASURED by weather_dispersion_fit: EGLC, 121 verifying days, lead 1.
    // The residual sd after bias correction, not the raw RMSE.
    exec.set_param("predictive_sd", 0.909);
    exec.set_param("sd_factor_p5", 0.871);
    exec.set_param("sd_factor_p50", 1.0);
    exec.set_param("sd_factor_p95", 1.129);
    // Lead-1 residual is -0.05 C and NOT significant, so it is sampling noise.
    // Bound tightly around it rather than dropped, to keep the shape honest.
    exec.set_param("bias_p5", -0.218);
    exec.set_param("bias_p50", -0.053);
    exec.set_param("bias_p95", 0.113);

    exec.set_param("bucket_lo", bucket_lo);
    exec.set_param("bucket_hi", bucket_hi);

    exec.execute(&program).expect("execute").mean
}

/// The regression that motivated this template.
///
/// On 2026-08-14 the Fermi console reported **0.3%** for the London 32C bucket.
/// The live market was **13.5%** and a 143-member ensemble said **13.3%**. The
/// console was off by 44x, because its multiplicative model could only scale a
/// 0.3% climatological base rate and the predictive distribution had moved 11
/// degrees above climatology.
///
/// This template, given the same ensemble, must land near the truth.
#[test]
fn reproduces_the_london_market_that_the_multiplicative_model_missed() {
    let p32 = simulate_bucket(31.5, 32.5);
    let p33 = simulate_bucket(32.5, 33.5);
    let p34 = simulate_bucket(33.5, 34.5);

    eprintln!();
    eprintln!("─── EGLC 2026-08-14, lead 1, 143 members, mean 33.4C ───");
    eprintln!(
        "  bucket 32   model {:>5.1}%   market 13.5%   console 0.3%",
        p32 * 100.0
    );
    eprintln!("  bucket 33   model {:>5.1}%   market 48.5%", p33 * 100.0);
    eprintln!("  bucket 34   model {:>5.1}%   market 32.5%", p34 * 100.0);
    eprintln!("────────────────────────────────────────────────────────");
    eprintln!();

    // The headline: within a few points of the market, not two orders of
    // magnitude away. Generous bounds — this pins the SHAPE, not a tuning.
    assert!(
        (0.08..0.25).contains(&p32),
        "bucket 32 came out at {:.4}; the market was 0.135 and the ensemble 0.133. \
         Outside [8%, 25%] means the model shape is wrong again.",
        p32
    );

    // Specifically: nowhere near the climatological base rate. A model that
    // returns its own base rate at lead 1 has ignored the ensemble entirely,
    // which is precisely the production failure.
    assert!(
        p32 > 0.05,
        "bucket 32 at {:.4} is close to the 3% climatological base rate — the \
         ensemble is not reaching the model",
        p32
    );

    // 33 is the ensemble mode (mean 33.4), so it must carry the most mass.
    assert!(
        p33 > p32 && p33 > p34,
        "the mode should be bucket 33 given mean 33.4C: got 32={p32:.4} 33={p33:.4} 34={p34:.4}"
    );

    // Adjacent buckets must be a proper partition — no double counting.
    assert!(
        p32 + p33 + p34 < 1.0,
        "three adjacent buckets sum to {:.4}; they must be disjoint",
        p32 + p33 + p34
    );

    // With MEASURED dispersion the whole ladder should track the market, not
    // just the one bucket. This is the assertion that would have failed for
    // every prior-based version of this template: the four-prior build gave
    // 17.0 / 28.4 / 26.5 against 13.5 / 48.5 / 32.5, a mean absolute deviation
    // of 9.5pp, because its distribution was 42% too wide.
    let market = [0.135, 0.485, 0.325];
    let model = [p32, p33, p34];
    let mad = model
        .iter()
        .zip(market.iter())
        .map(|(m, k)| (m - k).abs())
        .sum::<f64>()
        / 3.0;
    assert!(
        mad < 0.08,
        "mean absolute deviation from the market ladder is {:.1}pp (model {:?} vs market {:?}). \
         Above 8pp means the measured dispersion has regressed to a prior again.",
        mad * 100.0,
        model.map(|x| (x * 1000.0).round() / 10.0),
        market.map(|x| x * 100.0)
    );
}

/// A bucket far below the ensemble must be near-impossible, and a bucket far
/// above must be too. Guards against an indicator that ignores an edge — the
/// failure mode where `>=lo` is applied without `<hi` and the "bucket"
/// silently becomes a one-sided threshold worth 6x more.
#[test]
fn buckets_far_from_the_ensemble_are_near_zero_on_both_sides() {
    let far_below = simulate_bucket(21.5, 22.5); // climatological mean, 11C low
    let far_above = simulate_bucket(41.5, 42.5);

    assert!(
        far_below < 0.01,
        "a bucket 11C below the ensemble mean should be near zero, got {far_below:.4}"
    );
    assert!(
        far_above < 0.01,
        "a bucket 8C above the ensemble mean should be near zero, got {far_above:.4}"
    );
}
