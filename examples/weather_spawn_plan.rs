//! Build the workspace batch payload for a set of weather bucket ladders.
//!
//! Assembles every parameter `templates/weather/bucket_ladder.fpl` needs, from
//! the settlement registry, the live ensemble, the measured dispersion fit and
//! the live Polymarket ladder — then emits the exact JSON body the batch spawn
//! endpoint accepts.
//!
//! ```sh
//! cargo run --example weather_spawn_plan -- London,NYC,Tokyo 2026-08-16 > plan.json
//! python3 scripts/weather/spawn_bucket_ladders.py plan.json
//! ```
//!
//! Param computation lives here rather than in the Python poster so there is
//! one source of truth: the same tools the agents call. The dispersion fit is
//! cached per station, since it is the expensive call (120 days of history) and
//! does not vary by bucket or date.
//!
//! Buckets with negligible probability on BOTH sides — model and market — are
//! reported but not spawned. Eleven workspaces per city-day where five are dead
//! is just cost.

use fermi::agent_backend::weather_tools::dispatch;
use serde_json::{json, Value};
use std::collections::HashMap;

async fn call(name: &str, input: Value) -> Result<Value, String> {
    match dispatch(name, &input).await {
        Some(Ok(s)) => serde_json::from_str(&s).map_err(|e| e.to_string()),
        Some(Err(e)) => Err(e),
        None => Err(format!("{name} not dispatched")),
    }
}

