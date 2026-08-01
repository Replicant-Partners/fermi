//! Forecast API Handlers
//!
//! RESTful endpoints for the Fermi forecasting system:
//! - Forecast CRUD (create, read, update, delete)
//! - Forecast resolution with Brier score computation
//! - Probability updates (revision history)
//! - Portfolio CRUD and aggregation stats
//! - Leaderboard queries
//! - Public forecast discovery

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;
use fermi::gas::charge_gas;
use fermi_auth::visibility::{can_access, can_edit, can_view};
use fermi_auth::{get_or_create_wallet, AuthPrincipal, AuthProvider, ObjectType, Visibility};
use sqlx::PgPool;

/// Ensure the users row referenced by `principal.user_id()` exists.
/// Fixes a class of "insert or update on table 'fermi_forecasts'
/// violates foreign key constraint" 500s that surfaced when a session's
/// user_id didn't match a row in `users` — e.g. a session token minted
/// against a different environment's DB, or a users row deleted after
/// the session was created. The FK on `fermi_forecasts.owner_id →
/// users(user_id)` (migration 094) fires before our INSERT lands and
/// the raw sqlx error was surfacing to the operator as an inscrutable
/// "Saved locally, but backend save failed" toast.
///
/// Best-effort UPSERT: uses the principal's email + display_name when
/// available (AuthPrincipal::User branch). API-key principals only
/// carry user_id, so if their users row is missing we can't backfill
/// safely (users.email is UNIQUE NOT NULL) and return a clean
/// PRECONDITION_FAILED with an actionable message instead.
///
/// Idempotent: fast-path SELECT returns immediately when the row
/// already exists, so this adds one round trip per write handler in
/// the happy path. Cheap given the existing charge_gas + INSERT flow.
pub(crate) async fn ensure_user_row(
    pool: &PgPool,
    principal: &AuthPrincipal,
) -> Result<(), (StatusCode, String)> {
    let user_id = principal.user_id();
    let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM users WHERE user_id = $1 LIMIT 1")
        .bind(&user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if exists.is_some() {
        return Ok(());
    }

    // Missing users row. Try to backfill from the AuthPrincipal.
    match principal {
        AuthPrincipal::User(user) => {
            let provider_str = match user.auth_provider {
                AuthProvider::Google => "google",
                AuthProvider::GitHub => "github",
                AuthProvider::Ethereum => "ethereum",
                AuthProvider::Email => "email",
            };
            let result = sqlx::query(
                r#"INSERT INTO users (user_id, email, display_name, role, auth_provider,
                                       password_hash, password_salt, last_login_at)
                   VALUES ($1, $2, $3, 'developer', $4, '', '', NOW())
                   ON CONFLICT (user_id) DO NOTHING"#,
            )
            .bind(&user.user_id)
            .bind(&user.email)
            .bind(&user.display_name)
            .bind(provider_str)
            .execute(pool)
            .await;
            if let Err(e) = result {
                // v0.9.1 — self-heal the email-UNIQUE conflict case.
                //
                // When the operator has a STALE users row (created by
                // a partial provisioning: legacy migration, deleted
                // OAuth session, cross-env token, etc.) with their
                // email but a null/legacy/mismatched user_id, the
                // INSERT above hits users.email UNIQUE and fails. The
                // old code path returned PRECONDITION_FAILED here
                // ("sign out and sign in again") which is wrong —
                // signing out doesn't heal the DB state, so the
                // operator would loop through auth forever.
                //
                // Instead we detect the UNIQUE-on-email case and try
                // an UPDATE that re-parents the stale row to the
                // current OAuth session:
                //
                //   * Match by email (the unique constraint that just
                //     collided) — confirms we're healing the SAME row
                //     that blocked the INSERT.
                //   * Guard: only heal rows that look legacy /
                //     orphaned (user_id NULL, empty, or auth_provider
                //     = 'legacy'). Never re-parent a row that's
                //     already provisioned under a live user_id —
                //     that would be a silent account takeover.
                //   * If the guard passes, set user_id + auth_provider
                //     + last_login_at from the current session.
                //
                // Only when the heal is inapplicable (email row is
                // owned by a different provisioned user) do we return
                // PRECONDITION_FAILED. The error message now names
                // the actual failure mode rather than blaming the
                // operator's session.
                let err_str = e.to_string();
                let is_email_conflict = err_str.contains("users_email")
                    || (err_str.to_lowercase().contains("unique")
                        && err_str.to_lowercase().contains("email"));
                if is_email_conflict {
                    let heal = sqlx::query(
                        r#"UPDATE users
                              SET user_id       = $1,
                                  auth_provider = COALESCE(NULLIF(auth_provider, ''), $2, auth_provider),
                                  display_name  = COALESCE(display_name, $3),
                                  last_login_at = NOW(),
                                  updated_at    = NOW()
                            WHERE email = $4
                              AND (user_id IS NULL
                                   OR user_id = ''
                                   OR auth_provider = 'legacy'
                                   OR auth_provider IS NULL)"#,
                    )
                    .bind(&user.user_id)
                    .bind(provider_str)
                    .bind(&user.display_name)
                    .bind(&user.email)
                    .execute(pool)
                    .await;
                    match heal {
                        Ok(res) if res.rows_affected() > 0 => {
                            tracing::info!(
                                user_id = %user.user_id,
                                email = %user.email,
                                "[ensure_user_row] healed stale users row via email match",
                            );
                            return Ok(());
                        }
                        Ok(_no_rows) => {
                            // Email row exists but doesn't match the
                            // legacy/orphan guard — it belongs to a
                            // different, already-provisioned account.
                            // Do NOT reparent (would be a takeover);
                            // report the collision honestly.
                            tracing::warn!(
                                user_id = %user.user_id,
                                email = %user.email,
                                "[ensure_user_row] email already registered under a different provisioned user_id; refusing to reparent",
                            );
                            return Err((
                                StatusCode::CONFLICT,
                                format!(
                                    "Your email ({}) is already registered under a \
                                     different account. Contact support to merge \
                                     or use a different email.",
                                    user.email
                                ),
                            ));
                        }
                        Err(heal_err) => {
                            tracing::warn!(
                                user_id = %user.user_id,
                                error = %heal_err,
                                "[ensure_user_row] heal UPDATE failed",
                            );
                            // Fall through to the generic error below.
                        }
                    }
                }
                tracing::warn!(
                    user_id = %user.user_id,
                    error = %e,
                    "[ensure_user_row] backfill failed (no heal path applied)",
                );
                return Err((
                    StatusCode::PRECONDITION_FAILED,
                    "Your account isn't fully provisioned and we couldn't \
                     auto-heal it. Please contact support."
                        .into(),
                ));
            }
            tracing::info!(
                user_id = %user.user_id,
                email = %user.email,
                "[ensure_user_row] backfilled missing users row",
            );
            Ok(())
        }
        AuthPrincipal::ApiKey(_) => {
            // API key with orphan user_id — data integrity issue we
            // can't recover from here. The API key's user_id column
            // should FK to users(user_id), but if the row is gone we
            // don't have email/display_name to insert one.
            Err((
                StatusCode::PRECONDITION_FAILED,
                format!(
                    "Your account (user_id={}) isn't provisioned. Sign in \
                     through the web console once, then retry.",
                    user_id
                ),
            ))
        }
    }
}

// ════════════════════════════════════════════════════════════════
// Request / Response Types
// ═══════════════════════════════════════════════════════════════════

