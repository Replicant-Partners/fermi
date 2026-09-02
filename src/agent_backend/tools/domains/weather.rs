// src/agent_backend/tools/domains/weather.rs
//
// Phase 2 domain migration: Weather tools.
//
// Nine tools:
//   openweather_forecast        — requires_workspace: false
//   weather_settlement_spec     — requires_workspace: false
//   weather_ensemble_forecast   — requires_workspace: false, response_shape declared
//   weather_climatology         — requires_workspace: false, response_shape declared
//   weather_dispersion_fit      — requires_workspace: false
//   weather_station_observation — requires_workspace: false, response_shape declared
//   weather_portfolio_risk      — requires_workspace: false
//   polymarket_weather_markets  — requires_workspace: false
//   polymarket_orderbook        — requires_workspace: false, response_shape declared
//
// openweather_forecast is also handled in tools_legacy.rs for backward
// compatibility. The remaining eight wrap weather_tools::dispatch() via the
// standard ToolRegistry delegation path.
//
// Tools with a confirmed response_shape() call response_for(self.name()).
// The others rely on the trait default (None).

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;
use crate::tool_response_shapes::{response_for, ToolResponse};

/// All Weather-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![
        Arc::new(OpenweatherForecast),
        Arc::new(WeatherSettlementSpec),
        Arc::new(WeatherEnsembleForecast),
        Arc::new(WeatherClimatology),
        Arc::new(WeatherDispersionFit),
        Arc::new(WeatherStationObservation),
        Arc::new(WeatherPortfolioRisk),
        Arc::new(PolymarketWeatherMarkets),
        Arc::new(PolymarketOrderbook),
    ]
}

// ─── openweather_forecast ─────────────────────────────────────────────────────

struct OpenweatherForecast;

#[async_trait]
impl PlatformTool for OpenweatherForecast {
    fn name(&self) -> &'static str {
        "openweather_forecast"
    }

    fn description(&self) -> &'static str {
        "Call this tool to get current weather conditions and 5-day forecast for a location. Server-side — requires OPENWEATHER_API_KEY. Returns temperature, humidity, precipitation, wind, and 5-day outlook. Use for microclimate foraging condition assessment."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "lat": {
                    "type": "number",
                    "description": "Latitude"
                },
                "lng": {
                    "type": "number",
                    "description": "Longitude"
                },
                "include_forecast": {
                    "type": "boolean",
                    "description": "Include 5-day forecast in addition to current conditions (default: true)",
                    "default": true
                }
            },
            "required": ["lat", "lng"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Weather
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        do_openweather_forecast(input).await
    }
}

// ─── weather_settlement_spec ──────────────────────────────────────────────────

struct WeatherSettlementSpec;

#[async_trait]
impl PlatformTool for WeatherSettlementSpec {
    fn name(&self) -> &'static str {
        "weather_settlement_spec"
    }

    fn description(&self) -> &'static str {
        "Return the official settlement specification for a named Polymarket weather market: station identity, timezone, measurement unit, and settlement rounding rule. Must be called before any forecast to confirm which physical station defines resolution."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "market_id": {
                    "type": "string",
                    "description": "Polymarket market ID or slug, e.g. 'high-temp-nyc-jan-15-2026'"
                }
            },
            "required": ["market_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Weather
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::weather_tools::dispatch(self.name(), input)
            .await
            .unwrap_or_else(|| Err(format!("weather tool '{}' not found in dispatch", self.name())))
    }
}

// ─── weather_ensemble_forecast ────────────────────────────────────────────────

struct WeatherEnsembleForecast;

#[async_trait]
impl PlatformTool for WeatherEnsembleForecast {
    fn name(&self) -> &'static str {
        "weather_ensemble_forecast"
    }

    fn description(&self) -> &'static str {
        "Fetch a 5-model ensemble weather forecast (GFS, ECMWF, ICON, Gemini AI, UKMET) for a specific station and date. Returns per-model and ensemble-mean forecasts with cross-model epistemic disagreement. CRITICAL: always read `calibration_required` in the response — these figures require calibration before trading."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "icao": {
                    "type": "string",
                    "description": "ICAO station code, e.g. 'KLGA' for LaGuardia"
                },
                "target_date": {
                    "type": "string",
                    "description": "Settlement date in YYYY-MM-DD format"
                },
                "metric": {
                    "type": "string",
                    "enum": ["high_temp_f", "low_temp_f", "precip_inch", "high_temp_c", "low_temp_c"],
                    "description": "Weather metric to forecast"
                }
            },
            "required": ["icao", "target_date", "metric"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Weather
    }

    fn response_shape(&self) -> Option<&'static ToolResponse> {
        response_for(self.name())
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::weather_tools::dispatch(self.name(), input)
            .await
            .unwrap_or_else(|| Err(format!("weather tool '{}' not found in dispatch", self.name())))
    }
}

