//! ABW API Client for the Fermi Console
//!
//! Handles authenticated HTTP communication with the Agent Bestiary World API.
//! All methods return `Result<T, ApiError>` and handle token refresh, retries,
//! and error mapping.
//!
//! The client is designed to be used from GPUI's async executor via
//! `cx.spawn()` or `cx.background_executor().spawn()`.

use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use std::sync::RwLock;
use thiserror::Error;

// ═══════════════════════════════════════════════════════════════════
// Error types
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Not authenticated — set API key first")]
    NotAuthenticated,

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Rate limited — retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("Server error: {0}")]
    Server(String),
}

impl ApiError {
    /// HTTP status, when the failure came from a response rather than
    /// the transport or the deserializer.
    ///
    /// `ApiError` carries this already but every call site flattened
    /// the whole error to `e.to_string()`, so the status was only ever
    /// available as substring-matchable prose. The Activity log wants
    /// it as a discrete context field.
    pub fn status(&self) -> Option<u16> {
        match self {
            ApiError::Http { status, .. } => Some(*status),
            ApiError::NotAuthenticated => Some(401),
            ApiError::Forbidden(_) => Some(403),
            ApiError::NotFound(_) => Some(404),
            ApiError::RateLimited { .. } => Some(429),
            ApiError::Server(_) => Some(500),
            ApiError::Network(_) | ApiError::Json(_) => None,
        }
    }

    /// Short, stable classification for grouping and telemetry.
    /// Deliberately not `Display` — this is a key, not a sentence.
    pub fn kind(&self) -> &'static str {
        match self {
            ApiError::Http { .. } => "http",
            ApiError::Network(_) => "network",
            ApiError::Json(_) => "deserialize",
            ApiError::NotAuthenticated => "unauthenticated",
            ApiError::Forbidden(_) => "forbidden",
            ApiError::NotFound(_) => "not_found",
            ApiError::RateLimited { .. } => "rate_limited",
            ApiError::Server(_) => "server",
        }
    }

    /// True for failures that are plausibly worth retrying unchanged.
    /// 4xx (other than 429) means the request itself is wrong, so a
    /// bare retry will fail identically.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            ApiError::Network(_) | ApiError::RateLimited { .. } | ApiError::Server(_)
        )
    }

    fn from_status(status: u16, body: &str) -> Self {
        match status {
            401 => ApiError::NotAuthenticated,
            403 => ApiError::Forbidden(body.to_string()),
            404 => ApiError::NotFound(body.to_string()),
            429 => ApiError::RateLimited {
                retry_after_secs: 5,
            },
            500..=599 => ApiError::Server(body.to_string()),
            _ => ApiError::Http {
                status,
                message: body.to_string(),
            },
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Configuration
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub base_url: String,
    pub api_key: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://agent-bestiary.world".to_string(),
            api_key: None,
        }
    }
}

impl ApiConfig {
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    pub fn is_authenticated(&self) -> bool {
        self.api_key.is_some()
    }
}

// ═══════════════════════════════════════════════════════════════════
// API Response types
// ═══════════════════════════════════════════════════════════════════