// ── Forecasts ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateForecastRequest {
    pub question_text: String,
    pub predicted_probability: f64,
    pub domain: Option<String>,
    pub resolution_criteria: Option<String>,
    pub target_date: Option<String>, // ISO 8601
    pub confidence_interval_low: Option<f64>,
    pub confidence_interval_high: Option<f64>,
    pub fpl_source: Option<String>,
    pub notebook_id: Option<String>,
    pub simulation_results: Option<JsonValue>,
    pub iterations: Option<i32>,
    pub drivers: Option<JsonValue>,
    pub evidence: Option<JsonValue>,
    pub agents_used: Option<JsonValue>,
    pub visibility: Option<String>,
    pub team_id: Option<String>,
    pub tags: Option<Vec<String>>,
    pub portfolio_id: Option<String>, // auto-add to portfolio on creation
    pub status: Option<String>,       // "draft" or "active" (default: "draft")
    /// Optional ABW workspace UUID to link this forecast to. When set,
    /// `fermi_forecasts.workspace_id` is populated, which is the link the
    /// BayesOps refit hook (Spec 23 R-1) and the forecast spacetime
    /// trigger (migration 140/149) use to find the FPL and accumulate
    /// rate revisions. Without this, the forecast exists but is
    /// disconnected from any workspace-backed agent runtime.
    pub workspace_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateForecastRequest {
    pub question_text: Option<String>,
    pub predicted_probability: Option<f64>,
    pub domain: Option<String>,
    pub resolution_criteria: Option<String>,
    pub target_date: Option<String>,
    pub confidence_interval_low: Option<f64>,
    pub confidence_interval_high: Option<f64>,
    pub fpl_source: Option<String>,
    pub simulation_results: Option<JsonValue>,
    pub drivers: Option<JsonValue>,
    pub evidence: Option<JsonValue>,
    pub agents_used: Option<JsonValue>,
    pub visibility: Option<String>,
    pub tags: Option<Vec<String>>,
    pub status: Option<String>,
    /// Free-form JSON metadata. When present, the server MERGES this
    /// object into the existing `metadata` column (JSONB `||`), so a
    /// caller can PATCH `metadata.base_rate` without clobbering
    /// `metadata.polymarket`, and vice-versa. This is what powers the
    /// cockpit's "Update base rate" persistence — the AST mutation
    /// is echoed to `metadata.base_rate` here so a panel-switch reload
    /// doesn't silently revert it.
    pub metadata: Option<JsonValue>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveForecastRequest {
    pub actual_outcome: bool,
    pub resolution_notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProbabilityRequest {
    pub new_probability: f64,
    pub reason: Option<String>,
    pub agent_id: Option<String>,
    pub evidence_added: Option<JsonValue>,
}

#[derive(Debug, Deserialize)]
pub struct ListForecastsQuery {
    pub status: Option<String>,
    pub domain: Option<String>,
    pub visibility: Option<String>,
    pub portfolio_id: Option<String>,
    pub team_id: Option<String>,
    pub tag: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub sort: Option<String>, // "created", "updated", "target_date", "brier_score"
    pub order: Option<String>, // "asc", "desc"
    /// Ownership scope filter for the Portfolio panel's virtual buckets.
    /// * `"mine"`  — only forecasts owned by the caller.
    /// * `"shared"` — only forecasts the caller can see because they're
    ///                team-shared, object-shared, or public/shared
    ///                visibility, but **not** owned by the caller.
    /// * absent    — full accessible set (current behaviour).
    /// Drives the Portfolio panel's "📥 Shared with me" and
    /// "📌 Unassigned" virtual portfolios.
    pub scope: Option<String>,
    /// If `true`, restrict to forecasts that are NOT in any portfolio.
    /// Drives the "📌 Unassigned" virtual portfolio.
    pub unassigned: Option<bool>,
}

// ── Portfolios ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreatePortfolioRequest {
    pub title: String,
    pub description: Option<String>,
    pub domain: Option<String>,
    pub visibility: Option<String>,
    pub team_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PortfolioForecastRequest {
    pub forecast_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ListPortfoliosQuery {
    pub visibility: Option<String>,
    pub team_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ── Leaderboard ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct LeaderboardQuery {
    pub domain: Option<String>,
    pub team_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub min_forecasts: Option<i64>,
}

// ═══════════════════════════════════════════════════════════════════
// FORECAST CRUD
// ═══════════════════════════════════════════════════════════════════

/// POST /api/forecasts
pub async fn create_forecast_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreateForecastRequest>,
) -> Result<(StatusCode, Json<JsonValue>), (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // Backfill the users row if missing — guards against the FK
    // violation on fermi_forecasts.owner_id → users(user_id).
    ensure_user_row(pool, &principal).await?;

    // Validate probability
    if req.predicted_probability < 0.0 || req.predicted_probability > 1.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "predicted_probability must be between 0 and 1".into(),
        ));
    }

    let status = req.status.as_deref().unwrap_or("draft");
    if status != "draft" && status != "active" {
        return Err((
            StatusCode::BAD_REQUEST,
            "status must be 'draft' or 'active'".into(),
        ));
    }

    // Charge credits for active forecasts (drafts are free)
    if status == "active" {
        let wallet = get_or_create_wallet(pool, "user", &user_id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        charge_gas(
            pool,
            wallet.wallet_id,
            1, // 1 credit to publish a forecast
            "publish_forecast",
            &format!("Publish forecast: {}", &req.question_text),
            None,
        )
        .await?;
    }

    let forecast_id = Uuid::new_v4().to_string();
    let visibility = req.visibility.as_deref().unwrap_or("private");
    let tags = req.tags.clone().unwrap_or_default();
    let target_date: Option<chrono::DateTime<chrono::Utc>> = req
        .target_date
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));
    let team_id: Option<Uuid> = req.team_id.as_ref().and_then(|s| Uuid::parse_str(s).ok());
    let workspace_id: Option<Uuid> = req
        .workspace_id
        .as_ref()
        .and_then(|s| Uuid::parse_str(s).ok());

    sqlx::query(
        // v0.10.13: dropped `$2::uuid` cast on owner_id. Post-mig-165 the
        // column is TEXT (was UUID pre-drift), so binding a text
        // user_id directly matches. The cast was harmless before —
        // Postgres coerced text → uuid → text on assign — but broke
        // for non-UUID-shaped user_ids and was a source of drift
        // between the write and read paths.
        "INSERT INTO fermi_forecasts
         (id, owner_id, question_text, domain, resolution_criteria, target_date,
          predicted_probability, confidence_interval_low, confidence_interval_high,
          fpl_source, notebook_id, simulation_results, iterations,
          drivers, evidence, agents_used,
          status, visibility, team_id, workspace_id, tags, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                 $14, $15, $16, $17, $18, $19, $20, $21, NOW(), NOW())",
    )
    .bind(&forecast_id)
    .bind(&user_id)
    .bind(&req.question_text)
    .bind(&req.domain)
    .bind(&req.resolution_criteria)
    .bind(target_date)
    .bind(req.predicted_probability)
    .bind(req.confidence_interval_low)
    .bind(req.confidence_interval_high)
    .bind(&req.fpl_source)
    .bind(&req.notebook_id)
    .bind(&req.simulation_results)
    .bind(req.iterations.unwrap_or(10000))
    .bind(req.drivers.as_ref().unwrap_or(&json!([])))
    .bind(req.evidence.as_ref().unwrap_or(&json!([])))
    .bind(req.agents_used.as_ref().unwrap_or(&json!([])))
    .bind(status)
    .bind(visibility)
    .bind(team_id)
    .bind(workspace_id)
    .bind(&tags)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Auto-anchor + harness snapshot at forecast creation.
    {
        let now = chrono::Utc::now();
        let salt = std::env::var("BENCHMARK_SPLIT_SALT").unwrap_or_else(|_| "fermi-v1-2026".into());

        // Capture harness snapshot (conductor version from agents_used field)
        let conductor_version = req
            .agents_used
            .as_ref()
            .and_then(|au| au.as_array())
            .and_then(|arr| arr.first())
            .and_then(|a| a.get("agent_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("fermi");
        let harness_snapshot_id = crate::handlers::forecast_benchmark::capture_harness_snapshot(
            pool,
            conductor_version,
            req.agents_used.as_ref().unwrap_or(&serde_json::json!([])),
            None, // routing weights: populated later via calibration endpoint
            None, // bayesops_params: null until BayesOps operational
        )
        .await;

        let commitment_hash = crate::handlers::forecast_benchmark::anchor_forecast(
            pool,
            &forecast_id,
            None,
            req.predicted_probability as f64,
            req.fpl_source.as_deref(),
            now,
            Some("auto-anchor on create"),
        )
        .await
        .ok();

        // Link harness snapshot to the spacetime row if both exist
        if let (Some(snap_id), Some(_)) = (harness_snapshot_id, commitment_hash.as_ref()) {
            let _ = sqlx::query(
                "UPDATE forecast_spacetime SET harness_snapshot_id = $1
                 WHERE forecast_id = $2 AND revision_seq = 0",
            )
            .bind(snap_id)
            .bind(&forecast_id)
            .execute(pool)
            .await;
        }

        let _ = crate::handlers::forecast_benchmark::ensure_split(pool, &forecast_id, &salt).await;
    }

    // Auto-add to portfolio if specified
    if let Some(ref portfolio_id) = req.portfolio_id {
        sqlx::query(
            "INSERT INTO fermi_portfolio_forecasts (portfolio_id, forecast_id)
             VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(portfolio_id)
        .bind(&forecast_id)
        .execute(pool)
        .await
        .ok(); // Non-fatal if portfolio doesn't exist
    }

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": forecast_id,
            "status": status,
            "question_text": req.question_text,
            "predicted_probability": req.predicted_probability,
        })),
    ))
}

