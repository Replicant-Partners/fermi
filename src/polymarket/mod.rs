//! Polymarket Integration — Gamma API Client
//!
//! Server-side client for the Polymarket Gamma API (public, no auth).
//! All Polymarket data fetching happens here on the ABW server —
//! the Fermi Console never calls Polymarket directly.
//!
//! Architecture:
//!   - Stateless: each call is independent, no session state
//!   - Append-only: callers store observations in fermi_market_observations
//!   - Server-side: console calls ABW endpoints, ABW calls Gamma
//!
//! APIs:
//!   - Gamma: https://gamma-api.polymarket.com (events, markets, tags, search)
//!   - Data:  https://data-api.polymarket.com  (positions, trades — Mode 2)
//!   - CLOB:  https://clob.polymarket.com       (orderbook, trading — future)

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ═══════════════════════════════════════════════════════════════════
// Error types
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug)]
pub enum PolymarketError {
    Http(reqwest::Error),
    Api(u16, String),
    Parse(String),
    NoResults,
}

impl std::fmt::Display for PolymarketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(e) => write!(f, "Polymarket HTTP error: {}", e),
            Self::Api(code, msg) => write!(f, "Polymarket API error ({}): {}", code, msg),
            Self::Parse(msg) => write!(f, "Polymarket parse error: {}", msg),
            Self::NoResults => write!(f, "No matching Polymarket markets found"),
        }
    }
}

impl std::error::Error for PolymarketError {}

impl From<reqwest::Error> for PolymarketError {
    fn from(e: reqwest::Error) -> Self {
        Self::Http(e)
    }
}

// ═══════════════════════════════════════════════════════════════════
// Response types — deserialized from Gamma API JSON
// ═══════════════════════════════════════════════════════════════════

/// A Polymarket event — a top-level question that may contain multiple markets.
/// Example: "Fed decision in March?" contains markets for "No change", "25bp cut", etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolyEvent {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub volume: f64,
    #[serde(default, rename = "volume24hr")]
    pub volume_24hr: f64,
    #[serde(default)]
    pub liquidity: f64,
    #[serde(default, rename = "endDate")]
    pub end_date: Option<String>,
    #[serde(default, rename = "startDate")]
    pub start_date: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub markets: Vec<PolyMarket>,
    #[serde(default)]
    pub tags: Vec<PolyTag>,
    #[serde(default)]
    pub competitive: f64,
    #[serde(default, rename = "commentCount")]
    pub comment_count: u64,
    #[serde(default, rename = "negRisk")]
    pub neg_risk: bool,
}

/// A single binary market within a Polymarket event.
/// Each market has Yes/No outcomes with prices, volume, and liquidity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolyMarket {
    pub id: String,
    #[serde(default)]
    pub question: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub archived: bool,

    // Outcomes and prices — stored as JSON strings by Gamma API
    #[serde(default)]
    pub outcomes: Option<String>, // "[\"Yes\", \"No\"]"
    #[serde(default, rename = "outcomePrices")]
    pub outcome_prices: Option<String>, // "[\"0.65\", \"0.35\"]"

    // Volume and liquidity
    #[serde(default)]
    pub volume: Option<String>, // String in some responses, f64 in others
    #[serde(default, rename = "volumeNum")]
    pub volume_num: Option<f64>,
    #[serde(default, rename = "liquidityNum")]
    pub liquidity_num: Option<f64>,
    #[serde(default)]
    pub liquidity: Option<String>,
    #[serde(default, rename = "volume24hr")]
    pub volume_24hr: f64,

    // Pricing
    #[serde(default, rename = "lastTradePrice")]
    pub last_trade_price: f64,
    #[serde(default, rename = "bestBid")]
    pub best_bid: f64,
    #[serde(default, rename = "bestAsk")]
    pub best_ask: f64,
    #[serde(default)]
    pub spread: f64,

    // Price changes
    #[serde(default, rename = "oneHourPriceChange")]
    pub price_change_1h: Option<f64>,
    #[serde(default, rename = "oneDayPriceChange")]
    pub price_change_1d: Option<f64>,
    #[serde(default, rename = "oneWeekPriceChange")]
    pub price_change_1w: Option<f64>,
    #[serde(default, rename = "oneMonthPriceChange")]
    pub price_change_1m: Option<f64>,

    // Chain identifiers
    #[serde(default, rename = "conditionId")]
    pub condition_id: String,
    #[serde(default, rename = "questionID")]
    pub question_id: Option<String>,
    #[serde(default, rename = "clobTokenIds")]
    pub clob_token_ids: Option<String>,

    // Event grouping
    #[serde(default, rename = "groupItemTitle")]
    pub group_item_title: Option<String>,

    // Dates
    #[serde(default, rename = "endDate")]
    pub end_date: Option<String>,

    // Resolution
    #[serde(default, rename = "umaResolutionStatus")]
    pub uma_resolution_status: Option<String>,

    // Neg-risk (multi-market events)
    #[serde(default, rename = "negRisk")]
    pub neg_risk: bool,
    #[serde(default, rename = "negRiskMarketID")]
    pub neg_risk_market_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolyTag {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub slug: String,
}

