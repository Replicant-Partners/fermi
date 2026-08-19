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

    /// Serialised form for the `confidence_signal` column of
    /// `fermi_market_observations`. Must match the CHECK constraint
    /// declared in migration 099 exactly:
    ///   `IN ('very_high', 'high', 'medium', 'low')`.
    ///
    /// Historically call sites used `format!("{:?}", ...).to_lowercase()`,
    /// which mapped `VeryHigh` to `"veryhigh"` and made every
    /// high-confidence market INSERT fail the CHECK. The error was
    /// then swallowed by `.map_err(...).ok()`, so writes silently
    /// dropped while snapshot responses looked healthy — the
    /// trajectory view read zero observations for weeks.
    pub fn db_str(self) -> &'static str {
        match self {
            Self::VeryHigh => "very_high",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
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

        // Strategy 0: If the query looks like a slug (hyphens, no spaces), try
        // direct slug lookup first — this is what URL imports produce.
        let looks_like_slug = query.contains('-') && !query.contains(' ');
        if looks_like_slug {
            if let Ok(event) = self.get_event_by_slug(query).await {
                return Ok(vec![event]);
            }
        }

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

// ─── did the model earn its disagreement? ─────────────────────────────
//
// ## The case this exists for
//
// Chicago, KORD 78-79F on 2026-08-20, live on Polymarket:
//
//     model  5.91%    base rate 6.70%    crowd 46.5%
//     model - base    -0.8pp
//     model - crowd  -40.6pp
//     base  - crowd  -39.8pp
//
// The panel reported "DIVERGENCE: 40.6pp below crowd" and asked "Is this alpha or
// overconfidence?" — in front of a sizing decision. But the model had moved 0.8pp
// off its own base rate, so 98% of that gap is the BASE RATE disagreeing with the
// market and 2% is everything the drivers contributed. There was no forecast
// there to be alpha or overconfidence: `weather_oracle` had put the bucket at 35%
// against the market's 45.5%, and that view never reached the model because the
// driver could not carry it.
//
// Two situations produce one number, and they call for opposite actions:
//
//     moved a long way on evidence, disagrees with crowd   -> candidate edge
//     never moved, crowd is elsewhere                      -> dead driver
//
// A magnitude ladder cannot tell them apart, which is the same ambiguity
// `liveness_trust` exists to break: `count(*) = 0` is meaningless until you know
// what the opportunity was.
//
// ## How common
//
// 582 multiplier claims platform-wide, 115 of them (19.8%) within 5% of 1.0 — a
// neutral multiplier leaves the driver prior untouched, so the model sits at its
// base rate. Every domain, not just weather: football_analyst 21, macro_data_agent
// 20, fixture_context_agent 18, nba_analyst 6, biotech_analyst 5, and
// weather_oracle worst at 28 of 58. The 23 claims dropped for being out of range
// land in the same place. This is not a weather bug.
//
// ## Why the denominator is the gap and not the base rate
//
// The first threshold proposed for this was "model within 5% RELATIVE of base",
// and it would have missed Chicago: 0.79 / 6.70 = 11.8%, comfortably outside 5%.
// A 0.8pp move looks large next to a 6.7% base rate and is nothing next to a 40pp
// claim. What matters is the share of the ASSERTED DISAGREEMENT the drivers
// actually account for: 0.79 / 40.59 = 1.9%.
//
// So the test is scale-free in the dimension that matters, and it does not fire
// on a small base rate merely for being small.

/// Fraction of the model-versus-crowd gap that the drivers must account for
/// before the gap is attributed to the model rather than to the base rate.
pub const MIN_DRIVER_SHARE: f64 = 0.10;

/// Below this gap the question does not arise: nobody sizes a position on a few
/// points, and a small gap with a small driver move is simply consensus.
pub const DEAD_DRIVER_MIN_GAP_PP: f64 = 10.0;

/// What a model-versus-crowd gap means, once the base rate is taken into account.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DivergenceVerdict {
    Consensus,
    Minor,
    Moderate,
    Significant,
    Extreme,
    /// The drivers did not move this forecast; the disagreement belongs to the
    /// base rate. `driver_share` is the fraction they did account for.
    DriversSilent {
        driver_share: f64,
    },
}