/// GET /api/forecasts/:id
pub async fn get_forecast_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    let row = sqlx::query(
        "SELECT f.*, f.owner_id::text AS owner_id_text, COALESCE(u.display_name, u.name, u.email, u.user_id) AS owner_display_name
         FROM fermi_forecasts f
         LEFT JOIN users u ON u.user_id = f.owner_id
         WHERE f.id = $1",
    )
    .bind(&forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Forecast not found".into()))?;

    // Access control: owner, team member, user share, or public.
    // Spec 24 §3.2 Wave 2 (Sprint 2.4b): replaced inline owner/team check
    // with the canonical `can_view` helper which also honours direct
    // user-shares in object_shares.
    let owner_id: String = row.get("owner_id_text");
    let visibility: String = row.get("visibility");

    let vis = Visibility::from_legacy(&visibility);
    let granted = can_view(
        pool,
        &principal,
        ObjectType::Forecast,
        &forecast_id,
        &owner_id,
        vis,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !granted {
        return Err((StatusCode::FORBIDDEN, "Access denied".into()));
    }

    // Get update history
    let updates = sqlx::query(
        "SELECT id, previous_probability, new_probability, reason, agent_id, evidence_added, created_at
         FROM fermi_forecast_updates
         WHERE forecast_id = $1
         ORDER BY created_at DESC
         LIMIT 50",
    )
    .bind(&forecast_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let update_history: Vec<JsonValue> = updates
        .iter()
        .map(|u| {
            json!({
                "id": u.try_get::<String, _>("id").ok(),
                // Postgres REAL → sqlx f32. Cast to f64 only for JSON.
                "previous_probability": u.try_get::<f32, _>("previous_probability").ok().map(|v| v as f64),
                "new_probability": u.try_get::<f32, _>("new_probability").ok().map(|v| v as f64),
                "reason": u.try_get::<Option<String>, _>("reason").ok().flatten(),
                "agent_id": u.try_get::<Option<String>, _>("agent_id").ok().flatten(),
                "evidence_added": u.try_get::<Option<JsonValue>, _>("evidence_added").ok().flatten(),
                "created_at": u.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    // Get portfolio memberships
    let portfolios: Vec<String> = sqlx::query_scalar(
        "SELECT portfolio_id FROM fermi_portfolio_forecasts WHERE forecast_id = $1",
    )
    .bind(&forecast_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    Ok(Json(json!({
        "id": row.try_get::<String, _>("id").ok(),
        "owner_id": owner_id,
        "owner_display_name": row.try_get::<Option<String>, _>("owner_display_name").ok().flatten(),
        "question_text": row.try_get::<String, _>("question_text").ok(),
        "domain": row.try_get::<Option<String>, _>("domain").ok().flatten(),
        "resolution_criteria": row.try_get::<Option<String>, _>("resolution_criteria").ok().flatten(),
        "target_date": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("target_date").ok().flatten().map(|t| t.to_rfc3339()),
        // fermi_forecasts.predicted_probability is REAL → sqlx f32. The list,
        // detail, and portfolio serializers all hit this — the bug here makes
        // every forecast probability render as null in the API even when the
        // row is NOT NULL in the DB.
        "predicted_probability": row.try_get::<f32, _>("predicted_probability").ok().map(|v| v as f64),
        "confidence_interval_low": row.try_get::<Option<f32>, _>("confidence_interval_low").ok().flatten().map(|v| v as f64),
        "confidence_interval_high": row.try_get::<Option<f32>, _>("confidence_interval_high").ok().flatten().map(|v| v as f64),
        "fpl_source": row.try_get::<Option<String>, _>("fpl_source").ok().flatten(),
        "notebook_id": row.try_get::<Option<String>, _>("notebook_id").ok().flatten(),
        "simulation_results": row.try_get::<Option<JsonValue>, _>("simulation_results").ok().flatten(),
        "iterations": row.try_get::<Option<i32>, _>("iterations").ok().flatten(),
        "drivers": row.try_get::<JsonValue, _>("drivers").ok(),
        "evidence": row.try_get::<JsonValue, _>("evidence").ok(),
        "agents_used": row.try_get::<JsonValue, _>("agents_used").ok(),
        "status": row.try_get::<String, _>("status").ok(),
        "actual_outcome": row.try_get::<Option<bool>, _>("actual_outcome").ok().flatten(),
        "brier_score": row.try_get::<Option<f32>, _>("brier_score").ok().flatten().map(|v| v as f64),
        "resolved_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at").ok().flatten().map(|t| t.to_rfc3339()),
        "resolved_by": row.try_get::<Option<String>, _>("resolved_by").ok().flatten(),
        "resolution_notes": row.try_get::<Option<String>, _>("resolution_notes").ok().flatten(),
        "visibility": visibility,
        "team_id": row.try_get::<Option<Uuid>, _>("team_id").ok().flatten().map(|u| u.to_string()),
        "workspace_id": row.try_get::<Option<Uuid>, _>("workspace_id").ok().flatten().map(|u| u.to_string()),
        // metadata.polymarket carries the linked PM market shape written by
        // polymarket::link_handler — pm_event_id, pm_market_id, pm_url,
        // last_market_price, last_volume_24h, etc. Surfacing it here lets
        // the console hydrate the PM panel without a second round-trip.
        "metadata": row.try_get::<Option<JsonValue>, _>("metadata").ok().flatten(),
        "tags": row.try_get::<Vec<String>, _>("tags").ok(),
        "portfolios": portfolios,
        "update_history": update_history,
        "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
        "updated_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at").ok().map(|t| t.to_rfc3339()),
    })))
}

/// GET /api/forecasts
pub async fn list_forecasts_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<ListForecastsQuery>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);

    // Build dynamic query
    let mut conditions = vec!["1=1".to_string()];
    let mut bind_idx = 0u32;
    let mut binds: Vec<String> = Vec::new();

    // Default scope: own forecasts + shared/public + team-shared +
    // directly-shared. Spec 24 §3.2 Wave 2 (Sprint 2.4b): added the
    // object_shares user-share branch so that `permission='view'` (or
    // edit/admin) shares grant list visibility. The team-share branch
    // (via team_members) was already there from Wave 1.
    bind_idx += 1;
    conditions.push(format!(
        "(f.owner_id = ${idx} \
          OR f.visibility IN ('shared', 'public') \
          OR (f.team_id IS NOT NULL \
              AND EXISTS (SELECT 1 FROM team_members m \
                          WHERE m.team_id = f.team_id AND m.member_id = ${idx})) \
          OR EXISTS (SELECT 1 FROM object_shares s \
                     WHERE s.object_type = 'forecast' \
                       AND s.object_id = f.id::text \
                       AND s.share_type = 'user' \
                       AND s.share_target = ${idx}))",
        idx = bind_idx
    ));
    binds.push(user_id.clone());

    if let Some(ref status) = q.status {
        bind_idx += 1;
        conditions.push(format!("f.status = ${}", bind_idx));
        binds.push(status.clone());
    }

    if let Some(ref domain) = q.domain {
        bind_idx += 1;
        conditions.push(format!("f.domain = ${}", bind_idx));
        binds.push(domain.clone());
    }

    if let Some(ref tag) = q.tag {
        bind_idx += 1;
        conditions.push(format!("${} = ANY(f.tags)", bind_idx));
        binds.push(tag.clone());
    }

    if let Some(ref portfolio_id) = q.portfolio_id {
        bind_idx += 1;
        conditions.push(format!(
            "EXISTS(SELECT 1 FROM fermi_portfolio_forecasts pf WHERE pf.forecast_id = f.id AND pf.portfolio_id = ${})",
            bind_idx
        ));
        binds.push(portfolio_id.clone());
    }

    // Ownership-scope filter. Layered on top of the ACL clause above so
    // that `scope=shared` never leaks forecasts the caller can't already
    // see — it just narrows the accessible set to the non-owned slice.
    match q.scope.as_deref() {
        Some("mine") => {
            bind_idx += 1;
            conditions.push(format!("f.owner_id = ${}", bind_idx));
            binds.push(user_id.clone());
        }
        Some("shared") => {
            bind_idx += 1;
            conditions.push(format!("f.owner_id <> ${}", bind_idx));
            binds.push(user_id.clone());
        }
        _ => {}
    }

    // "Not in any portfolio" filter. Powers the "📌 Unassigned" virtual
    // portfolio in the console. When both `portfolio_id` and
    // `unassigned=true` are set the two conditions are contradictory and
    // the result set is empty — that's intentional; the caller shouldn't
    // send both.
    if q.unassigned.unwrap_or(false) {
        conditions.push(
            "NOT EXISTS(SELECT 1 FROM fermi_portfolio_forecasts pf WHERE pf.forecast_id = f.id)"
                .to_string(),
        );
    }

    let sort_col = match q.sort.as_deref() {
        Some("updated") => "f.updated_at",
        Some("target_date") => "f.target_date",
        Some("brier_score") => "f.brier_score",
        Some("probability") => "f.predicted_probability",
        _ => "f.created_at",
    };
    let sort_order = match q.order.as_deref() {
        Some("asc") => "ASC",
        _ => "DESC",
    };

    let where_clause = conditions.join(" AND ");
    let sql = format!(
        "SELECT f.id, f.owner_id::text AS owner_id, f.question_text, f.domain, f.predicted_probability,
                f.status, f.brier_score, f.actual_outcome, f.target_date, f.visibility,
                f.tags, f.created_at, f.updated_at, f.resolved_at,
                COALESCE(u.display_name, u.name, u.email, u.user_id) AS owner_display_name
         FROM fermi_forecasts f
         LEFT JOIN users u ON u.user_id = f.owner_id
         WHERE {}
         ORDER BY {} {} NULLS LAST
         LIMIT {} OFFSET {}",
        where_clause, sort_col, sort_order, limit, offset
    );

    // Build the query with dynamic binds
    let mut query = sqlx::query(&sql);
    for b in &binds {
        query = query.bind(b);
    }

    let rows = query
        .fetch_all(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let forecasts: Vec<JsonValue> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.try_get::<String, _>("id").ok(),
                "owner_id": r.try_get::<String, _>("owner_id").ok(),
                "owner_display_name": r.try_get::<Option<String>, _>("owner_display_name").ok().flatten(),
                "question_text": r.try_get::<String, _>("question_text").ok(),
                "domain": r.try_get::<Option<String>, _>("domain").ok().flatten(),
                // Postgres REAL → sqlx f32. See get_forecast_handler for the
                // full rationale; same bug in three list-style serializers.
                "predicted_probability": r.try_get::<f32, _>("predicted_probability").ok().map(|v| v as f64),
                "status": r.try_get::<String, _>("status").ok(),
                "brier_score": r.try_get::<Option<f32>, _>("brier_score").ok().flatten().map(|v| v as f64),
                "actual_outcome": r.try_get::<Option<bool>, _>("actual_outcome").ok().flatten(),
                "target_date": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("target_date").ok().flatten().map(|t| t.to_rfc3339()),
                "visibility": r.try_get::<String, _>("visibility").ok(),
                "tags": r.try_get::<Vec<String>, _>("tags").ok(),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
                "updated_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at").ok().map(|t| t.to_rfc3339()),
                "resolved_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at").ok().flatten().map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "forecasts": forecasts,
        "count": forecasts.len(),
        "limit": limit,
        "offset": offset,
    })))
}