// ═══════════════════════════════════════════════════════════════════
// Processed types — cleaned up for Fermi's consumption
// ═══════════════════════════════════════════════════════════════════

/// A cleaned-up market match ready for the Fermi console.
/// All string-encoded fields are parsed, confidence is classified,
/// and the Polymarket URL is constructed.
#[derive(Debug, Clone, Serialize)]
pub struct MarketMatch {
    pub pm_event_id: String,
    pub pm_market_id: String,
    pub event_title: String,
    pub question: String,
    pub slug: String,
    pub description: String,

    /// The "Yes" outcome implied probability (0.0–1.0)
    pub market_price: f64,
    /// (best_bid + best_ask) / 2 — more stable than last_trade_price
    pub midpoint_price: f64,
    pub bid_price: f64,
    pub ask_price: f64,
    pub spread: f64,
    pub last_trade_price: f64,

    pub volume_total: f64,
    pub volume_24h: f64,
    pub liquidity: f64,

    pub price_change_1h: Option<f64>,
    pub price_change_1d: Option<f64>,
    pub price_change_1w: Option<f64>,
    pub price_change_1m: Option<f64>,

    pub end_date: Option<String>,
    pub active: bool,
    pub closed: bool,
    pub resolved: bool,
    pub outcome: Option<String>,

    pub condition_id: String,
    pub tags: Vec<String>,
    pub polymarket_url: String,

    pub confidence_signal: ConfidenceSignal,

    /// For multi-market events: the group label (e.g. "No change", "25bp cut")
    pub group_item_title: Option<String>,
}

/// Confidence classification based on volume and spread.
/// Higher confidence = more informative market price.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceSignal {
    VeryHigh,
    High,
    Medium,
    Low,
}

impl ConfidenceSignal {
    pub fn classify(volume_24h: f64, spread: f64) -> Self {
        if volume_24h > 1_000_000.0 && spread < 0.01 {
            Self::VeryHigh
        } else if volume_24h > 100_000.0 && spread < 0.02 {
            Self::High
        } else if volume_24h > 10_000.0 && spread < 0.05 {
            Self::Medium
        } else {
            Self::Low
        }
    }

    /// Map to evidence quality score (0.0–1.0) for Fermi's evidence system.
    pub fn quality_score(self) -> f64 {
        match self {
            Self::VeryHigh => 0.95,
            Self::High => 0.80,
            Self::Medium => 0.60,
            Self::Low => 0.30,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::VeryHigh => "Very High",
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
        }
    }
}

impl std::fmt::Display for ConfidenceSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

// ═══════════════════════════════════════════════════════════════════
// Gamma API Client
// ═══════════════════════════════════════════════════════════════════

/// Stateless HTTP client for the Polymarket Gamma API.
/// All methods are independent — no session state, no cookies.
pub struct GammaClient {
    client: Client,
    base_url: String,
}