// ─── weather_climatology ──────────────────────────────────────────────────────

struct WeatherClimatology;

#[async_trait]
impl PlatformTool for WeatherClimatology {
    fn name(&self) -> &'static str {
        "weather_climatology"
    }

    fn description(&self) -> &'static str {
        "Fetch climatological base rate for a station/date/metric from ERA5 reanalysis (1985-2023). Returns raw base rate, trend-adjusted base rate, warming trend per decade, and observation count. Use as the reference probability for Brier Skill Score computation."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "icao": {
                    "type": "string",
                    "description": "ICAO station code"
                },
                "target_date": {
                    "type": "string",
                    "description": "Settlement date in YYYY-MM-DD format"
                },
                "metric": {
                    "type": "string",
                    "enum": ["high_temp_f", "low_temp_f", "precip_inch", "high_temp_c", "low_temp_c"]
                },
                "threshold": {
                    "type": "number",
                    "description": "Temperature or precipitation threshold"
                }
            },
            "required": ["icao", "target_date", "metric", "threshold"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Weather
    }

    fn response_shape(&self) -> Option<&'static ToolResponse> {
        response_for(self.name())
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::weather_tools::dispatch(self.name(), input)
            .await
            .unwrap_or_else(|| Err(format!("weather tool '{}' not found in dispatch", self.name())))
    }
}

// ─── weather_dispersion_fit ───────────────────────────────────────────────────

struct WeatherDispersionFit;

#[async_trait]
impl PlatformTool for WeatherDispersionFit {
    fn name(&self) -> &'static str {
        "weather_dispersion_fit"
    }

    fn description(&self) -> &'static str {
        "Fit a parametric probability distribution to the ensemble forecast output. Returns distribution parameters, P(X > threshold), and confidence intervals. Use for market pricing rather than the raw ensemble probabilities."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "ensemble_output": {
                    "type": "object",
                    "description": "Output from weather_ensemble_forecast"
                },
                "threshold": {
                    "type": "number",
                    "description": "Threshold value for P(X > threshold) computation"
                },
                "distribution": {
                    "type": "string",
                    "enum": ["normal", "lognormal", "beta"],
                    "description": "Distribution family to fit",
                    "default": "normal"
                }
            },
            "required": ["ensemble_output", "threshold"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Weather
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::weather_tools::dispatch(self.name(), input)
            .await
            .unwrap_or_else(|| Err(format!("weather tool '{}' not found in dispatch", self.name())))
    }
}

// ─── weather_station_observation ──────────────────────────────────────────────

struct WeatherStationObservation;

#[async_trait]
impl PlatformTool for WeatherStationObservation {
    fn name(&self) -> &'static str {
        "weather_station_observation"
    }

    fn description(&self) -> &'static str {
        "Fetch real-time or near-real-time station observations from NWS/ASOS for the settlement station. Returns current conditions, running max/min for today, and precipitation accumulation. Available only for US stations covered by NWS API."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "icao": {
                    "type": "string",
                    "description": "ICAO station code (US NWS stations only)"
                },
                "hours_back": {
                    "type": "integer",
                    "description": "Hours of observation history to fetch (default: 24, max: 72)",
                    "default": 24
                }
            },
            "required": ["icao"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Weather
    }

    fn response_shape(&self) -> Option<&'static ToolResponse> {
        response_for(self.name())
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::weather_tools::dispatch(self.name(), input)
            .await
            .unwrap_or_else(|| Err(format!("weather tool '{}' not found in dispatch", self.name())))
    }
}

// ─── weather_portfolio_risk ───────────────────────────────────────────────────

struct WeatherPortfolioRisk;

#[async_trait]
impl PlatformTool for WeatherPortfolioRisk {
    fn name(&self) -> &'static str {
        "weather_portfolio_risk"
    }

    fn description(&self) -> &'static str {
        "Compute cross-market correlation and portfolio risk metrics for a basket of weather positions. Returns correlation matrix, portfolio variance, and per-position contribution to tail risk. Use before sizing positions in correlated markets."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "positions": {
                    "type": "array",
                    "description": "Array of {market_id, position_size, direction} objects",
                    "items": {
                        "type": "object",
                        "properties": {
                            "market_id": {"type": "string"},
                            "position_size": {"type": "number"},
                            "direction": {"type": "string", "enum": ["yes", "no"]}
                        }
                    }
                },
                "correlation_lookback_days": {
                    "type": "integer",
                    "description": "Days of settlement history for correlation estimation",
                    "default": 30
                }
            },
            "required": ["positions"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Weather
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::weather_tools::dispatch(self.name(), input)
            .await
            .unwrap_or_else(|| Err(format!("weather tool '{}' not found in dispatch", self.name())))
    }
}