/// PUT /api/forecasts/:id
pub async fn update_forecast_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
    Json(req): Json<UpdateForecastRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // Verify edit access (Spec 24 §3.2 Wave 2: can_edit, not just owner).
    let row = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility, status, predicted_probability FROM fermi_forecasts WHERE id = $1",
    )
    .bind(&forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Forecast not found".into()))?;

    let owner_id: String = row.try_get("owner_id").unwrap_or_default();
    let visibility: String = row.try_get("visibility").unwrap_or_default();
    let vis = Visibility::from_legacy(&visibility);
    let granted = can_edit(
        pool,
        &principal,
        ObjectType::Forecast,
        &forecast_id,
        &owner_id,
        vis,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !granted {
        return Err((StatusCode::FORBIDDEN, "Edit access denied".into()));
    }

    let current_status: String = row.try_get("status").unwrap_or_default();
    if current_status == "resolved" {
        return Err((
            StatusCode::CONFLICT,
            "Cannot update a resolved forecast".into(),
        ));
    }

    // If probability is changing, record the update
    if let Some(new_prob) = req.predicted_probability {
        if new_prob < 0.0 || new_prob > 1.0 {
            return Err((
                StatusCode::BAD_REQUEST,
                "predicted_probability must be between 0 and 1".into(),
            ));
        }

        // predicted_probability is REAL → f32 in sqlx.
        let current_prob: f32 = row
            .try_get::<f32, _>("predicted_probability")
            .unwrap_or(0.0);
        // Compare in f64 (req.predicted_probability domain), bind f32 (DB
        // column type) on insert.
        let new_prob_f32 = new_prob as f32;
        if (new_prob - current_prob as f64).abs() > 0.001 {
            // Record the probability update
            sqlx::query(
                "INSERT INTO fermi_forecast_updates
                 (id, forecast_id, previous_probability, new_probability, reason, created_at)
                 VALUES ($1, $2, $3, $4, 'Manual update via API', NOW())",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&forecast_id)
            .bind(current_prob)
            .bind(new_prob_f32)
            .execute(pool)
            .await
            .ok();
        }
    }

    // If transitioning from draft to active, charge credits
    if let Some(ref new_status) = req.status {
        if new_status == "active" && current_status == "draft" {
            let wallet = get_or_create_wallet(pool, "user", &user_id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            charge_gas(
                pool,
                wallet.wallet_id,
                1,
                "publish_forecast",
                &format!("Publish forecast {}", forecast_id),
                Some(&forecast_id),
            )
            .await?;
        }
    }

    // Dynamic update — only set fields that are provided
    let target_date: Option<chrono::DateTime<chrono::Utc>> = req
        .target_date
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    sqlx::query(
        "UPDATE fermi_forecasts SET
            question_text = COALESCE($2, question_text),
            predicted_probability = COALESCE($3, predicted_probability),
            domain = COALESCE($4, domain),
            resolution_criteria = COALESCE($5, resolution_criteria),
            target_date = COALESCE($6, target_date),
            confidence_interval_low = COALESCE($7, confidence_interval_low),
            confidence_interval_high = COALESCE($8, confidence_interval_high),
            fpl_source = COALESCE($9, fpl_source),
            simulation_results = COALESCE($10, simulation_results),
            drivers = COALESCE($11, drivers),
            evidence = COALESCE($12, evidence),
            agents_used = COALESCE($13, agents_used),
            visibility = COALESCE($14, visibility),
            tags = COALESCE($15, tags),
            status = COALESCE($16, status),
            metadata = CASE
                WHEN $17::jsonb IS NULL THEN metadata
                ELSE COALESCE(metadata, '{}'::jsonb) || $17::jsonb
            END,
            updated_at = NOW()
         WHERE id = $1",
    )
    .bind(&forecast_id)
    .bind(&req.question_text)
    .bind(req.predicted_probability)
    .bind(&req.domain)
    .bind(&req.resolution_criteria)
    .bind(target_date)
    .bind(req.confidence_interval_low)
    .bind(req.confidence_interval_high)
    .bind(&req.fpl_source)
    .bind(&req.simulation_results)
    .bind(&req.drivers)
    .bind(&req.evidence)
    .bind(&req.agents_used)
    .bind(&req.visibility)
    .bind(&req.tags)
    .bind(&req.status)
    .bind(&req.metadata)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Return updated forecast
    get_forecast_handler(State(state), principal, Path(forecast_id)).await
}

/// DELETE /api/forecasts/:id
pub async fn delete_forecast_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    let row = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility FROM fermi_forecasts WHERE id = $1",
    )
    .bind(&forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match row {
        Some(r) => {
            let owner_id: String = r.try_get("owner_id").unwrap_or_default();
            let visibility: String = r.try_get("visibility").unwrap_or_default();
            let vis = Visibility::from_legacy(&visibility);
            let level = can_access(
                pool,
                &principal,
                ObjectType::Forecast,
                &forecast_id,
                &owner_id,
                vis,
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if !level.has_admin() {
                return Err((
                    StatusCode::FORBIDDEN,
                    "Admin access required to delete".into(),
                ));
            }
        }
        None => return Err((StatusCode::NOT_FOUND, "Forecast not found".into())),
    }

    sqlx::query("DELETE FROM fermi_forecasts WHERE id = $1")
        .bind(&forecast_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// ═══════════════════════════════════════════════════════════════════
// FORECAST RESOLUTION
// ═══════════════════════════════════════════════════════════════════

/// POST /api/forecasts/:id/resolve
///
/// Resolves a forecast with an actual outcome and computes the Brier score.
/// Only the owner can resolve their own forecasts.
pub async fn resolve_forecast_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
    Json(req): Json<ResolveForecastRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // Spec 24 §3.2 Wave 2 (Sprint 2.4b): can_edit check before resolving.
    // Previously there was no app-level owner check — any authenticated
    // user could resolve any active forecast.
    let acl_row = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility FROM fermi_forecasts WHERE id = $1",
    )
    .bind(&forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match acl_row {
        Some(r) => {
            let owner_id: String = r.try_get("owner_id").unwrap_or_default();
            let visibility: String = r.try_get("visibility").unwrap_or_default();
            let vis = Visibility::from_legacy(&visibility);
            let granted = can_edit(
                pool,
                &principal,
                ObjectType::Forecast,
                &forecast_id,
                &owner_id,
                vis,
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if !granted {
                return Err((StatusCode::FORBIDDEN, "Edit access denied".into()));
            }
        }
        None => return Err((StatusCode::NOT_FOUND, "Forecast not found".into())),
    }

    // Use the database function for atomic resolution
    let brier_score: f64 = sqlx::query_scalar("SELECT resolve_forecast($1, $2, $3, $4)")
        .bind(&forecast_id)
        .bind(req.actual_outcome)
        .bind(&user_id)
        .bind(&req.resolution_notes)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") {
                (StatusCode::NOT_FOUND, "Forecast not found".into())
            } else if msg.contains("not active") {
                (
                    StatusCode::CONFLICT,
                    "Forecast is not active — only active forecasts can be resolved".into(),
                )
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, msg)
            }
        })?;

    // Refresh leaderboard in background (non-blocking)
    let pool_bg = pool.clone();
    tokio::spawn(async move {
        let _ = sqlx::query("SELECT refresh_fermi_leaderboard()")
            .execute(&pool_bg)
            .await;
    });

    // ── Loop 5: annotate routing-decision episodes with this outcome ─────────
    //
    // When a forecast resolves, look for routing-decision episodes (tagged
    // "moe_routing_decision") from the agents used in this forecast, written
    // within the last 7 days. Annotate them with the outcome quality so the
    // moe_router_strategist's dreaming cycle can consolidate routing accuracy
    // into its classification rules.
    //
    // calibration_quality = 1.0 - brier_score (inverted: higher = better)
    {
        let forecast_id_clone = forecast_id.clone();
        let pool_annotate = pool.clone();
        let memory_store = state.memory_store.clone();
        let calibration_quality = 1.0 - brier_score.clamp(0.0, 1.0);

        tokio::spawn(async move {
            // Fetch the forecast to get agents_used
            let agents_used: Vec<serde_json::Value> =
                match sqlx::query("SELECT agents_used FROM fermi_forecasts WHERE id = $1")
                    .bind(&forecast_id_clone)
                    .fetch_optional(&pool_annotate)
                    .await
                {
                    Ok(Some(row)) => row
                        .try_get::<serde_json::Value, _>("agents_used")
                        .ok()
                        .and_then(|v| v.as_array().cloned())
                        .unwrap_or_default(),
                    _ => return,
                };

            let since = chrono::Utc::now() - chrono::Duration::days(7);

            for agent_entry in &agents_used {
                let agent_id_str = match agent_entry.get("agent_id").and_then(|v| v.as_str()) {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                let agent_uuid = match uuid::Uuid::parse_str(&agent_id_str) {
                    Ok(u) => u,
                    Err(_) => continue,
                };

                // Find routing-decision episodes for this agent in the last 7 days
                let routing_episodes = match sqlx::query(
                    "SELECT episode_id, context FROM episodes
                     WHERE agent_id = $1
                       AND timestamp_ref >= $2
                       AND $3 = ANY(tags)
                     ORDER BY timestamp_ref DESC
                     LIMIT 10",
                )
                .bind(agent_uuid)
                .bind(since)
                .bind("moe_routing_decision")
                .fetch_all(&pool_annotate)
                .await
                {
                    Ok(rows) => rows,
                    Err(_) => continue,
                };

                for row in &routing_episodes {
                    let episode_id: uuid::Uuid = match row.try_get("episode_id") {
                        Ok(id) => id,
                        Err(_) => continue,
                    };
                    let mut ctx: serde_json::Value = row
                        .try_get::<serde_json::Value, _>("context")
                        .unwrap_or(serde_json::json!({}));

                    // Annotate with outcome
                    if let Some(obj) = ctx.as_object_mut() {
                        obj.insert(
                            "outcome_quality".to_string(),
                            serde_json::json!(calibration_quality),
                        );
                        obj.insert(
                            "outcome_source".to_string(),
                            serde_json::json!("brier_forecast"),
                        );
                        obj.insert(
                            "outcome_brier_score".to_string(),
                            serde_json::json!(brier_score),
                        );
                        obj.insert(
                            "outcome_annotated_at".to_string(),
                            serde_json::json!(chrono::Utc::now().to_rfc3339()),
                        );
                    }

                    // Write the annotated context back
                    let _ = sqlx::query("UPDATE episodes SET context = $1 WHERE episode_id = $2")
                        .bind(&ctx)
                        .bind(episode_id)
                        .execute(&pool_annotate)
                        .await;
                }
            }

            // Drop memory_store ref — it was held to ensure the Arc stays alive
            drop(memory_store);
        });
    }

    // ── Queue cascade reviews for any relationships involving this
    //    forecast. Operator-gate rule: every parameter mutation passes
    //    through a human. We DON'T auto-propagate the cascade here;
    //    instead we queue a pending_cascade row per relationship and
    //    the operator reviews from the console queue.
    {
        let pool_q = pool.clone();
        let trigger = forecast_id.clone();
        let outcome = req.actual_outcome;
        let owner = user_id.to_string();
        tokio::spawn(async move {
            crate::handlers::pending_cascades::queue_pending_cascade(
                &pool_q,
                &trigger,
                "resolved",
                Some(outcome),
                "manual",
                &owner,
            )
            .await;
        });
    }

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "actual_outcome": req.actual_outcome,
        "brier_score": brier_score,
        "status": "resolved",
        "resolved_by": user_id,
        "resolution_notes": req.resolution_notes,
    })))
}

/// Feed a resolved forecast's Brier score to the MoE strategist.
///
/// Writes one `forecast_calibration` eval_signal per agent that contributed
/// to the forecast (score = 1 - brier, so 1.0 = perfect calibration). This
/// is the "BrierEvaluator" output that `get_agent_calibration` reads and
/// `moe_router_strategist` Stage 0 consumes.
///
/// Why this exists as a standalone fn: both resolution paths (the API
/// /resolve handler AND the polymarket oracle, which is the path real WC
/// results actually take) must feed the strategist. Previously only the
/// API handler had any feedback — and even that read `agents_used` with the
/// wrong key (`agent_id` vs the stored `name`), so nothing ever landed.
/// Here we resolve agent NAMES → agent_ids and write signals keyed by id.
///
/// Idempotent per (agent, forecast): re-resolving won't duplicate signals.
/// Best-effort — callers spawn it; failures are logged, never fatal.
pub async fn record_forecast_calibration_signals(
    pool: &sqlx::PgPool,
    forecast_id: &str,
    brier_score: f64,
) {
    let calibration = (1.0 - brier_score.clamp(0.0, 1.0)).clamp(0.0, 1.0);

    // agents_used entries look like {"name": "macro_data_agent", ...}.
    let agents_used: Vec<JsonValue> =
        match sqlx::query("SELECT agents_used FROM fermi_forecasts WHERE id = $1")
            .bind(forecast_id)
            .fetch_optional(pool)
            .await
        {
            Ok(Some(row)) => row
                .try_get::<JsonValue, _>("agents_used")
                .ok()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default(),
            _ => return,
        };

    if agents_used.is_empty() {
        tracing::info!(
            forecast = %forecast_id,
            "[brier-moe] no agents_used recorded; no calibration signals emitted"
        );
        return;
    }

    let rationale = format!(
        "forecast {} resolved (brier={:.4})",
        forecast_id, brier_score
    );

    for entry in &agents_used {
        // Accept either {"name": ...} (current schema) or {"agent_id": ...}.
        let agent_id: Option<uuid::Uuid> =
            if let Some(name) = entry.get("name").and_then(|v| v.as_str()) {
                sqlx::query_scalar::<_, uuid::Uuid>(
                    "SELECT agent_id FROM agents WHERE agent_name = $1 LIMIT 1",
                )
                .bind(name)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten()
            } else {
                entry
                    .get("agent_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| uuid::Uuid::parse_str(s).ok())
            };

        let Some(aid) = agent_id else { continue };

        // INSERT ... WHERE NOT EXISTS → idempotent per (agent, forecast).
        let res = sqlx::query(
            "INSERT INTO eval_signals
                  (agent_id, evaluator_name, evaluator_version, evaluator_tier,
                   dimension, score, confidence, rationale, created_at)
             SELECT $1, 'brier_forecast_resolver', 'v1', 'dimensional',
                    'forecast_calibration', $2, 1.0, $3, NOW()
              WHERE NOT EXISTS (
                  SELECT 1 FROM eval_signals
                   WHERE agent_id = $1
                     AND dimension = 'forecast_calibration'
                     AND rationale = $3
              )",
        )
        .bind(aid)
        .bind(calibration)
        .bind(&rationale)
        .execute(pool)
        .await;

        match res {
            Ok(r) if r.rows_affected() > 0 => tracing::info!(
                agent = %aid, forecast = %forecast_id, calibration = calibration,
                "[brier-moe] forecast_calibration signal recorded"
            ),
            Ok(_) => {} // already existed — idempotent skip
            Err(e) => tracing::warn!(
                agent = %aid, error = %e,
                "[brier-moe] failed to record calibration signal"
            ),
        }
    }
}

/// POST /api/forecasts/:id/void
///
/// Voids a forecast (cancels it without resolution). No Brier score computed.
pub async fn void_forecast_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // Spec 24 §3.2 Wave 2 (Sprint 2.4b): can_admin check before void.
    // Previously the owner check was embedded in the SQL WHERE clause.
    let acl_row = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility, status FROM fermi_forecasts WHERE id = $1",
    )
    .bind(&forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match acl_row {
        Some(r) => {
            let owner_id: String = r.try_get("owner_id").unwrap_or_default();
            let visibility: String = r.try_get("visibility").unwrap_or_default();
            let status: String = r.try_get("status").unwrap_or_default();
            if status != "draft" && status != "active" {
                return Err((
                    StatusCode::CONFLICT,
                    "Forecast already resolved/voided".into(),
                ));
            }
            let vis = Visibility::from_legacy(&visibility);
            let level = can_access(
                pool,
                &principal,
                ObjectType::Forecast,
                &forecast_id,
                &owner_id,
                vis,
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if !level.has_admin() {
                return Err((
                    StatusCode::FORBIDDEN,
                    "Admin access required to void".into(),
                ));
            }
        }
        None => return Err((StatusCode::NOT_FOUND, "Forecast not found".into())),
    }

    let result = sqlx::query(
        "UPDATE fermi_forecasts SET status = 'voided', updated_at = NOW()
         WHERE id = $1 AND status IN ('draft', 'active')
         RETURNING id",
    )
    .bind(&forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            "Forecast not found or already resolved/voided".into(),
        ));
    }

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "status": "voided",
    })))
}

// ═══════════════════════════════════════════════════════════════════
// PROBABILITY UPDATES
// ═══════════════════════════════════════════════════════════════════

