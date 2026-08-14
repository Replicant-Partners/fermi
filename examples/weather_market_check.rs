//! Price one weather market from the command line.
//!
//! The independent check on a Fermi decomposition: resolves the settlement
//! station, pulls the multi-model ensemble and the ERA5 base rate, and reports
//! the bucket probability the market actually settles on.
//!
//! ```sh
//! cargo run --example weather_market_check -- London 2026-08-14 32
//! cargo run --example weather_market_check -- NYC 2026-08-16 86
//! ```
//!
//! Args: `<city> <YYYY-MM-DD> <bucket_value>`. `bucket_value` is the integer
//! the market labels the bucket with — the tool converts it to the continuous
//! interval `[v-0.5, v+0.5)` for whole-degree settlement, because a bucket
//! label is an integer SET, not a threshold.

use fermi::agent_backend::weather_tools::dispatch;
use serde_json::{json, Value};

async fn call(name: &str, input: Value) -> Value {
    match dispatch(name, &input).await {
        Some(Ok(s)) => serde_json::from_str(&s).unwrap_or(Value::Null),
        Some(Err(e)) => {
            eprintln!("{name} failed: {e}");
            Value::Null
        }
        None => {
            eprintln!("{name}: not dispatched");
            Value::Null
        }
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let city = args.first().cloned().unwrap_or_else(|| "London".into());
    let date = args.get(1).cloned().unwrap_or_else(|| {
        (chrono::Utc::now() + chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string()
    });
    let bucket: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(32.0);

    // ── 1. What actually settles this? ───────────────────────────────────
    let spec = call(
        "weather_settlement_spec",
        json!({ "city": city, "variable": "high_temp" }),
    )
    .await;

    let station = spec["settlement_station"]["icao"].as_str().unwrap_or("?");
    let unit = spec["units_and_rounding"]["market_unit"]
        .as_str()
        .unwrap_or("celsius");
    println!("QUESTION   will {city} reach {bucket} on {date}?");
    println!(
        "SETTLES ON {station} — {}",
        spec["settlement_station"]["name"]
    );
    println!(
        "           tz={} unit={unit} step={}",
        spec["settlement_station"]["timezone"], spec["units_and_rounding"]["bucket_step"]
    );
    println!("SOURCE     {}", spec["resolution_source"]["id"]);
    for w in spec["warnings"].as_array().unwrap_or(&vec![]) {
        println!(
            "  ! {}",
            w.as_str()
                .unwrap_or("")
                .chars()
                .take(150)
                .collect::<String>()
        );
    }

    // A bucket label is an integer set: "32C" means the published integer is
    // 32, i.e. the continuous interval [31.5, 32.5).
    let (lo, hi) = (bucket - 0.5, bucket + 0.5);
    println!("\nBUCKET     label {bucket} -> continuous [{lo}, {hi})  (and >= {lo} for the 'at least' reading)");

    // ── 2. The ensemble ──────────────────────────────────────────────────
    let ens = call(
        "weather_ensemble_forecast",
        json!({
            "station": station,
            "target_date": date,
            "variable": "temperature_2m_max",
            "unit": unit,
            // icon_global rather than icon_eu so the call works for any station.
            "models": ["ecmwf_ifs025", "icon_global", "gfs025", "gem_global", "bom_access_global_ensemble"],
            "thresholds": [lo, hi],
            "bucket_edges": [lo, hi]
        }),
    )
    .await;

    let e = &ens["ensemble"];
    println!(
        "\nENSEMBLE   lead {} days, {} members from {}",
        ens["lead_days"], e["n_members"], e["models_returned"]
    );
    println!(
        "           mean={} median={} sd={}",
        e["mean"], e["median"], e["std_dev"]
    );
    println!(
        "           p05={} p25={} p75={} p95={} max={}",
        e["p05"], e["p25"], e["p75"], e["p95"], e["max"]
    );
    println!(
        "           cross-model median range={} (epistemic)",
        ens["epistemic_disagreement"]["cross_model_median_range"]
    );
    for m in e["models_missing"].as_array().unwrap_or(&vec![]) {
        println!("  ! missing {}", m["model"]);
    }

    println!("\nRAW PROBABILITIES (uncalibrated member frequencies)");
    for t in ens["threshold_probabilities"].as_array().unwrap_or(&vec![]) {
        println!(
            "  P(X >= {:>6}) = {:>7}   (+/- {} MC)",
            t["threshold"], t["p_at_or_above"], t["monte_carlo_std_error"]
        );
    }
    for b in ens["bucket_probabilities"].as_array().unwrap_or(&vec![]) {
        if b["lower"] == json!(lo) && b["upper"] == json!(hi) {
            println!(
                "  P(bucket {bucket})      = {:>7}   ({} members)",
                b["probability"], b["members"]
            );
        }
    }

    // ── 3. Climatology — the reference forecast ──────────────────────────
    let clim = call(
        "weather_climatology",
        json!({
            "station": station,
            "target_date": date,
            "variable": "temperature_2m_max",
            "unit": unit,
            "window_days": 5,
            "years_back": 30,
            "thresholds": [lo]
        }),
    )
    .await;

    println!(
        "\nCLIMATOLOGY {} years, {} obs",
        clim["sample"]["n_years"], clim["sample"]["n_observations"]
    );
    println!(
        "           mean={} sd={} p95={} trend={}/decade",
        clim["distribution"]["mean"],
        clim["distribution"]["std_dev"],
        clim["distribution"]["p95"],
        clim["trend"]["slope_per_decade"]
    );
    for b in clim["base_rates"].as_array().unwrap_or(&vec![]) {
        println!(
            "  base rate P(X >= {}) raw={} trend_adjusted={}",
            b["threshold"], b["raw_base_rate"], b["trend_adjusted_base_rate"]
        );
    }

    // ── 4. The sanity test the design mandates ───────────────────────────
    let ens_mean = e["mean"].as_f64().unwrap_or(f64::NAN);
    let lead = ens["lead_days"].as_i64().unwrap_or(99);
    println!("\nSANITY");
    println!(
        "  distance to bucket: ensemble mean is {:+.2} vs the {lo} edge",
        ens_mean - lo
    );
    if lead <= 2 {
        println!("  lead is {lead} day(s): the ensemble is highly informative here.");
        println!("  A model probability far BELOW the market at short lead is a red flag");
        println!("  on the model, not an edge — climatology alone is not a forecast.");
    }
}
