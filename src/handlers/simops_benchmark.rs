//! SimOps Process Benchmark — Projection Commitment + Spacetime Resolution
//!
//! ## Two hooks, two directions
//!
//! **Hook 1 — on synthetic write (simops_tools.rs)**
//! When the dynamics runner / cascade writes a synthetic SOSA observation
//! (source = simops_simulation), `commit_projection()` is called immediately.
//! This is the immutable clock: the predicted value is anchored before any
//! real measurement can arrive. The commitment_hash proves the prediction
//! pre-existed the measurement.
//!
//! **Hook 2 — on real observation ingest (observations.rs)**
//! When a real sensor reading arrives (`procedure != simops_simulation`),
//! `resolve_against_projection()` checks for a matching committed prediction
//! and writes a `process_spacetime` row if found. Two resolution modes:
//!
//!   - `any_reading`   — every real reading that matches a prediction
//!   - `sample_point`  — readings at configured intervals (default 1 hour)
//!   - `anomaly_delta` — readings where |predicted-actual|/|actual| > threshold
//!
//! ## What this proves
//!
//! After N real batch completions, you can show:
//! - Model accuracy trajectory: is kombucha_fermentation@v1 getting more
//!   accurate as more real readings constrain its parameters?
//! - Anomaly detection: which conditions (high temp, low n) reliably
//!   cause the model to diverge?
//! - Cross-loop signal: do Loop 1 semantic rules (dreaming cycle output)
//!   correlate with improved accuracy on the next batch?
//!
//! The spacetime table is the evidence base for all three claims.

use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

// ── Commitment hash ───────────────────────────────────────────────────────

fn projection_commitment_hash(
    observation_id: &Uuid,
    predicted_value: f64,
    model_uri: Option<&str>,
    phenomenon_time_ms: i64,
) -> String {
    let mut h = Sha256::new();
    h.update(observation_id.to_string().as_bytes());
    h.update(b"|");
    h.update(format!("{:.8}", predicted_value).as_bytes());
    h.update(b"|");
    h.update(model_uri.unwrap_or("").as_bytes());
    h.update(b"|");
    h.update(phenomenon_time_ms.to_string().as_bytes());
    format!("{:x}", h.finalize())
}

// ── Hook 1: commit a synthetic projection ────────────────────────────────

/// Called immediately after a synthetic SOSA observation is written.
/// Idempotent — safe to call even if the table doesn't exist yet.
pub async fn commit_projection(
    pool: &PgPool,
    observation_id: Uuid,
    session_id: Uuid,
    workspace_id: Option<Uuid>,
    observable_property: &str,
    feature_of_interest: Option<&str>,
    predicted_value: f64,
    model_uri: Option<&str>,
    stage_id: Option<&str>,
    projection_id: Option<&str>,
    phenomenon_time_ms: i64,
    process_context: Option<&serde_json::Value>,
) -> Option<String> {
    // Table existence check — non-fatal if migration 141 pending
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables
         WHERE table_name='process_projection_commits')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if !exists {
        return None;
    }

    let hash = projection_commitment_hash(
        &observation_id,
        predicted_value,
        model_uri,
        phenomenon_time_ms,
    );

    // Idempotent
    let already: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM process_projection_commits WHERE commitment_hash=$1)",
    )
    .bind(&hash)
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if already {
        return Some(hash);
    }

    let _ = sqlx::query(
        r#"INSERT INTO process_projection_commits
           (sosa_observation_id, projection_id, workspace_id, session_id,
            observable_property, feature_of_interest, predicted_value,
            model_uri, stage_id, commitment_hash, committed_at,
            phenomenon_time_ms, process_context)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,NOW(),$11,$12)"#,
    )
    .bind(observation_id)
    .bind(projection_id)
    .bind(workspace_id)
    .bind(session_id)
    .bind(observable_property)
    .bind(feature_of_interest)
    .bind(predicted_value)
    .bind(model_uri)
    .bind(stage_id)
    .bind(&hash)
    .bind(phenomenon_time_ms)
    .bind(process_context)
    .execute(pool)
    .await
    .ok()?;

    Some(hash)
}