impl DivergenceVerdict {
    /// One word, for a chip.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Consensus => "Consensus",
            Self::Minor => "Minor",
            Self::Moderate => "Moderate",
            Self::Significant => "Significant",
            Self::Extreme => "Extreme",
            Self::DriversSilent { .. } => "Drivers silent",
        }
    }

    /// The sentence a human should read before acting.
    pub fn describe(&self) -> &'static str {
        match self {
            Self::Consensus => "Consensus — your model agrees with the crowd",
            Self::Minor => "Minor divergence — within noise",
            Self::Moderate => "Moderate divergence — potential edge worth investigating",
            Self::Significant => {
                "Significant divergence — strong disagreement with crowd. Is this alpha?"
            }
            Self::Extreme => {
                "Extreme divergence — verify your model assumptions, this level of \
                 disagreement is unusual"
            }
            // Deliberately not phrased as a question. "Is this alpha?" invites a
            // judgement call about a forecast, and there is no forecast here to
            // judge — the drivers contributed almost none of this gap.
            Self::DriversSilent { .. } => {
                "The drivers did not move this forecast — almost all of this gap is \
                 your base rate disagreeing with the market, not your model. Check \
                 that the drivers received live data before treating it as an edge"
            }
        }
    }

    /// Is this a gap worth investigating as an edge at all?
    ///
    /// False for a silent-driver verdict: not because the market is right, but
    /// because nothing has yet been said about it.
    pub fn is_candidate_edge(&self) -> bool {
        matches!(self, Self::Moderate | Self::Significant | Self::Extreme)
    }
}

/// Classify a model-versus-crowd gap, using the base rate when it is known.
///
/// `base` is `Option` because a question need not declare one, and a missing base
/// rate must not silently become a dead-driver verdict — with nothing to compare
/// against, the honest answer is the magnitude ladder.
pub fn assess_divergence(model: f64, crowd: f64, base: Option<f64>) -> DivergenceVerdict {
    let gap_pp = ((model - crowd) * 100.0).abs();

    if let Some(b) = base {
        if b.is_finite() && gap_pp > DEAD_DRIVER_MIN_GAP_PP {
            let driver_pp = ((model - b) * 100.0).abs();
            let share = driver_pp / gap_pp;
            if share < MIN_DRIVER_SHARE {
                return DivergenceVerdict::DriversSilent {
                    driver_share: share,
                };
            }
        }
    }

    if gap_pp < 2.0 {
        DivergenceVerdict::Consensus
    } else if gap_pp < 5.0 {
        DivergenceVerdict::Minor
    } else if gap_pp < 15.0 {
        DivergenceVerdict::Moderate
    } else if gap_pp < 30.0 {
        DivergenceVerdict::Significant
    } else {
        DivergenceVerdict::Extreme
    }
}