impl GammaClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(15))
                .user_agent("FermiConsole/1.0 (agent-bestiary.world)")
                .build()
                .unwrap_or_else(|_| Client::new()),
            base_url: "https://gamma-api.polymarket.com".to_string(),
        }
    }

    /// Search for active events matching a text query.
    ///
    /// Strategy:
    /// 1. First try the search/text endpoint
    /// 2. Fall back to listing events sorted by volume and filtering client-side
    ///
    /// Returns events with their markets, sorted by relevance.
    pub async fn search_events(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<PolyEvent>, PolymarketError> {
        let limit = limit.min(20);

        // Strategy 1: Use the search endpoint if available
        let search_url = format!("{}/events", self.base_url);
        let response = self
            .client
            .get(&search_url)
            .query(&[
                ("limit", limit.to_string()),
                ("active", "true".to_string()),
                ("closed", "false".to_string()),
                ("order", "volume24hr".to_string()),
                ("ascending", "false".to_string()),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(PolymarketError::Api(status, body));
        }

        let all_events: Vec<PolyEvent> = response.json().await.map_err(|e| {
            PolymarketError::Parse(format!("Failed to parse events response: {}", e))
        })?;

        // Client-side relevance filtering: score each event against the query
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut scored: Vec<(f64, PolyEvent)> = all_events
            .into_iter()
            .map(|event| {
                let score = relevance_score(&event, &query_words, &query_lower);
                (score, event)
            })
            .filter(|(score, _)| *score > 0.0)
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        let results: Vec<PolyEvent> = scored.into_iter().map(|(_, event)| event).collect();

        if results.is_empty() {
            // Strategy 2: Try slug-based lookup
            let slug = slugify(query);
            if let Ok(event) = self.get_event_by_slug(&slug).await {
                return Ok(vec![event]);
            }
        }

        Ok(results)
    }

    /// Get a specific event by its Polymarket ID.
    pub async fn get_event(&self, event_id: &str) -> Result<PolyEvent, PolymarketError> {
        let url = format!("{}/events", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(&[("id", event_id)])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(PolymarketError::Api(status, body));
        }

        let events: Vec<PolyEvent> = response
            .json()
            .await
            .map_err(|e| PolymarketError::Parse(format!("Failed to parse event: {}", e)))?;

        events.into_iter().next().ok_or(PolymarketError::NoResults)
    }

    /// Get a specific event by its slug.
    pub async fn get_event_by_slug(&self, slug: &str) -> Result<PolyEvent, PolymarketError> {
        let url = format!("{}/events", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(&[("slug", slug)])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(PolymarketError::Api(status, body));
        }

        let events: Vec<PolyEvent> = response
            .json()
            .await
            .map_err(|e| PolymarketError::Parse(format!("Failed to parse event by slug: {}", e)))?;

        events.into_iter().next().ok_or(PolymarketError::NoResults)
    }

    /// Get a specific market by ID.
    pub async fn get_market(&self, market_id: &str) -> Result<PolyMarket, PolymarketError> {
        let url = format!("{}/markets", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(&[("id", market_id)])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(PolymarketError::Api(status, body));
        }

        let markets: Vec<PolyMarket> = response
            .json()
            .await
            .map_err(|e| PolymarketError::Parse(format!("Failed to parse market: {}", e)))?;

        markets.into_iter().next().ok_or(PolymarketError::NoResults)
    }

    /// Fetch the current snapshot of a market (price, volume, status).
    /// Returns a processed MarketMatch ready for the console.
    pub async fn snapshot_market(
        &self,
        event_id: &str,
        market_id: &str,
    ) -> Result<MarketMatch, PolymarketError> {
        let event = self.get_event(event_id).await?;
        let market = event
            .markets
            .iter()
            .find(|m| m.id == market_id)
            .cloned()
            .ok_or_else(|| {
                PolymarketError::Parse(format!(
                    "Market {} not found in event {}",
                    market_id, event_id
                ))
            })?;

        Ok(process_market_public(&event, &market))
    }

    /// Get all markets within an event (for multi-outcome events like Fed decisions).
    pub async fn get_event_markets(
        &self,
        event_id: &str,
    ) -> Result<Vec<MarketMatch>, PolymarketError> {
        let event = self.get_event(event_id).await?;
        let matches: Vec<MarketMatch> = event
            .markets
            .iter()
            .map(|m| process_market_public(&event, m))
            .collect();
        Ok(matches)
    }
}

impl Default for GammaClient {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════
// Data API Client (Mode 2 — position import)
// ═══════════════════════════════════════════════════════════════════

/// Client for the Polymarket Data API (public, no auth).
/// Used for fetching user positions, trades, and activity.
pub struct DataClient {
    client: Client,
    base_url: String,
}

/// A user's position on a Polymarket market.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolyPosition {
    #[serde(default)]
    pub market: Option<String>,
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub size: f64,
    #[serde(default, rename = "avgPrice")]
    pub avg_price: f64,
    #[serde(default, rename = "currentPrice")]
    pub current_price: f64,
    #[serde(default, rename = "initialValue")]
    pub initial_value: f64,
    #[serde(default, rename = "currentValue")]
    pub current_value: f64,
    #[serde(default, rename = "cashPnl")]
    pub cash_pnl: f64,
    #[serde(default, rename = "percentPnl")]
    pub percent_pnl: f64,
    #[serde(default)]
    pub asset: Option<String>,
    #[serde(default, rename = "conditionId")]
    pub condition_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default, rename = "proxyWallet")]
    pub proxy_wallet: Option<String>,
}