/// POST /api/forecasts/:id/update-probability
///
/// Records a probability revision with reason and optional agent attribution.
/// This is the core of the forecasting workflow — updating beliefs as new
/// evidence arrives.
pub async fn update_probability_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
    Json(req): Json<UpdateProbabilityRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    if req.new_probability < 0.0 || req.new_probability > 1.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "new_probability must be between 0 and 1".into(),
        ));
    }

    // Get current state
    let row = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility, status, predicted_probability FROM fermi_forecasts WHERE id = $1",
    )
    .bind(&forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Forecast not found".into()))?;

    // Defensive try_get — the column is TEXT in migrations but UUID in
    // prod for the WC dataset. The earlier handler explicitly aliased to
    // ::text in the SELECT above; we still go through try_get to ensure
    // a single bad row never panics the handler into a 502.
    let owner_id: String = row.try_get("owner_id").unwrap_or_default();
    let visibility: String = row.try_get("visibility").unwrap_or_default();
    let vis = Visibility::from_legacy(&visibility);
    let granted = can_edit(
        pool,
        &principal,
        ObjectType::Forecast,
        &forecast_id,
        &owner_id,
        vis,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !granted {
        return Err((StatusCode::FORBIDDEN, "Edit access denied".into()));
    }

    let status: String = row.try_get("status").unwrap_or_default();
    if status != "active" && status != "draft" {
        return Err((
            StatusCode::CONFLICT,
            format!("Cannot update probability on a {} forecast", status),
        ));
    }

    // predicted_probability is REAL → sqlx f32. Reading it as f64 panics
    // with a type mismatch. Cast at the boundary.
    let previous_probability: f32 = row
        .try_get::<f32, _>("predicted_probability")
        .unwrap_or(0.0);

    // Record the update. previous_probability and new_probability are REAL
    // in the DB so we bind f32 — sqlx silently coerces f64→REAL by lossy
    // round-trip but production has been observed leaving the value NULL
    // when the type mismatches at bind time. Going through f32 explicitly
    // makes the contract unambiguous.
    let update_id = Uuid::new_v4().to_string();
    let new_prob_f32 = req.new_probability as f32;

    // The incoming value is this forecast's STANDALONE Monte-Carlo mean.
    // Persist it as the raw sim_probability — the recompose below derives
    // the displayed predicted_probability from it (and from siblings'),
    // so re-running a sim never resets a cascade-adjusted value back to
    // the standalone. See migration 158 + relationships::recompose.
    sqlx::query("UPDATE fermi_forecasts SET sim_probability = $1 WHERE id = $2")
        .bind(new_prob_f32)
        .bind(&forecast_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Recompose any mutex group this forecast belongs to. This writes the
    // derived predicted_probability for ALL group members and returns this
    // forecast's displayed value. Forecasts in no mutex group fall back to
    // the raw standalone. Best-effort: a recompose failure must not block
    // the core probability update.
    let displayed = match crate::handlers::relationships::recompose::recompose_forecast_groups(
        &forecast_id,
        pool,
    )
    .await
    {
        Ok(Some(v)) => v,
        Ok(None) => req.new_probability,
        Err((_, e)) => {
            tracing::warn!(forecast = %forecast_id, error = %e, "[recompose] failed; using raw standalone");
            req.new_probability
        }
    };
    let displayed_f32 = displayed as f32;

    // Trajectory row reflects the DISPLAYED (recomposed) value so the
    // trajectory tab shows the smart number, not the bare standalone.
    sqlx::query(
        "INSERT INTO fermi_forecast_updates
         (id, forecast_id, previous_probability, new_probability, reason, agent_id, evidence_added, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())",
    )
    .bind(&update_id)
    .bind(&forecast_id)
    .bind(previous_probability)
    .bind(displayed_f32)
    .bind(&req.reason)
    .bind(&req.agent_id)
    .bind(&req.evidence_added)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // recompose already wrote predicted_probability for members it
    // changed; ensure this forecast lands on the displayed value even when
    // it is in no group (recompose was a no-op) or was unchanged there.
    sqlx::query(
        "UPDATE fermi_forecasts SET predicted_probability = $1, updated_at = NOW() WHERE id = $2",
    )
    .bind(displayed_f32)
    .bind(&forecast_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Anchor the new probability immediately — each revision gets its own
    // tamper-evident commitment so the rate-of-change is fully provable.
    let commitment_hash = {
        // Anchor the DISPLAYED probability — the recomposed value is what
        // the forecast actually asserts.
        crate::handlers::forecast_benchmark::anchor_forecast(
            pool,
            &forecast_id,
            Some(&update_id),
            displayed,
            None,
            chrono::Utc::now(),
            Some("auto-anchor on probability update"),
        )
        .await
        .ok()
    };

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "update_id": update_id,
        "previous_probability": previous_probability,
        // The standalone value the client sent, for reference.
        "new_probability": req.new_probability,
        // The displayed value after mutex-group recomposition. Equal to
        // new_probability when the forecast is in no mutex group. The
        // console adopts this so a re-sim keeps eliminations priced in.
        "recomposed_probability": displayed,
        "reason": req.reason,
        "agent_id": req.agent_id,
        "commitment_hash": commitment_hash,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// PORTFOLIO CRUD
// ═══════════════════════════════════════════════════════════════════

/// POST /api/portfolios
pub async fn create_portfolio_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreatePortfolioRequest>,
) -> Result<(StatusCode, Json<JsonValue>), (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // Same defensive backfill as create_forecast_handler: guards
    // against fermi_portfolios.owner_id → users(user_id) FK
    // violation when the session's user_id is orphaned.
    ensure_user_row(pool, &principal).await?;

    let portfolio_id = Uuid::new_v4().to_string();
    let visibility = req.visibility.as_deref().unwrap_or("private");
    let team_id: Option<Uuid> = req.team_id.as_ref().and_then(|s| Uuid::parse_str(s).ok());

    sqlx::query(
        // v0.10.13: dropped `$4::uuid` cast on owner_id (see fermi_forecasts note).
         "INSERT INTO fermi_portfolios (id, title, description, owner_id, visibility, team_id, domain, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())",
    )
    .bind(&portfolio_id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(&user_id)
    .bind(visibility)
    .bind(team_id)
    .bind(&req.domain)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": portfolio_id,
            "title": req.title,
            "visibility": visibility,
        })),
    ))
}

/// GET /api/portfolios
pub async fn list_portfolios_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<ListPortfoliosQuery>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;
    let limit = q.limit.unwrap_or(50).min(100);
    let offset = q.offset.unwrap_or(0);

    let rows = sqlx::query(
        "SELECT p.id, p.title, p.description, p.owner_id::text AS owner_id,
                p.visibility, p.domain, p.team_id, p.created_at, p.updated_at,
                (SELECT COUNT(*) FROM fermi_portfolio_forecasts pf WHERE pf.portfolio_id = p.id) AS forecast_count,
                (SELECT COUNT(*) FROM fermi_portfolio_forecasts pf
                 JOIN fermi_forecasts f ON f.id = pf.forecast_id
                 WHERE pf.portfolio_id = p.id AND f.status = 'resolved') AS resolved_count,
                (SELECT AVG(f.brier_score) FROM fermi_portfolio_forecasts pf
                 JOIN fermi_forecasts f ON f.id = pf.forecast_id
                 WHERE pf.portfolio_id = p.id AND f.brier_score IS NOT NULL) AS avg_brier
         FROM fermi_portfolios p
         WHERE p.owner_id = $1
            OR p.visibility IN ('shared', 'public')
            OR (p.team_id IS NOT NULL
                AND EXISTS (SELECT 1 FROM team_members m
                            WHERE m.team_id = p.team_id AND m.member_id = $1))
            OR EXISTS (SELECT 1 FROM object_shares s
                       WHERE s.object_type = 'portfolio'
                         AND s.object_id = p.id::text
                         AND s.share_type = 'user'
                         AND s.share_target = $1)
         ORDER BY p.updated_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(&user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let portfolios: Vec<JsonValue> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.try_get::<String, _>("id").ok(),
                "title": r.try_get::<String, _>("title").ok(),
                "description": r.try_get::<Option<String>, _>("description").ok().flatten(),
                "owner_id": r.try_get::<String, _>("owner_id").ok(),
                "visibility": r.try_get::<String, _>("visibility").ok(),
                "domain": r.try_get::<Option<String>, _>("domain").ok().flatten(),
                "forecast_count": r.try_get::<i64, _>("forecast_count").ok(),
                "resolved_count": r.try_get::<i64, _>("resolved_count").ok(),
                "avg_brier": r.try_get::<Option<f64>, _>("avg_brier").ok().flatten(),
                // Owning team (Spec 24 §3.5.4). Exposed here so the
                // console's Teams panel can filter portfolios owned by
                // a specific team without an extra per-portfolio round
                // trip. Nullable — personally-owned portfolios have
                // team_id NULL.
                "team_id": r.try_get::<Option<Uuid>, _>("team_id").ok().flatten().map(|u| u.to_string()),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
                "updated_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at").ok().map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "portfolios": portfolios,
        "count": portfolios.len(),
    })))
}