/// Parse a Polymarket ladder outcome into its continuous bucket edges.
///
/// The label is an integer SET, not a threshold: "32" with a 1C step means the
/// published integer is 32, i.e. `[31.5, 32.5)`. US ladders step 2F, so "86-87"
/// is `[85.5, 87.5)`. The two open tails are half-bounded.
///
/// Returns `(label, lo, hi)` where an infinite edge is represented by a wide
/// sentinel — the FPL indicator only ever compares, so a sentinel is safe and
/// avoids threading `Option` through the params.
fn parse_bucket(question: &str, step: f64) -> Option<(String, f64, f64)> {
    let q = question.to_lowercase();
    // Pull every integer in the question, then drop the trailing date number.
    let nums: Vec<f64> = q
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    // "be 32°C on August 14" -> [32, 14]; "be 86-87°F on ..." -> [86, 87, 14].
    // The temperature is the first number; a second number is the pair's upper
    // label only when it is within one step of the first.
    let lo_label = *nums.first()?;
    let hi_label = nums
        .get(1)
        .copied()
        .filter(|v| *v > lo_label && *v - lo_label < step + 0.5)
        .unwrap_or(lo_label);

    if q.contains("or below") || q.contains("or lower") {
        return Some((format!("{lo_label:.0} or below"), -99.0, lo_label + 0.5));
    }
    if q.contains("or higher") || q.contains("or above") {
        return Some((format!("{lo_label:.0} or higher"), lo_label - 0.5, 99.0));
    }
    let label = if (hi_label - lo_label).abs() < f64::EPSILON {
        format!("{lo_label:.0}")
    } else {
        format!("{lo_label:.0}-{hi_label:.0}")
    };
    Some((label, lo_label - 0.5, hi_label + 0.5))
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cities: Vec<String> = args
        .first()
        .map(|s| s.split(',').map(|c| c.trim().to_string()).collect())
        .unwrap_or_else(|| vec!["London".into()]);
    let date = args.get(1).cloned().unwrap_or_else(|| {
        (chrono::Utc::now() + chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string()
    });

    let mut instances: Vec<Value> = Vec::new();
    let mut skipped: Vec<Value> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    // The expensive call: one dispersion fit per station, reused across buckets.
    let mut fit_cache: HashMap<String, Value> = HashMap::new();

    for city in &cities {
        // ── 1. What settles this? ────────────────────────────────────────
        let spec = match call(
            "weather_settlement_spec",
            json!({ "city": city, "variable": "high_temp" }),
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                notes.push(format!("{city}: settlement spec failed — {e}"));
                continue;
            }
        };
        let station = spec["settlement_station"]["icao"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let station_name = spec["settlement_station"]["name"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let tz = spec["settlement_station"]["timezone"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let unit = spec["units_and_rounding"]["market_unit"]
            .as_str()
            .unwrap_or("celsius")
            .to_string();
        let step = f(&spec["units_and_rounding"]["bucket_step"]);
        let source = spec["resolution_source"]["id"]
            .as_str()
            .unwrap_or("")
            .to_string();

        // ── 2. Measured dispersion, cached per station ───────────────────
        let fit = match fit_cache.get(&station) {
            Some(v) => v.clone(),
            None => {
                let v = match call(
                    "weather_dispersion_fit",
                    json!({ "station": station, "days_back": 120, "max_lead": 7 }),
                )
                .await
                {
                    Ok(v) => v,
                    Err(e) => {
                        notes.push(format!("{station}: dispersion fit failed — {e}"));
                        continue;
                    }
                };
                fit_cache.insert(station.clone(), v.clone());
                v
            }
        };

        // ── 3. Live ensemble for the target date ─────────────────────────
        let ens = match call(
            "weather_ensemble_forecast",
            json!({
                "station": station, "target_date": date,
                "variable": "temperature_2m_max", "unit": unit,
                "models": ["ecmwf_ifs025", "icon_global", "gfs025", "gem_global"]
            }),
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                notes.push(format!("{station}: ensemble failed — {e}"));
                continue;
            }
        };
        let lead = ens["lead_days"].as_i64().unwrap_or(-1);
        let ens_mean = f(&ens["ensemble"]["mean"]);
        let ens_sd = f(&ens["ensemble"]["std_dev"]);
        let n_members = f(&ens["ensemble"]["n_members"]);
        let ens_mean_se = if n_members > 0.0 {
            ens_sd / n_members.sqrt()
        } else {
            0.1
        };

        // Pick the fitted row for THIS lead. Beyond the archive's 7 days there
        // is no measured sd, and guessing one is what this whole exercise
        // replaced — so decline instead.
        // Lead 0 has no fitted row: the archive exposes previous_day1..7, and
        // there is no "previous_day0". Fall back to the lead-1 sd as a
        // conservative UPPER BOUND — error at lead 0 cannot exceed error at
        // lead 1 — rather than declining, because same-day is exactly where the
        // realised-state edge lives. Flagged so the calibrator can tighten it
        // using the running maximum.
        let fit_lead = if lead == 0 { 1 } else { lead };
        let sd_is_upper_bound = lead == 0;
        let fitted = fit["fitted_fpl_params"]
            .as_array()
            .and_then(|a| a.iter().find(|r| r["lead_days"].as_i64() == Some(fit_lead)))
            .cloned();
        let Some(fitted) = fitted else {
            notes.push(format!(
                "{station} {date}: lead {lead} has no measured dispersion (archive covers 1-7). \
                 Not spawning — a guessed sd is what the measured fit exists to avoid."
            ));
            continue;
        };
        if sd_is_upper_bound {
            notes.push(format!(
                "{station} {date}: lead 0 (today). Using the lead-1 sd {} as an upper bound. \
                 Tighten it with weather_station_observation's running maximum — once the solar \
                 afternoon has passed the day's high is largely determined, which is the least \
                 model-dependent edge available.",
                fitted["predictive_sd"]
            ));
        }

        // Is the residual real, or sampling noise? Correcting for noise only
        // adds variance.
        let bias_sig = fit["per_lead_error"]
            .as_array()
            .and_then(|a| a.iter().find(|r| r["lead_days"].as_i64() == Some(fit_lead)))
            .and_then(|r| r["bias_is_significant"].as_bool())
            .unwrap_or(false);
        let (bias_p5, bias_p50, bias_p95) = if bias_sig {
            (
                f(&fitted["bias_p5"]),
                f(&fitted["bias_p50"]),
                f(&fitted["bias_p95"]),
            )
        } else {
            (0.0, 0.0, 0.0)
        };

        // ── 4. The live ladder ───────────────────────────────────────────
        let slug = format!(
            "highest-temperature-in-{}-on-{}",
            city.to_lowercase().replace(' ', "-"),
            chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                .map(|d| d.format("%B-%-d-%Y").to_string().to_lowercase())
                .unwrap_or_default()
        );
        let ladder = call("polymarket_weather_markets", json!({ "slug": slug })).await;

        let outcomes: Vec<Value> = match &ladder {
            Ok(v) => v["outcomes"].as_array().cloned().unwrap_or_default(),
            Err(e) => {
                notes.push(format!(
                    "{city}: no live ladder at slug '{slug}' ({e}). Spawning a synthetic \
                     ladder around the ensemble instead; verify the real slug before trading."
                ));
                Vec::new()
            }
        };

        // Build the bucket set: from the market if we have it, else synthesised
        // around the ensemble mean.
        let buckets: Vec<(String, f64, f64, Option<String>, Option<f64>)> = if outcomes.is_empty() {
            let centre = (ens_mean / step).round() * step;
            (-3..=3)
                .map(|k| {
                    let v = centre + k as f64 * step;
                    (
                        format!("{v:.0}"),
                        v - step / 2.0,
                        v + step / 2.0,
                        None,
                        None,
                    )
                })
                .collect()
        } else {
            outcomes
                .iter()
                .filter_map(|m| {
                    let q = m["question"].as_str()?;
                    let (label, lo, hi) = parse_bucket(q, step)?;
                    let token = m["clob_token_ids"][0].as_str().map(String::from);
                    let mid = m["best_bid"]
                        .as_f64()
                        .zip(m["best_ask"].as_f64())
                        .map(|(b, a)| (a + b) / 2.0)
                        .or_else(|| m["last_trade_price"].as_f64());
                    Some((label, lo, hi, token, mid))
                })
                .collect()
        };

        for (label, lo, hi, token, mkt) in buckets {
            // Model probability, from the same measured params the FPL will use.
            let sd = f(&fitted["predictive_sd"]);
            let centre = ens_mean + bias_p50;
            let z = |x: f64| 0.5 * (1.0 + erf((x - centre) / (sd * std::f64::consts::SQRT_2)));
            let p_model = (z(hi) - z(lo)).clamp(0.0, 1.0);
            let p_mkt = mkt.unwrap_or(0.0);

            // Neither side thinks this can happen: not worth a workspace.
            if p_model < 0.02 && p_mkt < 0.02 {
                skipped.push(json!({
                    "station": station, "date": date, "bucket": label,
                    "p_model": (p_model * 1e4).round() / 1e4,
                    "p_market": p_mkt,
                    "reason": "negligible on both model and market"
                }));
                continue;
            }

            instances.push(json!({
                "name": format!("Weather — {station} {date} bucket {label}"),
                "description": format!(
                    "P(daily max at {station_name} on {date} lands in bucket {label}). \
                     Lead {lead}d, measured predictive sd {sd:.3}{}. Settles via {source}.",
                    if bias_sig { format!(", bias {bias_p50:+.2}") } else { ", no significant bias".into() }
                ),
                "params": {
                    "program_type": "WEATHER_BUCKET",
                    "template": "templates/weather/bucket_ladder.fpl",

                    "station": station,
                    "station_name": station_name,
                    "market_date": date,
                    "timezone": tz,
                    "market_unit": unit,
                    "resolution_source": source,

                    "bucket_label": label,
                    "bucket_lo": lo,
                    "bucket_hi": hi,

                    "ens_mean": (ens_mean * 1e3).round() / 1e3,
                    "ens_sd": (ens_sd * 1e3).round() / 1e3,
                    "ens_mean_se": (ens_mean_se * 1e3).round() / 1e3,
                    "ens_n_members": n_members,
                    "lead_days": lead,

                    "predictive_sd": sd,
                    "sd_factor_p5": fitted["predictive_sd_factor_p5"],
                    "sd_factor_p50": fitted["predictive_sd_factor_p50"],
                    "sd_factor_p95": fitted["predictive_sd_factor_p95"],
                    "bias_p5": bias_p5,
                    "bias_p50": bias_p50,
                    "bias_p95": bias_p95,

                    // Not read by the FPL. Carried so the workspace can be
                    // priced and scored without re-fetching.
                    "clob_token_id": token,
                    "market_mid": mkt,
                    "model_prob_at_spawn": (p_model * 1e4).round() / 1e4,
                    "bias_is_significant": bias_sig,
                    "sd_is_upper_bound": sd_is_upper_bound,
                    "fit_lead_used": fit_lead
                }
            }));
        }
    }

    let plan = json!({
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "target_date": date,
        "cities": cities,
        "instance_count": instances.len(),
        "instances": instances,
        "skipped": skipped,
        "notes": notes,
        "reminder": "Per-market Kelly across these is WRONG — weather positions are strongly \
                     correlated. Run weather_portfolio_risk over the station set and apply the \
                     reported haircut before sizing."
    });
    println!("{}", serde_json::to_string_pretty(&plan).unwrap());

    eprintln!("── spawn plan ──────────────────────────────");
    eprintln!(
        "  {} instances across {} cities",
        plan["instance_count"],
        cities.len()
    );
    eprintln!(
        "  {} buckets skipped as negligible",
        plan["skipped"].as_array().map(|a| a.len()).unwrap_or(0)
    );
    for n in plan["notes"].as_array().unwrap_or(&vec![]) {
        eprintln!("  ! {}", n.as_str().unwrap_or(""));
    }
}

/// Abramowitz & Stegun 7.1.26. Plenty accurate for a spawn-time preview; the
/// authoritative probability comes from the FPL Monte Carlo run.
fn erf(x: f64) -> f64 {
    let s = x.signum();
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-x * x).exp();
    s * y
}