impl DataClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(15))
                .user_agent("FermiConsole/1.0 (agent-bestiary.world)")
                .build()
                .unwrap_or_else(|_| Client::new()),
            base_url: "https://data-api.polymarket.com".to_string(),
        }
    }

    /// Get open positions for a wallet address.
    pub async fn get_positions(
        &self,
        wallet_address: &str,
    ) -> Result<Vec<PolyPosition>, PolymarketError> {
        let url = format!("{}/positions", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(&[("user", wallet_address)])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(PolymarketError::Api(status, body));
        }

        let positions: Vec<PolyPosition> = response
            .json()
            .await
            .map_err(|e| PolymarketError::Parse(format!("Failed to parse positions: {}", e)))?;

        Ok(positions)
    }

    /// Get total portfolio value for a wallet address.
    pub async fn get_portfolio_value(&self, wallet_address: &str) -> Result<f64, PolymarketError> {
        let url = format!("{}/value", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(&[("user", wallet_address)])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(PolymarketError::Api(status, body));
        }

        // The value endpoint returns a simple number or JSON object
        let text = response.text().await?;
        text.trim()
            .trim_matches('"')
            .parse::<f64>()
            .map_err(|e| PolymarketError::Parse(format!("Failed to parse portfolio value: {}", e)))
    }
}