/// GET /api/portfolios/:id/stats
///
/// Detailed portfolio statistics including Brier aggregation,
/// calibration curve data, and domain breakdown.
pub async fn portfolio_stats_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(portfolio_id): Path<String>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // Verify access (Spec 24 §3.2 Wave 1: also honour team membership when
    // the portfolio is private but linked to a team, mirroring
    // get_forecast_handler).
    let portfolio = sqlx::query(
        "SELECT owner_id::text AS owner_id, title, visibility, domain, team_id
         FROM fermi_portfolios WHERE id = $1",
    )
    .bind(&portfolio_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Portfolio not found".into()))?;

    let owner_id: String = portfolio.get("owner_id");
    let visibility: String = portfolio.get("visibility");
    let vis = Visibility::from_legacy(&visibility);
    let granted = can_view(
        pool,
        &principal,
        ObjectType::Portfolio,
        &portfolio_id,
        &owner_id,
        vis,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !granted {
        return Err((StatusCode::FORBIDDEN, "Access denied".into()));
    }

    // Aggregate stats
    let stats = sqlx::query(
        "SELECT
            COUNT(*) AS total_forecasts,
            COUNT(*) FILTER (WHERE f.status = 'active') AS active_count,
            COUNT(*) FILTER (WHERE f.status = 'resolved') AS resolved_count,
            COUNT(*) FILTER (WHERE f.status = 'draft') AS draft_count,
            AVG(f.brier_score) FILTER (WHERE f.brier_score IS NOT NULL) AS avg_brier,
            MIN(f.brier_score) FILTER (WHERE f.brier_score IS NOT NULL) AS best_brier,
            MAX(f.brier_score) FILTER (WHERE f.brier_score IS NOT NULL) AS worst_brier,
            STDDEV(f.brier_score) FILTER (WHERE f.brier_score IS NOT NULL) AS brier_stddev,
            AVG(f.predicted_probability) AS avg_probability,
            -- Calibration: for each probability decile, what fraction resolved true?
            AVG(CASE WHEN f.predicted_probability < 0.2 AND f.brier_score IS NOT NULL
                     THEN f.actual_outcome::int END) AS cal_0_20,
            AVG(CASE WHEN f.predicted_probability >= 0.2 AND f.predicted_probability < 0.4 AND f.brier_score IS NOT NULL
                     THEN f.actual_outcome::int END) AS cal_20_40,
            AVG(CASE WHEN f.predicted_probability >= 0.4 AND f.predicted_probability < 0.6 AND f.brier_score IS NOT NULL
                     THEN f.actual_outcome::int END) AS cal_40_60,
            AVG(CASE WHEN f.predicted_probability >= 0.6 AND f.predicted_probability < 0.8 AND f.brier_score IS NOT NULL
                     THEN f.actual_outcome::int END) AS cal_60_80,
            AVG(CASE WHEN f.predicted_probability >= 0.8 AND f.brier_score IS NOT NULL
                     THEN f.actual_outcome::int END) AS cal_80_100,
            -- Domain breakdown
            array_agg(DISTINCT f.domain) FILTER (WHERE f.domain IS NOT NULL) AS domains
         FROM fermi_portfolio_forecasts pf
         JOIN fermi_forecasts f ON f.id = pf.forecast_id
         WHERE pf.portfolio_id = $1",
    )
    .bind(&portfolio_id)
    .fetch_one(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Recent resolutions
    let recent = sqlx::query(
        "SELECT f.id, f.question_text, f.predicted_probability, f.actual_outcome,
                f.brier_score, f.resolved_at
         FROM fermi_portfolio_forecasts pf
         JOIN fermi_forecasts f ON f.id = pf.forecast_id
         WHERE pf.portfolio_id = $1 AND f.status = 'resolved'
         ORDER BY f.resolved_at DESC
         LIMIT 10",
    )
    .bind(&portfolio_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let recent_resolutions: Vec<JsonValue> = recent
        .iter()
        .map(|r| {
            json!({
                "id": r.try_get::<String, _>("id").ok(),
                "question_text": r.try_get::<String, _>("question_text").ok(),
                "predicted_probability": r.try_get::<f32, _>("predicted_probability").ok().map(|v| v as f64),
                "actual_outcome": r.try_get::<Option<bool>, _>("actual_outcome").ok().flatten(),
                "brier_score": r.try_get::<Option<f32>, _>("brier_score").ok().flatten().map(|v| v as f64),
                "resolved_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at").ok().flatten().map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "portfolio_id": portfolio_id,
        "title": portfolio.try_get::<String, _>("title").ok(),
        "domain": portfolio.try_get::<Option<String>, _>("domain").ok().flatten(),
        "stats": {
            "total_forecasts": stats.try_get::<i64, _>("total_forecasts").ok(),
            "active_count": stats.try_get::<i64, _>("active_count").ok(),
            "resolved_count": stats.try_get::<i64, _>("resolved_count").ok(),
            "draft_count": stats.try_get::<i64, _>("draft_count").ok(),
            "avg_brier": stats.try_get::<Option<f64>, _>("avg_brier").ok().flatten(),
            "best_brier": stats.try_get::<Option<f64>, _>("best_brier").ok().flatten(),
            "worst_brier": stats.try_get::<Option<f64>, _>("worst_brier").ok().flatten(),
            "brier_stddev": stats.try_get::<Option<f64>, _>("brier_stddev").ok().flatten(),
            "avg_probability": stats.try_get::<Option<f64>, _>("avg_probability").ok().flatten(),
            "domains": stats.try_get::<Option<Vec<String>>, _>("domains").ok().flatten(),
        },
        "calibration": {
            "0-20": stats.try_get::<Option<f64>, _>("cal_0_20").ok().flatten(),
            "20-40": stats.try_get::<Option<f64>, _>("cal_20_40").ok().flatten(),
            "40-60": stats.try_get::<Option<f64>, _>("cal_40_60").ok().flatten(),
            "60-80": stats.try_get::<Option<f64>, _>("cal_60_80").ok().flatten(),
            "80-100": stats.try_get::<Option<f64>, _>("cal_80_100").ok().flatten(),
        },
        "recent_resolutions": recent_resolutions,
    })))
}

/// POST /api/portfolios/:id/forecasts
///
/// Add a forecast to a portfolio.
pub async fn add_forecast_to_portfolio_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(portfolio_id): Path<String>,
    Json(req): Json<PortfolioForecastRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // Verify edit access (Spec 24 §3.2 Wave 2: can_edit, not just owner).
    let acl_row = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility FROM fermi_portfolios WHERE id = $1",
    )
    .bind(&portfolio_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match acl_row {
        Some(r) => {
            let owner_id: String = r.try_get("owner_id").unwrap_or_default();
            let visibility: String = r.try_get("visibility").unwrap_or_default();
            let vis = Visibility::from_legacy(&visibility);
            let granted = can_edit(
                pool,
                &principal,
                ObjectType::Portfolio,
                &portfolio_id,
                &owner_id,
                vis,
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if !granted {
                return Err((StatusCode::FORBIDDEN, "Edit access denied".into()));
            }
        }
        None => return Err((StatusCode::NOT_FOUND, "Portfolio not found".into())),
    }

    sqlx::query(
        "INSERT INTO fermi_portfolio_forecasts (portfolio_id, forecast_id)
         VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(&portfolio_id)
    .bind(&req.forecast_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "portfolio_id": portfolio_id,
        "forecast_id": req.forecast_id,
        "status": "added",
    })))
}

/// DELETE /api/portfolios/:id/forecasts/:forecast_id
pub async fn remove_forecast_from_portfolio_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((portfolio_id, forecast_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // Verify edit access (Spec 24 §3.2 Wave 2: can_edit, not just owner).
    let acl_row = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility FROM fermi_portfolios WHERE id = $1",
    )
    .bind(&portfolio_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match acl_row {
        Some(r) => {
            let owner_id: String = r.try_get("owner_id").unwrap_or_default();
            let visibility: String = r.try_get("visibility").unwrap_or_default();
            let vis = Visibility::from_legacy(&visibility);
            let granted = can_edit(
                pool,
                &principal,
                ObjectType::Portfolio,
                &portfolio_id,
                &owner_id,
                vis,
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if !granted {
                return Err((StatusCode::FORBIDDEN, "Edit access denied".into()));
            }
        }
        None => return Err((StatusCode::NOT_FOUND, "Portfolio not found".into())),
    }

    sqlx::query(
        "DELETE FROM fermi_portfolio_forecasts WHERE portfolio_id = $1 AND forecast_id = $2",
    )
    .bind(&portfolio_id)
    .bind(&forecast_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/portfolios/:id
pub async fn delete_portfolio_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(portfolio_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    let acl_row = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility FROM fermi_portfolios WHERE id = $1",
    )
    .bind(&portfolio_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match acl_row {
        Some(r) => {
            let owner_id: String = r.try_get("owner_id").unwrap_or_default();
            let visibility: String = r.try_get("visibility").unwrap_or_default();
            let vis = Visibility::from_legacy(&visibility);
            let level = can_access(
                pool,
                &principal,
                ObjectType::Portfolio,
                &portfolio_id,
                &owner_id,
                vis,
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if !level.has_admin() {
                return Err((
                    StatusCode::FORBIDDEN,
                    "Admin access required to delete".into(),
                ));
            }
        }
        None => return Err((StatusCode::NOT_FOUND, "Portfolio not found".into())),
    }

    sqlx::query("DELETE FROM fermi_portfolios WHERE id = $1")
        .bind(&portfolio_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct PatchPortfolioRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    /// Spec 24 §3.2 Wave 1. Mirrors `CreatePortfolioRequest` so a portfolio's
    /// domain/visibility/team_id can change without requiring delete + recreate.
    /// Standard PATCH semantics: missing-or-null = unchanged. Use a
    /// dedicated "clear" mechanism (TBD) if the operator ever needs to
    /// detach a team explicitly. Until then, COALESCE preserves the old value.
    pub domain: Option<String>,
    pub visibility: Option<String>,
    pub team_id: Option<String>,
}

/// PATCH /api/portfolios/:id
///
/// Spec 24 §3.2 Wave 1: extended to actually persist `domain`, `visibility`,
/// and `team_id`. The previous handler accepted only `title`/`description`
/// — any other field on the wire was silently dropped at the serde layer.
/// That made the dishonest "Team" tile in the console commit sheet
/// undetectable: PATCH would 200 even though nothing changed.
pub async fn patch_portfolio_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(portfolio_id): Path<String>,
    Json(req): Json<PatchPortfolioRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    let acl_row = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility FROM fermi_portfolios WHERE id = $1",
    )
    .bind(&portfolio_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match acl_row {
        Some(r) => {
            let owner_id: String = r.try_get("owner_id").unwrap_or_default();
            let visibility: String = r.try_get("visibility").unwrap_or_default();
            let vis = Visibility::from_legacy(&visibility);
            let level = can_access(
                pool,
                &principal,
                ObjectType::Portfolio,
                &portfolio_id,
                &owner_id,
                vis,
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if !level.has_admin() {
                return Err((
                    StatusCode::FORBIDDEN,
                    "Admin access required to delete".into(),
                ));
            }
        }
        None => return Err((StatusCode::NOT_FOUND, "Portfolio not found".into())),
    }

    // Validate visibility up front: the DB CHECK constraint will reject any
    // other value with a 500-shaped error, but a 400 with a clear message is
    // the polite contract for a PATCH-time client mistake.
    if let Some(ref v) = req.visibility {
        if !matches!(v.as_str(), "private" | "shared" | "public") {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("invalid visibility '{}': expected private|shared|public", v),
            ));
        }
    }

    // team_id arrives as a string on the wire (Option<String>) so the JSON
    // shape stays uniform with CreatePortfolioRequest. Parse to UUID before
    // binding — fermi_portfolios.team_id is `uuid` in prod (verified
    // 2026-06-19). A bad uuid is a 400, not a 500.
    let team_id_uuid: Option<Uuid> = match req.team_id.as_deref() {
        Some(s) => Some(Uuid::parse_str(s).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("invalid team_id '{}': {}", s, e),
            )
        })?),
        None => None,
    };

    sqlx::query(
        "UPDATE fermi_portfolios
         SET title       = COALESCE($2, title),
             description = COALESCE($3, description),
             domain      = COALESCE($4, domain),
             visibility  = COALESCE($5, visibility),
             team_id     = COALESCE($6, team_id),
             updated_at  = NOW()
         WHERE id = $1",
    )
    .bind(&portfolio_id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(&req.domain)
    .bind(&req.visibility)
    .bind(team_id_uuid)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "id": portfolio_id, "status": "updated" })))
}