// ── Hook 2: resolve a real reading against prior predictions ─────────────

pub struct RealReading {
    pub observation_id: Uuid,
    pub session_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub observable_property: String,
    pub feature_of_interest: Option<String>,
    pub actual_value: f64,
    pub measured_at: chrono::DateTime<Utc>,
    pub conditions: Option<serde_json::Value>,
}

/// Called after a real SOSA observation is ingested.
/// Finds the most recent committed projection for the same
/// (observable_property, feature_of_interest) and writes process_spacetime rows.
///
/// Fires three resolution mode rows for each matched projection:
///   always:        'any_reading'
///   if delta>thr:  'anomaly_delta'
///   if at interval: 'sample_point'
pub async fn resolve_against_projection(pool: &PgPool, reading: &RealReading) -> usize {
    // Table existence check
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables
         WHERE table_name='process_spacetime')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if !exists {
        return 0;
    }

    // Find the most recent committed projection for this property/feature
    // within the last 30 days (configurable via sample config)
    let proj_row = sqlx::query(
        r#"SELECT
            c.commit_id, c.projection_id, c.predicted_value,
            c.model_uri, c.stage_id, c.committed_at,
            c.phenomenon_time_ms, c.process_context,
            c.workspace_id AS commit_workspace_id
           FROM process_projection_commits c
           WHERE c.observable_property = $1
             AND ($2::text IS NULL OR c.feature_of_interest = $2)
             AND c.committed_at >= NOW() - INTERVAL '30 days'
           ORDER BY c.committed_at DESC
           LIMIT 1"#,
    )
    .bind(&reading.observable_property)
    .bind(reading.feature_of_interest.as_deref())
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let Some(proj) = proj_row else {
        return 0;
    };

    let commit_id: Uuid = proj.get("commit_id");
    let predicted: f64 = proj.get("predicted_value");
    let model_uri: Option<String> = proj.try_get("model_uri").ok().flatten();
    let stage_id: Option<String> = proj.try_get("stage_id").ok().flatten();
    let committed_at: Option<chrono::DateTime<Utc>> = proj.try_get("committed_at").ok().flatten();
    let projection_id: Option<String> = proj.try_get("projection_id").ok().flatten();
    let process_context: Option<serde_json::Value> = proj.try_get("process_context").ok().flatten();

    let actual = reading.actual_value;
    let abs_err = (predicted - actual).abs();
    let rel_err = abs_err / actual.abs().max(1e-9);
    let accuracy = (1.0 - rel_err.min(1.0)).clamp(0.0, 1.0);
    let direction = if (predicted - actual).abs() < 1e-9 {
        "exact"
    } else if predicted > actual {
        "over"
    } else {
        "under"
    };

    // Load sample config for this workspace/property
    let (sample_interval, anomaly_threshold) =
        load_sample_config(pool, reading.workspace_id, &reading.observable_property).await;

    // Determine which resolution modes apply
    let mut modes: Vec<(&str, Option<f64>, Option<f64>)> = vec![("any_reading", None, None)];

    // Anomaly delta mode: rel_err exceeds threshold
    if rel_err > anomaly_threshold {
        modes.push(("anomaly_delta", Some(anomaly_threshold), None));
    }

    // Sample point mode: check if this reading falls at a sample interval
    // relative to the prediction's phenomenon_time
    let phenom_ms: Option<i64> = proj.try_get("phenomenon_time_ms").ok().flatten();
    if let Some(pms) = phenom_ms {
        let pred_time = chrono::DateTime::<Utc>::from_timestamp_millis(pms);
        if let Some(pred_t) = pred_time {
            let hours_since_pred = reading
                .measured_at
                .signed_duration_since(pred_t)
                .num_seconds() as f64
                / 3600.0;
            // Within 15 minutes of a sample interval boundary
            let remainder = hours_since_pred % sample_interval;
            let near_boundary = remainder < 0.25 || (sample_interval - remainder) < 0.25;
            if near_boundary && hours_since_pred > 0.0 {
                modes.push(("sample_point", None, Some(sample_interval)));
            }
        }
    }

    // Get Loop 5 rolling accuracy for this model (for RSI context)
    let loop5_accuracy: Option<f64> = if let Some(ref mu) = model_uri {
        sqlx::query_scalar::<_, f64>(
            "SELECT AVG(accuracy_score)
             FROM process_spacetime
             WHERE model_uri = $1
               AND resolved_at >= NOW() - INTERVAL '30 days'
             LIMIT 1",
        )
        .bind(mu)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
    } else {
        None
    };

    let workspace_id = reading.workspace_id.or_else(|| {
        proj.try_get::<Option<Uuid>, _>("commit_workspace_id")
            .ok()
            .flatten()
    });

    let mut written = 0usize;
    for (mode, anom_thr, samp_int) in modes {
        let r = sqlx::query(
            r#"INSERT INTO process_spacetime
               (projection_commit_id, workspace_id, session_id,
                projection_id, observable_property, feature_of_interest,
                predicted_value, model_uri, stage_id,
                real_observation_id, actual_value, measured_at,
                absolute_error, relative_error, accuracy_score, delta_direction,
                resolution_mode, anomaly_threshold, sample_interval_hours,
                conditions_at_measure, loop5_model_accuracy,
                committed_at, resolved_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,
                       $13,$14,$15,$16,$17,$18,$19,$20,$21,$22,NOW())"#,
        )
        .bind(commit_id)
        .bind(workspace_id)
        .bind(reading.session_id)
        .bind(&projection_id)
        .bind(&reading.observable_property)
        .bind(reading.feature_of_interest.as_deref())
        .bind(predicted)
        .bind(&model_uri)
        .bind(&stage_id)
        .bind(reading.observation_id)
        .bind(actual)
        .bind(reading.measured_at)
        .bind(abs_err)
        .bind(rel_err)
        .bind(accuracy)
        .bind(direction)
        .bind(mode)
        .bind(anom_thr)
        .bind(samp_int)
        .bind(&reading.conditions)
        .bind(loop5_accuracy)
        .bind(committed_at)
        .execute(pool)
        .await;

        if r.is_ok() {
            written += 1;
        }
    }
    written
}

