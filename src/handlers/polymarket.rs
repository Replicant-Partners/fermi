//! Polymarket Integration Handlers
//!
//! Server-side endpoints that proxy Polymarket's Gamma API and manage
//! market observations for Fermi forecasts. The console never calls
//! Polymarket directly — all data flows through these handlers.
//!
//! Architecture:
//!   - Stateless: each request is independent, no PM session state
//!   - Append-only: observations are inserted, never mutated
//!   - Credit-charged: search and snapshot operations cost credits
//!   - Server-side: ABW fetches from Gamma, console calls ABW
//!
//! Endpoints:
//!   POST /api/polymarket/search          — search for matching PM events
//!   POST /api/polymarket/snapshot        — refresh price for a linked market
//!   POST /api/polymarket/link            — link a PM market to a Fermi forecast
//!   GET  /api/polymarket/observations    — get observation time series
//!   POST /api/polymarket/check-resolutions — check if linked markets resolved
//!   POST /api/polymarket/import          — import PM question as new Fermi forecast

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sqlx::Row;

use crate::AppState;
use fermi::polymarket::{
    compute_divergence_pp, format_probability, format_volume, interpret_divergence,
    ConfidenceSignal, GammaClient, MarketMatch,
};
use fermi_auth::visibility::can_edit;
use fermi_auth::{AuthPrincipal, ObjectType, Visibility};
use sqlx::PgPool;

/// Confirm that `principal` is allowed to write an observation linked to
/// `forecast_id`. Returns the caller's ownership context (owner_id +
/// visibility) on success so callers can re-use it without a second
/// SELECT. Maps the ACL result to HTTP status codes:
///
///   404 — forecast doesn't exist.
///   403 — forecast exists but caller lacks edit permission.
///   500 — DB error.
///
/// A market observation is a write attributed to a specific forecast
/// (it appears in that forecast's trajectory, feeds its divergence
/// history, and shows up in its Brier-adjacent analytics). Historically
/// this endpoint accepted any authenticated caller and any forecast_id,
/// so anyone with an API key could spray observations onto anyone
/// else's forecast timeline. `can_edit` mirrors the check used by
/// `resolve_forecast_handler` — owner, admin, or share with edit
/// permission.
async fn require_forecast_edit(
    pool: &PgPool,
    principal: &AuthPrincipal,
    forecast_id: &str,
) -> Result<(String, Visibility), (StatusCode, String)> {
    let acl_row = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility
           FROM fermi_forecasts WHERE id = $1",
    )
    .bind(forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = acl_row.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Forecast {} not found", forecast_id),
        )
    })?;
    let owner_id: String = row.try_get("owner_id").unwrap_or_default();
    let visibility_str: String = row.try_get("visibility").unwrap_or_default();
    let visibility = Visibility::from_legacy(&visibility_str);

    let granted = can_edit(
        pool,
        principal,
        ObjectType::Forecast,
        forecast_id,
        &owner_id,
        visibility,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !granted {
        return Err((
            StatusCode::FORBIDDEN,
            "Edit access denied for this forecast".into(),
        ));
    }
    Ok((owner_id, visibility))
}

// ═══════════════════════════════════════════════════════════════════
// Request / Response Types
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_true")]
    pub active_only: bool,
}