/// Interpret a divergence value into a human-readable edge assessment.
///
/// Base-rate-blind, and kept because callers that genuinely hold only the gap
/// exist. Prefer [`assess_divergence`]: this cannot distinguish a model that
/// earned its disagreement from one that never moved.
pub fn interpret_divergence(divergence_pp: f64) -> &'static str {
    // Reconstructs a (model, crowd) pair with the right gap so there is one
    // ladder rather than two. A second copy of these thresholds is how the
    // console came to disagree with this function about the same number.
    assess_divergence(divergence_pp / 100.0, 0.0, None).describe()
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

    /// The Chicago forecast that prompted this, verbatim.
    ///
    /// KORD 78-79F on 2026-08-20: model 5.91%, base rate 6.70%, Polymarket 46.5%.
    /// The panel called it a 40.6pp edge and asked whether it was alpha. The model
    /// had moved 0.8pp off its own base rate.
    #[test]
    fn a_model_sitting_on_its_base_rate_is_not_an_edge() {
        let v = assess_divergence(0.0591, 0.465, Some(0.0670));
        match v {
            DivergenceVerdict::DriversSilent { driver_share } => {
                // 0.79pp of a 40.59pp claim.
                assert!(
                    (driver_share - 0.019).abs() < 0.002,
                    "driver share was {driver_share}"
                );
            }
            other => panic!("expected DriversSilent, got {other:?}"),
        }
        assert!(!v.is_candidate_edge());
        // The prose must not ask whether it is alpha: there is no forecast here to
        // be alpha, and the question would be put in front of a sizing decision.
        assert!(!v.describe().contains("alpha"));
        assert!(v.describe().contains("base rate"));
    }

    /// The threshold that was proposed first, and why it is not the one used.
    ///
    /// "Model within 5% RELATIVE of its base rate" sounds equivalent and is not:
    /// 0.79 / 6.70 = 11.8%, so it would have called Chicago a moved model. A 0.8pp
    /// step is large against a 6.7% base rate and negligible against a 40pp claim,
    /// so the denominator has to be the disagreement being asserted.
    #[test]
    fn the_denominator_is_the_gap_not_the_base_rate() {
        let (model, base): (f64, f64) = (0.0591, 0.0670);
        let relative_to_base = ((model - base) / base).abs();
        assert!(
            relative_to_base > 0.05,
            "if this drops below 5% the cautionary tale no longer holds: {relative_to_base}"
        );
        assert!(matches!(
            assess_divergence(model, 0.465, Some(base)),
            DivergenceVerdict::DriversSilent { .. }
        ));
    }

    /// A model that genuinely moved keeps its edge verdict.
    ///
    /// Without this the fix could suppress every large divergence and look like an
    /// improvement — trading a false positive for a false negative on the panel
    /// whose entire purpose is finding real disagreement.
    #[test]
    fn a_model_that_moved_a_long_way_still_reports_an_edge() {
        // base 10%, model 40%, crowd 60%: the drivers account for 30 of the 20pp
        // gap, so the model plainly has a thesis even though it disagrees.
        let v = assess_divergence(0.40, 0.60, Some(0.10));
        assert!(v.is_candidate_edge(), "got {v:?}");
        assert!(matches!(v, DivergenceVerdict::Significant));
    }

    /// An unknown base rate falls back to magnitude rather than to silence.
    ///
    /// Treating "no base rate declared" as a dead driver would mute the panel for
    /// every question that never declared one — a much larger silence than the bug
    /// being fixed.
    #[test]
    fn a_missing_base_rate_does_not_become_a_dead_driver_verdict() {
        let v = assess_divergence(0.0591, 0.465, None);
        assert!(matches!(v, DivergenceVerdict::Extreme));
        assert!(v.is_candidate_edge());
    }

    /// Small gaps are left alone even when the drivers are silent.
    ///
    /// A model sitting on its base rate 3pp from the crowd is agreement, not a
    /// finding, and flagging it would make the verdict fire constantly on
    /// well-calibrated questions.
    #[test]
    fn a_silent_driver_near_the_crowd_is_consensus_not_a_warning() {
        let v = assess_divergence(0.30, 0.33, Some(0.30));
        assert!(matches!(v, DivergenceVerdict::Minor), "got {v:?}");
    }

    /// One ladder, not two.
    ///
    /// `interpret_divergence` delegates, because a second copy of these thresholds
    /// is exactly how `cockpit.rs` came to hold its own inlined ladder and could
    /// disagree with this module about the same number.
    #[test]
    fn the_legacy_entry_point_agrees_with_the_ladder() {
        for gap in [1.0_f64, 3.5, 10.0, 25.0, 50.0] {
            assert_eq!(
                interpret_divergence(gap),
                assess_divergence(gap / 100.0, 0.0, None).describe()
            );
            // ...and sign must not change the verdict.
            assert_eq!(interpret_divergence(gap), interpret_divergence(-gap));
        }
    }

    #[test]
    fn test_format_volume() {
        assert_eq!(format_volume(1_500_000.0), "$1.5M");
        assert_eq!(format_volume(43_870_967.0), "$43.9M");
        assert_eq!(format_volume(500.0), "$500");
    }
}
