// src/agent_backend/tools/domains/financial.rs
//
// Phase 4 migration — Financial domain tools.
//
// All nine FMP tools call the private `do_fmp_api` helper directly instead
// of delegating through the legacy ToolRegistry dispatch path.
// Five declare confirmed response shapes (see TOOL_RESPONSES in
// src/tool_response_shapes.rs); the remaining four rely on the trait default
// (None), meaning the contract builder falls back to description extraction
// and marks those fields unconfirmed.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;
use crate::tool_response_shapes::{response_for, ToolResponse};

/// All Financial-category tools in declaration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![
        Arc::new(FmpCompanyProfile),
        Arc::new(FmpIncomeStatement),
        Arc::new(FmpBalanceSheet),
        Arc::new(FmpCashFlow),
        Arc::new(FmpRatios),
        Arc::new(FmpKeyMetrics),
        Arc::new(FmpDcf),
        Arc::new(FmpAnalystEstimates),
        Arc::new(FmpHistoricalPrice),
    ]
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Symbol-only input schema (fmp_company_profile, fmp_dcf).
fn symbol_only_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "symbol": {
                "type": "string",
                "description": "Stock ticker symbol (e.g., AAPL, MSFT, GOOGL, TSLA)"
            }
        },
        "required": ["symbol"]
    })
}

/// Symbol + period + limit input schema (income statement, balance sheet,
/// cash flow, ratios, key metrics).
fn symbol_period_limit_schema(limit_default: u32) -> Value {
    json!({
        "type": "object",
        "properties": {
            "symbol": {
                "type": "string",
                "description": "Stock ticker symbol"
            },
            "period": {
                "type": "string",
                "enum": ["annual", "quarter"],
                "description": "Reporting period (annual or quarter)"
            },
            "limit": {
                "type": "integer",
                "description": "Number of periods to return",
                "default": limit_default
            }
        },
        "required": ["symbol", "period"]
    })
}

// ── 1. fmp_company_profile ────────────────────────────────────────────────────

struct FmpCompanyProfile;

#[async_trait]
impl PlatformTool for FmpCompanyProfile {
    fn name(&self) -> &'static str {
        "fmp_company_profile"
    }

    fn description(&self) -> &'static str {
        "Get company profile including price, market cap, sector, industry, beta, \
         52-week range, CEO, description. Use this first to identify the company \
         and get current market data."
    }

    fn input_schema(&self) -> Value {
        symbol_only_schema()
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Financial
    }

    fn required_credential(&self) -> Option<&'static str> {
        Some("FMP_API_KEY")
    }

    fn response_shape(&self) -> Option<&'static ToolResponse> {
        response_for(self.name())
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        do_fmp_api(input, "/stable/profile", &["symbol"]).await
    }
}

// ── 2. fmp_income_statement ───────────────────────────────────────────────────

struct FmpIncomeStatement;

#[async_trait]
impl PlatformTool for FmpIncomeStatement {
    fn name(&self) -> &'static str {
        "fmp_income_statement"
    }

    fn description(&self) -> &'static str {
        "Get income statement data: revenue, gross profit, operating income, net income, \
         EPS, EBITDA. Essential for growth analysis and profitability assessment."
    }

    fn input_schema(&self) -> Value {
        symbol_period_limit_schema(3)
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Financial
    }

    fn required_credential(&self) -> Option<&'static str> {
        Some("FMP_API_KEY")
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        do_fmp_api(
            input,
            "/stable/income-statement",
            &["symbol", "period", "limit"],
        )
        .await
    }
}

// ── 3. fmp_balance_sheet ──────────────────────────────────────────────────────

struct FmpBalanceSheet;