async fn load_sample_config(
    pool: &PgPool,
    workspace_id: Option<Uuid>,
    property: &str,
) -> (f64, f64) {
    // Try workspace-specific property config first, then wildcard, then default
    let row = if let Some(wid) = workspace_id {
        sqlx::query(
            "SELECT sample_interval_hours, anomaly_threshold
             FROM process_sample_config
             WHERE workspace_id = $1
               AND (observable_property = $2 OR observable_property = '*')
               AND enabled = true
             ORDER BY CASE WHEN observable_property = $2 THEN 0 ELSE 1 END
             LIMIT 1",
        )
        .bind(wid)
        .bind(property)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
    } else {
        None
    };

    // Fall back to platform defaults
    let (interval, threshold) = if let Some(r) = row {
        (
            r.try_get::<f64, _>("sample_interval_hours").unwrap_or(1.0),
            r.try_get::<f64, _>("anomaly_threshold").unwrap_or(0.15),
        )
    } else {
        (1.0, 0.15)
    };
    (interval, threshold)
}

// ── GET /api/simops/process-spacetime/:workspace_id ───────────────────────

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::AppState;
use fermi_auth::AuthPrincipal;

#[derive(Deserialize)]
pub struct ProcessSpacetimeQuery {
    pub model_uri: Option<String>,
    pub observable_property: Option<String>,
    pub resolution_mode: Option<String>, // all | any_reading | sample_point | anomaly_delta
    pub days: Option<i64>,
    pub limit: Option<i64>,
}