impl Default for DataClient {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════
// Processing helpers
// ═══════════════════════════════════════════════════════════════════

/// Process a raw PolyMarket into a clean MarketMatch.
/// Parses string-encoded fields, computes midpoint, classifies confidence.
/// Public so handlers can call it directly with raw API responses.
pub fn process_market_public(event: &PolyEvent, market: &PolyMarket) -> MarketMatch {
    // Parse the "Yes" price from outcome_prices string
    let yes_price = parse_yes_price(market);

    // Compute midpoint — prefer bid/ask, fall back to last trade
    let (bid, ask) = (market.best_bid, market.best_ask);
    let midpoint = if bid > 0.0 && ask > 0.0 {
        (bid + ask) / 2.0
    } else {
        yes_price
    };

    let spread = if bid > 0.0 && ask > 0.0 {
        ask - bid
    } else {
        market.spread
    };

    // Volume: try numeric field first, fall back to string parse
    let volume_total = market
        .volume_num
        .or_else(|| market.volume.as_ref().and_then(|v| v.parse::<f64>().ok()))
        .unwrap_or(0.0);

    let liquidity = market
        .liquidity_num
        .or_else(|| {
            market
                .liquidity
                .as_ref()
                .and_then(|v| v.parse::<f64>().ok())
        })
        .unwrap_or(0.0);

    // Confidence signal based on 24h volume and spread
    let confidence = ConfidenceSignal::classify(market.volume_24hr, spread.abs());

    // Check resolution status
    let resolved = market
        .uma_resolution_status
        .as_deref()
        .map(|s| s == "resolved")
        .unwrap_or(false);

    // Determine outcome if resolved
    let outcome = if resolved {
        // If yes_price is ~1.0, outcome is Yes; if ~0.0, outcome is No
        if yes_price > 0.9 {
            Some("Yes".to_string())
        } else if yes_price < 0.1 {
            Some("No".to_string())
        } else {
            None
        }
    } else {
        None
    };

    // Build Polymarket URL
    let polymarket_url = format!("https://polymarket.com/event/{}", event.slug);

    // Extract tag labels
    let tags: Vec<String> = event.tags.iter().map(|t| t.label.clone()).collect();

    MarketMatch {
        pm_event_id: event.id.clone(),
        pm_market_id: market.id.clone(),
        event_title: event.title.clone(),
        question: market.question.clone(),
        slug: market.slug.clone(),
        description: market.description.clone(),

        market_price: yes_price,
        midpoint_price: midpoint,
        bid_price: bid,
        ask_price: ask,
        spread,
        last_trade_price: market.last_trade_price,

        volume_total,
        volume_24h: market.volume_24hr,
        liquidity,

        price_change_1h: market.price_change_1h,
        price_change_1d: market.price_change_1d,
        price_change_1w: market.price_change_1w,
        price_change_1m: market.price_change_1m,

        end_date: market.end_date.clone(),
        active: market.active,
        closed: market.closed,
        resolved,
        outcome,

        condition_id: market.condition_id.clone(),
        tags,
        polymarket_url,

        confidence_signal: confidence,
        group_item_title: market.group_item_title.clone(),
    }
}

/// Parse the "Yes" price from a market's outcome_prices field.
/// The field is a JSON-encoded string like "[\"0.65\", \"0.35\"]".
/// The first element is the "Yes" price.
fn parse_yes_price(market: &PolyMarket) -> f64 {
    if let Some(ref prices_str) = market.outcome_prices {
        // Try parsing as JSON array of strings
        if let Ok(prices) = serde_json::from_str::<Vec<String>>(prices_str) {
            if let Some(yes_str) = prices.first() {
                if let Ok(price) = yes_str.parse::<f64>() {
                    return price;
                }
            }
        }
        // Try parsing as JSON array of numbers
        if let Ok(prices) = serde_json::from_str::<Vec<f64>>(prices_str) {
            if let Some(&price) = prices.first() {
                return price;
            }
        }
    }
    // Fall back to last trade price
    market.last_trade_price
}

/// Compute a relevance score for an event against a search query.
/// Higher score = better match. Returns 0.0 for no match.
fn relevance_score(event: &PolyEvent, query_words: &[&str], query_lower: &str) -> f64 {
    let title_lower = event.title.to_lowercase();
    let desc_lower = event.description.to_lowercase();

    // Combine all market questions for matching
    let market_questions: String = event
        .markets
        .iter()
        .map(|m| m.question.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");

    let mut score = 0.0;

    // Exact title substring match (strongest signal)
    if title_lower.contains(query_lower) {
        score += 10.0;
    }

    // Word overlap scoring
    for word in query_words {
        if word.len() < 3 {
            continue; // Skip short words (the, is, a, etc.)
        }
        if title_lower.contains(word) {
            score += 2.0;
        }
        if market_questions.contains(word) {
            score += 1.5;
        }
        if desc_lower.contains(word) {
            score += 0.5;
        }
    }

    // Tag overlap
    let tag_text: String = event
        .tags
        .iter()
        .map(|t| t.label.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    for word in query_words {
        if word.len() < 3 {
            continue;
        }
        if tag_text.contains(word) {
            score += 1.0;
        }
    }

    // Volume bonus: prefer liquid markets (log scale)
    if event.volume_24hr > 0.0 {
        score += (event.volume_24hr.log10() - 3.0).max(0.0) * 0.5;
    }

    // Active market bonus
    if event.active && !event.closed {
        score += 1.0;
    }

    score
}

/// Convert a natural language query into a URL slug for direct lookup.
/// "Will the Fed cut rates?" → "will-the-fed-cut-rates"
fn slugify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

// ═══════════════════════════════════════════════════════════════════
// Utility: Divergence computation
// ═══════════════════════════════════════════════════════════════════

/// Compute the divergence between a Fermi probability and a market price.
/// Returns the divergence in percentage points (pp).
///
/// Positive = Fermi is more bullish than the crowd.
/// Negative = Fermi is more bearish than the crowd.
pub fn compute_divergence_pp(fermi_probability: f64, market_price: f64) -> f64 {
    (fermi_probability - market_price) * 100.0
}

/// Interpret a divergence value into a human-readable edge assessment.
pub fn interpret_divergence(divergence_pp: f64) -> &'static str {
    let abs_div = divergence_pp.abs();
    if abs_div < 2.0 {
        "Consensus — your model agrees with the crowd"
    } else if abs_div < 5.0 {
        "Minor divergence — within noise"
    } else if abs_div < 15.0 {
        "Moderate divergence — potential edge worth investigating"
    } else if abs_div < 30.0 {
        "Significant divergence — strong disagreement with crowd. Is this alpha?"
    } else {
        "Extreme divergence — verify your model assumptions, this level of disagreement is unusual"
    }
}

/// Format a USD volume value into a human-readable string.
pub fn format_volume(volume: f64) -> String {
    if volume >= 1_000_000_000.0 {
        format!("${:.1}B", volume / 1_000_000_000.0)
    } else if volume >= 1_000_000.0 {
        format!("${:.1}M", volume / 1_000_000.0)
    } else if volume >= 1_000.0 {
        format!("${:.1}K", volume / 1_000.0)
    } else {
        format!("${:.0}", volume)
    }
}

/// Format a probability as a percentage string.
pub fn format_probability(p: f64) -> String {
    format!("{:.1}%", p * 100.0)
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confidence_classification() {
        assert_eq!(
            ConfidenceSignal::classify(2_000_000.0, 0.005),
            ConfidenceSignal::VeryHigh
        );
        assert_eq!(
            ConfidenceSignal::classify(500_000.0, 0.015),
            ConfidenceSignal::High
        );
        assert_eq!(
            ConfidenceSignal::classify(50_000.0, 0.03),
            ConfidenceSignal::Medium
        );
        assert_eq!(
            ConfidenceSignal::classify(5_000.0, 0.08),
            ConfidenceSignal::Low
        );
    }

    #[test]
    fn test_quality_score() {
        assert!(ConfidenceSignal::VeryHigh.quality_score() > 0.9);
        assert!(ConfidenceSignal::Low.quality_score() < 0.4);
    }

    #[test]
    fn test_parse_yes_price() {
        let mut market = PolyMarket {
            id: "1".into(),
            question: "test".into(),
            slug: "test".into(),
            description: String::new(),
            active: true,
            closed: false,
            archived: false,
            outcomes: Some(r#"["Yes", "No"]"#.into()),
            outcome_prices: Some(r#"["0.65", "0.35"]"#.into()),
            volume: None,
            volume_num: None,
            liquidity_num: None,
            liquidity: None,
            volume_24hr: 0.0,
            last_trade_price: 0.0,
            best_bid: 0.0,
            best_ask: 0.0,
            spread: 0.0,
            price_change_1h: None,
            price_change_1d: None,
            price_change_1w: None,
            price_change_1m: None,
            condition_id: String::new(),
            question_id: None,
            clob_token_ids: None,
            group_item_title: None,
            end_date: None,
            uma_resolution_status: None,
            neg_risk: false,
            neg_risk_market_id: None,
        };

        assert!((parse_yes_price(&market) - 0.65).abs() < 0.001);

        market.outcome_prices = Some(r#"["0.989", "0.011"]"#.into());
        assert!((parse_yes_price(&market) - 0.989).abs() < 0.001);
    }

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Will the Fed cut rates?"), "will-the-fed-cut-rates");
        assert_eq!(slugify("  Hello   World!  "), "hello-world");
    }

    #[test]
    fn test_divergence() {
        let div = compute_divergence_pp(0.65, 0.50);
        assert!((div - 15.0).abs() < 0.1);

        let div_neg = compute_divergence_pp(0.30, 0.50);
        assert!((div_neg - (-20.0)).abs() < 0.1);
    }

    #[test]
    fn test_interpret_divergence() {
        assert!(interpret_divergence(1.0).contains("Consensus"));
        assert!(interpret_divergence(3.5).contains("Minor"));
        assert!(interpret_divergence(10.0).contains("Moderate"));
        assert!(interpret_divergence(-25.0).contains("Significant"));
        assert!(interpret_divergence(50.0).contains("Extreme"));
    }

    #[test]
    fn test_format_volume() {
        assert_eq!(format_volume(1_500_000.0), "$1.5M");
        assert_eq!(format_volume(43_870_967.0), "$43.9M");
        assert_eq!(format_volume(500.0), "$500");
    }
}