#[async_trait]
impl PlatformTool for FmpBalanceSheet {
    fn name(&self) -> &'static str {
        "fmp_balance_sheet"
    }

    fn description(&self) -> &'static str {
        "Get balance sheet data: assets, liabilities, equity, cash, debt, inventory. \
         Essential for financial health and leverage analysis."
    }

    fn input_schema(&self) -> Value {
        symbol_period_limit_schema(3)
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Financial
    }

    fn required_credential(&self) -> Option<&'static str> {
        Some("FMP_API_KEY")
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        do_fmp_api(
            input,
            "/stable/balance-sheet-statement",
            &["symbol", "period", "limit"],
        )
        .await
    }
}

// ── 4. fmp_cash_flow ──────────────────────────────────────────────────────────

struct FmpCashFlow;

#[async_trait]
impl PlatformTool for FmpCashFlow {
    fn name(&self) -> &'static str {
        "fmp_cash_flow"
    }

    fn description(&self) -> &'static str {
        "Get cash flow statement: operating cash flow, capex, free cash flow, buybacks, \
         dividends. Essential for cash generation quality and capital allocation analysis."
    }

    fn input_schema(&self) -> Value {
        symbol_period_limit_schema(3)
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Financial
    }

    fn required_credential(&self) -> Option<&'static str> {
        Some("FMP_API_KEY")
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        do_fmp_api(
            input,
            "/stable/cash-flow-statement",
            &["symbol", "period", "limit"],
        )
        .await
    }
}

// ── 5. fmp_ratios ─────────────────────────────────────────────────────────────

struct FmpRatios;

#[async_trait]
impl PlatformTool for FmpRatios {
    fn name(&self) -> &'static str {
        "fmp_ratios"
    }

    fn description(&self) -> &'static str {
        "Get pre-calculated financial ratios: profitability margins, liquidity ratios, \
         leverage ratios, valuation multiples (P/E, P/B, P/S, EV/EBITDA, PEG), \
         efficiency ratios, dividend yield."
    }

    fn input_schema(&self) -> Value {
        symbol_period_limit_schema(3)
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Financial
    }

    fn required_credential(&self) -> Option<&'static str> {
        Some("FMP_API_KEY")
    }

    fn response_shape(&self) -> Option<&'static ToolResponse> {
        response_for(self.name())
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        do_fmp_api(input, "/stable/ratios", &["symbol", "period", "limit"]).await
    }
}

// ── 6. fmp_key_metrics ────────────────────────────────────────────────────────

struct FmpKeyMetrics;

#[async_trait]
impl PlatformTool for FmpKeyMetrics {
    fn name(&self) -> &'static str {
        "fmp_key_metrics"
    }

    fn description(&self) -> &'static str {
        "Get key financial metrics: market cap, enterprise value, EV/EBITDA, EV/Sales, \
         ROE, ROA, ROIC, FCF yield, debt/equity, earnings yield, book value per share, \
         Graham number."
    }

    fn input_schema(&self) -> Value {
        symbol_period_limit_schema(3)
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Financial
    }

    fn required_credential(&self) -> Option<&'static str> {
        Some("FMP_API_KEY")
    }

    fn response_shape(&self) -> Option<&'static ToolResponse> {
        response_for(self.name())
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        do_fmp_api(input, "/stable/key-metrics", &["symbol", "period", "limit"]).await
    }
}

// ── 7. fmp_dcf ────────────────────────────────────────────────────────────────

struct FmpDcf;

#[async_trait]
impl PlatformTool for FmpDcf {
    fn name(&self) -> &'static str {
        "fmp_dcf"
    }

    fn description(&self) -> &'static str {
        "Get discounted cash flow (DCF) intrinsic value estimate vs current stock price. \
         Shows whether the stock is over- or under-valued based on fundamental analysis."
    }

    fn input_schema(&self) -> Value {
        symbol_only_schema()
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Financial
    }

    fn required_credential(&self) -> Option<&'static str> {
        Some("FMP_API_KEY")
    }

    fn response_shape(&self) -> Option<&'static ToolResponse> {
        response_for(self.name())
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        do_fmp_api(input, "/stable/discounted-cash-flow", &["symbol"]).await
    }
}