fn default_limit() -> usize {
    10
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct SnapshotRequest {
    pub pm_event_id: String,
    pub pm_market_id: String,
    /// Optional: link this snapshot to a Fermi forecast
    pub forecast_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LinkRequest {
    pub forecast_id: String,
    pub pm_event_id: String,
    pub pm_market_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    pub pm_event_id: String,
    pub pm_market_id: String,
    /// Override the question text (default: use PM question)
    pub question_text: Option<String>,
    /// Optional portfolio to add the forecast to
    pub portfolio_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ObservationsQuery {
    pub forecast_id: Option<String>,
    pub pm_market_id: Option<String>,
    #[serde(default = "default_obs_limit")]
    pub limit: i64,
}

fn default_obs_limit() -> i64 {
    50
}

#[derive(Debug, Serialize)]
pub struct MarketMatchResponse {
    pub pm_event_id: String,
    pub pm_market_id: String,
    pub event_title: String,
    pub question: String,
    pub market_price: f64,
    pub market_price_pct: String,
    pub midpoint_price: f64,
    pub bid_price: f64,
    pub ask_price: f64,
    pub spread: f64,
    pub volume_total: f64,
    pub volume_total_fmt: String,
    pub volume_24h: f64,
    pub volume_24h_fmt: String,
    pub liquidity: f64,
    pub liquidity_fmt: String,
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
    pub confidence_signal: String,
    pub confidence_quality: f64,
    pub group_item_title: Option<String>,
    pub slug: String,
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/polymarket/search
// ═══════════════════════════════════════════════════════════════════
//
// Search Polymarket for events matching a natural language query.
// Returns matched markets with prices, volume, and confidence signals.
// Each result is also recorded as an observation (append-only).

pub async fn search_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<SearchRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let query = body.query.trim().to_string();

    if query.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Query cannot be empty".into()));
    }

    // ── Charge credit ──────────────────────────────────────────
    let wallet = fermi_auth::get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    if wallet.balance <= 0 {
        return Err((StatusCode::PAYMENT_REQUIRED, "Insufficient credits".into()));
    }

    fermi::gas::charge_gas(
        &state.db,
        wallet.wallet_id,
        1, // 1 credit per search
        "polymarket_search",
        &format!("Polymarket search: {}", &query[..query.len().min(80)]),
        None,
    )
    .await?;

    // ── Search Gamma API ───────────────────────────────────────
    let gamma = GammaClient::new();
    let events = gamma.search_events(&query, body.limit).await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Polymarket API error: {}", e),
        )
    })?;

    // ── Process results ────────────────────────────────────────
    let mut matches: Vec<MarketMatchResponse> = Vec::new();

    for event in &events {
        for market in &event.markets {
            // Skip inactive/archived markets if active_only
            if body.active_only && (!market.active || market.archived) {
                continue;
            }

            let processed = fermi::polymarket::process_market_public(event, market);

            // Record observation (append-only)
            let obs_id = uuid::Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO fermi_market_observations (
                    id, pm_event_id, pm_market_id, pm_condition_id, pm_slug,
                    pm_question, pm_event_title,
                    market_price, bid_price, ask_price, midpoint_price, spread,
                    volume_total, volume_24h, liquidity,
                    price_change_1h, price_change_1d, price_change_1w, price_change_1m,
                    pm_end_date, pm_active, pm_closed, pm_resolved, pm_outcome,
                    confidence_signal, observation_type, observer_id,
                    tags, metadata
                ) VALUES (
                    $1, $2, $3, $4, $5, $6, $7,
                    $8, $9, $10, $11, $12,
                    $13, $14, $15,
                    $16, $17, $18, $19,
                    $20::timestamptz, $21, $22, $23, $24,
                    $25, 'search', $26,
                    $27, $28
                )",
            )
            .bind(&obs_id)
            .bind(&processed.pm_event_id)
            .bind(&processed.pm_market_id)
            .bind(&processed.condition_id)
            .bind(&processed.slug)
            .bind(&processed.question)
            .bind(&processed.event_title)
            .bind(processed.market_price as f32)
            .bind(processed.bid_price as f32)
            .bind(processed.ask_price as f32)
            .bind(processed.midpoint_price as f32)
            .bind(processed.spread as f32)
            .bind(processed.volume_total as f32)
            .bind(processed.volume_24h as f32)
            .bind(processed.liquidity as f32)
            .bind(processed.price_change_1h.map(|v| v as f32))
            .bind(processed.price_change_1d.map(|v| v as f32))
            .bind(processed.price_change_1w.map(|v| v as f32))
            .bind(processed.price_change_1m.map(|v| v as f32))
            .bind(&processed.end_date)
            .bind(processed.active)
            .bind(processed.closed)
            .bind(processed.resolved)
            .bind(&processed.outcome)
            .bind(processed.confidence_signal.db_str())
            .bind(&user_id)
            .bind(&processed.tags)
            .bind(json!({"search_query": &query}))
            .execute(&state.db)
            .await;

            matches.push(MarketMatchResponse {
                pm_event_id: processed.pm_event_id,
                pm_market_id: processed.pm_market_id,
                event_title: processed.event_title,
                question: processed.question,
                market_price: processed.market_price,
                market_price_pct: format_probability(processed.market_price),
                midpoint_price: processed.midpoint_price,
                bid_price: processed.bid_price,
                ask_price: processed.ask_price,
                spread: processed.spread,
                volume_total: processed.volume_total,
                volume_total_fmt: format_volume(processed.volume_total),
                volume_24h: processed.volume_24h,
                volume_24h_fmt: format_volume(processed.volume_24h),
                liquidity: processed.liquidity,
                liquidity_fmt: format_volume(processed.liquidity),
                price_change_1h: processed.price_change_1h,
                price_change_1d: processed.price_change_1d,
                price_change_1w: processed.price_change_1w,
                price_change_1m: processed.price_change_1m,
                end_date: processed.end_date,
                active: processed.active,
                closed: processed.closed,
                resolved: processed.resolved,
                outcome: processed.outcome,
                condition_id: processed.condition_id,
                tags: processed.tags,
                polymarket_url: processed.polymarket_url,
                confidence_signal: processed.confidence_signal.label().to_string(),
                confidence_quality: processed.confidence_signal.quality_score(),
                group_item_title: processed.group_item_title,
                slug: processed.slug,
            });
        }
    }

    Ok(Json(json!({
        "matches": matches,
        "search_query": query,
        "results_count": matches.len(),
        "events_searched": events.len(),
        "credits_charged": 1
    })))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/polymarket/snapshot
// ═══════════════════════════════════════════════════════════════════
//
// Fetch the current price for a specific Polymarket market.
// Optionally links the snapshot to a Fermi forecast (for divergence tracking).
// Records an append-only observation row.