/// GET /api/portfolios/:id/forecasts
///
/// Returns forecasts in a portfolio with question, probability, status,
/// Brier score (if resolved), and when they were added.
pub async fn list_portfolio_forecasts_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(portfolio_id): Path<String>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // Allow access if owner OR portfolio is shared/public OR caller is a
    // member of the portfolio's team (Spec 24 §3.2 Wave 1, matching
    // portfolio_stats_handler and get_forecast_handler).
    let portfolio = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility, team_id
         FROM fermi_portfolios WHERE id = $1",
    )
    .bind(&portfolio_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match portfolio {
        None => return Err((StatusCode::NOT_FOUND, "Portfolio not found".into())),
        Some(row) => {
            let owner: String = row.get("owner_id");
            let visibility: String = row.get("visibility");
            let vis = Visibility::from_legacy(&visibility);
            let granted = can_view(
                pool,
                &principal,
                ObjectType::Portfolio,
                &portfolio_id,
                &owner,
                vis,
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if !granted {
                return Err((StatusCode::FORBIDDEN, "Not your portfolio".into()));
            }
        }
    }

    // Enriched projection: include metadata.polymarket so the console can
    // compute Fermi-vs-crowd divergence inline (the dashboard's
    // "biggest opportunity" sort), updated_at so we can show recent
    // activity, tags for grouping/filtering, and the COUNT of recent
    // forecast updates so the operator can see which rows have moved
    // recently without opening each one.
    //
    // Subquery (n_recent_updates) is bounded by a 7-day window so the
    // count means "how active is this forecast lately", not "how
    // hand-tuned in total".
    //
    // Spec 24 §3.2 Wave 1 #4: also COUNT object_shares rows so the
    // console can render the visibility badge correctly (🔒 vs 🔗 vs
    // 👥 vs 🌐) without a per-row second roundtrip. Always 0 today —
    // Sprint 2 is when /api/forecasts/:id/shares starts producing rows.
    // idx_object_shares_object(object_type, object_id) is in place so
    // the subquery is index-fast even at scale.
    let rows = sqlx::query(
        "SELECT f.id,
                f.question_text,
                f.predicted_probability,
                f.status,
                f.brier_score,
                f.actual_outcome,
                f.resolved_at,
                f.visibility,
                f.updated_at,
                f.metadata,
                f.tags,
                f.team_id,
                pf.added_at,
                (SELECT COUNT(*) FROM fermi_forecast_updates u
                 WHERE u.forecast_id = f.id
                   AND u.created_at > NOW() - INTERVAL '7 days') AS n_recent_updates,
                (SELECT COUNT(*) FROM object_shares s
                 WHERE s.object_type = 'forecast'
                   AND s.object_id = f.id) AS share_count
         FROM fermi_portfolio_forecasts pf
         JOIN fermi_forecasts f ON f.id = pf.forecast_id
         WHERE pf.portfolio_id = $1
         ORDER BY pf.added_at DESC",
    )
    .bind(&portfolio_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let forecasts: Vec<JsonValue> = rows
        .iter()
        .map(|r| {
            // Defensive try_get on every column — a single bad row should
            // never bring down the response.
            let prob: Option<f64> = r.try_get::<Option<f32>, _>("predicted_probability")
                .ok().flatten().map(|v| v as f64);
            let metadata: Option<JsonValue> = r.try_get("metadata").ok();
            // Extract Polymarket fields once so the row can carry the
            // crowd price (last_market_price) and the divergence vs the
            // Fermi probability inline — saves a per-row PM API call on
            // the client.
            let (pm_price, pm_url, pm_volume_24h, pm_divergence_pp) = match metadata.as_ref()
                .and_then(|m| m.get("polymarket"))
            {
                Some(pm) => {
                    let price = pm.get("last_market_price").and_then(|v| v.as_f64());
                    let url = pm.get("pm_url").and_then(|v| v.as_str()).map(String::from);
                    let vol = pm.get("last_volume_24h").and_then(|v| v.as_f64());
                    let div = match (prob, price) {
                        (Some(p), Some(c)) => Some((p - c) * 100.0),
                        _ => None,
                    };
                    (price, url, vol, div)
                }
                None => (None, None, None, None),
            };
            json!({
                "id":                   r.try_get::<String, _>("id").ok(),
                "question_text":        r.try_get::<String, _>("question_text").ok(),
                "predicted_probability":prob,
                "status":               r.try_get::<String, _>("status").ok(),
                "brier_score":          r.try_get::<Option<f32>, _>("brier_score").ok().flatten().map(|v| v as f64),
                "actual_outcome":       r.try_get::<Option<bool>, _>("actual_outcome").ok().flatten(),
                "resolved_at":          r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at").ok().flatten().map(|d| d.to_rfc3339()),
                "visibility":           r.try_get::<String, _>("visibility").ok(),
                "updated_at":           r.try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at").ok().map(|d| d.to_rfc3339()),
                "added_at":             r.try_get::<chrono::DateTime<chrono::Utc>, _>("added_at").ok().map(|d| d.to_rfc3339()),
                "tags":                 r.try_get::<Vec<String>, _>("tags").ok(),
                "team_id":              r.try_get::<Option<Uuid>, _>("team_id").ok().flatten().map(|u| u.to_string()),
                "n_recent_updates":     r.try_get::<i64, _>("n_recent_updates").ok(),
                "share_count":          r.try_get::<i64, _>("share_count").ok(),
                "pm_market_price":      pm_price,
                "pm_url":               pm_url,
                "pm_volume_24h":        pm_volume_24h,
                "pm_divergence_pp":     pm_divergence_pp,
            })
        })
        .collect();

    Ok(Json(json!({
        "portfolio_id": portfolio_id,
        "forecasts": forecasts,
        "count": forecasts.len(),
    })))
}

// ═══════════════════════════════════════════════════════════════════
// LEADERBOARD
// ═══════════════════════════════════════════════════════════════════

/// GET /api/leaderboard
///
/// Returns the forecasting leaderboard ranked by average Brier score.
/// Lower is better. Minimum 5 resolved forecasts to appear.
pub async fn leaderboard_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Query(q): Query<LeaderboardQuery>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);
    let min_forecasts = q.min_forecasts.unwrap_or(5);

    // Try materialized view first, fall back to live query
    let rows = sqlx::query(
        "SELECT owner_id, display_name, total_resolved, avg_brier_score,
                best_brier_score, worst_brier_score, brier_stddev,
                accuracy_0_20, accuracy_20_40, accuracy_40_60, accuracy_60_80, accuracy_80_100,
                last_resolved_at, domains,
                ROW_NUMBER() OVER (ORDER BY avg_brier_score ASC) AS rank
         FROM fermi_leaderboard
         WHERE total_resolved >= $1
         ORDER BY avg_brier_score ASC
         LIMIT $2 OFFSET $3",
    )
    .bind(min_forecasts)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await;

    // If materialized view doesn't exist yet, compute live
    let rows = match rows {
        Ok(r) => r,
        Err(_) => {
            // Fallback: live query (slower but works before first REFRESH)
            sqlx::query(
                "SELECT f.owner_id::text AS owner_id, COALESCE(u.display_name, u.name, u.email, u.user_id) AS display_name,
                        COUNT(*) AS total_resolved,
                        AVG(f.brier_score) AS avg_brier_score,
                        MIN(f.brier_score) AS best_brier_score,
                        MAX(f.brier_score) AS worst_brier_score,
                        STDDEV(f.brier_score) AS brier_stddev,
                        MAX(f.resolved_at) AS last_resolved_at,
                        ROW_NUMBER() OVER (ORDER BY AVG(f.brier_score) ASC) AS rank
                 FROM fermi_forecasts f
                 JOIN users u ON u.user_id = f.owner_id
                 WHERE f.status = 'resolved' AND f.brier_score IS NOT NULL
                 GROUP BY f.owner_id, u.display_name, u.name, u.email, u.user_id
                 HAVING COUNT(*) >= $1
                 ORDER BY avg_brier_score ASC
                 LIMIT $2 OFFSET $3",
            )
            .bind(min_forecasts)
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        }
    };

    let entries: Vec<JsonValue> = rows
        .iter()
        .map(|r| {
            json!({
                "rank": r.try_get::<i64, _>("rank").ok(),
                "owner_id": r.try_get::<String, _>("owner_id").ok(),
                "display_name": r.try_get::<Option<String>, _>("display_name").ok().flatten(),
                "total_resolved": r.try_get::<i64, _>("total_resolved").ok(),
                "avg_brier_score": r.try_get::<Option<f64>, _>("avg_brier_score").ok().flatten(),
                "best_brier_score": r.try_get::<Option<f64>, _>("best_brier_score").ok().flatten(),
                "worst_brier_score": r.try_get::<Option<f64>, _>("worst_brier_score").ok().flatten(),
                "brier_stddev": r.try_get::<Option<f64>, _>("brier_stddev").ok().flatten(),
                "calibration": {
                    "0-20": r.try_get::<Option<f64>, _>("accuracy_0_20").ok().flatten(),
                    "20-40": r.try_get::<Option<f64>, _>("accuracy_20_40").ok().flatten(),
                    "40-60": r.try_get::<Option<f64>, _>("accuracy_40_60").ok().flatten(),
                    "60-80": r.try_get::<Option<f64>, _>("accuracy_60_80").ok().flatten(),
                    "80-100": r.try_get::<Option<f64>, _>("accuracy_80_100").ok().flatten(),
                },
                "last_resolved_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_resolved_at").ok().flatten().map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "leaderboard": entries,
        "count": entries.len(),
        "min_forecasts": min_forecasts,
    })))
}