// ── 8. fmp_analyst_estimates ──────────────────────────────────────────────────

struct FmpAnalystEstimates;

#[async_trait]
impl PlatformTool for FmpAnalystEstimates {
    fn name(&self) -> &'static str {
        "fmp_analyst_estimates"
    }

    fn description(&self) -> &'static str {
        "Get Wall Street analyst consensus estimates: revenue, EBITDA, EBIT, net income, \
         EPS (low/avg/high) with number of analysts. Forward-looking data for 1-5 years."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "Stock ticker symbol"
                },
                "period": {
                    "type": "string",
                    "enum": ["annual", "quarter"],
                    "description": "Reporting period"
                },
                "limit": {
                    "type": "integer",
                    "description": "Number of estimate periods to return",
                    "default": 5
                }
            },
            "required": ["symbol", "period"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Financial
    }

    fn required_credential(&self) -> Option<&'static str> {
        Some("FMP_API_KEY")
    }

    fn response_shape(&self) -> Option<&'static ToolResponse> {
        response_for(self.name())
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        do_fmp_api(
            input,
            "/stable/analyst-estimates",
            &["symbol", "period", "limit"],
        )
        .await
    }
}

// ── 9. fmp_historical_price ───────────────────────────────────────────────────

struct FmpHistoricalPrice;

#[async_trait]
impl PlatformTool for FmpHistoricalPrice {
    fn name(&self) -> &'static str {
        "fmp_historical_price"
    }

    fn description(&self) -> &'static str {
        "Get historical daily price data (OHLCV) for a date range. Useful for trend \
         analysis, volatility assessment, and price momentum."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "Stock ticker symbol"
                },
                "from": {
                    "type": "string",
                    "description": "Start date in YYYY-MM-DD format"
                },
                "to": {
                    "type": "string",
                    "description": "End date in YYYY-MM-DD format"
                }
            },
            "required": ["symbol"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Financial
    }

    fn required_credential(&self) -> Option<&'static str> {
        Some("FMP_API_KEY")
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        do_fmp_api(
            input,
            "/stable/historical-price-eod/full",
            &["symbol", "from", "to"],
        )
        .await
    }
}

// ── Shared HTTP helper ────────────────────────────────────────────────────────