pub async fn snapshot_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<SnapshotRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // ── Charge credit ──────────────────────────────────────────
    let wallet = fermi_auth::get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    if wallet.balance <= 0 {
        return Err((StatusCode::PAYMENT_REQUIRED, "Insufficient credits".into()));
    }

    fermi::gas::charge_gas(
        &state.db,
        wallet.wallet_id,
        1,
        "polymarket_snapshot",
        &format!(
            "PM snapshot: event={} market={}",
            body.pm_event_id, body.pm_market_id
        ),
        None,
    )
    .await?;

    // ── Fetch from Gamma API ───────────────────────────────────
    let gamma = GammaClient::new();
    let market_match = gamma
        .snapshot_market(&body.pm_event_id, &body.pm_market_id)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                format!("Polymarket API error: {}", e),
            )
        })?;

    // ── ACL: caller must be able to edit the forecast they're
    //    attributing this observation to. Missing forecast_id is
    //    allowed (ambient search-style snapshot with no forecast link).
    let (fermi_prob, divergence) = if let Some(ref fc_id) = body.forecast_id {
        require_forecast_edit(&state.db, &principal, fc_id).await?;

        let row = sqlx::query("SELECT predicted_probability FROM fermi_forecasts WHERE id = $1")
            .bind(fc_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if let Some(row) = row {
            let prob: f32 = row.get("predicted_probability");
            let div = compute_divergence_pp(prob as f64, market_match.market_price);
            (Some(prob as f64), Some(div))
        } else {
            // Race: forecast was deleted between the ACL check and this
            // SELECT. Treat as not-linked rather than 500'ing.
            (None, None)
        }
    } else {
        (None, None)
    };

    // ── Record observation (append-only) ───────────────────────
    let obs_id = uuid::Uuid::new_v4().to_string();
    let obs_type = if body.forecast_id.is_some() {
        "refresh"
    } else {
        "search"
    };

    // Sanitise the incoming forecast_id: the console's cockpit sends
    // `self.forecast_id.clone().unwrap_or_default()` on every poll, so
    // uncommitted forecasts arrive as `Some("")`. That would fail the
    // observations table's FK to fermi_forecasts.id and drop the whole
    // write. Normalise to None so those snapshots persist as orphaned
    // ambient observations instead of being silently discarded.
    let effective_forecast_id: Option<String> = body
        .forecast_id
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .cloned();

    let insert_result = sqlx::query(
        "INSERT INTO fermi_market_observations (
            id, forecast_id,
            pm_event_id, pm_market_id, pm_condition_id, pm_slug,
            pm_question, pm_event_title,
            market_price, bid_price, ask_price, midpoint_price, spread,
            volume_total, volume_24h, liquidity,
            price_change_1h, price_change_1d, price_change_1w, price_change_1m,
            pm_end_date, pm_active, pm_closed, pm_resolved, pm_outcome,
            fermi_probability, divergence_pp,
            confidence_signal, observation_type, observer_id
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8,
            $9, $10, $11, $12, $13,
            $14, $15, $16,
            $17, $18, $19, $20,
            $21::timestamptz, $22, $23, $24, $25,
            $26, $27,
            $28, $29, $30
        )",
    )
    .bind(&obs_id)
    .bind(&effective_forecast_id)
    .bind(&market_match.pm_event_id)
    .bind(&market_match.pm_market_id)
    .bind(&market_match.condition_id)
    .bind(&market_match.slug)
    .bind(&market_match.question)
    .bind(&market_match.event_title)
    .bind(market_match.market_price as f32)
    .bind(market_match.bid_price as f32)
    .bind(market_match.ask_price as f32)
    .bind(market_match.midpoint_price as f32)
    .bind(market_match.spread as f32)
    .bind(market_match.volume_total as f32)
    .bind(market_match.volume_24h as f32)
    .bind(market_match.liquidity as f32)
    .bind(market_match.price_change_1h.map(|v| v as f32))
    .bind(market_match.price_change_1d.map(|v| v as f32))
    .bind(market_match.price_change_1w.map(|v| v as f32))
    .bind(market_match.price_change_1m.map(|v| v as f32))
    .bind(&market_match.end_date)
    .bind(market_match.active)
    .bind(market_match.closed)
    .bind(market_match.resolved)
    .bind(&market_match.outcome)
    .bind(fermi_prob.map(|v| v as f32))
    .bind(divergence.map(|v| v as f32))
    .bind(market_match.confidence_signal.db_str())
    .bind(obs_type)
    .bind(&user_id)
    .execute(&state.db)
    .await;

    // Capture the outcome so we can (a) log it, and (b) surface it in
    // the response body. Historically this INSERT was fire-and-forget:
    // every failure was swallowed by `.map_err(...).ok()`, so bugs like
    // the confidence_signal CHECK-constraint violation cost us weeks of
    // "the trajectory shows 0 observations". Now the caller can see the
    // error immediately.
    let (observation_persisted, observation_error) = match insert_result {
        Ok(_) => (true, None),
        Err(e) => {
            tracing::warn!(
                forecast_id = ?effective_forecast_id,
                pm_market_id = %market_match.pm_market_id,
                error = %e,
                "failed to record PM observation"
            );
            (false, Some(e.to_string()))
        }
    };

    // ── Update forecast metadata with latest PM price ──────────
    if let Some(ref fc_id) = body.forecast_id {
        let pm_metadata = json!({
            "polymarket": {
                "pm_event_id": market_match.pm_event_id,
                "pm_market_id": market_match.pm_market_id,
                "pm_slug": market_match.slug,
                "pm_question": market_match.question,
                "pm_url": market_match.polymarket_url,
                "last_snapshot": chrono::Utc::now().to_rfc3339(),
                "last_market_price": market_match.market_price,
                "last_volume_24h": market_match.volume_24h,
                "last_confidence": market_match.confidence_signal.label(),
            }
        });

        // ACL already enforced above via require_forecast_edit(). Do
        // NOT re-filter by owner_id here — a shared editor passed the
        // ACL check, so their metadata write must land too. Filtering
        // by owner_id here would silently swallow the update and leave
        // metadata.polymarket stale for shared editors.
        let _ = sqlx::query(
            "UPDATE fermi_forecasts
             SET metadata = metadata || $1::jsonb,
                 updated_at = NOW()
             WHERE id = $2",
        )
        .bind(&pm_metadata)
        .bind(fc_id)
        .execute(&state.db)
        .await;
    }

    let divergence_interpretation = divergence.map(interpret_divergence);

    Ok(Json(json!({
        "observation_id": obs_id,
        "pm_event_id": market_match.pm_event_id,
        "pm_market_id": market_match.pm_market_id,
        "question": market_match.question,
        "event_title": market_match.event_title,
        "market_price": market_match.market_price,
        "market_price_pct": format_probability(market_match.market_price),
        "midpoint_price": market_match.midpoint_price,
        "bid": market_match.bid_price,
        "ask": market_match.ask_price,
        "spread": market_match.spread,
        "volume_24h": market_match.volume_24h,
        "volume_24h_fmt": format_volume(market_match.volume_24h),
        "liquidity": market_match.liquidity,
        "liquidity_fmt": format_volume(market_match.liquidity),
        "price_change_1w": market_match.price_change_1w,
        "price_change_1m": market_match.price_change_1m,
        "confidence_signal": market_match.confidence_signal.label(),
        "confidence_quality": market_match.confidence_signal.quality_score(),
        "active": market_match.active,
        "closed": market_match.closed,
        "resolved": market_match.resolved,
        "outcome": market_match.outcome,
        "polymarket_url": market_match.polymarket_url,
        "fermi_probability": fermi_prob,
        "divergence_pp": divergence,
        "divergence_interpretation": divergence_interpretation,
        "forecast_id": effective_forecast_id,
        // Explicit success/failure signal for the observation write.
        // The console displays this so an operator immediately sees
        // when a snapshot fetched cleanly but the DB write dropped.
        "observation_persisted": observation_persisted,
        "observation_error": observation_error,
        "credits_charged": 1
    })))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/polymarket/link
// ═══════════════════════════════════════════════════════════════════
//
// Permanently link a Polymarket market to a Fermi forecast.
// Fetches the current price, records an observation, and stores
// the PM metadata in the forecast's metadata JSONB field.