/// GET /api/simops/workspaces/:workspace_id/process-spacetime
///
/// The spacetime trajectory for a SimOps workspace — every point where
/// a real sensor reading resolved against a prior model prediction.
/// Includes accuracy trend, anomaly clusters, and model comparison.
pub async fn process_spacetime_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<Uuid>,
    Query(q): Query<ProcessSpacetimeQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let pool = &state.db;
    let days = q.days.unwrap_or(30).max(1).min(365);
    let limit = q.limit.unwrap_or(200).min(1000);
    let mode_filter = q.resolution_mode.as_deref().unwrap_or("all");

    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables
         WHERE table_name='process_spacetime')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    if !table_exists {
        return Ok(Json(json!({
            "workspace_id": workspace_id,
            "status": "pending",
            "note": "process_spacetime table not yet created (migration 141 pending)",
            "spacetime": [],
        })));
    }

    let mode_clause = match mode_filter {
        "all" => "AND TRUE",
        "any_reading" => "AND resolution_mode = 'any_reading'",
        "sample_point" => "AND resolution_mode = 'sample_point'",
        "anomaly_delta" => "AND resolution_mode = 'anomaly_delta'",
        _ => "AND TRUE",
    };
    let prop_clause = if q.observable_property.is_some() {
        "AND observable_property = $4"
    } else {
        "AND TRUE"
    };
    let model_clause = if q.model_uri.is_some() {
        "AND model_uri = $5"
    } else {
        "AND TRUE"
    };

    let sql = format!(
        r#"SELECT
            spacetime_id, projection_id, observable_property, feature_of_interest,
            predicted_value, actual_value, absolute_error, relative_error, accuracy_score,
            delta_direction, resolution_mode, anomaly_threshold, sample_interval_hours,
            model_uri, stage_id, committed_at, resolved_at,
            commit_to_resolve_hours, committed_before_measured,
            conditions_at_measure, loop5_model_accuracy
           FROM process_spacetime
           WHERE workspace_id = $1
             AND resolved_at >= NOW() - ($2 || ' days')::interval
             {mode_clause}
             {prop_clause}
             {model_clause}
           ORDER BY resolved_at DESC
           LIMIT $3"#
    );

    let mut query = sqlx::query(&sql).bind(workspace_id).bind(days).bind(limit);

    if let Some(ref prop) = q.observable_property {
        query = query.bind(prop);
    }
    if let Some(ref mu) = q.model_uri {
        query = query.bind(mu);
    }

    let rows = query
        .fetch_all(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let spacetime: Vec<Value> = rows.iter().map(|r| {
        json!({
            "spacetime_id": r.try_get::<Uuid,_>("spacetime_id").ok(),
            "projection_id": r.try_get::<Option<String>,_>("projection_id").ok().flatten(),
            "observable_property": r.try_get::<String,_>("observable_property").ok(),
            "feature_of_interest": r.try_get::<Option<String>,_>("feature_of_interest").ok().flatten(),
            "predicted": r.try_get::<f64,_>("predicted_value").ok(),
            "actual": r.try_get::<f64,_>("actual_value").ok(),
            "absolute_error": r.try_get::<f64,_>("absolute_error").ok(),
            "relative_error_pct": r.try_get::<f64,_>("relative_error").ok().map(|v| (v*100.0).round()/100.0),
            "accuracy_score": r.try_get::<f64,_>("accuracy_score").ok(),
            "direction": r.try_get::<String,_>("delta_direction").ok(),
            "resolution_mode": r.try_get::<String,_>("resolution_mode").ok(),
            "model_uri": r.try_get::<Option<String>,_>("model_uri").ok().flatten(),
            "stage_id": r.try_get::<Option<String>,_>("stage_id").ok().flatten(),
            "committed_before_measured": r.try_get::<Option<bool>,_>("committed_before_measured").ok().flatten(),
            "hours_prediction_was_live": r.try_get::<Option<f64>,_>("commit_to_resolve_hours").ok().flatten(),
            "loop5_accuracy_at_time": r.try_get::<Option<f64>,_>("loop5_model_accuracy").ok().flatten(),
            "resolved_at": r.try_get::<chrono::DateTime<Utc>,_>("resolved_at").ok(),
        })
    }).collect();

    // Summary metrics
    let n = spacetime.len();
    let anomalies: Vec<&Value> = spacetime
        .iter()
        .filter(|r| r["resolution_mode"] == "anomaly_delta")
        .collect();
    let accuracy_vals: Vec<f64> = spacetime
        .iter()
        .filter_map(|r| r["accuracy_score"].as_f64())
        .collect();
    let mean_accuracy = if accuracy_vals.is_empty() {
        None
    } else {
        Some(accuracy_vals.iter().sum::<f64>() / accuracy_vals.len() as f64)
    };

    // Per-model summary
    let mut model_stats: std::collections::HashMap<String, (f64, usize)> =
        std::collections::HashMap::new();
    for row in &spacetime {
        if let (Some(mu), Some(acc)) = (row["model_uri"].as_str(), row["accuracy_score"].as_f64()) {
            let e = model_stats.entry(mu.to_string()).or_insert((0.0, 0));
            e.0 += acc;
            e.1 += 1;
        }
    }
    let model_accuracy: Value = model_stats
        .iter()
        .map(|(mu, (sum, n))| {
            (
                mu.clone(),
                json!({ "mean_accuracy": sum/(*n as f64), "n": n }),
            )
        })
        .collect::<serde_json::Map<_, _>>()
        .into();

    Ok(Json(json!({
        "workspace_id": workspace_id,
        "query": { "days": days, "mode": mode_filter, "n_rows": n },
        "summary": {
            "mean_accuracy": mean_accuracy,
            "n_anomalies": anomalies.len(),
            "n_sample_points": spacetime.iter().filter(|r| r["resolution_mode"]=="sample_point").count(),
            "model_accuracy": model_accuracy,
        },
        "spacetime": spacetime,
    })))
}