// ─── polymarket_weather_markets ───────────────────────────────────────────────

struct PolymarketWeatherMarkets;

#[async_trait]
impl PlatformTool for PolymarketWeatherMarkets {
    fn name(&self) -> &'static str {
        "polymarket_weather_markets"
    }

    fn description(&self) -> &'static str {
        "Fetch active Polymarket weather markets and their current prices. Filters by city or metric type. Returns market IDs, settlement specs, current YES prices, and volume."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "city": {
                    "type": "string",
                    "description": "City filter, e.g. 'NYC', 'Chicago'. Omit for all cities."
                },
                "metric": {
                    "type": "string",
                    "enum": ["temperature", "precipitation"],
                    "description": "Metric filter. Omit for all metrics."
                },
                "limit": {
                    "type": "integer",
                    "description": "Max markets to return (default: 20)",
                    "default": 20
                }
            },
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Weather
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::weather_tools::dispatch(self.name(), input)
            .await
            .unwrap_or_else(|| Err(format!("weather tool '{}' not found in dispatch", self.name())))
    }
}

// ─── polymarket_orderbook ─────────────────────────────────────────────────────

struct PolymarketOrderbook;

#[async_trait]
impl PlatformTool for PolymarketOrderbook {
    fn name(&self) -> &'static str {
        "polymarket_orderbook"
    }

    fn description(&self) -> &'static str {
        "Fetch the live Polymarket CLOB orderbook for a market token. Returns best bid/ask, spread, midpoint, implied probability, and book quality flags. Use for execution pricing and liquidity assessment."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "token_id": {
                    "type": "string",
                    "description": "Polymarket CLOB token ID (the YES or NO token address)"
                },
                "depth": {
                    "type": "integer",
                    "description": "Orderbook depth levels to fetch (default: 10)",
                    "default": 10
                }
            },
            "required": ["token_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Weather
    }

    fn response_shape(&self) -> Option<&'static ToolResponse> {
        response_for(self.name())
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::weather_tools::dispatch(self.name(), input)
            .await
            .unwrap_or_else(|| Err(format!("weather tool '{}' not found in dispatch", self.name())))
    }
}

// ─── Private helpers ─────────────────────────────────────────────────────────