pub async fn link_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<LinkRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // ── ACL: caller must be able to edit the target forecast ──────
    //    (owner, admin, or share-with-edit — same rule
    //    resolve_forecast_handler enforces). Linking writes to the
    //    forecast's metadata + inserts an observation, so this is a
    //    write operation, not a view.
    require_forecast_edit(&state.db, &principal, &body.forecast_id).await?;

    let forecast = sqlx::query(
        "SELECT id, predicted_probability, question_text
         FROM fermi_forecasts
         WHERE id = $1",
    )
    .bind(&body.forecast_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let fermi_prob: f32 = forecast.get("predicted_probability");

    // ── Fetch current market state ─────────────────────────────
    let gamma = GammaClient::new();
    let market_match = gamma
        .snapshot_market(&body.pm_event_id, &body.pm_market_id)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                format!("Polymarket API error: {}", e),
            )
        })?;

    let divergence = compute_divergence_pp(fermi_prob as f64, market_match.market_price);

    // ── Store PM link in forecast metadata ─────────────────────
    let pm_metadata = json!({
        "polymarket": {
            "pm_event_id": market_match.pm_event_id,
            "pm_market_id": market_match.pm_market_id,
            "pm_condition_id": market_match.condition_id,
            "pm_slug": market_match.slug,
            "pm_question": market_match.question,
            "pm_event_title": market_match.event_title,
            "pm_url": market_match.polymarket_url,
            "pm_end_date": market_match.end_date,
            "linked_at": chrono::Utc::now().to_rfc3339(),
            "last_snapshot": chrono::Utc::now().to_rfc3339(),
            "last_market_price": market_match.market_price,
            "last_volume_24h": market_match.volume_24h,
            "last_confidence": market_match.confidence_signal.label(),
        }
    });

    // ACL already enforced above via require_forecast_edit(); no need
    // to re-filter by owner_id here — same reasoning as snapshot_handler.
    sqlx::query(
        "UPDATE fermi_forecasts
         SET metadata = metadata || $1::jsonb,
             updated_at = NOW()
         WHERE id = $2",
    )
    .bind(&pm_metadata)
    .bind(&body.forecast_id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to link: {}", e),
        )
    })?;

    // ── Record observation ─────────────────────────────────────
    let obs_id = uuid::Uuid::new_v4().to_string();
    let _ = sqlx::query(
        "INSERT INTO fermi_market_observations (
            id, forecast_id,
            pm_event_id, pm_market_id, pm_condition_id, pm_slug,
            pm_question, pm_event_title,
            market_price, bid_price, ask_price, midpoint_price, spread,
            volume_total, volume_24h, liquidity,
            price_change_1w, price_change_1m,
            pm_active, pm_closed, pm_resolved,
            fermi_probability, divergence_pp,
            confidence_signal, observation_type, observer_id
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8,
            $9, $10, $11, $12, $13,
            $14, $15, $16,
            $17, $18,
            $19, $20, $21,
            $22, $23,
            $24, 'manual_link', $25
        )",
    )
    .bind(&obs_id)
    .bind(&body.forecast_id)
    .bind(&market_match.pm_event_id)
    .bind(&market_match.pm_market_id)
    .bind(&market_match.condition_id)
    .bind(&market_match.slug)
    .bind(&market_match.question)
    .bind(&market_match.event_title)
    .bind(market_match.market_price as f32)
    .bind(market_match.bid_price as f32)
    .bind(market_match.ask_price as f32)
    .bind(market_match.midpoint_price as f32)
    .bind(market_match.spread as f32)
    .bind(market_match.volume_total as f32)
    .bind(market_match.volume_24h as f32)
    .bind(market_match.liquidity as f32)
    .bind(market_match.price_change_1w.map(|v| v as f32))
    .bind(market_match.price_change_1m.map(|v| v as f32))
    .bind(market_match.active)
    .bind(market_match.closed)
    .bind(market_match.resolved)
    .bind(fermi_prob)
    .bind(divergence as f32)
    .bind(market_match.confidence_signal.db_str())
    .bind(&user_id)
    .execute(&state.db)
    .await
    .ok();

    Ok(Json(json!({
        "observation_id": obs_id,
        "forecast_id": body.forecast_id,
        "linked": true,
        "market_price": market_match.market_price,
        "market_price_pct": format_probability(market_match.market_price),
        "fermi_probability": fermi_prob,
        "fermi_probability_pct": format_probability(fermi_prob as f64),
        "divergence_pp": divergence,
        "divergence_interpretation": interpret_divergence(divergence),
        "confidence_signal": market_match.confidence_signal.label(),
        "polymarket_url": market_match.polymarket_url,
        "message": format!(
            "Linked to Polymarket. Crowd price: {}. Your model: {}. Divergence: {:.1}pp — {}",
            format_probability(market_match.market_price),
            format_probability(fermi_prob as f64),
            divergence,
            interpret_divergence(divergence)
        )
    })))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/polymarket/import
// ═══════════════════════════════════════════════════════════════════
//
// Import a Polymarket question as a new Fermi forecast.
// Creates the forecast with PM metadata pre-linked, and records
// an initial observation. The user can then Ctrl+Enter to decompose.

