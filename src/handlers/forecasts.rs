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
use fermi_auth::{get_or_create_wallet, AuthPrincipal};

// ═══════════════════════════════════════════════════════════════════
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
pub struct UpdatePortfolioRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub domain: Option<String>,
    pub visibility: Option<String>,
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
        "INSERT INTO fermi_forecasts
         (id, owner_id, question_text, domain, resolution_criteria, target_date,
          predicted_probability, confidence_interval_low, confidence_interval_high,
          fpl_source, notebook_id, simulation_results, iterations,
          drivers, evidence, agents_used,
          status, visibility, team_id, workspace_id, tags, created_at, updated_at)
         VALUES ($1, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
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
        let conductor_version = req.agents_used.as_ref()
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
        ).await;

        let commitment_hash = crate::handlers::forecast_benchmark::anchor_forecast(
            pool, &forecast_id, None,
            req.predicted_probability as f64,
            req.fpl_source.as_deref(),
            now,
            Some("auto-anchor on create"),
        ).await.ok();

        // Link harness snapshot to the spacetime row if both exist
        if let (Some(snap_id), Some(_)) = (harness_snapshot_id, commitment_hash.as_ref()) {
            let _ = sqlx::query(
                "UPDATE forecast_spacetime SET harness_snapshot_id = $1
                 WHERE forecast_id = $2 AND revision_seq = 0"
            ).bind(snap_id).bind(&forecast_id).execute(pool).await;
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
        "SELECT f.*, f.owner_id::text AS owner_id_text, u.name AS owner_display_name
         FROM fermi_forecasts f
         LEFT JOIN users u ON u.id = f.owner_id
         WHERE f.id = $1",
    )
    .bind(&forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Forecast not found".into()))?;

    // Access control: owner, team member, or public
    let owner_id: String = row.get("owner_id_text");
    let visibility: String = row.get("visibility");
    let team_id: Option<Uuid> = row.try_get("team_id").ok();

    if owner_id != user_id && visibility == "private" {
        // Check team membership if team_id is set
        if let Some(tid) = team_id {
            let is_member = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM team_members WHERE team_id = $1 AND user_id = $2)",
            )
            .bind(tid)
            .bind(&user_id)
            .fetch_one(pool)
            .await
            .unwrap_or(false);

            if !is_member {
                return Err((StatusCode::FORBIDDEN, "Access denied".into()));
            }
        } else {
            return Err((StatusCode::FORBIDDEN, "Access denied".into()));
        }
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
        "team_id": team_id,
        "workspace_id": row.try_get::<Option<Uuid>, _>("workspace_id").ok().flatten().map(|u| u.to_string()),
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

    // Default: show own forecasts + shared/public
    bind_idx += 1;
    conditions.push(format!(
        "(f.owner_id = ${}::uuid OR f.visibility IN ('shared', 'public'))",
        bind_idx
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
                u.name AS owner_display_name
         FROM fermi_forecasts f
         LEFT JOIN users u ON u.id = f.owner_id
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

    // Verify ownership
    let row = sqlx::query(
        "SELECT owner_id::text AS owner_id, status, predicted_probability FROM fermi_forecasts WHERE id = $1",
    )
    .bind(&forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Forecast not found".into()))?;

    let owner_id: String = row.get("owner_id");
    if owner_id != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your forecast".into()));
    }

    let current_status: String = row.get("status");
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

        let current_prob: f64 = row.get("predicted_probability");
        if (new_prob - current_prob).abs() > 0.001 {
            // Record the probability update
            sqlx::query(
                "INSERT INTO fermi_forecast_updates
                 (id, forecast_id, previous_probability, new_probability, reason, created_at)
                 VALUES ($1, $2, $3, $4, 'Manual update via API', NOW())",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&forecast_id)
            .bind(current_prob)
            .bind(new_prob)
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

    let owner: Option<String> =
        sqlx::query_scalar("SELECT owner_id::text FROM fermi_forecasts WHERE id = $1")
            .bind(&forecast_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match owner {
        Some(oid) if oid == user_id => {}
        Some(_) => return Err((StatusCode::FORBIDDEN, "Not your forecast".into())),
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
            let agents_used: Vec<serde_json::Value> = match sqlx::query(
                "SELECT agents_used FROM fermi_forecasts WHERE id = $1",
            )
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
                        obj.insert("outcome_quality".to_string(), serde_json::json!(calibration_quality));
                        obj.insert("outcome_source".to_string(), serde_json::json!("brier_forecast"));
                        obj.insert("outcome_brier_score".to_string(), serde_json::json!(brier_score));
                        obj.insert("outcome_annotated_at".to_string(),
                            serde_json::json!(chrono::Utc::now().to_rfc3339()));
                    }

                    // Write the annotated context back
                    let _ = sqlx::query(
                        "UPDATE episodes SET context = $1 WHERE episode_id = $2",
                    )
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

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "actual_outcome": req.actual_outcome,
        "brier_score": brier_score,
        "status": "resolved",
        "resolved_by": user_id,
        "resolution_notes": req.resolution_notes,
    })))
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

    let result = sqlx::query(
        "UPDATE fermi_forecasts SET status = 'voided', updated_at = NOW()
         WHERE id = $1 AND owner_id = $2::uuid AND status IN ('draft', 'active')
         RETURNING id",
    )
    .bind(&forecast_id)
    .bind(&user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            "Forecast not found, not yours, or already resolved/voided".into(),
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
        "SELECT owner_id::text AS owner_id, status, predicted_probability FROM fermi_forecasts WHERE id = $1",
    )
    .bind(&forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Forecast not found".into()))?;

    let owner_id: String = row.get("owner_id");
    if owner_id != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your forecast".into()));
    }

    let status: String = row.get("status");
    if status != "active" && status != "draft" {
        return Err((
            StatusCode::CONFLICT,
            format!("Cannot update probability on a {} forecast", status),
        ));
    }

    let previous_probability: f64 = row.get("predicted_probability");

    // Record the update
    let update_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO fermi_forecast_updates
         (id, forecast_id, previous_probability, new_probability, reason, agent_id, evidence_added, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())",
    )
    .bind(&update_id)
    .bind(&forecast_id)
    .bind(previous_probability)
    .bind(req.new_probability)
    .bind(&req.reason)
    .bind(&req.agent_id)
    .bind(&req.evidence_added)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Update the forecast's current probability
    sqlx::query(
        "UPDATE fermi_forecasts SET predicted_probability = $1, updated_at = NOW() WHERE id = $2",
    )
    .bind(req.new_probability)
    .bind(&forecast_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Anchor the new probability immediately — each revision gets its own
    // tamper-evident commitment so the rate-of-change is fully provable.
    let commitment_hash = {
        let _ = crate::handlers::forecast_benchmark::anchor_forecast(
            pool,
            &forecast_id,
            Some(&update_id),
            req.new_probability as f64,
            None, // fpl_source not available here without a re-fetch
            chrono::Utc::now(),
            Some("auto-anchor on probability update"),
        ).await;
        // Return the hash for the response (best-effort)
        crate::handlers::forecast_benchmark::anchor_forecast(
            pool, &forecast_id, Some(&update_id),
            req.new_probability as f64, None,
            chrono::Utc::now(), Some("auto-anchor on probability update"),
        ).await.ok()
    };

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "update_id": update_id,
        "previous_probability": previous_probability,
        "new_probability": req.new_probability,
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
    let portfolio_id = Uuid::new_v4().to_string();
    let visibility = req.visibility.as_deref().unwrap_or("private");
    let team_id: Option<Uuid> = req.team_id.as_ref().and_then(|s| Uuid::parse_str(s).ok());

    sqlx::query(
        "INSERT INTO fermi_portfolios (id, title, description, owner_id, visibility, team_id, domain, created_at, updated_at)
         VALUES ($1, $2, $3, $4::uuid, $5, $6, $7, NOW(), NOW())",
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
                p.visibility, p.domain, p.created_at, p.updated_at,
                (SELECT COUNT(*) FROM fermi_portfolio_forecasts pf WHERE pf.portfolio_id = p.id) AS forecast_count,
                (SELECT COUNT(*) FROM fermi_portfolio_forecasts pf
                 JOIN fermi_forecasts f ON f.id = pf.forecast_id
                 WHERE pf.portfolio_id = p.id AND f.status = 'resolved') AS resolved_count,
                (SELECT AVG(f.brier_score) FROM fermi_portfolio_forecasts pf
                 JOIN fermi_forecasts f ON f.id = pf.forecast_id
                 WHERE pf.portfolio_id = p.id AND f.brier_score IS NOT NULL) AS avg_brier
         FROM fermi_portfolios p
         WHERE p.owner_id = $1::uuid OR p.visibility IN ('shared', 'public')
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

    // Verify access
    let portfolio = sqlx::query(
        "SELECT owner_id::text AS owner_id, title, visibility, domain FROM fermi_portfolios WHERE id = $1",
    )
    .bind(&portfolio_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Portfolio not found".into()))?;

    let owner_id: String = portfolio.get("owner_id");
    let visibility: String = portfolio.get("visibility");
    if owner_id != user_id && visibility == "private" {
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

    // Verify portfolio ownership
    let owner: Option<String> =
        sqlx::query_scalar("SELECT owner_id::text FROM fermi_portfolios WHERE id = $1")
            .bind(&portfolio_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match owner {
        Some(oid) if oid == user_id => {}
        Some(_) => return Err((StatusCode::FORBIDDEN, "Not your portfolio".into())),
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

    let owner: Option<String> =
        sqlx::query_scalar("SELECT owner_id::text FROM fermi_portfolios WHERE id = $1")
            .bind(&portfolio_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match owner {
        Some(oid) if oid == user_id => {}
        Some(_) => return Err((StatusCode::FORBIDDEN, "Not your portfolio".into())),
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

    let owner: Option<String> =
        sqlx::query_scalar("SELECT owner_id::text FROM fermi_portfolios WHERE id = $1")
            .bind(&portfolio_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match owner {
        Some(oid) if oid == user_id => {}
        Some(_) => return Err((StatusCode::FORBIDDEN, "Not your portfolio".into())),
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
}

/// PATCH /api/portfolios/:id
pub async fn patch_portfolio_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(portfolio_id): Path<String>,
    Json(req): Json<PatchPortfolioRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    let owner: Option<String> =
        sqlx::query_scalar("SELECT owner_id::text FROM fermi_portfolios WHERE id = $1")
            .bind(&portfolio_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match owner {
        Some(oid) if oid == user_id => {}
        Some(_) => return Err((StatusCode::FORBIDDEN, "Not your portfolio".into())),
        None => return Err((StatusCode::NOT_FOUND, "Portfolio not found".into())),
    }

    sqlx::query(
        "UPDATE fermi_portfolios
         SET title       = COALESCE($2, title),
             description = COALESCE($3, description),
             updated_at  = NOW()
         WHERE id = $1",
    )
    .bind(&portfolio_id)
    .bind(&req.title)
    .bind(&req.description)
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

    // Allow access if owner OR portfolio is public/team
    let portfolio = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility FROM fermi_portfolios WHERE id = $1",
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
            if owner != user_id && visibility == "private" {
                return Err((StatusCode::FORBIDDEN, "Not your portfolio".into()));
            }
        }
    }

    let rows = sqlx::query(
        "SELECT f.id,
                f.question_text,
                f.predicted_probability,
                f.status,
                f.brier_score,
                f.actual_outcome,
                f.resolved_at,
                f.visibility,
                pf.added_at
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
            json!({
                "id":                   r.get::<String, _>("id"),
                "question_text":        r.get::<String, _>("question_text"),
                // REAL columns → f32 in sqlx; cast to f64 at the JSON
                // boundary. Empirically the WC fleet has rows with NULL
                // predicted_probability (some spawn paths left the column
                // unset even though the schema declares NOT NULL — likely a
                // pre-fix bind silently coerced to NULL). Treat it as
                // Option<f32> here so a bad row degrades to null in the
                // response instead of panicking the handler.
                "predicted_probability":r.get::<Option<f32>, _>("predicted_probability").map(|v| v as f64),
                "status":               r.get::<String, _>("status"),
                "brier_score":          r.get::<Option<f32>, _>("brier_score").map(|v| v as f64),
                "actual_outcome":       r.get::<Option<bool>, _>("actual_outcome"),
                "resolved_at":          r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at")
                                         .map(|d| d.to_rfc3339()),
                "visibility":           r.get::<String, _>("visibility"),
                "added_at":             r.get::<chrono::DateTime<chrono::Utc>, _>("added_at").to_rfc3339(),
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
                "SELECT f.owner_id::text AS owner_id, u.name AS display_name,
                        COUNT(*) AS total_resolved,
                        AVG(f.brier_score) AS avg_brier_score,
                        MIN(f.brier_score) AS best_brier_score,
                        MAX(f.brier_score) AS worst_brier_score,
                        STDDEV(f.brier_score) AS brier_stddev,
                        MAX(f.resolved_at) AS last_resolved_at,
                        ROW_NUMBER() OVER (ORDER BY AVG(f.brier_score) ASC) AS rank
                 FROM fermi_forecasts f
                 JOIN users u ON u.id = f.owner_id
                 WHERE f.status = 'resolved' AND f.brier_score IS NOT NULL
                 GROUP BY f.owner_id, u.name
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
         WHERE owner_id = $1::uuid",
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
        ) ranked WHERE owner_id = $1::uuid",
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
                u.name AS owner_display_name
         FROM fermi_forecasts f
         LEFT JOIN users u ON u.id = f.owner_id
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

    if req.interval_hours < 1 || req.interval_hours > 8760 {
        return Err((StatusCode::BAD_REQUEST, "interval_hours must be 1–8760".into()));
    }

    let next_run_at = chrono::Utc::now() + chrono::Duration::hours(req.interval_hours as i64);

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
        .and_then(|r| r.try_get::<chrono::DateTime<chrono::Utc>, _>("next_run_at").ok())
        .map(|t| t.to_rfc3339());

    Ok(Json(json!({ "recorded": true, "next_run_at": next_run_at })))
}
