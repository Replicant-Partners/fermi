//! Live smoke test for the weather_oracle tool stack.
//!
//! Exercises every network-touching weather tool against the real upstream
//! APIs and prints a compact report. Run manually — it is deliberately NOT a
//! `#[test]`, because it depends on live third-party services and on today's
//! Polymarket listings, and a CI failure here would mean "Open-Meteo is down",
//! not "the code is broken".
//!
//! ```sh
//! cargo run --example weather_oracle_smoke
//! ```

use fermi::agent_backend::weather_tools::dispatch;
use serde_json::{json, Value};

async fn run(name: &str, input: Value) -> Option<Value> {
    print!("── {name} … ");
    match dispatch(name, &input).await {
        Some(Ok(s)) => {
            let v: Value = serde_json::from_str(&s).unwrap_or(Value::Null);
            println!("ok ({} bytes)", s.len());
            Some(v)
        }
        Some(Err(e)) => {
            println!("FAILED: {e}");
            None
        }
        None => {
            println!("FAILED: not dispatched");
            None
        }
    }
}

#[tokio::main]
async fn main() {
    println!("weather_oracle live smoke test\n");

    // ── 1. Settlement spec: local, must always work ──────────────────────
    let spec = run(
        "weather_settlement_spec",
        json!({ "city": "NYC", "variable": "high_temp" }),
    )
    .await;
    if let Some(v) = &spec {
        println!(
            "   station={} tz={} unit={} step={}",
            v["settlement_station"]["icao"],
            v["settlement_station"]["timezone"],
            v["units_and_rounding"]["market_unit"],
            v["units_and_rounding"]["bucket_step"]
        );
        assert_eq!(v["settlement_station"]["icao"], "KLGA", "NYC must be KLGA");
    }

    // ── 2. Ensemble: the ~161-member multi-model cloud ───────────────────
    let target = (chrono::Utc::now() + chrono::Duration::days(2))
        .format("%Y-%m-%d")
        .to_string();
    let ens = run(
        "weather_ensemble_forecast",
        json!({
            "station": "KLGA",
            "target_date": target,
            "variable": "temperature_2m_max",
            "unit": "fahrenheit",
            // A plausible Polymarket US ladder: 2F buckets on integer
            // settlement, so continuous edges sit on the .5 boundaries.
            "bucket_edges": [79.5, 81.5, 83.5, 85.5, 87.5, 89.5],
            "thresholds": [85.5, 87.5]
        }),
    )
    .await;
    if let Some(v) = &ens {
        println!(
            "   n_members={} mean={} sd={} models={}",
            v["ensemble"]["n_members"],
            v["ensemble"]["mean"],
            v["ensemble"]["std_dev"],
            v["ensemble"]["models_returned"]
        );
        println!(
            "   cross_model_median_range={}",
            v["epistemic_disagreement"]["cross_model_median_range"]
        );
        for m in v["ensemble"]["models_missing"]
            .as_array()
            .unwrap_or(&vec![])
        {
            println!("   MISSING {}: {}", m["model"], m["likely_reason"]);
        }
        let n = v["ensemble"]["n_members"].as_u64().unwrap_or(0);
        assert!(n >= 100, "expected a large multi-model ensemble, got {n}");

        // Bucket probabilities must form a proper distribution.
        let sum: f64 = v["bucket_probabilities"]
            .as_array()
            .map(|a| a.iter().filter_map(|b| b["probability"].as_f64()).sum())
            .unwrap_or(0.0);
        println!("   bucket probabilities sum to {sum:.4} (must be 1.0)");
        assert!((sum - 1.0).abs() < 1e-6, "bucket probs must sum to 1");

        for b in v["bucket_probabilities"].as_array().unwrap_or(&vec![]) {
            if b["probability"].as_f64().unwrap_or(0.0) > 0.0 {
                println!("     {} -> {}", b["label"], b["probability"]);
            }
        }
    }

    // ── 3. Climatology: ERA5 base rate + trend ───────────────────────────
    let clim = run(
        "weather_climatology",
        json!({
            "station": "KLGA",
            "target_date": target,
            "variable": "temperature_2m_max",
            "unit": "fahrenheit",
            "window_days": 5,
            "years_back": 15,
            "thresholds": [85.5, 87.5]
        }),
    )
    .await;
    if let Some(v) = &clim {
        println!(
            "   n_obs={} mean={} sd={} trend/decade={}",
            v["sample"]["n_observations"],
            v["distribution"]["mean"],
            v["distribution"]["std_dev"],
            v["trend"]["slope_per_decade"]
        );
        for b in v["base_rates"].as_array().unwrap_or(&vec![]) {
            println!(
                "     >= {}: raw={} trend_adjusted={}",
                b["threshold"], b["raw_base_rate"], b["trend_adjusted_base_rate"]
            );
        }
    }

    // ── 4. Station observation: NWS truth feed ───────────────────────────
    let obs = run(
        "weather_station_observation",
        json!({ "station": "KLGA", "include_cli": true, "hours_back": 24 }),
    )
    .await;
    if let Some(v) = &obs {
        println!(
            "   running max={}F min={}F over {} obs",
            v["running_extremes_in_window"]["max_f"],
            v["running_extremes_in_window"]["min_f"],
            v["n_with_temperature"]
        );
        for rep in v["climatological_reports"]["reports_newest_first"]
            .as_array()
            .unwrap_or(&vec![])
        {
            println!(
                "     CLI issued {} covers {} -> max {} at {} (normal {}, record {} in {})",
                rep["issuance_time_utc"],
                rep["summary_is_for_date"],
                rep["maximum"]["observed"],
                rep["maximum"]["observed_at_local"],
                rep["maximum"]["normal_1991_2020"],
                rep["maximum"]["record"],
                rep["maximum"]["record_year"]
            );
        }
    }

    // Non-US must degrade gracefully, not lie.
    if let Some(v) = run("weather_station_observation", json!({ "station": "RJTT" })).await {
        println!("   RJTT available={} (expected false)", v["available"]);
        assert_eq!(v["available"], false, "non-US must report unavailable");
    }

    // ── 5. Polymarket: list, then drill into one event ───────────────────
    let list = run(
        "polymarket_weather_markets",
        json!({ "series_slug": "nyc-daily-weather", "limit": 3 }),
    )
    .await;

    let mut token_id: Option<String> = None;
    if let Some(v) = &list {
        for e in v["events"].as_array().unwrap_or(&vec![]) {
            println!("     {} (vol24h={})", e["slug"], e["volume_24hr"]);
        }
        // Drill into the top event.
        if let Some(slug) = v["events"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|e| e["slug"].as_str())
        {
            if let Some(ev) = run("polymarket_weather_markets", json!({ "slug": slug })).await {
                let rules = ev["resolution_criteria_verbatim"]
                    .as_str()
                    .unwrap_or("(none)");
                println!(
                    "   rules snippet: {}…",
                    rules.chars().take(220).collect::<String>()
                );
                println!("   outcome_count={}", ev["outcome_count"]);
                token_id = ev["outcomes"]
                    .as_array()
                    .and_then(|a| a.iter().find(|m| m["clob_token_ids"].is_array()))
                    .and_then(|m| m["clob_token_ids"][0].as_str())
                    .map(String::from);
            }
        }
    }

    // ── 6. Order book + valuation ────────────────────────────────────────
    if let Some(tid) = token_id {
        if let Some(v) = run(
            "polymarket_orderbook",
            json!({
                "token_id": tid,
                "fair_probability": 0.55,
                "bankroll_usd": 1000.0,
                "kelly_fraction": 0.25
            }),
        )
        .await
        {
            println!(
                "   bid={} ask={} mid={} spread={} tradeable={}",
                v["best_bid"],
                v["best_ask"],
                v["midpoint"],
                v["spread"],
                v["book_quality"]["tradeable"]
            );
            for i in v["book_quality"]["issues"].as_array().unwrap_or(&vec![]) {
                println!("     book issue: {i}");
            }
            for s in v["valuation"]["sides"].as_array().unwrap_or(&vec![]) {
                println!(
                    "     {} @ {} -> ev_taker={} ev_maker={} stake=${} | {}",
                    s["side"],
                    s["entry_price"],
                    s["ev_per_share_taker"],
                    s["ev_per_share_maker"],
                    s["recommended_stake_usd"],
                    s["verdict"]
                );
            }
        }
    } else {
        println!("── polymarket_orderbook … skipped (no token id resolved)");
    }

    println!("\nsmoke test complete");
}
