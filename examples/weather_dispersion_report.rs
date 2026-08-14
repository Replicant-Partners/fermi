//! Fit and report a station's measured forecast error by lead time.
//!
//! ```sh
//! cargo run --example weather_dispersion_report -- EGLC
//! cargo run --example weather_dispersion_report -- KLGA
//! ```
//!
//! Prints the numbers to bind into `templates/weather/bucket_ladder.fpl`.
//! Replaces guessed calibration priors with measurement against 120 days of
//! this station's own forecast-versus-outcome history.

use fermi::agent_backend::weather_tools::dispatch;
use serde_json::json;
#[tokio::main]
async fn main() {
    let st = std::env::args().nth(1).unwrap_or_else(|| "EGLC".into());
    let s = dispatch(
        "weather_dispersion_fit",
        &json!({"station": st, "days_back": 120, "max_lead": 7}),
    )
    .await
    .unwrap()
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    println!("STATION {} · {}", v["station"], v["location"]["timezone"]);
    println!(
        "\n{:>4} {:>4} {:>9} {:>7} {:>6} {:>6}",
        "lead", "n", "bias(o-f)", "sig", "mae", "rmse"
    );
    for e in v["per_lead_error"].as_array().unwrap() {
        if e["usable"] != json!(true) {
            continue;
        }
        println!(
            "{:>4} {:>4} {:>9} {:>7} {:>6} {:>6}",
            e["lead_days"],
            e["n"],
            e["bias_actual_minus_forecast"],
            e["bias_is_significant"],
            e["mae"],
            e["rmse"]
        );
    }
    println!(
        "\n{:>4} {:>8} {:>9} {:>9} {:>12} {:>11}",
        "lead", "target", "pooled", "ref_model", "f_vs_pooled", "f_vs_ref"
    );
    for c in v["dispersion_comparison"].as_array().unwrap() {
        println!(
            "{:>4} {:>8} {:>9} {:>9} {:>12} {:>11}",
            c["lead_days"],
            c["target_predictive_sd"],
            c["pooled_multimodel_spread"],
            c["reference_model_spread"],
            c["implied_factor_vs_pooled"],
            c["implied_factor_vs_reference"]
        );
    }
    println!("\nFITTED FPL PARAMS");
    for f in v["fitted_fpl_params"].as_array().unwrap() {
        println!(
            "  lead {}: predictive_sd={}  bias=({}, {}, {})  sd_factor=({}, {}, {})",
            f["lead_days"],
            f["predictive_sd"],
            f["bias_p5"],
            f["bias_p50"],
            f["bias_p95"],
            f["predictive_sd_factor_p5"],
            f["predictive_sd_factor_p50"],
            f["predictive_sd_factor_p95"]
        );
    }
}