/// Generic FMP API executor — builds a GET request from the input parameters
/// and the endpoint path. Appends the FMP API key from env or hardcoded fallback.
async fn do_fmp_api(
    input: &serde_json::Value,
    endpoint: &str,
    param_names: &[&str],
) -> Result<String, String> {
    let api_key = std::env::var("FMP_API_KEY")
        .unwrap_or_else(|_| "xadhcaZJ9suK6jthYq2axsDINSE31Nxj".to_string());

    let base_url = "https://financialmodelingprep.com";
    let mut url = format!("{}{}", base_url, endpoint);

    // Build query string from known parameter names
    let mut params: Vec<(String, String)> = Vec::new();
    for &name in param_names {
        if let Some(val) = input.get(name) {
            let s = match val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                other => other.to_string().trim_matches('"').to_string(),
            };
            if !s.is_empty() {
                params.push((name.to_string(), s));
            }
        }
    }
    params.push(("apikey".to_string(), api_key));

    let query_string: String = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&");

    url = format!("{}?{}", url, query_string);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "FermiConsole/1.0")
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("FMP API request failed: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read FMP response: {}", e))?;

    if !status.is_success() {
        return Err(format!(
            "FMP API error (HTTP {}): {}",
            status.as_u16(),
            body
        ));
    }

    // If response is empty array, return a clear message
    if body.trim() == "[]" {
        return Ok("No data found for the given parameters.".to_string());
    }

    // Compact the JSON if it's very large (>8k chars) — keep structure but trim
    if body.len() > 8000 {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&body) {
            // For arrays, limit to first 3 entries to save token budget
            if let Some(arr) = parsed.as_array() {
                let limited: Vec<&serde_json::Value> = arr.iter().take(3).collect();
                let note = if arr.len() > 3 {
                    format!("\n[Showing 3 of {} results]", arr.len())
                } else {
                    String::new()
                };
                return Ok(format!(
                    "{}{}",
                    serde_json::to_string_pretty(&limited).unwrap_or(body),
                    note
                ));
            }
        }
    }

    Ok(body)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_names_are_dispatchable() {
        for tool in tools() {
            assert!(!tool.name().is_empty());
        }
    }

    #[test]
    fn tool_count_is_nine() {
        assert_eq!(tools().len(), 9);
    }

    #[test]
    fn all_tools_are_financial_category() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Financial,
                "{} has wrong category",
                tool.name()
            );
        }
    }

    #[test]
    fn all_tools_require_fmp_api_key() {
        for tool in tools() {
            assert_eq!(
                tool.required_credential(),
                Some("FMP_API_KEY"),
                "{} is missing FMP_API_KEY credential",
                tool.name()
            );
        }
    }

    #[test]
    fn response_shapes_exist_for_declared_tools() {
        let shaped = [
            "fmp_company_profile",
            "fmp_ratios",
            "fmp_key_metrics",
            "fmp_dcf",
            "fmp_analyst_estimates",
        ];
        for name in shaped {
            assert!(
                crate::tool_response_shapes::response_for(name).is_some(),
                "{name} missing shape"
            );
        }
    }

    #[test]
    fn unshaped_tools_return_none() {
        let unshaped = [
            "fmp_income_statement",
            "fmp_balance_sheet",
            "fmp_cash_flow",
            "fmp_historical_price",
        ];
        for name in unshaped {
            // Verify the trait default fires — find the tool by name and call its method.
            let tool = tools()
                .into_iter()
                .find(|t| t.name() == name)
                .unwrap_or_else(|| panic!("{name} not found in tools()"));
            assert!(
                tool.response_shape().is_none(),
                "{name} unexpectedly has a response shape"
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
                "{} schema is not an object",
                tool.name()
            );
            assert!(
                schema["properties"].is_object(),
                "{} schema has no properties",
                tool.name()
            );
        }
    }

    #[test]
    fn all_tools_require_symbol() {
        for tool in tools() {
            let schema = tool.input_schema();
            let required = schema["required"]
                .as_array()
                .unwrap_or_else(|| panic!("{} has no required array", tool.name()));
            assert!(
                required.iter().any(|v| v == "symbol"),
                "{} does not require symbol",
                tool.name()
            );
        }
    }

    #[test]
    fn period_tools_require_period() {
        let period_tools = [
            "fmp_income_statement",
            "fmp_balance_sheet",
            "fmp_cash_flow",
            "fmp_ratios",
            "fmp_key_metrics",
            "fmp_analyst_estimates",
        ];
        for tool in tools() {
            if period_tools.contains(&tool.name()) {
                let schema = tool.input_schema();
                let required = schema["required"]
                    .as_array()
                    .unwrap_or_else(|| panic!("{} has no required array", tool.name()));
                assert!(
                    required.iter().any(|v| v == "period"),
                    "{} does not require period",
                    tool.name()
                );
            }
        }
    }

    #[test]
    fn no_tool_requires_workspace() {
        for tool in tools() {
            assert!(
                !tool.requires_workspace(),
                "{} unexpectedly requires workspace",
                tool.name()
            );
        }
    }

    #[test]
    fn no_tool_is_delegation() {
        for tool in tools() {
            assert!(
                !tool.is_delegation(),
                "{} is unexpectedly a delegation tool",
                tool.name()
            );
        }
    }

    #[test]
    fn all_tools_are_llm_visible() {
        for tool in tools() {
            assert!(tool.is_llm_visible(), "{} is not LLM visible", tool.name());
        }
    }
}