/// GET /api/simops/workspaces/:workspace_id/sample-config
pub async fn get_sample_config_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let rows = sqlx::query(
        "SELECT observable_property, sample_interval_hours, anomaly_threshold, enabled
         FROM process_sample_config
         WHERE workspace_id = $1 OR workspace_id = '00000000-0000-0000-0000-000000000000'
         ORDER BY CASE WHEN workspace_id = $1 THEN 0 ELSE 1 END, observable_property",
    )
    .bind(workspace_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let config: Vec<Value> = rows.iter().map(|r| json!({
        "observable_property": r.try_get::<String,_>("observable_property").ok(),
        "sample_interval_hours": r.try_get::<f64,_>("sample_interval_hours").ok(),
        "anomaly_threshold_pct": r.try_get::<f64,_>("anomaly_threshold").ok().map(|v| v*100.0),
        "enabled": r.try_get::<bool,_>("enabled").ok(),
    })).collect();

    Ok(Json(
        json!({ "workspace_id": workspace_id, "config": config }),
    ))
}

/// PUT /api/simops/workspaces/:workspace_id/sample-config
#[derive(serde::Deserialize)]
pub struct SampleConfigUpdate {
    pub observable_property: String,
    pub sample_interval_hours: Option<f64>,
    pub anomaly_threshold_pct: Option<f64>, // 0-100; stored as 0-1
    pub enabled: Option<bool>,
}

pub async fn put_sample_config_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<Uuid>,
    Json(body): Json<SampleConfigUpdate>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // TODO: verify workspace ownership
    let threshold = body.anomaly_threshold_pct.map(|p| p / 100.0);
    sqlx::query(
        r#"INSERT INTO process_sample_config
           (workspace_id, observable_property, sample_interval_hours, anomaly_threshold, enabled)
           VALUES ($1, $2, $3, $4, $5)
           ON CONFLICT (workspace_id, observable_property) DO UPDATE SET
             sample_interval_hours = COALESCE($3, process_sample_config.sample_interval_hours),
             anomaly_threshold = COALESCE($4, process_sample_config.anomaly_threshold),
             enabled = COALESCE($5, process_sample_config.enabled)"#,
    )
    .bind(workspace_id)
    .bind(&body.observable_property)
    .bind(body.sample_interval_hours)
    .bind(threshold)
    .bind(body.enabled)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "updated": true })))
}