/// GET /api/forecasts/my-stats
///
/// Returns the authenticated user's personal forecasting statistics.
pub async fn my_stats_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    let stats = sqlx::query(
        "SELECT
            COUNT(*) AS total_forecasts,
            COUNT(*) FILTER (WHERE status = 'active') AS active_count,
            COUNT(*) FILTER (WHERE status = 'resolved') AS resolved_count,
            COUNT(*) FILTER (WHERE status = 'draft') AS draft_count,
            AVG(brier_score) FILTER (WHERE brier_score IS NOT NULL) AS avg_brier,
            MIN(brier_score) FILTER (WHERE brier_score IS NOT NULL) AS best_brier,
            MAX(brier_score) FILTER (WHERE brier_score IS NOT NULL) AS worst_brier,
            -- Calibration
            AVG(CASE WHEN predicted_probability < 0.2 AND brier_score IS NOT NULL
                     THEN actual_outcome::int END) AS cal_0_20,
            AVG(CASE WHEN predicted_probability >= 0.2 AND predicted_probability < 0.4 AND brier_score IS NOT NULL
                     THEN actual_outcome::int END) AS cal_20_40,
            AVG(CASE WHEN predicted_probability >= 0.4 AND predicted_probability < 0.6 AND brier_score IS NOT NULL
                     THEN actual_outcome::int END) AS cal_40_60,
            AVG(CASE WHEN predicted_probability >= 0.6 AND predicted_probability < 0.8 AND brier_score IS NOT NULL
                     THEN actual_outcome::int END) AS cal_60_80,
            AVG(CASE WHEN predicted_probability >= 0.8 AND brier_score IS NOT NULL
                     THEN actual_outcome::int END) AS cal_80_100,
            -- Streak: consecutive days with at least one forecast created or resolved
            -- (simplified — just count distinct active days in last 30)
            COUNT(DISTINCT DATE(created_at)) FILTER (WHERE created_at > NOW() - INTERVAL '30 days') AS active_days_30d,
            array_agg(DISTINCT domain) FILTER (WHERE domain IS NOT NULL) AS domains
         FROM fermi_forecasts
         WHERE owner_id = $1",
    )
    .bind(&user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Get rank from leaderboard (if enough forecasts)
    let rank: Option<i64> = sqlx::query_scalar(
        "SELECT rank FROM (
            SELECT owner_id, ROW_NUMBER() OVER (ORDER BY AVG(brier_score) ASC) AS rank
            FROM fermi_forecasts
            WHERE status = 'resolved' AND brier_score IS NOT NULL
            GROUP BY owner_id
            HAVING COUNT(*) >= 5
        ) ranked WHERE owner_id = $1",
    )
    .bind(&user_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    Ok(Json(json!({
        "owner_id": user_id,
        "stats": {
            "total_forecasts": stats.try_get::<i64, _>("total_forecasts").ok(),
            "active_count": stats.try_get::<i64, _>("active_count").ok(),
            "resolved_count": stats.try_get::<i64, _>("resolved_count").ok(),
            "draft_count": stats.try_get::<i64, _>("draft_count").ok(),
            "avg_brier": stats.try_get::<Option<f64>, _>("avg_brier").ok().flatten(),
            "best_brier": stats.try_get::<Option<f64>, _>("best_brier").ok().flatten(),
            "worst_brier": stats.try_get::<Option<f64>, _>("worst_brier").ok().flatten(),
            "active_days_30d": stats.try_get::<i64, _>("active_days_30d").ok(),
            "domains": stats.try_get::<Option<Vec<String>>, _>("domains").ok().flatten(),
        },
        "calibration": {
            "0-20": stats.try_get::<Option<f64>, _>("cal_0_20").ok().flatten(),
            "20-40": stats.try_get::<Option<f64>, _>("cal_20_40").ok().flatten(),
            "40-60": stats.try_get::<Option<f64>, _>("cal_40_60").ok().flatten(),
            "60-80": stats.try_get::<Option<f64>, _>("cal_60_80").ok().flatten(),
            "80-100": stats.try_get::<Option<f64>, _>("cal_80_100").ok().flatten(),
        },
        "rank": rank,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// PUBLIC DISCOVERY
// ═══════════════════════════════════════════════════════════════════

/// GET /api/forecasts/public
///
/// Browse public forecasts. No authentication required (but we still
/// accept it for personalization).
pub async fn public_forecasts_handler(
    State(state): State<AppState>,
    Query(q): Query<ListForecastsQuery>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);

    let sort_col = match q.sort.as_deref() {
        Some("updated") => "f.updated_at",
        Some("target_date") => "f.target_date",
        Some("brier_score") => "f.brier_score",
        _ => "f.created_at",
    };
    let sort_order = match q.order.as_deref() {
        Some("asc") => "ASC",
        _ => "DESC",
    };

    let mut conditions = vec!["f.visibility = 'public'".to_string()];
    let mut binds: Vec<String> = Vec::new();
    let mut bind_idx = 0u32;

    if let Some(ref status) = q.status {
        bind_idx += 1;
        conditions.push(format!("f.status = ${}", bind_idx));
        binds.push(status.clone());
    }

    if let Some(ref domain) = q.domain {
        bind_idx += 1;
        conditions.push(format!("f.domain = ${}", bind_idx));
        binds.push(domain.clone());
    }

    if let Some(ref tag) = q.tag {
        bind_idx += 1;
        conditions.push(format!("${} = ANY(f.tags)", bind_idx));
        binds.push(tag.clone());
    }

    let where_clause = conditions.join(" AND ");
    let sql = format!(
        "SELECT f.id, f.owner_id::text AS owner_id, f.question_text, f.domain, f.predicted_probability,
                f.status, f.brier_score, f.actual_outcome, f.target_date,
                f.tags, f.created_at, f.resolved_at,
                COALESCE(u.display_name, u.name, u.email, u.user_id) AS owner_display_name
         FROM fermi_forecasts f
         LEFT JOIN users u ON u.user_id = f.owner_id
         WHERE {}
         ORDER BY {} {} NULLS LAST
         LIMIT {} OFFSET {}",
        where_clause, sort_col, sort_order, limit, offset
    );

    let mut query = sqlx::query(&sql);
    for b in &binds {
        query = query.bind(b);
    }

    let rows = query
        .fetch_all(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let forecasts: Vec<JsonValue> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.try_get::<String, _>("id").ok(),
                "owner_id": r.try_get::<String, _>("owner_id").ok(),
                "owner_display_name": r.try_get::<Option<String>, _>("owner_display_name").ok().flatten(),
                "question_text": r.try_get::<String, _>("question_text").ok(),
                "domain": r.try_get::<Option<String>, _>("domain").ok().flatten(),
                // Postgres REAL → sqlx f32. See get_forecast_handler for the
                // full rationale; same bug in three list-style serializers.
                "predicted_probability": r.try_get::<f32, _>("predicted_probability").ok().map(|v| v as f64),
                "status": r.try_get::<String, _>("status").ok(),
                "brier_score": r.try_get::<Option<f32>, _>("brier_score").ok().flatten().map(|v| v as f64),
                "actual_outcome": r.try_get::<Option<bool>, _>("actual_outcome").ok().flatten(),
                "target_date": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("target_date").ok().flatten().map(|t| t.to_rfc3339()),
                "tags": r.try_get::<Vec<String>, _>("tags").ok(),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
                "resolved_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at").ok().flatten().map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "forecasts": forecasts,
        "count": forecasts.len(),
    })))
}

// ═══════════════════════════════════════════════════════════════════
// Forecast Agent Schedules
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct UpsertScheduleRequest {
    pub agent_id: String,
    pub driver_name: String,
    pub query: String,
    pub interval_hours: i32,
}

/// GET /api/forecasts/:id/schedules — list active schedules for this forecast.
pub async fn list_forecast_schedules_handler(
    Path(forecast_id): Path<String>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    let owner: Option<String> =
        sqlx::query_scalar("SELECT owner_id::text FROM fermi_forecasts WHERE id = $1")
            .bind(&forecast_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match owner {
        None => return Err((StatusCode::NOT_FOUND, "Forecast not found".into())),
        Some(ref oid) if oid != &user_id => {
            return Err((StatusCode::FORBIDDEN, "Not your forecast".into()))
        }
        _ => {}
    }

    let rows = sqlx::query(
        "SELECT id::text, forecast_id, agent_id, driver_name, query, interval_hours,
                last_run_at, next_run_at, enabled, created_at
         FROM fermi_forecast_schedules
         WHERE forecast_id = $1
         ORDER BY created_at ASC",
    )
    .bind(&forecast_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let schedules: Vec<JsonValue> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.try_get::<String, _>("id").unwrap_or_default(),
                "forecast_id": r.try_get::<String, _>("forecast_id").unwrap_or_default(),
                "agent_id": r.try_get::<String, _>("agent_id").unwrap_or_default(),
                "driver_name": r.try_get::<String, _>("driver_name").unwrap_or_default(),
                "query": r.try_get::<String, _>("query").unwrap_or_default(),
                "interval_hours": r.try_get::<i32, _>("interval_hours").unwrap_or(24),
                "last_run_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_run_at")
                    .ok().flatten().map(|t| t.to_rfc3339()),
                "next_run_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("next_run_at")
                    .ok().map(|t| t.to_rfc3339()).unwrap_or_default(),
                "enabled": r.try_get::<bool, _>("enabled").unwrap_or(true),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                    .ok().map(|t| t.to_rfc3339()).unwrap_or_default(),
            })
        })
        .collect();

    Ok(Json(json!({ "schedules": schedules })))
}

/// PUT /api/forecasts/:id/schedules — upsert a schedule (one per agent+driver).
pub async fn upsert_forecast_schedule_handler(
    Path(forecast_id): Path<String>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<UpsertScheduleRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    let owner: Option<String> =
        sqlx::query_scalar("SELECT owner_id::text FROM fermi_forecasts WHERE id = $1")
            .bind(&forecast_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match owner {
        None => return Err((StatusCode::NOT_FOUND, "Forecast not found".into())),
        Some(ref oid) if oid != &user_id => {
            return Err((StatusCode::FORBIDDEN, "Not your forecast".into()))
        }
        _ => {}
    }

    // interval_hours = 0 is the "on-demand only" cadence: the schedule is
    // saved (so the operator can fire it via Run Now without re-typing the
    // query) but the overdue-driven auto-fire never triggers because
    // next_run_at is set to the year-3000 sentinel.
    if req.interval_hours < 0 || req.interval_hours > 8760 {
        return Err((
            StatusCode::BAD_REQUEST,
            "interval_hours must be 0–8760".into(),
        ));
    }

    let next_run_at = if req.interval_hours == 0 {
        // Sentinel: never overdue. Keeping it as a real timestamp (rather
        // than NULL) avoids a column-nullability migration; the load
        // path's overdue check (next_run_at <= NOW()) just never matches.
        chrono::DateTime::parse_from_rfc3339("3000-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    } else {
        chrono::Utc::now() + chrono::Duration::hours(req.interval_hours as i64)
    };

    let row = sqlx::query(
        "INSERT INTO fermi_forecast_schedules
             (forecast_id, agent_id, driver_name, query, interval_hours, next_run_at)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (forecast_id, agent_id, driver_name) DO UPDATE SET
             query          = EXCLUDED.query,
             interval_hours = EXCLUDED.interval_hours,
             next_run_at    = EXCLUDED.next_run_at,
             enabled        = true,
             updated_at     = NOW()
         RETURNING id::text, next_run_at",
    )
    .bind(&forecast_id)
    .bind(&req.agent_id)
    .bind(&req.driver_name)
    .bind(&req.query)
    .bind(req.interval_hours)
    .bind(next_run_at)
    .fetch_one(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "id": row.try_get::<String, _>("id").unwrap_or_default(),
        "next_run_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("next_run_at")
            .ok().map(|t| t.to_rfc3339()),
    })))
}

/// DELETE /api/forecasts/:id/schedules/:schedule_id
pub async fn delete_forecast_schedule_handler(
    Path((forecast_id, schedule_id)): Path<(String, String)>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    let owner: Option<String> =
        sqlx::query_scalar("SELECT owner_id::text FROM fermi_forecasts WHERE id = $1")
            .bind(&forecast_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match owner {
        None => return Err((StatusCode::NOT_FOUND, "Forecast not found".into())),
        Some(ref oid) if oid != &user_id => {
            return Err((StatusCode::FORBIDDEN, "Not your forecast".into()))
        }
        _ => {}
    }

    let sid = Uuid::parse_str(&schedule_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid schedule ID".into()))?;

    sqlx::query("DELETE FROM fermi_forecast_schedules WHERE id = $1 AND forecast_id = $2")
        .bind(sid)
        .bind(&forecast_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "deleted": true })))
}

/// POST /api/forecasts/:id/schedules/:schedule_id/run
/// Records a completed run — bumps last_run_at, advances next_run_at by interval.
pub async fn record_schedule_run_handler(
    Path((forecast_id, schedule_id)): Path<(String, String)>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    let owner: Option<String> =
        sqlx::query_scalar("SELECT owner_id::text FROM fermi_forecasts WHERE id = $1")
            .bind(&forecast_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match owner {
        None => return Err((StatusCode::NOT_FOUND, "Forecast not found".into())),
        Some(ref oid) if oid != &user_id => {
            return Err((StatusCode::FORBIDDEN, "Not your forecast".into()))
        }
        _ => {}
    }

    let sid = Uuid::parse_str(&schedule_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid schedule ID".into()))?;

    let row = sqlx::query(
        "UPDATE fermi_forecast_schedules
         SET last_run_at = NOW(),
             next_run_at = NOW() + (interval_hours * INTERVAL '1 hour'),
             updated_at  = NOW()
         WHERE id = $1 AND forecast_id = $2
         RETURNING next_run_at",
    )
    .bind(sid)
    .bind(&forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let next_run_at = row
        .and_then(|r| {
            r.try_get::<chrono::DateTime<chrono::Utc>, _>("next_run_at")
                .ok()
        })
        .map(|t| t.to_rfc3339());

    Ok(Json(
        json!({ "recorded": true, "next_run_at": next_run_at }),
    ))
}