// ── Forecasts ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Forecast {
    pub id: String,
    pub owner_id: String,
    pub owner_display_name: Option<String>,
    pub question_text: String,
    pub domain: Option<String>,
    pub resolution_criteria: Option<String>,
    pub target_date: Option<String>,
    pub predicted_probability: f64,
    pub confidence_interval_low: Option<f64>,
    pub confidence_interval_high: Option<f64>,
    pub fpl_source: Option<String>,
    pub notebook_id: Option<String>,
    pub simulation_results: Option<JsonValue>,
    pub iterations: Option<i32>,
    pub drivers: Option<JsonValue>,
    pub evidence: Option<JsonValue>,
    pub agents_used: Option<JsonValue>,
    pub status: String,
    pub actual_outcome: Option<bool>,
    pub brier_score: Option<f64>,
    pub resolved_at: Option<String>,
    pub resolved_by: Option<String>,
    pub resolution_notes: Option<String>,
    pub visibility: String,
    pub team_id: Option<String>,
    /// COUNT of `object_shares` rows targeting this forecast (Spec 24
    /// §3.5.6). Not currently returned by the main list projection, so
    /// `default`s to None — the badge treats None as 0.
    #[serde(default)]
    pub share_count: Option<i64>,
    /// The ABW workspace this forecast is backed by, when set. Populated by
    /// the server JOIN in get_forecast_handler. The cockpit needs this to
    /// fire workspace-scoped endpoints (BayesOps state, refit, set output).
    #[serde(default)]
    pub workspace_id: Option<String>,
    /// Free-form metadata JSON written by various handlers. Notably:
    ///   metadata.polymarket = { pm_event_id, pm_market_id, pm_url,
    ///     last_market_price, last_volume_24h, … }
    /// is set by polymarket::link_handler. The cockpit reads this to
    /// hydrate the PM panel when opening a workspace-backed forecast.
    #[serde(default)]
    pub metadata: Option<JsonValue>,
    pub tags: Option<Vec<String>>,
    pub portfolios: Option<Vec<String>>,
    /// Spec 26 §3.2: rich portfolio membership — titles plus who curated.
    /// An empty vec means **standalone**; that distinction was previously
    /// invisible in every list.
    #[serde(default)]
    pub portfolio_refs: Option<Vec<PortfolioRef>>,
    /// Spec 26 §3.1: why the caller can see this, and who is responsible.
    /// `None` against older API builds — the UI then renders no
    /// provenance chrome rather than guessing.
    #[serde(default)]
    pub access: Option<AccessProvenance>,
    pub update_history: Option<Vec<ForecastUpdate>>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastUpdate {
    pub id: Option<String>,
    pub previous_probability: Option<f64>,
    pub new_probability: Option<f64>,
    pub reason: Option<String>,
    pub agent_id: Option<String>,
    pub evidence_added: Option<JsonValue>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastListResponse {
    pub forecasts: Vec<Forecast>,
    pub count: usize,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateForecastRequest {
    pub question_text: String,
    pub predicted_probability: f64,
    /// v0.11.4: naive-baseline counterfactual probability.
    ///
    /// What a naive baseline model (the raw reference-class base
    /// rate, before any Fermi/specialist adjustments) would have
    /// predicted for the same question. The server persists this
    /// verbatim and, at resolution, computes
    /// `counterfactual_brier = (cf_prob - outcome)^2`, exposing
    /// `manager_effect = brier - counterfactual_brier` — the
    /// football-manager metric that separates roster-locked team
    /// performance from roster-orthogonal manager skill.
    ///
    /// **POST-only.** The counterfactual is defined at
    /// forecast-creation-time and is not updated on PUT. If the
    /// base rate wasn't known when the operator first saved, this
    /// field stays `NULL` and the manager-effect delta stays
    /// unavailable for that row — an honest gap, not a bug.
    ///
    /// Range: `[0, 1]`. Server enforces via `CHECK` constraint
    /// (v0.11.3). Client should clamp defensively before send.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counterfactual_probability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_criteria: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_interval_low: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_interval_high: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fpl_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simulation_results: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drivers: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub portfolio_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveForecastRequest {
    pub actual_outcome: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveForecastResponse {
    pub forecast_id: String,
    pub actual_outcome: bool,
    pub brier_score: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProbabilityRequest {
    pub new_probability: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_added: Option<JsonValue>,
}

// ── Portfolios ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portfolio {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub owner_id: Option<String>,
    pub visibility: Option<String>,
    pub domain: Option<String>,
    pub forecast_count: Option<i64>,
    pub resolved_count: Option<i64>,
    pub avg_brier: Option<f64>,
    /// Owning team (Spec 24 §3.5.4). Populated by
    /// `list_portfolios_handler` when the portfolio is team-owned.
    /// `default`s to None against older API builds that don't return
    /// it. Used by the Teams panel's Shared tab to filter portfolios
    /// owned by a specific team.
    #[serde(default)]
    pub team_id: Option<String>,
    /// Spec 26 §3.1. Lets the portfolio list say "shared by Alice via WC
    /// analysts" without a per-row /shares round trip.
    #[serde(default)]
    pub access: Option<AccessProvenance>,
    #[serde(default)]
    pub share_count: Option<i64>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioListResponse {
    pub portfolios: Vec<Portfolio>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioStats {
    pub portfolio_id: String,
    pub title: Option<String>,
    pub domain: Option<String>,
    pub stats: PortfolioStatsInner,
    pub calibration: CalibrationData,
    pub recent_resolutions: Vec<RecentResolution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioStatsInner {
    pub total_forecasts: Option<i64>,
    pub active_count: Option<i64>,
    pub resolved_count: Option<i64>,
    pub draft_count: Option<i64>,
    pub avg_brier: Option<f64>,
    pub best_brier: Option<f64>,
    pub worst_brier: Option<f64>,
    pub brier_stddev: Option<f64>,
    pub avg_probability: Option<f64>,
    pub domains: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationData {
    #[serde(rename = "0-20")]
    pub bucket_0_20: Option<f64>,
    #[serde(rename = "20-40")]
    pub bucket_20_40: Option<f64>,
    #[serde(rename = "40-60")]
    pub bucket_40_60: Option<f64>,
    #[serde(rename = "60-80")]
    pub bucket_60_80: Option<f64>,
    #[serde(rename = "80-100")]
    pub bucket_80_100: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentResolution {
    pub id: Option<String>,
    pub question_text: Option<String>,
    pub predicted_probability: Option<f64>,
    pub actual_outcome: Option<bool>,
    pub brier_score: Option<f64>,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioForecast {
    pub id: String,
    pub question_text: String,
    // Tolerate null on broken legacy rows. Without `default` the whole
    // forecast deserialization fails and the portfolio detail goes blank.
    #[serde(default)]
    pub predicted_probability: Option<f64>,
    pub status: String,
    pub brier_score: Option<f64>,
    pub actual_outcome: Option<bool>,
    pub resolved_at: Option<String>,
    pub visibility: Option<String>,
    pub added_at: String,
    /// Last server-side write to fermi_forecasts.updated_at — used to sort
    /// "recently active" in the portfolio detail view.
    #[serde(default)]
    pub updated_at: Option<String>,
    /// Free-form tag list (e.g. wc2026, group-l, conmebol) — drives the
    /// portfolio detail's filter/search panel.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// COUNT of fermi_forecast_updates rows in the last 7 days. Higher
    /// means the forecast has been moving recently.
    #[serde(default)]
    pub n_recent_updates: Option<i64>,
    /// Polymarket crowd price (0.0–1.0), pulled from
    /// metadata.polymarket.last_market_price when the forecast is linked.
    #[serde(default)]
    pub pm_market_price: Option<f64>,
    /// Polymarket URL for click-through.
    #[serde(default)]
    pub pm_url: Option<String>,
    /// Polymarket 24h volume — useful for ranking "high-conviction" markets.
    #[serde(default)]
    pub pm_volume_24h: Option<f64>,
    /// (Fermi - crowd) in percentage points. Positive = Fermi sees more
    /// probability than the market. Drives the "biggest opportunity" sort.
    #[serde(default)]
    pub pm_divergence_pp: Option<f64>,
    /// UUID of the team this forecast is shared with, if any. When set
    /// AND visibility=='private', the row is "team-shared" — drives the
    /// 👥 badge in the portfolio detail (Spec 24 §3.5.6).
    #[serde(default)]
    pub team_id: Option<String>,
    /// COUNT of `object_shares` rows targeting this forecast. Spec 24
    /// §3.2 Wave 1 #4: ships the wire format ahead of need so the
    /// console badge logic doesn't require a second backend pass once
    /// Sprint 2 starts producing share rows.
    ///
    /// Spec 26: this is now authoritative — it counts real `object_shares`
    /// rows. It was a permanent 0 for the whole Spec 24 era while the
    /// console rendered badges off it.
    #[serde(default)]
    pub share_count: Option<i64>,
    /// Spec 26 §3.1 — owner + access path for rows inside a shared book.
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub owner_display_name: Option<String>,
    #[serde(default)]
    pub access: Option<AccessProvenance>,
    /// Which other books this forecast also sits in. Signals shared
    /// curation — relevant when a cascade edit here will surprise someone
    /// reading it over there.
    #[serde(default)]
    pub portfolio_refs: Option<Vec<PortfolioRef>>,
    /// Spec 26 §4.1: who pulled this forecast into *this* portfolio.
    #[serde(default)]
    pub added_by: Option<String>,
    #[serde(default)]
    pub added_by_display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioForecastsResponse {
    pub portfolio_id: String,
    pub forecasts: Vec<PortfolioForecast>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchPortfolioRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePortfolioRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
}

// ── Schedules ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastSchedule {
    pub id: String,
    pub forecast_id: String,
    pub agent_id: String,
    pub driver_name: String,
    pub query: String,
    pub interval_hours: i32,
    pub last_run_at: Option<String>,
    pub next_run_at: String,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertScheduleRequest {
    pub agent_id: String,
    pub driver_name: String,
    pub query: String,
    pub interval_hours: i32,
}

// ── Leaderboard ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub rank: Option<i64>,
    pub owner_id: Option<String>,
    pub display_name: Option<String>,
    pub total_resolved: Option<i64>,
    pub avg_brier_score: Option<f64>,
    pub best_brier_score: Option<f64>,
    pub worst_brier_score: Option<f64>,
    pub brier_stddev: Option<f64>,
    pub calibration: Option<CalibrationData>,
    pub last_resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardResponse {
    pub leaderboard: Vec<LeaderboardEntry>,
    pub count: usize,
    pub min_forecasts: i64,
}

// ── Personal Stats ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyStats {
    pub owner_id: String,
    pub stats: MyStatsInner,
    pub calibration: CalibrationData,
    pub rank: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyStatsInner {
    pub total_forecasts: Option<i64>,
    pub active_count: Option<i64>,
    pub resolved_count: Option<i64>,
    pub draft_count: Option<i64>,
    pub avg_brier: Option<f64>,
    pub best_brier: Option<f64>,
    pub worst_brier: Option<f64>,
    pub active_days_30d: Option<i64>,
    pub domains: Option<Vec<String>>,
}

// ── Agents ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub agent_id: String,
    pub agent_type: Option<String>,
    pub description: Option<String>,
    pub tier: Option<String>,
    pub model: Option<String>,
    pub tags: Option<Vec<String>>,
    pub performance: Option<JsonValue>,
    pub usage: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecutionResult {
    pub agent_id: String,
    #[serde(default)]
    pub episode_id: Option<String>,
    pub status: String,
    pub evidence: Option<Vec<JsonValue>>,
    pub confidence: Option<f64>,
    pub execution_time_ms: Option<u64>,
    pub tokens_used: Option<u32>,
    pub credits_charged: Option<f64>,
    pub loop_iterations: Option<u32>,
    pub tool_invocations: Option<Vec<JsonValue>>,
    pub metadata: Option<JsonValue>,
}

// ── Auth ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthMe {
    pub user_id: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
}

/// Wallet snapshot from `GET /api/wallet`.
///
/// Balance components:
///   * `granted_balance`  — non-transferable (onboarding, admin grants).
///   * `purchased_balance` — transferable (Stripe top-ups, earned royalties).
///   * `balance == granted_balance + purchased_balance` (DB-enforced).
///
/// We only need `balance` for the sidebar chip; the components come
/// along for the welcome modal ("100 free credits") + future top-up UI.
#[derive(Debug, Clone, Deserialize)]
pub struct Wallet {
    #[serde(default)]
    pub balance: i64,
    #[serde(default)]
    pub granted_balance: i64,
    #[serde(default)]
    pub purchased_balance: i64,
    #[serde(default)]
    pub total_deposited: i64,
    #[serde(default)]
    pub total_spent: i64,
}

impl AuthMe {
    /// Best label to show in a compact UI cell. Prefers `display_name`,
    /// then `email`, then a shortened form of the raw user_id (never
    /// the full UUID — that's what the footer used to render and looks
    /// like a system error to end users).
    pub fn friendly_label(&self) -> String {
        if let Some(name) = &self.display_name {
            if !name.trim().is_empty() {
                return name.clone();
            }
        }
        if let Some(email) = &self.email {
            if !email.trim().is_empty() {
                return email.clone();
            }
        }
        // Shorten UUIDs to their leading segment.
        let uid = self.user_id.trim();
        if uid.len() >= 24 {
            if let Some(head) = uid.split('-').next() {
                if head.len() >= 6 {
                    return format!("{}…", head);
                }
            }
            return format!("{}…", &uid[..8]);
        }
        uid.to_string()
    }
}

// ── Collaboration: shares / teams / invites (Spec 24) ──────────────

/// One `object_shares` row, as returned by the per-target share list
/// endpoints. Mirrors `fermi_auth::types::ObjectShare`; the enum fields
/// arrive as lowercase strings (`"user"|"team"`, `"view"|"edit"|"admin"`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareEntry {
    pub id: String,
    pub object_type: String,
    pub object_id: String,
    pub share_type: String,
    pub share_target: String,
    pub permission: String,
    pub granted_by: String,
    /// Display name of `share_target` resolved by the server via a JOIN
    /// to `users` (user shares) or `teams` (team shares). Renders as the
    /// primary label in the Access UI, falling back to `share_target`.
    #[serde(default)]
    pub share_target_display_name: Option<String>,
    /// Display name of `granted_by` — who created this share row.
    #[serde(default)]
    pub granted_by_display_name: Option<String>,
}

/// Response envelope for `GET /api/{forecasts|portfolios}/:id/shares`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareListResponse {
    #[serde(default)]
    pub shares: Vec<ShareEntry>,
    #[serde(default)]
    pub count: usize,
}

/// Body for `POST /api/{forecasts|portfolios}/:id/shares`
/// (`handlers::shares::CreateShareRequest`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareRequest {
    /// `"user"` or `"team"`.
    pub share_type: String,
    /// For `user`: the recipient's `user_id`. For `team`: the team UUID.
    pub share_target: String,
    /// `"view"|"edit"|"admin"`. Server defaults to `"view"` when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub owner_id: String,
    /// Which vertical created this team (`fermi_forecast`, `rabble_swarm`,
    /// `kask_simops`, …). ABW is shared substrate, so `/api/teams` returns
    /// every vertical's teams; the console filters to `fermi_forecast`.
    /// `default`s to None against API builds that don't yet return it.
    #[serde(default)]
    pub origin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamListResponse {
    #[serde(default)]
    pub teams: Vec<Team>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub team_id: String,
    pub member_type: String,
    pub member_id: String,
    pub role: String,
    pub joined_at: Option<String>,
    /// Server-resolved display name for user members. `None` for agent
    /// members or when the users row is missing. The UI falls back to
    /// `member_id` when unset.
    #[serde(default)]
    pub member_display_name: Option<String>,
    /// Discrete powers over the team's *work*, orthogonal to `role`
    /// (Spec 30). `role` says who administers the team; this says who
    /// may take terminal actions on what the team owns — today
    /// `resolve` (enforced) and `spend` (declared, not yet enforced).
    ///
    /// Deliberately `Vec<String>` and not an enum: the vocabulary is
    /// open and grows server-side. A console talking to a newer API
    /// must render a capability it has never heard of as an unknown
    /// chip, not fail the whole team-detail deserialization and blank
    /// the roster.
    ///
    /// Effective, not stored: the server reports owners with the full
    /// set regardless of their column, because it refuses to edit
    /// owners. Treat this as truth; never recompute it client-side.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// Response for `GET /api/teams/:id` — the team plus its members.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamDetail {
    pub team: Team,
    #[serde(default)]
    pub members: Vec<TeamMember>,
}

/// Body for `POST /api/teams`. `slug` must pass server slug validation
/// (lowercase, no path separators).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Tag the team with the fermi vertical so it's distinguishable from
    /// rabble/kask/etc workspaces in `/api/teams`. The server's
    /// `create_team_handler` already reads this (defaults to
    /// `bestiary_workspace` when omitted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

/// Body for `POST /api/teams/:id/members` (direct add, back-compat path).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMemberRequest {
    pub member_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Body for `POST /api/{forecasts|portfolios|teams}/:id/invites`
/// (`handlers::invites::InviteRequest`). Exactly one of
/// `invitee_user_id` / `invitee_email` must be set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invitee_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invitee_email: Option<String>,
    /// `"view"|"edit"|"admin"` for forecast/portfolio;
    /// `"owner"|"admin"|"member"|"viewer"` for team.
    pub permission: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// One pending invite from `GET /api/me/invites`. The server omits the
/// token deliberately (the invitee sees the row in their inbox already).
///
/// Also returned by the target-scoped list endpoints
/// (`GET /api/{forecasts,portfolios,teams}/:id/invites`) and the
/// outbound view (`GET /api/me/invites/sent`); the display-name fields
/// arrive populated on those paths (JOINed to `users` server-side) so
/// the console can render "Alice — pending (view)" instead of a UUID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invite {
    pub id: String,
    pub target_type: String,
    /// The forecast/portfolio/team UUID this invite targets. Present on
    /// all invite list responses, including `/api/me/invites/sent` where
    /// the caller needs to render "invited X to <target>".
    #[serde(default)]
    pub target_id: String,
    pub permission: String,
    #[serde(default)]
    pub invitee_user_id: Option<String>,
    #[serde(default)]
    pub invitee_email: Option<String>,
    /// Shareable token for email/link invites. Present when the invite
    /// was addressed to an email (or created as a link-invite); None
    /// for direct user-id invites that surface in the recipient's Inbox.
    /// The console renders this as a copyable link the operator can
    /// send via any channel while native email delivery is in flight.
    #[serde(default)]
    pub token: Option<String>,
    pub inviter_id: String,
    #[serde(default)]
    pub message: Option<String>,
    pub status: String,
    pub expires_at: String,
    pub created_at: String,
    #[serde(default)]
    pub invitee_display_name: Option<String>,
    #[serde(default)]
    pub inviter_display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteListResponse {
    #[serde(default)]
    pub invites: Vec<Invite>,
    #[serde(default)]
    pub count: usize,
}

// ── Collaboration v2: provenance, attribution, activity (Spec 26) ────

/// Why the caller can see an object, and who is responsible for that.
///
/// Attached as `access` to every forecast and portfolio list row by the
/// server. Before this existed the console could see *that* something
/// was shared with it and nothing more — no grantor, no timestamp, no
/// distinction between "a teammate shared this with the whole team" and
/// "this is public". Rendering one true sentence per row is the whole
/// point.
///
/// `access_via` ∈ `owner` | `user_share` | `team_owned` | `team_share` |
/// `portfolio` | `public` | `link` | `unknown`, in that precedence order
/// (matches the server's `can_access` chain exactly).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccessProvenance {
    #[serde(default)]
    pub access_via: String,
    #[serde(default)]
    pub permission: String,
    #[serde(default)]
    pub shared_by: Option<String>,
    #[serde(default)]
    pub shared_by_display_name: Option<String>,
    #[serde(default)]
    pub shared_at: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub team_name: Option<String>,
    /// Set when `access_via == "portfolio"`: the book whose share reached
    /// this forecast.
    #[serde(default)]
    pub via_portfolio_id: Option<String>,
    #[serde(default)]
    pub via_portfolio_title: Option<String>,
    #[serde(default)]
    pub share_count: i64,
}

impl AccessProvenance {
    /// True when the caller owns the object — the common case, for which
    /// the UI renders no provenance chrome at all.
    pub fn is_owned(&self) -> bool {
        self.access_via == "owner"
    }

    /// Short badge glyph + label for the access path.
    pub fn badge(&self) -> (&'static str, &'static str) {
        match self.access_via.as_str() {
            "owner" => ("◉", "yours"),
            "user_share" => ("→", "shared with you"),
            "team_owned" => ("👥", "team-owned"),
            "team_share" => ("👥", "team share"),
            "portfolio" => ("◈", "via portfolio"),
            "public" => ("🌐", "public"),
            "link" => ("🔗", "link"),
            _ => ("·", ""),
        }
    }

    /// The single sentence the operator actually needs: "shared by Alice
    /// via WC analysts · edit". Returns `None` for owned rows, where
    /// there is nothing to explain.
    pub fn provenance_line(&self) -> Option<String> {
        if self.is_owned() {
            return None;
        }
        let mut parts: Vec<String> = Vec::new();
        if let Some(by) = self
            .shared_by_display_name
            .clone()
            .or_else(|| self.shared_by.clone())
        {
            parts.push(format!("shared by {}", by));
        }
        match self.access_via.as_str() {
            "team_share" | "team_owned" => {
                if let Some(t) = &self.team_name {
                    parts.push(format!("via {}", t));
                }
            }
            "portfolio" => {
                if let Some(p) = &self.via_portfolio_title {
                    parts.push(format!("via portfolio ‹{}›", p));
                }
                if let Some(t) = &self.team_name {
                    parts.push(format!("→ {}", t));
                }
            }
            "public" => parts.push("public".into()),
            "link" => parts.push("anyone with the link".into()),
            _ => {}
        }
        if !self.permission.is_empty() {
            parts.push(self.permission.clone());
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" · "))
        }
    }
}

/// One portfolio a forecast belongs to, with curation attribution.
///
/// An empty `portfolio_refs` list means the forecast is **standalone** —
/// which the console previously had no way to know, so every forecast
/// looked equally context-free.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioRef {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub added_at: Option<String>,
    /// Who pulled this forecast into that book. On a shared portfolio
    /// this is frequently not the portfolio owner.
    #[serde(default)]
    pub added_by: Option<String>,
    #[serde(default)]
    pub added_by_display_name: Option<String>,
}

/// One attributed event from any of the three activity feeds.
///
/// `actor_kind`: `user` | `agent` | `system`. `system` means the row
/// predates attribution (migration 176) or was written by a cron path —
/// the server deliberately does not guess an actor, so the UI renders
/// these as unattributed rather than blaming the owner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEvent {
    #[serde(default)]
    pub ts: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub actor_id: Option<String>,
    #[serde(default)]
    pub actor_display_name: Option<String>,
    #[serde(default)]
    pub actor_kind: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub object_type: String,
    #[serde(default)]
    pub object_id: String,
    #[serde(default)]
    pub object_title: Option<String>,
    /// Server-rendered one-liner ("revised 41% → 47%"). Built server-side
    /// so the console, web UI and MCP tools tell the same story.
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub detail: JsonValue,
}

impl ActivityEvent {
    /// Glyph + accent for the event kind. Kept next to the wire type so
    /// every surface that renders a feed agrees on the vocabulary.
    pub fn glyph(&self) -> &'static str {
        match self.kind.as_str() {
            "created" => "✎",
            "published" => "◐",
            "revised" => "→",
            "resolved" => "✓",
            "shared" => "↱",
            "portfolio_add" => "◈",
            "portfolio_created" => "◇",
            "invited" => "✉",
            "member_joined" => "🧑",
            _ => "·",
        }
    }

    pub fn actor_label(&self) -> String {
        match self.actor_kind.as_str() {
            "system" => "—".into(),
            _ => self
                .actor_display_name
                .clone()
                .or_else(|| self.actor_id.clone())
                .unwrap_or_else(|| "—".into()),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivityResponse {
    #[serde(default)]
    pub events: Vec<ActivityEvent>,
    #[serde(default)]
    pub count: usize,
    /// Present on the team feed: how big the team's shared surface is.
    #[serde(default)]
    pub surface: Option<ActivitySurface>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivitySurface {
    #[serde(default)]
    pub forecast_count: usize,
    #[serde(default)]
    pub portfolio_count: usize,
}

/// One member's contribution roll-up over a team's shared surface.
/// Turns the Roster tab from a list of names into a working document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberContribution {
    pub member_id: String,
    #[serde(default)]
    pub member_display_name: Option<String>,
    #[serde(default)]
    pub member_type: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub joined_at: Option<String>,
    #[serde(default)]
    pub invited_by: Option<String>,
    #[serde(default)]
    pub revisions: i64,
    #[serde(default)]
    pub resolutions: i64,
    #[serde(default)]
    pub authored: i64,
    #[serde(default)]
    pub shares_granted: i64,
    #[serde(default)]
    pub curations: i64,
    #[serde(default)]
    pub total_actions: i64,
    #[serde(default)]
    pub last_active_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeamContributionsResponse {
    #[serde(default)]
    pub members: Vec<MemberContribution>,
    #[serde(default)]
    pub surface: Option<ActivitySurface>,
}

/// A forecast on a team's shared surface, with the reason it's there.
///
/// `via` ∈ `team_owned` | `team_share` | `portfolio`. The `portfolio`
/// case is inherited access — the forecast was never shared with the
/// team directly, it came along with a book.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamSharedForecast {
    pub id: String,
    #[serde(default)]
    pub question_text: String,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub owner_display_name: Option<String>,
    #[serde(default)]
    pub predicted_probability: Option<f64>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub brier_score: Option<f64>,
    #[serde(default)]
    pub actual_outcome: Option<bool>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub target_date: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub resolved_at: Option<String>,
    #[serde(default)]
    pub n_recent_updates: i64,
    #[serde(default)]
    pub via: Option<String>,
    #[serde(default)]
    pub permission: Option<String>,
    #[serde(default)]
    pub shared_by: Option<String>,
    #[serde(default)]
    pub shared_by_display_name: Option<String>,
    #[serde(default)]
    pub shared_at: Option<String>,
    #[serde(default)]
    pub via_portfolio_id: Option<String>,
    #[serde(default)]
    pub via_portfolio_title: Option<String>,
}

impl TeamSharedForecast {
    pub fn is_inherited(&self) -> bool {
        self.via.as_deref() == Some("portfolio")
    }

    /// "Bo · team share · edit" — why this row is on the team surface.
    pub fn provenance_line(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        match self.via.as_deref() {
            Some("team_owned") => parts.push("owned by this team".into()),
            Some("portfolio") => parts.push(format!(
                "via portfolio ‹{}›",
                self.via_portfolio_title
                    .clone()
                    .unwrap_or_else(|| "—".into())
            )),
            _ => parts.push("shared with this team".into()),
        }
        if let Some(by) = self
            .shared_by_display_name
            .clone()
            .or_else(|| self.shared_by.clone())
        {
            parts.push(format!("by {}", by));
        }
        if let Some(p) = &self.permission {
            parts.push(p.clone());
        }
        parts.join(" · ")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamSharedPortfolio {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub owner_display_name: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub forecast_count: i64,
    #[serde(default)]
    pub resolved_count: i64,
    #[serde(default)]
    pub avg_brier: Option<f64>,
    #[serde(default)]
    pub via: Option<String>,
    #[serde(default)]
    pub permission: Option<String>,
    #[serde(default)]
    pub shared_by: Option<String>,
    #[serde(default)]
    pub shared_by_display_name: Option<String>,
    #[serde(default)]
    pub shared_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeamSharedResponse {
    #[serde(default)]
    pub forecasts: Vec<TeamSharedForecast>,
    #[serde(default)]
    pub portfolios: Vec<TeamSharedPortfolio>,
    #[serde(default)]
    pub counts: TeamSharedCounts,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeamSharedCounts {
    #[serde(default)]
    pub forecasts: usize,
    #[serde(default)]
    pub portfolios: usize,
    /// How many of `forecasts` are there only by portfolio inheritance.
    #[serde(default)]
    pub inherited: usize,
}

/// A share row enriched with `created_at` and, for team shares, the
/// team's inline roster. `ShareEntry` (Spec 24) lacks both, which is why
/// the Access tab couldn't answer "does the whole team actually have
/// this, and since when".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RichShare {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub share_type: String,
    #[serde(default)]
    pub share_target: String,
    #[serde(default)]
    pub share_target_display_name: Option<String>,
    #[serde(default)]
    pub permission: Option<String>,
    #[serde(default)]
    pub granted_by: Option<String>,
    #[serde(default)]
    pub granted_by_display_name: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    /// Roster for team shares; `None`/empty for user shares.
    #[serde(default)]
    pub members: Option<Vec<ShareMember>>,
    /// Present only on `inherited_shares`: the portfolio the grant lives
    /// on. Such a share is read-only here — you revoke it on the
    /// portfolio, not on the forecast.
    #[serde(default)]
    pub portfolio_id: Option<String>,
    #[serde(default)]
    pub portfolio_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareMember {
    #[serde(default)]
    pub member_id: Option<String>,
    #[serde(default)]
    pub member_display_name: Option<String>,
    #[serde(default)]
    pub member_type: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

/// One human who can actually see the object, and why. Teams are already
/// expanded server-side, so this is the flat truth: "these people".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveViewer {
    pub user_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub permission: String,
    /// `owner` | `user_share` | `team_share` | `portfolio_team_share` |
    /// `portfolio_user_share`
    #[serde(default)]
    pub via: String,
    #[serde(default)]
    pub via_label: Option<String>,
}

/// `GET /api/{forecasts,portfolios}/:id/access` — the complete access
/// picture for one object.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccessSummary {
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub owner_display_name: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub my_permission: String,
    #[serde(default)]
    pub my_access: AccessProvenance,
    #[serde(default)]
    pub direct_shares: Vec<RichShare>,
    /// Forecast-only: grants that live on a containing portfolio.
    #[serde(default)]
    pub inherited_shares: Vec<RichShare>,
    #[serde(default)]
    pub viewers: Vec<EffectiveViewer>,
    /// Portfolio-only: how many member forecasts inherit these grants.
    /// Makes the consequence of sharing a book legible before you click.
    #[serde(default)]
    pub cascades_to: i64,
    #[serde(default)]
    pub forecast_count: i64,
}

// ── Forecast version history (Spec 31) ────────────────────────────

/// One commit in a forecast's history — a single attributed change.
///
/// Backed by the workspace git repo, not a table. `author` is the acting
/// human, which is the whole point: before Spec 31 an FPL or driver edit
/// that didn't move the probability left no record at all.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastCommit {
    pub sha: String,
    #[serde(default)]
    pub short_sha: String,
    /// Server-composed, e.g. `"Alice Labra: revised 41% → 47% — new elo data"`.
    #[serde(default)]
    pub message: String,
    /// Git author name. `"Fermi System"` for genuinely systemic writes.
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub timestamp: Option<String>,
}

impl ForecastCommit {
    /// Split `"Alice Labra: revised 41% → 47%"` into actor and action.
    ///
    /// The server composes the message so every client tells the same story;
    /// splitting it lets the UI put the actor in its own column without a
    /// second field that could disagree with the message.
    pub fn actor_and_action(&self) -> (String, String) {
        match self.message.split_once(':') {
            Some((who, what)) if !who.contains(' ') || who.len() < 40 => {
                (who.trim().to_string(), what.trim().to_string())
            }
            // No recognisable prefix (a commit made outside the hook, e.g.
            // the repo's `initial structure`): fall back to the git author
            // rather than mangling the message.
            _ => (self.author.clone(), self.message.clone()),
        }
    }

    pub fn is_system(&self) -> bool {
        self.author == "Fermi System" || self.author.is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ForecastHistoryResponse {
    /// False when the forecast has never been committed — an honest state,
    /// not an error. History begins at its next save.
    #[serde(default)]
    pub versioned: bool,
    #[serde(default)]
    pub commits: Vec<ForecastCommit>,
    #[serde(default)]
    pub count: usize,
    /// Present when `versioned` is false, explaining why.
    #[serde(default)]
    pub note: Option<String>,
}

/// A unified diff between two revisions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ForecastDiffResponse {
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    /// Raw unified diff across `forecast.fpl`, `drivers.json`,
    /// `evidence.json`, `state.json` — render with per-line +/- colouring.
    #[serde(default)]
    pub diff: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevertRequest {
    pub sha: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ── Driver annotations (Spec 32) ──────────────────────────────────────

/// An objection, question or note anchored to one driver of a forecast.
///
/// Anchored at `(forecast, driver)` rather than the forecast because
/// disagreement here is almost never about the question — it's about one
/// input. That's what lets the composer render a challenge next to the
/// number it disputes, and lets it survive a revision of some other driver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub id: String,
    /// `None` = about the forecast as a whole, not any single input.
    #[serde(default)]
    pub driver_name: Option<String>,
    #[serde(default)]
    pub author_id: String,
    #[serde(default)]
    pub author_display_name: Option<String>,
    #[serde(default)]
    pub body: String,
    /// `challenge` (this input is wrong) | `question` | `note`.
    #[serde(default)]
    pub kind: String,
    /// `open` | `accepted` | `declined` | `orphaned`.
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub resolved_by: Option<String>,
    #[serde(default)]
    pub resolved_by_display_name: Option<String>,
    #[serde(default)]
    pub resolved_at: Option<String>,
    #[serde(default)]
    pub resolution_note: Option<String>,
    /// The Spec 31 revision this was written against, so the UI can say
    /// "raised when this read 1780" after the value has moved.
    #[serde(default)]
    pub at_commit: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

impl Annotation {
    pub fn is_open(&self) -> bool {
        self.status == "open"
    }

    /// The driver it disputes, or the sentinel the server uses in
    /// `open_by_driver` for forecast-level annotations. Keeping the sentinel
    /// in one place stops the badge lookup and the grouping disagreeing.
    pub fn driver_key(&self) -> &str {
        self.driver_name.as_deref().unwrap_or(FORECAST_LEVEL_KEY)
    }

    pub fn author_label(&self) -> &str {
        self.author_display_name
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.author_id)
    }

    /// Glyph + label for the kind. `challenge` is the load-bearing one and
    /// is the only one that becomes coordination work on the ops board.
    pub fn kind_glyph(&self) -> &'static str {
        match self.kind.as_str() {
            "question" => "?",
            "note" => "·",
            _ => "!",
        }
    }
}

/// Key the server uses in `open_by_driver` for annotations with no driver.
pub const FORECAST_LEVEL_KEY: &str = "__forecast__";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnnotationsResponse {
    #[serde(default)]
    pub forecast_id: String,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    #[serde(default)]
    pub count: usize,
    /// Open count per driver name, computed server-side so the composer's
    /// "contested" badge can't drift from the ops-board detector.
    /// Forecast-level annotations are keyed by [`FORECAST_LEVEL_KEY`].
    #[serde(default)]
    pub open_by_driver: std::collections::HashMap<String, usize>,
    /// The asking user's id, echoed by the server. Delete is author-only,
    /// and this is how the client knows which rows are its own instead of
    /// offering Delete everywhere and letting the server refuse.
    #[serde(default)]
    pub me: Option<String>,
}

impl AnnotationsResponse {
    pub fn open_on(&self, driver: &str) -> usize {
        self.open_by_driver.get(driver).copied().unwrap_or(0)
    }

    /// Every annotation for one driver, open first (server order preserved).
    pub fn for_driver(&self, driver: &str) -> Vec<&Annotation> {
        self.annotations
            .iter()
            .filter(|a| a.driver_key() == driver)
            .collect()
    }

    /// Did the asking user write this? False when the server didn't say —
    /// failing closed, since the alternative is showing a Delete button
    /// that 403s.
    pub fn is_mine(&self, a: &Annotation) -> bool {
        self.me
            .as_deref()
            .is_some_and(|me| !me.is_empty() && me == a.author_id)
    }

    /// Annotations not attached to any driver — "the whole framing is
    /// wrong", which is a real thing to say and would be misfiled if
    /// forced onto an arbitrary driver.
    pub fn forecast_level(&self) -> Vec<&Annotation> {
        self.for_driver(FORECAST_LEVEL_KEY)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateAnnotationRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver_name: Option<String>,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolveAnnotationRequest {
    /// `accepted` — acted on, the driver changed.
    /// `declined` — considered and rejected. Both are answers; the record
    /// keeps the difference.
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ── Ops board: detected coordination work (Spec 27) ───────────────────

/// One unit of coordinated work a team should pick up.
///
/// Ops are **detected, never authored**. Nothing stores them: each is
/// derived from a condition that is currently true of the team's shared
/// surface. The consequence — and the reason this needs no lifecycle, no
/// assignment table and no "close" button — is that **the definition of
/// done is the detector going quiet.** An op exists exactly as long as the
/// situation does, so the board can never accumulate stale tickets.
///
/// `objective` and `done_when` are generated server-side, so the goal
/// constraint is stated in one voice everywhere it appears.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Op {
    /// Stable across refreshes: `"<kind>:<primary scope id>"`. Lets the
    /// console hold a selection while the board re-polls; a random id per
    /// poll would make the list unusable.
    pub id: String,
    /// `cascade_review` | `contested` | `unreviewed` | `resolution_due`
    pub kind: String,
    /// 0–100. Comparable ACROSS kinds so one board can rank them.
    #[serde(default)]
    pub urgency: i32,
    /// `critical` | `high` | `normal` | `low` — bucketed `urgency`, so the
    /// console doesn't hardcode thresholds the server may retune.
    #[serde(default)]
    pub urgency_label: String,
    /// What the team is being asked to achieve.
    #[serde(default)]
    pub objective: String,
    /// The clearing condition, in words. This is the contract: when it
    /// becomes true the op disappears on its own.
    #[serde(default)]
    pub done_when: String,
    /// When the underlying condition started (not when it was detected) —
    /// an op that has been true for three weeks is a different problem
    /// from one that appeared this morning.
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub primary: Option<OpTarget>,
    #[serde(default)]
    pub scope: OpScope,
    /// Who is already involved, and how. Not an assignment — ops aren't
    /// assigned, they're claimed by acting.
    #[serde(default)]
    pub participants: Vec<OpParticipant>,
    /// Flat, kind-specific numbers for the summary line.
    #[serde(default)]
    pub metrics: JsonValue,
    #[serde(default)]
    pub detail: JsonValue,
}

impl Op {
    pub fn glyph(&self) -> &'static str {
        match self.kind.as_str() {
            "cascade_review" => "⚡",
            "contested" => "⚔",
            "contested_assumption" => "⚖",
            "unreviewed" => "👁",
            "ungrounded" => "◌",
            "resolution_due" => "⏱",
            _ => "◈",
        }
    }

    /// Human label for the kind, for the filter chips and row badges.
    pub fn kind_label(&self) -> &'static str {
        match self.kind.as_str() {
            "cascade_review" => "cascade",
            "contested" => "contested",
            "contested_assumption" => "challenged",
            "unreviewed" => "unreviewed",
            "ungrounded" => "ungrounded",
            "resolution_due" => "due",
            _ => "op",
        }
    }

    /// The members of a rolled-up condition, if this op is one.
    ///
    /// `unreviewed` and `ungrounded` describe a property of the whole
    /// surface, so they emit ONE op naming a count rather than one op per
    /// forecast — six rows of the same sentence was a lint list, not
    /// coordination. The members ride along in `detail.items` so the row
    /// can still show *which* forecasts it means, which is the thing the
    /// count alone doesn't tell you.
    pub fn rollup_items(&self) -> Vec<RollupItem> {
        self.detail
            .get("items")
            .and_then(|v| serde_json::from_value::<Vec<RollupItem>>(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// One forecast inside a rolled-up op.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RollupItem {
    #[serde(default)]
    pub forecast_id: String,
    #[serde(default)]
    pub question: String,
    #[serde(default)]
    pub age_days: i64,
    #[serde(default)]
    pub probability_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpTarget {
    /// `forecast` | `portfolio`
    #[serde(rename = "type", default)]
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpScope {
    #[serde(default)]
    pub forecast_ids: Vec<String>,
    #[serde(default)]
    pub portfolio_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpParticipant {
    pub user_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    /// Why they're on this op: `owner` | `reviser` | `trigger_owner`
    #[serde(default)]
    pub role: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpsResponse {
    #[serde(default)]
    pub ops: Vec<Op>,
    #[serde(default)]
    pub counts: OpsCounts,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpsCounts {
    #[serde(default)]
    pub total: usize,
    /// kind -> count. Drives the filter chips without a second pass over
    /// the list, and stays correct when the list is truncated.
    #[serde(default)]
    pub by_kind: JsonValue,
}

/// Minimal percent-encoder for query-string *values*.
///
/// The crate has no `urlencoding` dependency and pulling one in for two
/// call sites isn't worth it. Conservative by construction: anything
/// outside the unreserved set plus `,` (which the `kind` filter uses as a
/// separator and servers accept literally) gets escaped, so this can
/// never under-encode even if a user_id shape changes to something
/// exotic like an ENS name or an email.
fn encode_query_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b',' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Result of `GET /api/users/lookup?email=…`. `None` (404 → mapped to
/// `Ok(None)`) means "no account with that email → send an email invite."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSummary {
    pub user_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════
// Query parameter builders
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Default, Clone)]
pub struct ForecastQuery {
    pub status: Option<String>,
    pub domain: Option<String>,
    pub portfolio_id: Option<String>,
    pub tag: Option<String>,
    pub sort: Option<String>,
    pub order: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// Ownership-scope filter served by
    /// `list_forecasts_handler`. `"mine"` restricts to forecasts owned
    /// by the caller; `"shared"` restricts to the accessible-but-not-owned
    /// slice (team-shared, object-shared, public/shared visibility).
    /// Powers the Portfolio panel's virtual buckets.
    pub scope: Option<String>,
    /// If `Some(true)`, restrict to forecasts that are NOT a member of
    /// any portfolio. Powers the "📌 Unassigned" virtual portfolio.
    pub unassigned: Option<bool>,
}

impl ForecastQuery {
    pub fn active() -> Self {
        Self {
            status: Some("active".into()),
            ..Default::default()
        }
    }

    pub fn resolved() -> Self {
        Self {
            status: Some("resolved".into()),
            sort: Some("brier_score".into()),
            order: Some("asc".into()),
            ..Default::default()
        }
    }

    pub fn drafts() -> Self {
        Self {
            status: Some("draft".into()),
            ..Default::default()
        }
    }

    pub fn in_portfolio(portfolio_id: impl Into<String>) -> Self {
        Self {
            portfolio_id: Some(portfolio_id.into()),
            ..Default::default()
        }
    }

    /// Forecasts the caller can see but does NOT own — team-shared,
    /// object-shared, or public/shared visibility. Feeds the Portfolio
    /// panel's "📥 Shared with me" virtual bucket so shared work
    /// isn't orphaned in the UX.
    pub fn shared_with_me() -> Self {
        Self {
            scope: Some("shared".into()),
            sort: Some("updated".into()),
            ..Default::default()
        }
    }

    /// Forecasts owned by the caller that aren't in any portfolio.
    /// Feeds the Portfolio panel's "📌 Unassigned" virtual bucket so
    /// non-published/loose forecasts have a discoverable home.
    pub fn mine_unassigned() -> Self {
        Self {
            scope: Some("mine".into()),
            unassigned: Some(true),
            sort: Some("updated".into()),
            ..Default::default()
        }
    }

    fn to_query_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        if let Some(ref v) = self.status {
            pairs.push(("status".into(), v.clone()));
        }
        if let Some(ref v) = self.domain {
            pairs.push(("domain".into(), v.clone()));
        }
        if let Some(ref v) = self.portfolio_id {
            pairs.push(("portfolio_id".into(), v.clone()));
        }
        if let Some(ref v) = self.tag {
            pairs.push(("tag".into(), v.clone()));
        }
        if let Some(ref v) = self.sort {
            pairs.push(("sort".into(), v.clone()));
        }
        if let Some(ref v) = self.order {
            pairs.push(("order".into(), v.clone()));
        }
        if let Some(v) = self.limit {
            pairs.push(("limit".into(), v.to_string()));
        }
        if let Some(v) = self.offset {
            pairs.push(("offset".into(), v.to_string()));
        }
        if let Some(ref v) = self.scope {
            pairs.push(("scope".into(), v.clone()));
        }
        if let Some(v) = self.unassigned {
            pairs.push(("unassigned".into(), v.to_string()));
        }
        pairs
    }
}

#[derive(Debug, Default, Clone)]
pub struct LeaderboardQuery {
    pub domain: Option<String>,
    pub team_id: Option<String>,
    pub min_forecasts: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl LeaderboardQuery {
    fn to_query_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        if let Some(ref v) = self.domain {
            pairs.push(("domain".into(), v.clone()));
        }
        if let Some(ref v) = self.team_id {
            pairs.push(("team_id".into(), v.clone()));
        }
        if let Some(v) = self.min_forecasts {
            pairs.push(("min_forecasts".into(), v.to_string()));
        }
        if let Some(v) = self.limit {
            pairs.push(("limit".into(), v.to_string()));
        }
        if let Some(v) = self.offset {
            pairs.push(("offset".into(), v.to_string()));
        }
        pairs
    }
}

// ═══════════════════════════════════════════════════════════════════
// API Client
// ═══════════════════════════════════════════════════════════════════

/// Thread-safe API client for the ABW backend.
///
/// Designed to be shared across GPUI entities via `Arc<ApiClient>`.
/// Configuration (including API key) can be updated at runtime via
/// the interior `RwLock`.
#[derive(Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    config: Arc<RwLock<ApiConfig>>,
}

impl ApiClient {
    /// Create a new API client with the given configuration.
    pub fn new(config: ApiConfig) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(120))
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .user_agent("fermi-console/0.1.0")
            .build()
            .expect("Failed to create HTTP client");

        Self {
            http,
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// Create a client with default configuration (production API, no auth).
    pub fn default_client() -> Self {
        Self::new(ApiConfig::default())
    }

    /// Update the API key at runtime (e.g., after user enters it in settings).
    pub async fn set_api_key(&self, key: impl Into<String>) {
        let mut config = self.config.write().unwrap();
        config.api_key = Some(key.into());
    }

    /// Clear the API key (e.g., on logout).
    pub async fn clear_api_key(&self) {
        let mut config = self.config.write().unwrap();
        config.api_key = None;
    }

    /// Check if the client has an API key set.
    pub async fn is_authenticated(&self) -> bool {
        let config = self.config.read().unwrap();
        config.api_key.is_some()
    }

    /// Get the current API key (for SSE stream authentication).
    pub fn api_key(&self) -> Option<String> {
        let config = self.config.read().unwrap();
        config.api_key.clone()
    }

    /// Get the current base URL.
    pub async fn base_url(&self) -> String {
        let config = self.config.read().unwrap();
        config.base_url.clone()
    }

    /// Synchronous accessor for the base URL. Suitable for GPUI render
    /// paths where `.await` isn't available (invite copy-link, share
    /// widgets, etc.). The underlying read is a non-blocking
    /// `RwLock::read()` on an in-memory config, so making this
    /// synchronous carries no cost — the `async` variant above exists
    /// only because early call sites happened to be in async contexts.
    pub fn base_url_sync(&self) -> String {
        let config = self.config.read().unwrap();
        config.base_url.clone()
    }

    // ── Internal helpers ──────────────────────────────────────────────

    async fn headers(&self) -> Result<reqwest::header::HeaderMap, ApiError> {
        let config = self.config.read().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        if let Some(ref key) = config.api_key {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", key))
                    .map_err(|e| ApiError::Server(format!("Invalid API key format: {}", e)))?,
            );
        }

        Ok(headers)
    }

    async fn url(&self, path: &str) -> String {
        let config = self.config.read().unwrap();
        format!("{}{}", config.base_url, path)
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let url = self.url(path).await;
        let headers = self.headers().await?;
        log::debug!("[api] GET {}", url);

        let response = self.http.get(&url).headers(headers).send().await?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::from_status(status, &body));
        }

        let body = response.text().await?;
        serde_json::from_str(&body).map_err(ApiError::Json)
    }

    async fn get_with_query<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<T, ApiError> {
        let url = self.url(path).await;
        let headers = self.headers().await?;

        let response = self
            .http
            .get(&url)
            .headers(headers)
            .query(query)
            .send()
            .await?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::from_status(status, &body));
        }

        let body = response.text().await?;
        serde_json::from_str(&body).map_err(ApiError::Json)
    }

    async fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let url = self.url(path).await;
        let headers = self.headers().await?;

        log::debug!("[api] POST {}", url);
        let response = self
            .http
            .post(&url)
            .headers(headers)
            .json(body)
            .send()
            .await
            .map_err(|e| {
                log::error!("[api] POST {} network error: {}", url, e);
                if e.is_timeout() {
                    ApiError::Server(format!("Request timed out after 120s: {}", url))
                } else if e.is_connect() {
                    ApiError::Server(format!("Connection failed to {}: {}", url, e))
                } else {
                    ApiError::Network(e)
                }
            })?;

        let status = response.status().as_u16();
        log::debug!("[api] POST {} → {}", url, status);
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::from_status(status, &body));
        }

        let body = response.text().await?;
        serde_json::from_str(&body).map_err(ApiError::Json)
    }

    async fn put<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let url = self.url(path).await;
        let headers = self.headers().await?;

        let response = self
            .http
            .put(&url)
            .headers(headers)
            .json(body)
            .send()
            .await?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::from_status(status, &body));
        }

        let body = response.text().await?;
        serde_json::from_str(&body).map_err(ApiError::Json)
    }

    async fn delete(&self, path: &str) -> Result<(), ApiError> {
        let url = self.url(path).await;
        let headers = self.headers().await?;

        let response = self.http.delete(&url).headers(headers).send().await?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::from_status(status, &body));
        }

        Ok(())
    }

    async fn patch<B: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R, ApiError> {
        let url = self.url(path).await;
        let mut headers = self.headers().await?;
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        let response = self
            .http
            .patch(&url)
            .headers(headers)
            .json(body)
            .send()
            .await?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::from_status(status, &body));
        }

        let text = response.text().await?;
        serde_json::from_str(&text).map_err(ApiError::Json)
    }

    // ═══════════════════════════════════════════════════════════════
    // Auth
    // ═══════════════════════════════════════════════════════════════

    /// Verify the API key and get the authenticated user's info.
    pub async fn auth_me(&self) -> Result<AuthMe, ApiError> {
        self.get("/api/auth/me").await
    }

    /// Fetch the authenticated user's wallet snapshot: balance,
    /// granted vs purchased split, totals. Used for the sidebar
    /// credits chip and the welcome-modal balance display.
    pub async fn get_wallet(&self) -> Result<Wallet, ApiError> {
        self.get("/api/wallet").await
    }

    // ═══════════════════════════════════════════════════════════════
    // Forecasts
    // ═══════════════════════════════════════════════════════════════

    /// Create a new forecast.
    pub async fn create_forecast(
        &self,
        req: &CreateForecastRequest,
    ) -> Result<JsonValue, ApiError> {
        self.post("/api/forecasts", req).await
    }

    /// Get a single forecast by ID.
    pub async fn get_forecast(&self, forecast_id: &str) -> Result<Forecast, ApiError> {
        self.get(&format!("/api/forecasts/{}", forecast_id)).await
    }

    /// List forecasts with optional filters.
    pub async fn list_forecasts(
        &self,
        query: &ForecastQuery,
    ) -> Result<ForecastListResponse, ApiError> {
        let pairs = query.to_query_pairs();
        self.get_with_query("/api/forecasts", &pairs).await
    }

    /// Update a forecast.
    pub async fn update_forecast(
        &self,
        forecast_id: &str,
        updates: &JsonValue,
    ) -> Result<Forecast, ApiError> {
        self.put(&format!("/api/forecasts/{}", forecast_id), updates)
            .await
    }

    /// Delete a forecast.
    pub async fn delete_forecast(&self, forecast_id: &str) -> Result<(), ApiError> {
        self.delete(&format!("/api/forecasts/{}", forecast_id))
            .await
    }

    /// Resolve a forecast with an actual outcome. Returns the Brier score.
    pub async fn resolve_forecast(
        &self,
        forecast_id: &str,
        req: &ResolveForecastRequest,
    ) -> Result<ResolveForecastResponse, ApiError> {
        self.post(&format!("/api/forecasts/{}/resolve", forecast_id), req)
            .await
    }

    /// Void a forecast (cancel without resolution).
    pub async fn void_forecast(&self, forecast_id: &str) -> Result<JsonValue, ApiError> {
        self.post(&format!("/api/forecasts/{}/void", forecast_id), &json!({}))
            .await
    }

    /// Update a forecast's probability with reason and optional agent attribution.
    pub async fn update_probability(
        &self,
        forecast_id: &str,
        req: &UpdateProbabilityRequest,
    ) -> Result<JsonValue, ApiError> {
        self.post(
            &format!("/api/forecasts/{}/update-probability", forecast_id),
            req,
        )
        .await
    }

    /// Get the authenticated user's personal forecasting stats.
    pub async fn my_stats(&self) -> Result<MyStats, ApiError> {
        self.get("/api/forecasts/my-stats").await
    }

    /// Browse public forecasts (no auth required, but auth adds personalization).
    pub async fn public_forecasts(
        &self,
        query: &ForecastQuery,
    ) -> Result<ForecastListResponse, ApiError> {
        let pairs = query.to_query_pairs();
        self.get_with_query("/api/forecasts/public", &pairs).await
    }

    // ═══════════════════════════════════════════════════════════════
    // Portfolios
    // ═══════════════════════════════════════════════════════════════

    /// Create a new portfolio.
    pub async fn create_portfolio(
        &self,
        req: &CreatePortfolioRequest,
    ) -> Result<JsonValue, ApiError> {
        self.post("/api/portfolios", req).await
    }

    /// List portfolios.
    pub async fn list_portfolios(&self) -> Result<PortfolioListResponse, ApiError> {
        self.get("/api/portfolios").await
    }

    /// Get detailed portfolio statistics including Brier aggregation and calibration.
    pub async fn portfolio_stats(&self, portfolio_id: &str) -> Result<PortfolioStats, ApiError> {
        self.get(&format!("/api/portfolios/{}/stats", portfolio_id))
            .await
    }

    /// Add a forecast to a portfolio.
    pub async fn add_to_portfolio(
        &self,
        portfolio_id: &str,
        forecast_id: &str,
    ) -> Result<JsonValue, ApiError> {
        self.post(
            &format!("/api/portfolios/{}/forecasts", portfolio_id),
            &json!({ "forecast_id": forecast_id }),
        )
        .await
    }

    /// Remove a forecast from a portfolio.
    pub async fn remove_from_portfolio(
        &self,
        portfolio_id: &str,
        forecast_id: &str,
    ) -> Result<(), ApiError> {
        self.delete(&format!(
            "/api/portfolios/{}/forecasts/{}",
            portfolio_id, forecast_id
        ))
        .await
    }

    /// Delete a portfolio.
    pub async fn delete_portfolio(&self, portfolio_id: &str) -> Result<(), ApiError> {
        self.delete(&format!("/api/portfolios/{}", portfolio_id))
            .await
    }

    /// Rename or update a portfolio's description.
    pub async fn patch_portfolio(
        &self,
        portfolio_id: &str,
        req: &PatchPortfolioRequest,
    ) -> Result<JsonValue, ApiError> {
        self.patch(&format!("/api/portfolios/{}", portfolio_id), req)
            .await
    }

    /// List forecasts in a portfolio.
    pub async fn list_portfolio_forecasts(
        &self,
        portfolio_id: &str,
    ) -> Result<PortfolioForecastsResponse, ApiError> {
        self.get(&format!("/api/portfolios/{}/forecasts", portfolio_id))
            .await
    }

    // ═══════════════════════════════════════════════════════════════
    // Polymarket
    // ═══════════════════════════════════════════════════════════════

    /// Search Polymarket for events matching a query or URL slug.
    /// Used by the composer's type-ahead — no explicit limit, server
    /// picks a small default so the suggestion strip stays snappy.
    pub async fn pm_search(&self, query: &str) -> Result<JsonValue, ApiError> {
        self.post("/api/polymarket/search", &json!({ "query": query }))
            .await
    }

    /// Search Polymarket with an explicit result cap. Used by the
    /// Dashboard's "Browse Polymarket" card where we want up to 10
    /// results in a scrollable list.
    ///
    /// v0.9.3 bugfix: the Dashboard used to build its own inline
    /// `reqwest::Client::new()` here, which shipped without our
    /// `user_agent("fermi-console/0.1.0")` and without connect/pool
    /// timeouts. Cloudflare's bot-detection heuristics on
    /// `agent-bestiary.world` reject generic-reqwest user agents on
    /// POST endpoints that echo external URLs in the body — which is
    /// exactly what /api/polymarket/search does. Routing through the
    /// pre-configured `self.http` (same client the composer typeahead
    /// uses successfully) fixes the network error Mario saw.
    pub async fn pm_search_full(&self, query: &str, limit: u32) -> Result<JsonValue, ApiError> {
        self.post(
            "/api/polymarket/search",
            &json!({ "query": query, "limit": limit }),
        )
        .await
    }

    /// Link a Polymarket market to an existing Fermi forecast. Writes
    /// the `metadata.polymarket` block that `get_forecast_handler`
    /// surfaces on load, so a subsequent `open_forecast` can hydrate
    /// the cockpit's PM state (pm_event_id, pm_market_id, pm_url, and
    /// the last cached crowd snapshot) without re-importing.
    ///
    /// This is the missing wire that made `import_polymarket_forecast`
    /// silently drop the market link on save: the cockpit stored the
    /// PM ids in RAM, `persist_backend_save` never sent them, and on
    /// reload `metadata.polymarket` was null. Fire this after the
    /// forecast row exists (i.e. after first save/publish returns a
    /// forecast_id) whenever the cockpit has PM state in memory.
    pub async fn pm_link(
        &self,
        forecast_id: &str,
        pm_event_id: &str,
        pm_market_id: &str,
    ) -> Result<JsonValue, ApiError> {
        self.post(
            "/api/polymarket/link",
            &json!({
                "forecast_id": forecast_id,
                "pm_event_id": pm_event_id,
                "pm_market_id": pm_market_id,
            }),
        )
        .await
    }

    /// Refresh the latest crowd price for a linked forecast.
    ///
    /// The server's `SnapshotRequest` REQUIRES both `pm_event_id` and
    /// `pm_market_id` — the Gamma API is keyed by (event, market) and
    /// omitting `pm_event_id` causes the request body to fail to
    /// deserialize (which historically manifested as "Polymarket is not
    /// updating" for linked forecasts like the WC winner event).
    pub async fn pm_snapshot(
        &self,
        forecast_id: &str,
        pm_event_id: &str,
        pm_market_id: &str,
    ) -> Result<JsonValue, ApiError> {
        self.post(
            "/api/polymarket/snapshot",
            &json!({
                "forecast_id": forecast_id,
                "pm_event_id": pm_event_id,
                "pm_market_id": pm_market_id,
            }),
        )
        .await
    }

    /// Check all the user's active PM-linked forecasts for resolution.
    /// Returns { checked: N, resolved: M, results: [...] }.
    pub async fn check_polymarket_resolutions(&self) -> Result<JsonValue, ApiError> {
        self.post("/api/polymarket/check-resolutions", &json!({}))
            .await
    }

    // ═══════════════════════════════════════════════════════════════
    // Leaderboard
    // ═══════════════════════════════════════════════════════════════

    /// Get the forecasting leaderboard.
    pub async fn leaderboard(
        &self,
        query: &LeaderboardQuery,
    ) -> Result<LeaderboardResponse, ApiError> {
        let pairs = query.to_query_pairs();
        self.get_with_query("/api/leaderboard", &pairs).await
    }

    // ═══════════════════════════════════════════════════════════════
    // Agents
    // ═══════════════════════════════════════════════════════════════

    /// List the Fermi orchestra roster. ABW is shared substrate:
    /// `/api/agents` returns every vertical's agents (rabble swarms,
    /// kask sim ops, adaptogen research, AR, …), which drowns the
    /// Fermi console's Agent Fleet in unrelated cards.
    ///
    /// Filters on `?orchestra=fermi`, which the server resolves against
    /// the `orchestra_fermi_members` roster view (mig-172) — the same
    /// predicate `/api/orchestras/fermi/members` and the agent Manage
    /// page's MEMBER badge use.
    ///
    /// This replaced `?tag=fermi-orchestra`. That tag is a hand-authored
    /// `metadata.tags` convention from v0.8.8 which the v0.11.2 approval
    /// flow never writes, so admin-approved third-party members were
    /// invisible in this console while their own Manage page showed them
    /// as MEMBER.
    pub async fn list_agents(&self) -> Result<JsonValue, ApiError> {
        self.get("/api/agents?orchestra=fermi&limit=200").await
    }

    /// The authoritative Fermi roster: `GET /api/orchestras/fermi/members`,
    /// served straight from the `orchestra_fermi_members` view.
    ///
    /// Used to *verify* the `?orchestra=` filter on `list_agents` actually
    /// applied. A query parameter an older server doesn't recognise is
    /// silently ignored, and the response then contains every agent on the
    /// platform — which the console would render as "104 fermi orchestra
    /// agents", confidently wrong. A missing endpoint 404s loudly; an
    /// ignored parameter does not. So membership is confirmed against a
    /// dedicated endpoint that *cannot* return a non-member.
    pub async fn list_orchestra_members(&self, orchestra: &str) -> Result<JsonValue, ApiError> {
        self.get(&format!("/api/orchestras/{orchestra}/members"))
            .await
    }

    /// Get a specific agent's card.
    pub async fn get_agent(&self, agent_id: &str) -> Result<JsonValue, ApiError> {
        self.get(&format!("/api/agents/{}", agent_id)).await
    }

    /// Execute an agent with a query.
    pub async fn execute_agent(
        &self,
        agent_id: &str,
        query: &str,
    ) -> Result<AgentExecutionResult, ApiError> {
        self.post(
            &format!("/api/agents/{}/execute", agent_id),
            &json!({ "query": query }),
        )
        .await
    }

    // ═══════════════════════════════════════════════════════════════
    // Teams
    // ═══════════════════════════════════════════════════════════════

    /// List teams the authenticated user belongs to.
    pub async fn list_teams(&self) -> Result<JsonValue, ApiError> {
        self.get("/api/teams").await
    }

    /// Get a specific team.
    pub async fn get_team(&self, team_id: &str) -> Result<JsonValue, ApiError> {
        self.get(&format!("/api/teams/{}", team_id)).await
    }

    // ═══════════════════════════════════════════════════════════════
    // Schedules
    // ═══════════════════════════════════════════════════════════════

    pub async fn list_forecast_schedules(
        &self,
        forecast_id: &str,
    ) -> Result<Vec<ForecastSchedule>, ApiError> {
        let resp: JsonValue = self
            .get(&format!("/api/forecasts/{}/schedules", forecast_id))
            .await?;
        let schedules = resp
            .get("schedules")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        Ok(schedules)
    }

    pub async fn upsert_forecast_schedule(
        &self,
        forecast_id: &str,
        req: &UpsertScheduleRequest,
    ) -> Result<JsonValue, ApiError> {
        self.put(&format!("/api/forecasts/{}/schedules", forecast_id), req)
            .await
    }

    pub async fn delete_forecast_schedule(
        &self,
        forecast_id: &str,
        schedule_id: &str,
    ) -> Result<(), ApiError> {
        self.delete(&format!(
            "/api/forecasts/{}/schedules/{}",
            forecast_id, schedule_id
        ))
        .await
    }

    pub async fn record_schedule_run(
        &self,
        forecast_id: &str,
        schedule_id: &str,
    ) -> Result<JsonValue, ApiError> {
        self.post(
            &format!(
                "/api/forecasts/{}/schedules/{}/run",
                forecast_id, schedule_id
            ),
            &json!({}),
        )
        .await
    }

    // ═══════════════════════════════════════════════════════════════
    // Workspaces (Fermi forecast ↔ ABW workspace bridge)
    // ═══════════════════════════════════════════════════════════════

    /// Spawn a workspace from the `fermi_forecast` app.
    /// Returns { workspace_id, workspace_slug, name, origin, budget, provisioned }.
    pub async fn spawn_forecast_workspace(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<JsonValue, ApiError> {
        let mut body = serde_json::json!({ "name": name });
        if let Some(desc) = description {
            body["description"] = serde_json::json!(desc);
        }
        self.post("/api/apps/fermi_forecast/workspaces", &body)
            .await
    }

    /// Post a message to a workspace (agent evidence, system events, etc.).
    pub async fn post_workspace_message(
        &self,
        workspace_id: &str,
        sender_type: &str,
        sender_id: &str,
        sender_name: Option<&str>,
        content: &str,
        message_type: &str,
        metadata: Option<&JsonValue>,
    ) -> Result<JsonValue, ApiError> {
        let mut body = serde_json::json!({
            "sender_type": sender_type,
            "sender_id": sender_id,
            "content": content,
            "message_type": message_type,
        });
        if let Some(name) = sender_name {
            body["sender_name"] = serde_json::json!(name);
        }
        if let Some(meta) = metadata {
            body["metadata"] = meta.clone();
        }
        self.post(&format!("/api/workspaces/{}/messages", workspace_id), &body)
            .await
    }

    /// Set a workspace output (typed KV for cross-workspace consumption).
    pub async fn set_workspace_output(
        &self,
        workspace_id: &str,
        key: &str,
        value: &JsonValue,
    ) -> Result<JsonValue, ApiError> {
        self.put(
            &format!("/api/workspaces/{}/outputs/{}", workspace_id, key),
            &serde_json::json!({ "value": value }),
        )
        .await
    }

    /// Read a workspace output by key.
    pub async fn get_workspace_output(
        &self,
        workspace_id: &str,
        key: &str,
    ) -> Result<JsonValue, ApiError> {
        self.get(&format!("/api/workspaces/{}/outputs/{}", workspace_id, key))
            .await
    }

    /// List all outputs for a workspace.
    pub async fn list_workspace_outputs(&self, workspace_id: &str) -> Result<JsonValue, ApiError> {
        self.get(&format!("/api/workspaces/{}/outputs", workspace_id))
            .await
    }

    /// Add a dependency edge (this workspace depends on upstream).
    pub async fn add_workspace_dependency(
        &self,
        workspace_id: &str,
        upstream_id: &str,
        dependency_type: &str,
    ) -> Result<JsonValue, ApiError> {
        self.post(
            &format!("/api/workspaces/{}/dependencies", workspace_id),
            &serde_json::json!({
                "upstream_id": upstream_id,
                "dependency_type": dependency_type,
            }),
        )
        .await
    }

    /// List all fermi_forecast workspaces the user is a member of.
    /// Returns workspace metadata + params + latest outputs.
    pub async fn list_forecast_workspaces(&self) -> Result<JsonValue, ApiError> {
        self.get("/api/apps/fermi_forecast/workspaces").await
    }

    /// Post a workspace action (decompose, research, update_distribution, etc.).
    pub async fn post_workspace_action(
        &self,
        workspace_id: &str,
        action_type: &str,
        payload: &JsonValue,
    ) -> Result<JsonValue, ApiError> {
        let mut body = payload.clone();
        if let Some(obj) = body.as_object_mut() {
            obj.insert("type".to_string(), serde_json::json!(action_type));
        }
        self.post(
            &format!("/api/workspaces/{}/actions/{}", workspace_id, action_type),
            &body,
        )
        .await
    }

    // ═══════════════════════════════════════════════════════════════
    // Health
    // ═══════════════════════════════════════════════════════════════

    /// Check if the API server is reachable.
    pub async fn health(&self) -> Result<JsonValue, ApiError> {
        // Health endpoint doesn't need auth
        let url = self.url("/api/health").await;
        let response = self.http.get(&url).send().await?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::from_status(status, &body));
        }

        let body = response.text().await?;
        serde_json::from_str(&body).map_err(ApiError::Json)
    }

    /// v0.10.6: authenticated self-diagnostic. Answers "does my JWT
    /// sub align with users.user_id for this session, and if not,
    /// what class of drift am I looking at?". Used by the composer's
    /// FK-error handler to swap the raw error text for an actionable
    /// remediation string.
    pub async fn rbac_self_check(&self) -> Result<JsonValue, ApiError> {
        self.get("/api/rbac/self-check").await
    }

    // ─── BayesOps R-2 sparkline UX ────────────────────────────────────────
    //
    // Three calls power the per-driver sparkline affordance in the forecast
    // editor (Spec 23 §4):
    //
    //   - workspace_bayesops_state: single round-trip on render and after
    //     every refit event. Returns per-driver latest snapshot + pending fit.
    //   - accept_pending_fit / reject_pending_fit: targets for the inline
    //     ✓ / ✗ buttons next to a pending-fit badge.
    //
    // Both decision endpoints require workspace membership.

    /// Read every learnable driver's BayesOps state in one call. Returns
    /// `{ workspace_id, drivers: [{ driver_name, latest_snapshot?, pending_fit? }] }`.
    /// No auth needed (read-only).
    pub async fn workspace_bayesops_state(
        &self,
        workspace_id: &str,
    ) -> Result<JsonValue, ApiError> {
        self.get(&format!("/api/workspaces/{}/bayesops/state", workspace_id))
            .await
    }

    /// Accept a pending fit: writes `params.<driver>_fitted`, marks the
    /// pending row accepted, posts an evidence event. Idempotent on rows
    /// already accepted.
    pub async fn accept_pending_fit(
        &self,
        pending_id: &str,
        notes: Option<&str>,
    ) -> Result<JsonValue, ApiError> {
        self.post(
            &format!("/api/bayesops/pending/{}/accept", pending_id),
            &serde_json::json!({ "notes": notes }),
        )
        .await
    }

    /// Reject a pending fit: marks the pending row rejected with notes,
    /// posts an evidence event. No params write.
    pub async fn reject_pending_fit(
        &self,
        pending_id: &str,
        notes: Option<&str>,
    ) -> Result<JsonValue, ApiError> {
        self.post(
            &format!("/api/bayesops/pending/{}/reject", pending_id),
            &serde_json::json!({ "notes": notes }),
        )
        .await
    }

    /// Trigger a manual refit for a workspace. Same code path as the
    /// post-commit refit hook on resolution. Useful for the editor's
    /// "refit now" button.
    pub async fn refit_workspace(&self, workspace_id: &str) -> Result<JsonValue, ApiError> {
        self.post(
            &format!("/api/workspaces/{}/refit", workspace_id),
            &serde_json::json!({}),
        )
        .await
    }

    /// Spec 23 R-3 Piece 2: fetch the unified forecast timeline.
    /// Returns rate + market traces plus a chronological event list
    /// (BayesOps fits, agent runs, upstream resolutions, system events,
    /// market polls). Drives the Trajectory tab in the cockpit.
    pub async fn forecast_timeline(&self, forecast_id: &str) -> Result<JsonValue, ApiError> {
        self.get(&format!("/api/forecasts/{}/timeline", forecast_id))
            .await
    }

    /// Phase 2.5 (cascades): fetch the redistribution waterfall for one
    /// forecast. Returns baseline probability + a list of per-trigger
    /// contributions sorted by |delta_pp| desc, so the operator can see
    /// exactly which upstream resolutions moved this forecast's
    /// probability and by how much. Drives the Provenance tab.
    pub async fn forecast_cascade_provenance(
        &self,
        forecast_id: &str,
    ) -> Result<JsonValue, ApiError> {
        self.get(&format!(
            "/api/forecasts/{}/cascade-provenance",
            forecast_id
        ))
        .await
    }

    // ════════════════════════════════════════════════════════════
    // Cascade group composition — Spec 25 authoring API.
    //
    // UI vocabulary is "cascade group" but the server contract stays
    // `relationship_group` — same primitive, older name. These methods
    // mirror /api/relationship-groups + /api/forecasts/:id/groups as
    // registered in api_server.rs.
    // ════════════════════════════════════════════════════════════

    /// List cascade groups owned by the caller. Each entry includes
    /// `member_count` computed by the server. Powers the picker's
    /// "pick an existing group" list and the future All-Groups page.
    pub async fn list_cascade_groups(&self) -> Result<JsonValue, ApiError> {
        self.get("/api/relationship-groups").await
    }

    /// Read one cascade group (kind, parameters, description) plus its
    /// member list. Powers the Group detail modal.
    pub async fn get_cascade_group(&self, group_id: &str) -> Result<JsonValue, ApiError> {
        self.get(&format!("/api/relationship-groups/{}", group_id))
            .await
    }

    /// Create a new cascade group. `kind` must be one of "mutex",
    /// "at_most_n", or "implies" — the server validates and requires
    /// `parameters.n` for at_most_n and `parameters.antecedent` +
    /// `.consequent` for implies. `group_id` is operator-chosen (slug-
    /// style, e.g. "wc_2026_winner"); server 409s on collision.
    pub async fn create_cascade_group(
        &self,
        group_id: &str,
        kind: &str,
        parameters: JsonValue,
        description: Option<&str>,
    ) -> Result<JsonValue, ApiError> {
        self.post(
            "/api/relationship-groups",
            &serde_json::json!({
                "group_id": group_id,
                "kind": kind,
                "parameters": parameters,
                "description": description,
            }),
        )
        .await
    }

    /// Which cascade groups is this forecast a member of? Read straight
    /// from `fermi_forecasts.relationship_groups`; returns a `{groups:
    /// [group_id…]}` shape. Powers the chip strip on the composer.
    pub async fn get_forecast_cascade_groups(
        &self,
        forecast_id: &str,
    ) -> Result<JsonValue, ApiError> {
        self.get(&format!("/api/forecasts/{}/groups", forecast_id))
            .await
    }

    /// Add a forecast to a cascade group. Idempotent server-side (uses
    /// `array_append` guarded by `NOT ($2 = ANY(relationship_groups))`).
    /// Fires on the operator's "add" click in the picker.
    pub async fn add_forecast_to_cascade_group(
        &self,
        forecast_id: &str,
        group_id: &str,
    ) -> Result<JsonValue, ApiError> {
        // POST body is unused server-side but must be valid JSON.
        self.post(
            &format!("/api/forecasts/{}/groups/{}", forecast_id, group_id),
            &serde_json::json!({}),
        )
        .await
    }

    /// Remove a forecast from a cascade group. Fires on the chip's
    /// remove-x click. Idempotent server-side (`array_remove`).
    pub async fn remove_forecast_from_cascade_group(
        &self,
        forecast_id: &str,
        group_id: &str,
    ) -> Result<(), ApiError> {
        self.delete(&format!(
            "/api/forecasts/{}/groups/{}",
            forecast_id, group_id
        ))
        .await
    }

    /// Phase 2.5 Slice B: dry-run propagate for the cascade detail
    /// panel's preview. "What would happen if I resolved this member
    /// NO?" Returns a `PropagateResult` with per-forecast deltas but
    /// writes nothing to the DB. Powers the preview table in the
    /// detail panel.
    ///
    /// Server contract:
    /// `POST /api/relationship-groups/:group_id/propagate`
    /// with `{ trigger_forecast_id, trigger_kind: "resolved", outcome,
    /// dry_run: true }`. See
    /// src/handlers/relationships/groups.rs::preview_group_propagation_handler.
    pub async fn preview_cascade_propagation(
        &self,
        group_id: &str,
        trigger_forecast_id: &str,
        outcome: bool,
    ) -> Result<JsonValue, ApiError> {
        self.post(
            &format!("/api/relationship-groups/{}/propagate", group_id),
            &serde_json::json!({
                "trigger_forecast_id": trigger_forecast_id,
                "trigger_kind": "resolved",
                "outcome": outcome,
                "dry_run": true,
            }),
        )
        .await
    }

    // ═══════════════════════════════════════════════════════════════
    // Forecast relationships — generalized inter-forecast dependencies.
    //
    // The cockpit uses these to surface "Cascade to N forecasts" after
    // a resolve, when the resolved forecast is part of one or more
    // declared relationships (e.g. WC sims mutex group).
    // ═══════════════════════════════════════════════════════════════

    /// Declare a new forecast relationship (mutex / implies /
    /// at_most_n / etc.). Server validates kind + forecast_ids and
    /// returns the created row. Powers the Portfolio panel's cascade
    /// declaration UI (Sprint B).
    pub async fn create_relationship(
        &self,
        kind: &str,
        forecast_ids: &[String],
        parameters: JsonValue,
        description: Option<&str>,
    ) -> Result<JsonValue, ApiError> {
        let body = json!({
            "kind": kind,
            "forecast_ids": forecast_ids,
            "parameters": parameters,
            "description": description,
        });
        self.post("/api/forecast-relationships", &body).await
    }

    /// List all relationships the calling user owns (no forecast_id
    /// filter). Used to hydrate the Portfolio panel's relationships
    /// sub-panel with every declared cascade rule.
    pub async fn list_all_relationships(&self) -> Result<JsonValue, ApiError> {
        self.get("/api/forecast-relationships").await
    }

    /// Delete a relationship by id. Owner-only (server-enforced).
    pub async fn delete_relationship(&self, relationship_id: &str) -> Result<(), ApiError> {
        self.delete(&format!("/api/forecast-relationships/{}", relationship_id))
            .await
    }

    /// List relationships involving a given forecast. Returns
    /// `{relationships: [{id, kind, forecast_ids, ...}], count}`.
    pub async fn list_relationships_for_forecast(
        &self,
        forecast_id: &str,
    ) -> Result<JsonValue, ApiError> {
        self.get(&format!(
            "/api/forecast-relationships?forecast_id={}",
            forecast_id
        ))
        .await
    }

    /// Fire propagation on a relationship. The `req` body shape:
    /// `{trigger_forecast_id, trigger_kind: "resolved" | "updated", outcome?}`.
    /// Returns `{n_updated, deltas: [{forecast_id, previous_probability,
    /// new_probability, delta_pp}], note?}`.
    pub async fn propagate_relationship(
        &self,
        relationship_id: &str,
        req: &JsonValue,
    ) -> Result<JsonValue, ApiError> {
        self.post(
            &format!("/api/forecast-relationships/{}/propagate", relationship_id),
            req,
        )
        .await
    }

    // ═══════════════════════════════════════════════════════════════
    // Pending cascades — operator-gated cascade queue.
    //
    // When a forecast resolves (manually OR via upstream workspace
    // resolution), the server queues a pending_cascade row per
    // non-archived relationship. The console badge surfaces the
    // count; the queue sheet lets the operator Apply/Dismiss each.
    // ═══════════════════════════════════════════════════════════════

    /// List pending cascades for the calling user. Returns
    /// `{pending: [...], count, status}` where each entry includes the
    /// trigger forecast, relationship info, and the dry-run projected
    /// deltas (proposed_snapshot).
    pub async fn list_pending_cascades(&self) -> Result<JsonValue, ApiError> {
        self.get("/api/pending-cascades?status=pending").await
    }

    pub async fn apply_pending_cascade(
        &self,
        cascade_id: &str,
        notes: Option<&str>,
    ) -> Result<JsonValue, ApiError> {
        let body = serde_json::json!({ "notes": notes });
        self.post(
            &format!("/api/pending-cascades/{}/apply", cascade_id),
            &body,
        )
        .await
    }

    pub async fn dismiss_pending_cascade(
        &self,
        cascade_id: &str,
        notes: Option<&str>,
    ) -> Result<JsonValue, ApiError> {
        let body = serde_json::json!({ "notes": notes });
        self.post(
            &format!("/api/pending-cascades/{}/dismiss", cascade_id),
            &body,
        )
        .await
    }

    // ═══════════════════════════════════════════════════════════════
    // Collaboration — forecast/portfolio sharing (Spec 24 §3.4)
    // ═══════════════════════════════════════════════════════════════

    /// List the `object_shares` rows on a forecast. Caller must be able
    /// to view the forecast.
    pub async fn list_forecast_shares(
        &self,
        forecast_id: &str,
    ) -> Result<ShareListResponse, ApiError> {
        self.get(&format!("/api/forecasts/{}/shares", forecast_id))
            .await
    }

    /// Add a share to a forecast (caller must have admin access).
    /// Idempotent server-side: repeat POSTs upgrade/downgrade the grant.
    pub async fn add_forecast_share(
        &self,
        forecast_id: &str,
        body: &ShareRequest,
    ) -> Result<ShareEntry, ApiError> {
        self.post(&format!("/api/forecasts/{}/shares", forecast_id), body)
            .await
    }

    /// Revoke a forecast share by its `object_shares` id.
    pub async fn revoke_forecast_share(
        &self,
        forecast_id: &str,
        share_id: &str,
    ) -> Result<(), ApiError> {
        self.delete(&format!(
            "/api/forecasts/{}/shares/{}",
            forecast_id, share_id
        ))
        .await
    }

    pub async fn list_portfolio_shares(
        &self,
        portfolio_id: &str,
    ) -> Result<ShareListResponse, ApiError> {
        self.get(&format!("/api/portfolios/{}/shares", portfolio_id))
            .await
    }

    pub async fn add_portfolio_share(
        &self,
        portfolio_id: &str,
        body: &ShareRequest,
    ) -> Result<ShareEntry, ApiError> {
        self.post(&format!("/api/portfolios/{}/shares", portfolio_id), body)
            .await
    }

    pub async fn revoke_portfolio_share(
        &self,
        portfolio_id: &str,
        share_id: &str,
    ) -> Result<(), ApiError> {
        self.delete(&format!(
            "/api/portfolios/{}/shares/{}",
            portfolio_id, share_id
        ))
        .await
    }

    // ═══════════════════════════════════════════════════════════════
    // Collaboration — teams (Spec 24 §3.4)
    // ═══════════════════════════════════════════════════════════════

    /// List teams the authenticated user belongs to (typed wrapper over
    /// the existing `list_teams` which returns raw JSON).
    pub async fn list_my_teams(&self) -> Result<TeamListResponse, ApiError> {
        self.get("/api/teams").await
    }

    /// Get a team plus its member roster.
    pub async fn get_team_detail(&self, team_id: &str) -> Result<TeamDetail, ApiError> {
        self.get(&format!("/api/teams/{}", team_id)).await
    }

    /// Create a new team. Returns the created team as raw JSON (the
    /// server's response carries extra composition fields).
    pub async fn create_team(&self, body: &CreateTeamRequest) -> Result<JsonValue, ApiError> {
        self.post("/api/teams", body).await
    }

    /// Directly add a member to a team (owner/admin only). For unknown
    /// users prefer `invite_to_team`.
    pub async fn add_team_member(
        &self,
        team_id: &str,
        body: &AddMemberRequest,
    ) -> Result<JsonValue, ApiError> {
        self.post(&format!("/api/teams/{}/members", team_id), body)
            .await
    }

    /// Remove a member from a team.
    pub async fn remove_team_member(&self, team_id: &str, member_id: &str) -> Result<(), ApiError> {
        self.delete(&format!("/api/teams/{}/members/{}", team_id, member_id))
            .await
    }

    /// Delete a team. Only the team OWNER can delete (server enforces
    /// via `WHERE owner_id = $1`). Cascades to team_members via FK.
    /// Object shares that target this team are not automatically
    /// revoked — they become orphan rows the visibility model
    /// gracefully ignores (no team_members row → no access).
    pub async fn delete_team(&self, team_id: &str) -> Result<(), ApiError> {
        self.delete(&format!("/api/teams/{}", team_id)).await
    }

    /// Change a member's role (owner/admin only).
    pub async fn update_team_member_role(
        &self,
        team_id: &str,
        member_id: &str,
        role: &str,
    ) -> Result<JsonValue, ApiError> {
        self.put(
            &format!("/api/teams/{}/members/{}", team_id, member_id),
            &json!({ "role": role }),
        )
        .await
    }

    /// Replace a member's capability set (owner/admin only; the server
    /// 403s on any attempt to edit the team owner).
    ///
    /// Whole-set replacement, not add/remove — `capabilities` must be
    /// the complete desired set, so a caller that sends a stale set
    /// silently revokes whatever it omitted.
    pub async fn set_team_member_capabilities(
        &self,
        team_id: &str,
        member_id: &str,
        capabilities: &[String],
    ) -> Result<JsonValue, ApiError> {
        self.put(
            &format!("/api/teams/{}/members/{}/capabilities", team_id, member_id),
            &json!({ "capabilities": capabilities }),
        )
        .await
    }

    // ═══════════════════════════════════════════════════════════════
    // Collaboration v2 — provenance, attribution, activity (Spec 26)
    // ═══════════════════════════════════════════════════════════════

    /// Complete access picture for one forecast: my path in, every direct
    /// share (with grantor, timestamp and team roster), every share
    /// *inherited* from a containing portfolio, and the flattened list of
    /// humans who can actually see it.
    pub async fn forecast_access(&self, forecast_id: &str) -> Result<AccessSummary, ApiError> {
        self.get(&format!("/api/forecasts/{}/access", forecast_id))
            .await
    }

    /// Portfolio counterpart. Adds `cascades_to`: how many member
    /// forecasts inherit the portfolio's grants.
    pub async fn portfolio_access(&self, portfolio_id: &str) -> Result<AccessSummary, ApiError> {
        self.get(&format!("/api/portfolios/{}/access", portfolio_id))
            .await
    }

    /// Attributed history of one forecast.
    pub async fn forecast_activity(
        &self,
        forecast_id: &str,
        limit: u32,
    ) -> Result<ActivityResponse, ApiError> {
        self.get(&format!(
            "/api/forecasts/{}/activity?limit={}",
            forecast_id, limit
        ))
        .await
    }

    /// Portfolio feed: portfolio-level events plus every member forecast.
    pub async fn portfolio_activity(
        &self,
        portfolio_id: &str,
        limit: u32,
    ) -> Result<ActivityResponse, ApiError> {
        self.get(&format!(
            "/api/portfolios/{}/activity?limit={}",
            portfolio_id, limit
        ))
        .await
    }

    /// Team feed over the team's whole shared surface.
    ///
    /// `actor` narrows to one teammate — the "which team members did which
    /// things" query. `kind` takes a comma-separated list of event kinds.
    pub async fn team_activity(
        &self,
        team_id: &str,
        limit: u32,
        actor: Option<&str>,
        kind: Option<&str>,
    ) -> Result<ActivityResponse, ApiError> {
        let mut path = format!("/api/teams/{}/activity?limit={}", team_id, limit);
        if let Some(a) = actor {
            path.push_str(&format!("&actor={}", encode_query_value(a)));
        }
        if let Some(k) = kind {
            path.push_str(&format!("&kind={}", encode_query_value(k)));
        }
        self.get(&path).await
    }

    /// Per-member contribution roll-up for a team.
    pub async fn team_contributions(
        &self,
        team_id: &str,
    ) -> Result<TeamContributionsResponse, ApiError> {
        self.get(&format!("/api/teams/{}/contributions", team_id))
            .await
    }

    /// A forecast's commit history — who changed what, when, and why.
    ///
    /// View-gated server-side: if you can read a forecast you can read how
    /// it got that way.
    pub async fn forecast_history(
        &self,
        forecast_id: &str,
        limit: u32,
    ) -> Result<ForecastHistoryResponse, ApiError> {
        self.get(&format!(
            "/api/forecasts/{}/history?limit={}",
            forecast_id, limit
        ))
        .await
    }

    /// What one commit changed. Diffs against its parent by default;
    /// `against` compares two arbitrary revisions.
    pub async fn forecast_diff(
        &self,
        forecast_id: &str,
        sha: &str,
        against: Option<&str>,
    ) -> Result<ForecastDiffResponse, ApiError> {
        let mut path = format!("/api/forecasts/{}/history/{}", forecast_id, sha);
        if let Some(a) = against {
            path.push_str(&format!("?against={}", encode_query_value(a)));
        }
        self.get(&path).await
    }

    /// Restore a forecast's analysis to an earlier revision.
    ///
    /// Edit-gated, and writes a forward commit rather than rewriting
    /// history — so a revert is itself revertible. Restores probability,
    /// drivers, evidence and FPL only: the server refuses on resolved or
    /// voided forecasts, because their score is frozen and reverting the
    /// analysis behind it would make that score unreproducible.
    pub async fn revert_forecast(
        &self,
        forecast_id: &str,
        body: &RevertRequest,
    ) -> Result<JsonValue, ApiError> {
        self.post(&format!("/api/forecasts/{}/revert", forecast_id), body)
            .await
    }

    /// Every annotation on a forecast, plus per-driver open counts.
    ///
    /// View-gated, matching history: if you can read the forecast you can
    /// read what people have said about it. An objection visible only to
    /// editors would leave readers trusting a number the team is disputing.
    pub async fn forecast_annotations(
        &self,
        forecast_id: &str,
    ) -> Result<AnnotationsResponse, ApiError> {
        self.get(&format!("/api/forecasts/{}/annotations", forecast_id))
            .await
    }

    /// Raise an objection against a driver.
    ///
    /// Only **view** access is required, deliberately: a view grant exists
    /// so people can read and react, and annotating mutates no forecast
    /// state. It is the cheapest reversible act in the product.
    pub async fn create_annotation(
        &self,
        forecast_id: &str,
        body: &CreateAnnotationRequest,
    ) -> Result<JsonValue, ApiError> {
        self.post(&format!("/api/forecasts/{}/annotations", forecast_id), body)
            .await
    }

    /// Answer an annotation — `accepted` or `declined`. Requires **edit**,
    /// because closing someone else's objection is a claim about what the
    /// forecast now says.
    pub async fn resolve_annotation(
        &self,
        forecast_id: &str,
        annotation_id: &str,
        body: &ResolveAnnotationRequest,
    ) -> Result<JsonValue, ApiError> {
        self.post(
            &format!(
                "/api/forecasts/{}/annotations/{}/resolve",
                forecast_id, annotation_id
            ),
            body,
        )
        .await
    }

    /// Hard-delete an annotation. **Author only** — for genuine mistakes.
    /// Anyone else's route out is `declined`, which stays on the record.
    pub async fn delete_annotation(
        &self,
        forecast_id: &str,
        annotation_id: &str,
    ) -> Result<(), ApiError> {
        self.delete(&format!(
            "/api/forecasts/{}/annotations/{}",
            forecast_id, annotation_id
        ))
        .await
    }

    /// The team's detected ops board (Spec 27).
    ///
    /// Nothing is stored server-side: every op is a condition currently
    /// true of the team's shared surface. Safe and cheap to re-poll —
    /// re-polling is how ops disappear when they're done.
    pub async fn team_ops(&self, team_id: &str) -> Result<OpsResponse, ApiError> {
        self.get(&format!("/api/teams/{}/ops", team_id)).await
    }

    /// Canonical inventory of what is shared with a team, and by whom.
    ///
    /// Server-side truth, replacing the console's old client-side filter
    /// over the caller's OWN forecasts — which structurally could not see
    /// work a teammate had shared with the team.
    pub async fn team_shared(&self, team_id: &str) -> Result<TeamSharedResponse, ApiError> {
        self.get(&format!("/api/teams/{}/shared", team_id)).await
    }

    // ════════════════════════════════════════════════════════════
    // Collaboration — invites (Spec 24 §3.4)
    // ════════════════════════════════════════════════════════════

    pub async fn invite_to_forecast(
        &self,
        forecast_id: &str,
        body: &InviteRequest,
    ) -> Result<JsonValue, ApiError> {
        self.post(&format!("/api/forecasts/{}/invites", forecast_id), body)
            .await
    }

    pub async fn invite_to_portfolio(
        &self,
        portfolio_id: &str,
        body: &InviteRequest,
    ) -> Result<JsonValue, ApiError> {
        self.post(&format!("/api/portfolios/{}/invites", portfolio_id), body)
            .await
    }

    pub async fn invite_to_team(
        &self,
        team_id: &str,
        body: &InviteRequest,
    ) -> Result<JsonValue, ApiError> {
        self.post(&format!("/api/teams/{}/invites", team_id), body)
            .await
    }

    /// List the calling user's pending invites (the Inbox feed).
    pub async fn list_my_invites(&self) -> Result<InviteListResponse, ApiError> {
        self.get("/api/me/invites").await
    }

    /// List invites the calling user has SENT (all statuses). Used by
    /// the console to surface outbound invitations that haven't yet
    /// been accepted or declined — without this endpoint the send
    /// action is fire-and-forget from the operator's perspective.
    pub async fn list_my_sent_invites(&self) -> Result<InviteListResponse, ApiError> {
        self.get("/api/me/invites/sent").await
    }

    /// List invites (pending + terminal) attached to a specific forecast.
    /// Caller must be a forecast admin. Powers the "Pending invites"
    /// section of the Access tab.
    pub async fn list_forecast_invites(
        &self,
        forecast_id: &str,
    ) -> Result<InviteListResponse, ApiError> {
        self.get(&format!("/api/forecasts/{}/invites", forecast_id))
            .await
    }

    /// Symmetric with `list_forecast_invites` for portfolios.
    pub async fn list_portfolio_invites(
        &self,
        portfolio_id: &str,
    ) -> Result<InviteListResponse, ApiError> {
        self.get(&format!("/api/portfolios/{}/invites", portfolio_id))
            .await
    }

    /// List invites attached to a team. Caller must be an owner/admin.
    pub async fn list_team_invites(&self, team_id: &str) -> Result<InviteListResponse, ApiError> {
        self.get(&format!("/api/teams/{}/invites", team_id)).await
    }

    /// Accept an invite. Materialises the grant server-side.
    pub async fn accept_invite(&self, invite_id: &str) -> Result<JsonValue, ApiError> {
        self.post(&format!("/api/invites/{}/accept", invite_id), &json!({}))
            .await
    }

    /// Decline an invite.
    pub async fn decline_invite(&self, invite_id: &str) -> Result<JsonValue, ApiError> {
        self.post(&format!("/api/invites/{}/decline", invite_id), &json!({}))
            .await
    }

    /// Revoke an invite (inviter or team admin).
    pub async fn revoke_invite(&self, invite_id: &str) -> Result<(), ApiError> {
        self.delete(&format!("/api/invites/{}", invite_id)).await
    }

    // ═══════════════════════════════════════════════════════════════
    // Collaboration — user lookup (Spec 24 §3.4)
    // ═══════════════════════════════════════════════════════════════

    /// Exact, case-insensitive email lookup for the share typeahead.
    /// Returns `Ok(None)` when no account exists (→ send an email invite)
    /// and `Ok(Some(user))` for an instant share.
    pub async fn lookup_user(&self, email: &str) -> Result<Option<UserSummary>, ApiError> {
        match self
            .get_with_query::<UserSummary>("/api/users/lookup", &[("email".into(), email.into())])
            .await
        {
            Ok(u) => Ok(Some(u)),
            Err(ApiError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ApiConfig::default();
        assert_eq!(config.base_url, "https://agent-bestiary.world");
        assert!(config.api_key.is_none());
        assert!(!config.is_authenticated());
    }

    #[test]
    fn test_config_builder() {
        let config = ApiConfig::default()
            .with_base_url("http://localhost:3000")
            .with_api_key("test-key-123");
        assert_eq!(config.base_url, "http://localhost:3000");
        assert_eq!(config.api_key.as_deref(), Some("test-key-123"));
        assert!(config.is_authenticated());
    }

    #[test]
    fn test_forecast_query_active() {
        let q = ForecastQuery::active();
        let pairs = q.to_query_pairs();
        assert!(pairs.iter().any(|(k, v)| k == "status" && v == "active"));
    }

    #[test]
    fn test_forecast_query_resolved() {
        let q = ForecastQuery::resolved();
        let pairs = q.to_query_pairs();
        assert!(pairs.iter().any(|(k, v)| k == "status" && v == "resolved"));
        assert!(pairs.iter().any(|(k, v)| k == "sort" && v == "brier_score"));
        assert!(pairs.iter().any(|(k, v)| k == "order" && v == "asc"));
    }

    #[test]
    fn test_error_mapping() {
        assert!(matches!(
            ApiError::from_status(401, ""),
            ApiError::NotAuthenticated
        ));
        assert!(matches!(
            ApiError::from_status(403, "nope"),
            ApiError::Forbidden(_)
        ));
        assert!(matches!(
            ApiError::from_status(404, "gone"),
            ApiError::NotFound(_)
        ));
        assert!(matches!(
            ApiError::from_status(429, ""),
            ApiError::RateLimited { .. }
        ));
        assert!(matches!(
            ApiError::from_status(500, "boom"),
            ApiError::Server(_)
        ));
    }
}
