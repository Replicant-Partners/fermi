//! Measure the two correlation problems in a weather portfolio.
//!
//! ```sh
//! cargo run --example weather_portfolio_risk -- EGLC,LFPB,EHAM,EDDM,KLGA,KLAX
//! ```
//!
//! Part 1 measures cross-station forecast-error correlation and the resulting
//! Kelly haircut. Part 2 demonstrates why per-bucket Kelly is wrong inside a
//! single ladder, using the live London ladder.

use fermi::agent_backend::weather_tools::dispatch;
use serde_json::{json, Value};

async fn call(input: Value) -> Value {
    match dispatch("weather_portfolio_risk", &input).await {
        Some(Ok(s)) => serde_json::from_str(&s).unwrap_or(Value::Null),
        Some(Err(e)) => {
            eprintln!("failed: {e}");
            Value::Null
        }
        None => Value::Null,
    }
}

#[tokio::main]
async fn main() {
    let stations: Vec<String> = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "EGLC,LFPB,EHAM,EDDM,KLGA,KLAX".into())
        .split(',')
        .map(|s| s.trim().to_uppercase())
        .collect();

    // ── Part 1 ───────────────────────────────────────────────────────────
    let v = call(json!({ "stations": stations, "lead_days": 2, "days_back": 120 })).await;
    let c = &v["cross_station"];

    println!("CROSS-STATION FORECAST-ERROR CORRELATION");
    println!(
        "  lead {} days, {} common verifying days\n",
        c["lead_days"], c["common_days"]
    );

    print!("{:>6}", "");
    for s in &stations {
        print!("{s:>7}");
    }
    println!();
    for row in c["correlation_matrix"].as_array().unwrap_or(&vec![]) {
        print!("{:>6}", row["station"].as_str().unwrap_or(""));
        for x in row["row"].as_array().unwrap_or(&vec![]) {
            print!("{:>7.2}", x.as_f64().unwrap_or(f64::NAN));
        }
        println!();
    }

    println!("\n  per-station RMSE:");
    for r in c["per_station_rmse"].as_array().unwrap_or(&vec![]) {
        println!(
            "    {:<6} {:>6} ({} days)",
            r["station"].as_str().unwrap_or(""),
            r["rmse"],
            r["n_days"]
        );
    }
    for o in c["rmse_outliers"].as_array().unwrap_or(&vec![]) {
        println!(
            "    ! OUTLIER {} rmse {} vs median {}",
            o["station"], o["rmse"], o["median_rmse"]
        );
    }

    println!(
        "\n  naive independent bets      {}",
        c["naive_independent_bets"]
    );
    println!(
        "  effective independent bets  {}",
        c["effective_independent_bets"]
    );
    println!("  KELLY STAKE HAIRCUT         {}", c["kelly_stake_haircut"]);
    println!(
        "  variance overstated by      {}x if ignored",
        c["variance_overstatement_if_ignored"]
    );

    // ── Part 2: the live London ladder ───────────────────────────────────
    // Model probabilities from the measured-dispersion template; prices from
    // the live CLOB books.
    // The COMPLETE 11-bucket ladder. Passing only the central buckets
    // manufactures a phantom arbitrage — the tool now rejects that.
    let ladder = json!([
        { "label": "20 or below", "model_prob": 0.0002, "price": 0.0015 },
        { "label": "21",          "model_prob": 0.0003, "price": 0.0010 },
        { "label": "22",          "model_prob": 0.0006, "price": 0.0015 },
        { "label": "23",          "model_prob": 0.0071, "price": 0.0090 },
        { "label": "24",          "model_prob": 0.0006, "price": 0.0650 },
        { "label": "25",          "model_prob": 0.0157, "price": 0.1450 },
        { "label": "26",          "model_prob": 0.1340, "price": 0.3350 },
        { "label": "27",          "model_prob": 0.3761, "price": 0.2950 },
        { "label": "28",          "model_prob": 0.3521, "price": 0.1450 },
        { "label": "29",          "model_prob": 0.1099, "price": 0.0450 },
        { "label": "30 or higher","model_prob": 0.0116, "price": 0.0170 }
    ]);
    let v2 = call(json!({ "ladder": ladder })).await;
    let l = &v2["within_ladder"];

    println!("\n\nWITHIN-LADDER SIZING — London 2026-08-15, mutually exclusive buckets");
    println!(
        "\n{:>7} {:>7} {:>7} {:>7} {:>13} {:>13}",
        "bucket", "model", "price", "edge", "multi-Kelly", "per-bucket"
    );
    for o in l["outcomes"].as_array().unwrap_or(&vec![]) {
        println!(
            "{:>7} {:>7} {:>7} {:>7} {:>13} {:>13}",
            o["label"].as_str().unwrap_or(""),
            o["model_prob"],
            o["price"],
            o["edge"],
            o["multi_outcome_kelly_fraction"],
            o["per_bucket_kelly_fraction"]
        );
    }
    println!(
        "\n  total stake, multi-outcome Kelly  {}",
        l["total_stake_multi_outcome"]
    );
    println!(
        "  total stake, per-bucket summed    {}",
        l["total_stake_per_bucket_naive"]
    );
    println!(
        "  naive/multi stake ratio          {}x",
        l["naive_over_stakes_by"]
    );
    println!(
        "  ladder: model sum {}  price sum {}  overround {}",
        l["model_prob_sum"], l["price_sum"], l["ladder_overround"]
    );
    println!(
        "  LOG-GROWTH LOST BY GOING NAIVE   {}",
        l["naive_is_worse_by"]
    );
    println!("  hit 95% stake cap: {}", l["hit_stake_cap"]);

    println!("\nSIZING ORDER");
    for s in v2["sizing_order"].as_array().unwrap_or(&vec![]) {
        println!("  {}", s.as_str().unwrap_or(""));
    }
}