pub async fn import_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<ImportRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // ── Charge credit ──────────────────────────────────────────
    let wallet = fermi_auth::get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    if wallet.balance <= 0 {
        return Err((StatusCode::PAYMENT_REQUIRED, "Insufficient credits".into()));
    }

    fermi::gas::charge_gas(
        &state.db,
        wallet.wallet_id,
        2, // 2 credits for import (search + create)
        "polymarket_import",
        &format!("PM import: event={}", body.pm_event_id),
        None,
    )
    .await?;

    // ── Fetch market data ──────────────────────────────────────
    let gamma = GammaClient::new();
    let market_match = gamma
        .snapshot_market(&body.pm_event_id, &body.pm_market_id)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                format!("Polymarket API error: {}", e),
            )
        })?;

    let question = body
        .question_text
        .as_deref()
        .unwrap_or(&market_match.question);

    // ── Detect domain from PM tags ─────────────────────────────
    let domain = detect_domain_from_tags(&market_match.tags);

    // ── Create Fermi forecast ──────────────────────────────────
    let forecast_id = uuid::Uuid::new_v4().to_string();
    let pm_metadata = json!({
        "polymarket": {
            "pm_event_id": market_match.pm_event_id,
            "pm_market_id": market_match.pm_market_id,
            "pm_condition_id": market_match.condition_id,
            "pm_slug": market_match.slug,
            "pm_question": market_match.question,
            "pm_event_title": market_match.event_title,
            "pm_url": market_match.polymarket_url,
            "pm_end_date": market_match.end_date,
            "linked_at": chrono::Utc::now().to_rfc3339(),
            "last_snapshot": chrono::Utc::now().to_rfc3339(),
            "last_market_price": market_match.market_price,
            "last_volume_24h": market_match.volume_24h,
            "last_confidence": market_match.confidence_signal.label(),
            "imported": true
        },
        "source": "polymarket_import"
    });

    // Use the market price as the initial probability anchor
    let initial_prob = market_match.market_price.clamp(0.01, 0.99);

    // v0.10.13: dropped `$2::uuid` cast on owner_id. Post-mig-165
    // fermi_forecasts.owner_id is TEXT and FKs users(user_id) — the
    // principal's user_id() returns TEXT so binding it directly is now
    // the correct type. Casting to ::uuid on a TEXT column produces
    // `operator does not exist: text = uuid` for downstream reads.
    sqlx::query(
        "INSERT INTO fermi_forecasts (
            id, owner_id, question_text, predicted_probability,
            domain, status, visibility,
            tags, metadata, target_date
        ) VALUES ($1, $2, $3, $4, $5, 'active', 'private', $6, $7, $8::timestamptz)",
    )
    .bind(&forecast_id)
    .bind(&user_id)
    .bind(question)
    .bind(initial_prob as f32)
    .bind(&domain)
    .bind(&vec!["polymarket".to_string()])
    .bind(&pm_metadata)
    .bind(&market_match.end_date)
    .execute(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create forecast: {}", e),
        )
    })?;

    // ── Add to portfolio if specified ───────────────────────────
    if let Some(ref portfolio_id) = body.portfolio_id {
        let _ = sqlx::query(
            "INSERT INTO fermi_portfolio_forecasts (portfolio_id, forecast_id)
             VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(portfolio_id)
        .bind(&forecast_id)
        .execute(&state.db)
        .await;
    }

    // ── Record initial observation ─────────────────────────────
    let obs_id = uuid::Uuid::new_v4().to_string();
    let _ = sqlx::query(
        "INSERT INTO fermi_market_observations (
            id, forecast_id,
            pm_event_id, pm_market_id, pm_condition_id, pm_slug,
            pm_question, pm_event_title,
            market_price, bid_price, ask_price, midpoint_price, spread,
            volume_total, volume_24h, liquidity,
            price_change_1w, price_change_1m,
            pm_active, pm_closed, pm_resolved,
            fermi_probability, divergence_pp,
            confidence_signal, observation_type, observer_id
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8,
            $9, $10, $11, $12, $13,
            $14, $15, $16,
            $17, $18,
            $19, $20, $21,
            $22, $23,
            $24, 'import', $25
        )",
    )
    .bind(&obs_id)
    .bind(&forecast_id)
    .bind(&market_match.pm_event_id)
    .bind(&market_match.pm_market_id)
    .bind(&market_match.condition_id)
    .bind(&market_match.slug)
    .bind(&market_match.question)
    .bind(&market_match.event_title)
    .bind(market_match.market_price as f32)
    .bind(market_match.bid_price as f32)
    .bind(market_match.ask_price as f32)
    .bind(market_match.midpoint_price as f32)
    .bind(market_match.spread as f32)
    .bind(market_match.volume_total as f32)
    .bind(market_match.volume_24h as f32)
    .bind(market_match.liquidity as f32)
    .bind(market_match.price_change_1w.map(|v| v as f32))
    .bind(market_match.price_change_1m.map(|v| v as f32))
    .bind(market_match.active)
    .bind(market_match.closed)
    .bind(market_match.resolved)
    .bind(initial_prob as f32)
    .bind(0.0f32) // divergence is 0 at import (we used market price)
    .bind(market_match.confidence_signal.db_str())
    .bind(&user_id)
    .execute(&state.db)
    .await
    .ok();

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "question": question,
        "initial_probability": initial_prob,
        "initial_probability_pct": format_probability(initial_prob),
        "market_price": market_match.market_price,
        "market_price_pct": format_probability(market_match.market_price),
        "polymarket_url": market_match.polymarket_url,
        "confidence_signal": market_match.confidence_signal.label(),
        "domain": domain,
        "observation_id": obs_id,
        "credits_charged": 2,
        "message": format!(
            "Imported '{}' from Polymarket. Initial probability anchored to crowd price: {}. Run Fermi decomposition to build your inside view.",
            question,
            format_probability(market_match.market_price)
        )
    })))
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/polymarket/observations
// ═══════════════════════════════════════════════════════════════════
//
// Get the observation time series for a forecast or market.
// Returns all append-only snapshots, newest first.

