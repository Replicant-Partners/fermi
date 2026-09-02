// src/agent_backend/tools/domains/sports.rs
//
// Phase 2 domain migration: Sports tools.
//
// One tool: call_football_api.
//
// The struct is zero-size; execute() delegates to ToolRegistry::standard()
// so that dispatch semantics are identical to the pre-migration path.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;

/// All Sports-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![Arc::new(CallFootballApi)]
}

// ─── call_football_api ───────────────────────────────────────────────────────

struct CallFootballApi;

#[async_trait]
impl PlatformTool for CallFootballApi {
    fn name(&self) -> &'static str {
        "call_football_api"
    }

    fn description(&self) -> &'static str {
        "Call the API-Football v3 REST API (api-football.com) to get live football/soccer data. Returns current standings, fixtures, results, team stats, player stats, injuries, lineups, head-to-head records, and match predictions. Requires FOOTBALL_API_KEY environment variable."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "endpoint": {
                    "type": "string",
                    "description": "API endpoint path (without leading slash). Examples: 'standings', 'fixtures', 'teams/statistics', 'players/topscorers', 'injuries', 'predictions', 'fixtures/headtohead', 'fixtures/statistics', 'fixtures/events', 'fixtures/lineups', 'players', 'leagues'"
                },
                "params": {
                    "type": "object",
                    "description": "Query parameters as key-value pairs. Common params: league (league ID), season (e.g. 2025), team (team ID), fixture (fixture ID), date (YYYY-MM-DD), from/to (date range), last (last N fixtures), next (next N fixtures), player (player ID). Example for PL standings: {\"league\": 39, \"season\": 2025}"
                }
            },
            "required": ["endpoint"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Sports
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        execute_call_football_api(input).await
    }
}

// ─── Private helpers ─────────────────────────────────────────────────────────

pub(crate) async fn execute_call_football_api(input: &Value) -> Result<String, String> {
    let endpoint = input
        .get("endpoint")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: endpoint")?
        .trim_start_matches('/');

    let api_key = std::env::var("FOOTBALL_API_KEY")
        .map_err(|_| "FOOTBALL_API_KEY environment variable not set.".to_string())?;

    let client = reqwest::Client::new();
    let url = format!("https://v3.football.api-sports.io/{}", endpoint);

    let mut req = client
        .get(&url)
        .header("x-apisports-key", &api_key)
        .header("Accept", "application/json");

    // Apply query params from the `params` object
    if let Some(params) = input.get("params").and_then(|v| v.as_object()) {
        let query: Vec<(String, String)> = params
            .iter()
            .map(|(k, v)| {
                let val = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                (k.clone(), val)
            })
            .collect();
        req = req.query(&query);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("API-Football request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("API-Football error {}: {}", status, body));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse API-Football response: {}", e))?;

    // Check API-level errors
    if let Some(errors) = data.get("errors") {
        if !errors.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            return Err(format!("API-Football errors: {}", errors));
        }
    }

    // Return the response, truncated if very large
    let result =
        serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))?;

    if result.len() > 16000 {
        Ok(format!(
            "{}... [truncated, {} total chars]",
            &result[..16000],
            result.len()
        ))
    } else {
        Ok(result)
    }
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
    fn all_categories_are_sports() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Sports,
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
    fn tool_count_is_one() {
        assert_eq!(tools().len(), 1);
    }
}
