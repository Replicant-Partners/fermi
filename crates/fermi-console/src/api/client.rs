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
    /// Sprint 2 starts producing share rows. Always 0 today.
    #[serde(default)]
    pub share_count: Option<i64>,
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
    pub async fn pm_search(&self, query: &str) -> Result<JsonValue, ApiError> {
        self.post("/api/polymarket/search", &json!({ "query": query }))
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

    /// List all available agents.
    pub async fn list_agents(&self) -> Result<JsonValue, ApiError> {
        self.get("/api/agents").await
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

    // ═══════════════════════════════════════════════════════════════
    // Forecast relationships — generalized inter-forecast dependencies.
    //
    // The cockpit uses these to surface "Cascade to N forecasts" after
    // a resolve, when the resolved forecast is part of one or more
    // declared relationships (e.g. WC sims mutex group).
    // ═══════════════════════════════════════════════════════════════

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

    // ═══════════════════════════════════════════════════════════════
    // Collaboration — invites (Spec 24 §3.4)
    // ═══════════════════════════════════════════════════════════════

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