async fn do_openweather_forecast(input: &Value) -> Result<String, String> {
    let lat = input
        .get("lat")
        .and_then(|v| v.as_f64())
        .ok_or("lat is required")?;
    let lng = input
        .get("lng")
        .and_then(|v| v.as_f64())
        .ok_or("lng is required")?;
    let include_forecast = input
        .get("include_forecast")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let api_key = std::env::var("OPENWEATHER_API_KEY").map_err(|_| {
        "OPENWEATHER_API_KEY not set. Get a free key at https://openweathermap.org/api".to_string()
    })?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();

    // Current conditions
    let current_resp = client
        .get("https://api.openweathermap.org/data/2.5/weather")
        .query(&[
            ("lat", lat.to_string()),
            ("lon", lng.to_string()),
            ("appid", api_key.clone()),
            ("units", "metric".to_string()),
        ])
        .send()
        .await
        .map_err(|e| format!("OpenWeather current request failed: {}", e))?;

    if !current_resp.status().is_success() {
        return Err(format!("OpenWeather API error: {}", current_resp.status()));
    }

    let current: serde_json::Value = current_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse current weather: {}", e))?;

    let current_summary = json!({
        "temp_c": current.pointer("/main/temp"),
        "feels_like_c": current.pointer("/main/feels_like"),
        "humidity_pct": current.pointer("/main/humidity"),
        "pressure_hpa": current.pointer("/main/pressure"),
        "description": current.pointer("/weather/0/description"),
        "wind_speed_ms": current.pointer("/wind/speed"),
        "wind_direction_deg": current.pointer("/wind/deg"),
        "rain_1h_mm": current.pointer("/rain/1h").unwrap_or(&serde_json::Value::Null),
        "clouds_pct": current.pointer("/clouds/all"),
        "visibility_m": current.get("visibility"),
        "sunrise": current.pointer("/sys/sunrise"),
        "sunset": current.pointer("/sys/sunset"),
    });

    if !include_forecast {
        return serde_json::to_string_pretty(&json!({
            "location": { "lat": lat, "lng": lng },
            "current": current_summary,
        }))
        .map_err(|e| format!("Serialization error: {}", e));
    }

    // 5-day / 3-hour forecast
    let forecast_resp = client
        .get("https://api.openweathermap.org/data/2.5/forecast")
        .query(&[
            ("lat", lat.to_string()),
            ("lon", lng.to_string()),
            ("appid", api_key),
            ("units", "metric".to_string()),
            ("cnt", "40".to_string()), // 5 days × 8 readings/day
        ])
        .send()
        .await
        .map_err(|e| format!("OpenWeather forecast request failed: {}", e))?;

    let forecast: serde_json::Value = forecast_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse forecast: {}", e))?;

    // Summarise by day
    let mut daily: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    if let Some(list) = forecast.get("list").and_then(|v| v.as_array()) {
        for entry in list {
            let dt_txt = entry.get("dt_txt").and_then(|v| v.as_str()).unwrap_or("");
            let day = dt_txt.split(' ').next().unwrap_or(dt_txt).to_string();
            let temp = entry
                .pointer("/main/temp")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let rain = entry
                .pointer("/rain/3h")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let humidity = entry
                .pointer("/main/humidity")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            let d = daily.entry(day).or_insert(json!({
                "temps": [], "rain_total_mm": 0.0, "humidity_avg": 0.0, "count": 0
            }));
            if let Some(obj) = d.as_object_mut() {
                if let Some(arr) = obj.get_mut("temps").and_then(|v| v.as_array_mut()) {
                    arr.push(json!(temp));
                }
                let rain_total = obj
                    .get("rain_total_mm")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let hum_acc = obj
                    .get("humidity_avg")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let count = obj.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                obj.insert("rain_total_mm".to_string(), json!(rain_total + rain));
                obj.insert(
                    "humidity_avg".to_string(),
                    json!((hum_acc * count as f64 + humidity) / (count + 1) as f64),
                );
                obj.insert("count".to_string(), json!(count + 1));
            }
        }
    }

    let forecast_summary: Vec<serde_json::Value> = daily
        .iter()
        .map(|(day, d)| {
            let temps = d
                .get("temps")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let temps_f: Vec<f64> = temps.iter().filter_map(|v| v.as_f64()).collect();
            let min_t = temps_f.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_t = temps_f.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            json!({
                "date": day,
                "temp_min_c": if min_t.is_finite() { min_t } else { 0.0 },
                "temp_max_c": if max_t.is_finite() { max_t } else { 0.0 },
                "rain_total_mm": d.get("rain_total_mm"),
                "humidity_avg_pct": d.get("humidity_avg"),
            })
        })
        .collect();

    // Foraging condition assessment
    let current_temp = current
        .pointer("/main/temp")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let current_humidity = current
        .pointer("/main/humidity")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let recent_rain: f64 = forecast_summary
        .iter()
        .take(2)
        .filter_map(|d| d.get("rain_total_mm").and_then(|v| v.as_f64()))
        .sum();

    let foraging_signal = if current_temp > 5.0
        && current_temp < 25.0
        && current_humidity > 70.0
        && recent_rain > 5.0
    {
        "good"
    } else if current_temp > 0.0 && current_temp < 30.0 && current_humidity > 50.0 {
        "fair"
    } else {
        "poor"
    };

    serde_json::to_string_pretty(&json!({
        "location": { "lat": lat, "lng": lng },
        "current": current_summary,
        "forecast_5day": forecast_summary,
        "foraging_conditions": {
            "signal": foraging_signal,
            "temp_in_range": current_temp > 5.0 && current_temp < 25.0,
            "humidity_sufficient": current_humidity > 70.0,
            "recent_rainfall_mm": recent_rain,
            "note": match foraging_signal {
                "good" => "Conditions are favourable for fungal fruiting. Scout within 1-4 days.",
                "fair" => "Conditions are marginal. Check specific species requirements.",
                _ => "Conditions are unfavourable. Wait for rain and temperature moderation.",
            }
        }
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_names_are_dispatchable() {
        for tool in tools() {
            assert!(!tool.name().is_empty(), "tool has empty name");
        }
    }

    #[test]
    fn all_categories_are_weather() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Weather,
                "tool `{}` has wrong category",
                tool.name()
            );
        }
    }

    #[test]
    fn input_schemas_are_objects() {
        for tool in tools() {
            let schema = tool.input_schema();
            assert_eq!(
                schema["type"],
                "object",
                "tool `{}` input_schema missing \"type\": \"object\"",
                tool.name()
            );
        }
    }

    #[test]
    fn tool_count_is_nine() {
        assert_eq!(tools().len(), 9);
    }

    #[test]
    fn no_tool_requires_workspace() {
        for tool in tools() {
            assert!(
                !tool.requires_workspace(),
                "tool `{}` should NOT require workspace",
                tool.name()
            );
        }
    }

    #[test]
    fn response_shape_tools_are_declared() {
        let with_shapes = [
            "weather_ensemble_forecast",
            "weather_climatology",
            "weather_station_observation",
            "polymarket_orderbook",
        ];
        for tool in tools() {
            let has_shape = tool.response_shape().is_some();
            let expected = with_shapes.contains(&tool.name());
            assert_eq!(
                has_shape,
                expected,
                "tool `{}`: response_shape presence mismatch (expected: {expected})",
                tool.name()
            );
        }
    }
}
