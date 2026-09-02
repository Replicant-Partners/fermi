// src/agent_backend/tools/domains/prediction_market.rs
//
// Phase 2 domain migration: PredictionMarket tools.
//
// Two tools, both requires_workspace: false:
//   polymarket_search
//   polymarket_event
//
// Each is a zero-size struct implementing PlatformTool. execute() delegates
// to the legacy ToolRegistry::standard() so that dispatch semantics are
// identical to the pre-migration path.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;

/// All PredictionMarket-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![Arc::new(PolymarketSearch), Arc::new(PolymarketEvent)]
}

// ─── polymarket_search ────────────────────────────────────────────────────────

struct PolymarketSearch;

#[async_trait]
impl PlatformTool for PolymarketSearch {
    fn name(&self) -> &'static str {
        "polymarket_search"
    }

    fn description(&self) -> &'static str {
        "Search Polymarket for prediction market events matching a query. Returns markets with titles, current prices, volume, and resolution criteria."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query for market topics"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results to return (default: 10)",
                    "default": 10
                }
            },
            "required": ["query"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::PredictionMarket
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        do_polymarket_search(input).await
    }
}

// ─── polymarket_event ─────────────────────────────────────────────────────────

struct PolymarketEvent;

#[async_trait]
impl PlatformTool for PolymarketEvent {
    fn name(&self) -> &'static str {
        "polymarket_event"
    }

    fn description(&self) -> &'static str {
        "Get detailed information about a specific Polymarket event including all markets, current prices, volume, and resolution criteria."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "event_id": {
                    "type": "string",
                    "description": "Polymarket event ID or slug"
                }
            },
            "required": ["event_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::PredictionMarket
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        do_polymarket_event(input).await
    }
}

// ─── Private helpers ─────────────────────────────────────────────────────────

async fn do_polymarket_search(input: &Value) -> Result<String, String> {
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: query")?;
    let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

    let gamma = crate::polymarket::GammaClient::new();
    let events = gamma
        .search_events(query, limit)
        .await
        .map_err(|e| format!("Polymarket search failed: {}", e))?;

    if events.is_empty() {
        return Ok("No matching Polymarket markets found for this query.".to_string());
    }

    let mut output = String::new();
    for event in &events {
        output.push_str(&format!("## {}\n", event.title));
        output.push_str(&format!(
            "Event ID: {} | Volume 24h: ${:.0} | Liquidity: ${:.0}\n",
            event.id, event.volume_24hr, event.liquidity
        ));
        if let Some(ref end) = event.end_date {
            output.push_str(&format!("End date: {}\n", end));
        }
        for market in &event.markets {
            let processed = crate::polymarket::process_market_public(event, market);
            output.push_str(&format!(
                "  → {} | YES: {:.1}% | bid/ask: {:.3}/{:.3} | vol24h: ${:.0} | confidence: {}\n",
                processed.question,
                processed.market_price * 100.0,
                processed.bid_price,
                processed.ask_price,
                processed.volume_24h,
                processed.confidence_signal.label(),
            ));
            if let Some(ref change) = processed.price_change_1w {
                output.push_str(&format!("    1-week change: {:+.1}pp\n", change * 100.0));
            }
        }
        output.push('\n');
    }

    // Truncate if very large
    if output.len() > 24_000 {
        output.truncate(24_000);
        output.push_str("\n... [truncated]");
    }

    Ok(output)
}

async fn do_polymarket_event(input: &Value) -> Result<String, String> {
    let event_id = input
        .get("event_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: event_id")?;

    let gamma = crate::polymarket::GammaClient::new();
    let event = gamma
        .get_event(event_id)
        .await
        .map_err(|e| format!("Polymarket event fetch failed: {}", e))?;

    let mut output = String::new();
    output.push_str(&format!("# {}\n\n", event.title));
    output.push_str(&format!(
        "Description: {}\n\n",
        &event.description[..event.description.len().min(500)]
    ));
    output.push_str(&format!(
        "Total volume: ${:.0} | 24h volume: ${:.0} | Liquidity: ${:.0}\n",
        event.volume, event.volume_24hr, event.liquidity
    ));
    if let Some(ref end) = event.end_date {
        output.push_str(&format!("End date: {}\n", end));
    }
    output.push_str(&format!(
        "Active: {} | Closed: {}\n\n",
        event.active, event.closed
    ));

    output.push_str("## Markets\n\n");
    for market in &event.markets {
        let processed = crate::polymarket::process_market_public(&event, market);
        output.push_str(&format!("### {}\n", processed.question));
        output.push_str(&format!("  Market ID: {}\n", processed.pm_market_id));
        output.push_str(&format!(
            "  YES price: {:.1}% (midpoint: {:.1}%)\n",
            processed.market_price * 100.0,
            processed.midpoint_price * 100.0
        ));
        output.push_str(&format!(
            "  Bid/Ask: {:.3} / {:.3} (spread: {:.3})\n",
            processed.bid_price, processed.ask_price, processed.spread
        ));
        output.push_str(&format!(
            "  Volume 24h: ${:.0} | Total: ${:.0}\n",
            processed.volume_24h, processed.volume_total
        ));
        output.push_str(&format!("  Liquidity: ${:.0}\n", processed.liquidity));
        output.push_str(&format!(
            "  Confidence: {} ({:.0}% quality)\n",
            processed.confidence_signal.label(),
            processed.confidence_signal.quality_score() * 100.0
        ));
        if let Some(change) = processed.price_change_1w {
            output.push_str(&format!(
                "  1-week price change: {:+.1}pp\n",
                change * 100.0
            ));
        }
        if let Some(change) = processed.price_change_1m {
            output.push_str(&format!(
                "  1-month price change: {:+.1}pp\n",
                change * 100.0
            ));
        }
        output.push_str(&format!(
            "  Status: {}\n",
            if processed.resolved {
                "RESOLVED"
            } else if processed.closed {
                "CLOSED"
            } else if processed.active {
                "ACTIVE"
            } else {
                "INACTIVE"
            }
        ));
        if let Some(ref group) = processed.group_item_title {
            output.push_str(&format!("  Group: {}\n", group));
        }
        output.push('\n');
    }

    output.push_str(&format!(
        "Tags: {}\n",
        event
            .tags
            .iter()
            .map(|t| t.label.clone())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    output.push_str(&format!(
        "URL: https://polymarket.com/event/{}\n",
        event.slug
    ));

    Ok(output)
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
    fn all_categories_are_prediction_market() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::PredictionMarket,
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
    fn tool_count_is_two() {
        assert_eq!(tools().len(), 2);
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
}