pub async fn observations_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(params): Query<ObservationsQuery>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let (where_clause, bind_values) = if let Some(ref fc_id) = params.forecast_id {
        (
            "forecast_id = $1 AND observer_id = $2",
            vec![fc_id.clone(), user_id.clone()],
        )
    } else if let Some(ref pm_id) = params.pm_market_id {
        (
            "pm_market_id = $1 AND observer_id = $2",
            vec![pm_id.clone(), user_id.clone()],
        )
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            "Provide either forecast_id or pm_market_id".into(),
        ));
    };

    let query_str = format!(
        "SELECT id, forecast_id, pm_event_id, pm_market_id, pm_question, pm_event_title,
                market_price, bid_price, ask_price, midpoint_price, spread,
                volume_24h, liquidity,
                price_change_1w, price_change_1m,
                pm_active, pm_closed, pm_resolved, pm_outcome,
                fermi_probability, divergence_pp,
                confidence_signal, observation_type, created_at
         FROM fermi_market_observations
         WHERE {}
         ORDER BY created_at DESC
         LIMIT $3",
        where_clause
    );

    let rows = sqlx::query(&query_str)
        .bind(&bind_values[0])
        .bind(&bind_values[1])
        .bind(params.limit)
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let observations: Vec<JsonValue> = rows
        .iter()
        .map(|row| {
            let market_price: Option<f32> = row.try_get("market_price").ok();
            let fermi_prob: Option<f32> = row.try_get("fermi_probability").ok();
            let divergence: Option<f32> = row.try_get("divergence_pp").ok();
            let created_at: Option<chrono::DateTime<chrono::Utc>> =
                row.try_get("created_at").ok();

            json!({
                "id": row.try_get::<String, _>("id").unwrap_or_default(),
                "forecast_id": row.try_get::<Option<String>, _>("forecast_id").unwrap_or(None),
                "pm_event_id": row.try_get::<String, _>("pm_event_id").unwrap_or_default(),
                "pm_market_id": row.try_get::<String, _>("pm_market_id").unwrap_or_default(),
                "pm_question": row.try_get::<String, _>("pm_question").unwrap_or_default(),
                "market_price": market_price,
                "market_price_pct": market_price.map(|p| format_probability(p as f64)),
                "bid_price": row.try_get::<Option<f32>, _>("bid_price").unwrap_or(None),
                "ask_price": row.try_get::<Option<f32>, _>("ask_price").unwrap_or(None),
                "volume_24h": row.try_get::<Option<f32>, _>("volume_24h").unwrap_or(None),
                "pm_closed": row.try_get::<bool, _>("pm_closed").unwrap_or(false),
                "pm_resolved": row.try_get::<bool, _>("pm_resolved").unwrap_or(false),
                "pm_outcome": row.try_get::<Option<String>, _>("pm_outcome").unwrap_or(None),
                "fermi_probability": fermi_prob,
                "fermi_probability_pct": fermi_prob.map(|p| format_probability(p as f64)),
                "divergence_pp": divergence,
                "divergence_interpretation": divergence.map(|d| interpret_divergence(d as f64)),
                "confidence_signal": row.try_get::<Option<String>, _>("confidence_signal").unwrap_or(None),
                "observation_type": row.try_get::<String, _>("observation_type").unwrap_or_default(),
                "created_at": created_at.map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    // Compute trend if we have multiple observations
    let trend = if observations.len() >= 2 {
        let newest = observations
            .first()
            .and_then(|o| o["market_price"].as_f64());
        let oldest = observations.last().and_then(|o| o["market_price"].as_f64());
        match (newest, oldest) {
            (Some(n), Some(o)) => {
                let delta = n - o;
                if delta > 0.02 {
                    Some("strengthening")
                } else if delta < -0.02 {
                    Some("weakening")
                } else {
                    Some("stable")
                }
            }
            _ => None,
        }
    } else {
        None
    };

    Ok(Json(json!({
        "observations": observations,
        "count": observations.len(),
        "trend": trend,
        "forecast_id": params.forecast_id,
        "pm_market_id": params.pm_market_id,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/polymarket/check-resolutions
// ═══════════════════════════════════════════════════════════════════
//
// Check all linked Polymarket markets for resolution.
// When a market resolves, auto-resolve the linked Fermi forecast
// and compute the Brier score. This is the calibration flywheel.

pub async fn check_resolutions_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Find all active forecasts with PM links that haven't resolved yet
    // v0.10.13: `f.owner_id = $1` (was `$1::uuid`) — owner_id is TEXT
    // post-mig-165.
    let forecasts = sqlx::query(
        "SELECT f.id, f.predicted_probability, f.question_text,
                f.metadata->'polymarket'->>'pm_event_id' AS pm_event_id,
                f.metadata->'polymarket'->>'pm_market_id' AS pm_market_id
         FROM fermi_forecasts f
         WHERE f.owner_id = $1
           AND f.status = 'active'
           AND f.metadata->'polymarket' IS NOT NULL
           AND f.metadata->'polymarket'->>'pm_market_id' IS NOT NULL",
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let gamma = GammaClient::new();
    let mut resolved_count = 0;
    let mut checked_count = 0;
    let mut results: Vec<JsonValue> = Vec::new();

    for row in &forecasts {
        let forecast_id: String = row.get("id");
        let fermi_prob: f32 = row.get("predicted_probability");
        let pm_event_id: Option<String> = row.get("pm_event_id");
        let pm_market_id: Option<String> = row.get("pm_market_id");

        let (event_id, market_id) = match (pm_event_id, pm_market_id) {
            (Some(e), Some(m)) => (e, m),
            _ => continue,
        };

        checked_count += 1;

        // Fetch current market state
        let market_match = match gamma.snapshot_market(&event_id, &market_id).await {
            Ok(m) => m,
            Err(e) => {
                results.push(json!({
                    "forecast_id": forecast_id,
                    "status": "error",
                    "error": format!("{}", e),
                }));
                continue;
            }
        };

        // Record observation
        let obs_id = uuid::Uuid::new_v4().to_string();
        let divergence = compute_divergence_pp(fermi_prob as f64, market_match.market_price);
        let _ = sqlx::query(
            "INSERT INTO fermi_market_observations (
                id, forecast_id, pm_event_id, pm_market_id, pm_question, pm_event_title,
                market_price, pm_active, pm_closed, pm_resolved, pm_outcome,
                fermi_probability, divergence_pp,
                confidence_signal, observation_type, observer_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, 'resolution_check', $15)",
        )
        .bind(&obs_id)
        .bind(&forecast_id)
        .bind(&market_match.pm_event_id)
        .bind(&market_match.pm_market_id)
        .bind(&market_match.question)
        .bind(&market_match.event_title)
        .bind(market_match.market_price as f32)
        .bind(market_match.active)
        .bind(market_match.closed)
        .bind(market_match.resolved)
        .bind(&market_match.outcome)
        .bind(fermi_prob)
        .bind(divergence as f32)
        .bind(market_match.confidence_signal.db_str())
        .bind(&user_id)
        .execute(&state.db)
        .await
        .ok();

        // Check if resolved
        if market_match.closed && market_match.resolved {
            if let Some(ref outcome_str) = market_match.outcome {
                match apply_settlement(&state.db, &forecast_id, &user_id, fermi_prob, &market_match)
                    .await
                {
                    Some(s) => {
                        resolved_count += 1;
                        results.push(json!({
                            "forecast_id": forecast_id,
                            "status": "resolved",
                            "outcome": outcome_str,
                            "actual_outcome": s.actual_outcome,
                            "fermi_probability": fermi_prob,
                            "brier_score": s.brier,
                            "market_final_price": market_match.market_price,
                        }));
                    }
                    None => results.push(json!({
                        "forecast_id": forecast_id,
                        "status": "settled_but_not_applied",
                        "outcome": outcome_str,
                    })),
                }
            }
        } else {
            results.push(json!({
                "forecast_id": forecast_id,
                "status": "still_active",
                "market_price": market_match.market_price,
                "divergence_pp": divergence,
            }));
        }
    }

    Ok(Json(json!({
        "checked": checked_count,
        "resolved": resolved_count,
        "results": results,
    })))
}

// ══════════════════════════════════════════════════════════════════
// Settlement — the single write path for a market-derived resolution
// ══════════════════════════════════════════════════════════════════

pub(crate) struct Settlement {
    pub actual_outcome: bool,
    pub brier: f64,
}

/// Apply a settled market outcome to a forecast and close Loop 5.
///
/// Shared by the operator-triggered `check_resolutions_handler` and the
/// background sweeper, so the two can't drift. Everything a resolution
/// must do lives here exactly once:
///
/// 1. Write the outcome, Brier, and `scored_probability` (the immutable
///    audit anchor from mig-174 — without it the score becomes
///    unreproducible the moment any cascade/recompose/refit path touches
///    `predicted_probability`, which is how every pre-174 resolution got
///    corrupted).
/// 2. Append to `fermi_forecast_updates`.
/// 3. Queue an operator-gated cascade review.
/// 4. Emit `forecast_calibration` eval_signals per contributing agent.
/// 5. Backfill the trajectory's retrospective calibration columns.
///
/// `resolution_source` is deliberately `'polymarket_price_heuristic'`,
/// NOT `'polymarket_oracle'`: `market_match.outcome` is inferred from a
/// settled price threshold (`yes_price > 0.9` / `< 0.1` in
/// `src/polymarket/mod.rs:688-699`), not read from a UMA oracle.
/// Labelling it "oracle" made a price heuristic look like hard-verified
/// ground truth. Reserve `'polymarket_oracle'` for genuine
/// settlement-lifecycle reads.
///
/// Returns `None` if the row was not updated — typically because it is
/// no longer `active` (already resolved, or a concurrent writer won).
/// The `WHERE status = 'active'` guard makes this idempotent.
pub(crate) async fn apply_settlement(
    db: &sqlx::PgPool,
    forecast_id: &str,
    owner_id: &str,
    fermi_prob: f32,
    market_match: &MarketMatch,
) -> Option<Settlement> {
    let outcome_str = market_match.outcome.as_deref()?;
    let actual_outcome = outcome_str == "Yes";
    let brier = (fermi_prob as f64 - if actual_outcome { 1.0 } else { 0.0 }).powi(2);

    // v0.10.13: owner_id = $7 (was `$7::uuid`) — TEXT post-mig-165
    let resolve_result = sqlx::query(
        "UPDATE fermi_forecasts
         SET status = 'resolved',
             actual_outcome = $1,
             brier_score = $2,
             scored_probability = $8,
             resolution_source = 'polymarket_price_heuristic',
             resolved_at = NOW(),
             resolved_by = $3,
             resolution_notes = $4,
             metadata = metadata || $5::jsonb,
             updated_at = NOW()
         WHERE id = $6 AND owner_id = $7 AND status = 'active'",
    )
    .bind(actual_outcome)
    .bind(brier as f32)
    .bind("polymarket_oracle")
    .bind(format!(
        "Auto-resolved via Polymarket settled price ({} → {}, p={:.4}). Brier: {:.4}",
        market_match.question, outcome_str, fermi_prob, brier
    ))
    .bind(json!({
        "resolution": {
            "source": "polymarket_price_heuristic",
            "pm_outcome": outcome_str,
            "pm_final_price": market_match.market_price,
            "brier_score": brier,
            "fermi_probability_at_resolution": fermi_prob,
            "resolved_at": chrono::Utc::now().to_rfc3339(),
        }
    }))
    .bind(forecast_id)
    .bind(owner_id)
    .bind(fermi_prob)
    .execute(db)
    .await;

    match resolve_result {
        Ok(r) if r.rows_affected() > 0 => {}
        Ok(_) => return None,
        Err(e) => {
            tracing::warn!(
                forecast = %forecast_id,
                error = %e,
                "[pm-settle] resolution UPDATE failed"
            );
            return None;
        }
    }

    // Append-only revision record.
    let _ = sqlx::query(
        "INSERT INTO fermi_forecast_updates (
            id, forecast_id, previous_probability, new_probability,
            reason, evidence_added
        ) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(forecast_id)
    .bind(fermi_prob)
    .bind(if actual_outcome { 1.0f32 } else { 0.0f32 })
    .bind(format!(
        "Auto-resolved via Polymarket settled price: {} (Brier: {:.4})",
        outcome_str, brier
    ))
    .bind(json!({
        "polymarket_resolution": {
            "outcome": outcome_str,
            "final_market_price": market_match.market_price,
            "brier_score": brier,
        }
    }))
    .execute(db)
    .await;

    // Queue a cascade review for any relationship group this forecast
    // belongs to (mutex/at_most_n). Operator-gated: we queue, the
    // operator reviews and applies in the console.
    crate::handlers::pending_cascades::queue_pending_cascade(
        db,
        forecast_id,
        "resolved",
        Some(actual_outcome),
        "polymarket_oracle",
        owner_id,
    )
    .await;

    // Loop 5: one forecast_calibration eval_signal per contributing agent
    // (score = 1 - brier). This is what get_agent_calibration reads and
    // moe_router_strategist Stage 0 consumes.
    crate::handlers::forecasts::record_forecast_calibration_signals(db, forecast_id, brier).await;

    // Fill the trajectory's retrospective calibration columns
    // (brier_at_this_point per revision + loop5_calibration snapshot).
    crate::handlers::forecasts::backfill_spacetime_calibration(
        db,
        forecast_id,
        actual_outcome,
        brier,
    )
    .await;

    Some(Settlement {
        actual_outcome,
        brier,
    })
}

// ══════════════════════════════════════════════════════════════════
// Background resolution sweeper
// ══════════════════════════════════════════════════════════════════

/// Default sweep cadence. Prediction-market settlement is not
/// latency-sensitive — what matters is that it happens at all.
const DEFAULT_SWEEP_INTERVAL_SECS: u64 = 900; // 15 min

/// Max forecasts examined per sweep. Bounds both Gamma load and the
/// blast radius of a bad sweep.
const SWEEP_BATCH_LIMIT: i64 = 40;

/// Pause between Gamma calls. Gamma documents roughly 100 req/min; at
/// 750ms we sit near 80/min worst case, and the sweeper is the only
/// unattended caller.
const SWEEP_PACING_MS: u64 = 750;

/// Spawn the periodic Polymarket resolution sweep.
///
/// # Why this exists
///
/// Nothing scheduled resolution before this. `check_resolutions_handler`
/// ran only when an operator clicked a button in the console
/// (`crates/fermi-console/src/main.rs:8926`), and the 5-minute cockpit
/// poll refreshes *prices* only. So a market could settle and the linked
/// forecast would sit `active` indefinitely — no Brier, no calibration
/// signal, Loop 5 cold. That is the single biggest reason the calibration
/// loop looked broken: it was never being driven.
///
/// Design notes:
/// - **Owner-scoped writes.** The sweep spans all owners, but
///   `apply_settlement` still constrains its UPDATE to the row's own
///   `owner_id`, so nothing crosses a tenancy boundary.
/// - **Idempotent.** `WHERE status = 'active'` means a concurrent
///   operator click and a sweep can't double-resolve.
/// - **Paced and bounded**, because these endpoints have no rate limiting
///   (PM routes sit in `protected_routes`, which gets `auth_middleware`
///   but no rate-limit layer) and Gamma is a third party.
/// - **Disable with `PM_RESOLUTION_SWEEP_SECS=0`.**
pub fn spawn_resolution_sweeper(db: sqlx::PgPool) {
    let interval_secs = std::env::var("PM_RESOLUTION_SWEEP_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_SWEEP_INTERVAL_SECS);

    if interval_secs == 0 {
        println!("[pm-sweep] disabled (PM_RESOLUTION_SWEEP_SECS=0)");
        return;
    }

    println!("[pm-sweep] resolution sweep every {interval_secs}s");

    tokio::spawn(async move {
        // Stagger the first run so boot isn't competing with migrations
        // and schema ensures for the pool.
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        loop {
            match sweep_resolutions_once(&db).await {
                Ok((checked, resolved)) if checked > 0 => {
                    println!("[pm-sweep] checked {checked}, resolved {resolved}");
                }
                Ok(_) => {}
                Err(e) => eprintln!("[pm-sweep] sweep failed: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        }
    });
}

/// One pass of the resolution sweep. Returns `(checked, resolved)`.
///
/// Separated from the spawn loop so it is directly testable and can be
/// driven from an admin endpoint later.
pub async fn sweep_resolutions_once(db: &sqlx::PgPool) -> Result<(usize, usize), sqlx::Error> {
    // Oldest-target-date first: those are the most likely to have
    // settled, so a bounded batch converges rather than starving.
    let rows = sqlx::query(
        "SELECT id,
                owner_id::text AS owner_id,
                predicted_probability,
                metadata->'polymarket'->>'pm_event_id'  AS pm_event_id,
                metadata->'polymarket'->>'pm_market_id' AS pm_market_id
           FROM fermi_forecasts
          WHERE status = 'active'
            AND metadata->'polymarket'->>'pm_market_id' IS NOT NULL
          ORDER BY target_date NULLS LAST
          LIMIT $1",
    )
    .bind(SWEEP_BATCH_LIMIT)
    .fetch_all(db)
    .await?;

    if rows.is_empty() {
        return Ok((0, 0));
    }

    let gamma = GammaClient::new();
    let mut checked = 0usize;
    let mut resolved = 0usize;

    for row in &rows {
        let forecast_id: String = row.get("id");
        let owner_id: String = row.try_get("owner_id").unwrap_or_default();
        let fermi_prob: f32 = row.try_get("predicted_probability").unwrap_or(0.5);
        let (Ok(Some(event_id)), Ok(Some(market_id))) = (
            row.try_get::<Option<String>, _>("pm_event_id"),
            row.try_get::<Option<String>, _>("pm_market_id"),
        ) else {
            continue;
        };

        if owner_id.is_empty() {
            continue;
        }

        tokio::time::sleep(std::time::Duration::from_millis(SWEEP_PACING_MS)).await;
        checked += 1;

        let market_match = match gamma.snapshot_market(&event_id, &market_id).await {
            Ok(m) => m,
            Err(e) => {
                // A single unreachable market must not abort the sweep;
                // the next pass retries it.
                tracing::debug!(
                    forecast = %forecast_id,
                    error = %e,
                    "[pm-sweep] snapshot failed; will retry next sweep"
                );
                continue;
            }
        };

        if !(market_match.closed && market_match.resolved) {
            continue;
        }

        if apply_settlement(db, &forecast_id, &owner_id, fermi_prob, &market_match)
            .await
            .is_some()
        {
            resolved += 1;
            tracing::info!(
                forecast = %forecast_id,
                outcome = ?market_match.outcome,
                "[pm-sweep] resolved"
            );
        }
    }

    Ok((checked, resolved))
}

// ══════════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════════

/// Detect a Fermi domain from Polymarket tags.
fn detect_domain_from_tags(tags: &[String]) -> Option<String> {
    let tags_lower: Vec<String> = tags.iter().map(|t| t.to_lowercase()).collect();
    let combined = tags_lower.join(" ");

    if combined.contains("fed")
        || combined.contains("economy")
        || combined.contains("gdp")
        || combined.contains("inflation")
        || combined.contains("interest rate")
    {
        Some("finance".into())
    } else if combined.contains("politic")
        || combined.contains("election")
        || combined.contains("president")
        || combined.contains("congress")
        || combined.contains("senate")
    {
        Some("politics".into())
    } else if combined.contains("crypto")
        || combined.contains("bitcoin")
        || combined.contains("ethereum")
    {
        Some("crypto".into())
    } else if combined.contains("sport")
        || combined.contains("nba")
        || combined.contains("nfl")
        || combined.contains("soccer")
        || combined.contains("football")
    {
        Some("sports".into())
    } else if combined.contains("tech") || combined.contains("ai") || combined.contains("software")
    {
        Some("technology".into())
    } else if combined.contains("geopolitic")
        || combined.contains("war")
        || combined.contains("military")
        || combined.contains("foreign policy")
    {
        Some("geopolitics".into())
    } else if combined.contains("oil")
        || combined.contains("energy")
        || combined.contains("commodit")
    {
        Some("commodities".into())
    } else if combined.contains("culture")
        || combined.contains("entertainment")
        || combined.contains("celebrity")
    {
        Some("culture".into())
    } else {
        None
    }
}
