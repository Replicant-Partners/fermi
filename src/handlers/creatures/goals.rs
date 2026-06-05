//! Creature goal management — POST/GET /api/creatures/:id/goals
//!
//! Goals are standing foraging objectives that agents evaluate on each
//! observation run, accumulating progress over time via the kask-wild
//! cross-workspace intelligence loop.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;
use fermi_auth::AuthPrincipal;

use super::helpers::verify_creature_ownership;

#[derive(Deserialize)]
pub struct CreateGoalRequest {
    pub title: String,
    pub description: String,
    pub goal_type: Option<String>,  // species_watch | accumulation | location_scout | condition_track | bioconversion | custom
    pub parameters: Option<Value>,
    pub wild_workspace_id: Option<String>,  // UUID of kask-wild workspace if already created
}

/// POST /api/creatures/:creature_id/goals
pub async fn create_goal_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<CreateGoalRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // Verify ownership
    let _ = verify_creature_ownership(pool, creature_id, &user_id).await?;

    let goal_type = req.goal_type.as_deref().unwrap_or("custom");
    let valid_types = ["species_watch", "accumulation", "location_scout",
                       "condition_track", "bioconversion", "custom"];
    if !valid_types.contains(&goal_type) {
        return Err((StatusCode::BAD_REQUEST,
            format!("Invalid goal_type '{}' — must be one of: {}", goal_type, valid_types.join("|"))));
    }

    let wild_workspace_uuid: Option<Uuid> = req.wild_workspace_id
        .as_deref()
        .and_then(|s| s.parse().ok());

    let parameters = req.parameters.unwrap_or_else(|| json!({}));

    let goal_id: Uuid = sqlx::query(
        r#"INSERT INTO creature_goals (
            creature_id, owner_id, title, description,
            goal_type, parameters, wild_workspace_id,
            status, progress
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'active', '{}')
        RETURNING goal_id"#,
    )
    .bind(creature_id)
    .bind(&user_id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(goal_type)
    .bind(&parameters)
    .bind(wild_workspace_uuid)
    .fetch_one(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR,
        format!("Failed to create goal: {}", e)))?
    .try_get("goal_id")
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "goal_id": goal_id,
        "creature_id": creature_id,
        "title": req.title,
        "description": req.description,
        "goal_type": goal_type,
        "parameters": parameters,
        "status": "active",
        "progress": {},
        "wild_workspace_id": wild_workspace_uuid,
    })))
}

/// GET /api/creatures/:creature_id/goals
pub async fn list_goals_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    let _ = verify_creature_ownership(pool, creature_id, &user_id).await?;

    let rows = sqlx::query(
        r#"SELECT goal_id, title, description, goal_type, parameters,
                  status, progress, forecast_accuracy,
                  predictions_made, predictions_scored,
                  achieved_at, last_evaluated_at, created_at,
                  wild_workspace_id
           FROM creature_goals
           WHERE creature_id = $1
           ORDER BY created_at DESC"#,
    )
    .bind(creature_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let goals: Vec<Value> = rows.iter().map(|r| {
        json!({
            "goal_id": r.try_get::<Uuid, _>("goal_id").ok(),
            "title": r.try_get::<String, _>("title").unwrap_or_default(),
            "description": r.try_get::<String, _>("description").unwrap_or_default(),
            "goal_type": r.try_get::<String, _>("goal_type").unwrap_or_default(),
            "parameters": r.try_get::<Value, _>("parameters").ok(),
            "status": r.try_get::<String, _>("status").unwrap_or_default(),
            "progress": r.try_get::<Value, _>("progress").ok(),
            "forecast_accuracy": r.try_get::<Option<f64>, _>("forecast_accuracy").ok().flatten(),
            "predictions_made": r.try_get::<i32, _>("predictions_made").unwrap_or(0),
            "predictions_scored": r.try_get::<i32, _>("predictions_scored").unwrap_or(0),
            "achieved_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("achieved_at").ok().flatten(),
            "last_evaluated_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_evaluated_at").ok().flatten(),
            "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok(),
            "wild_workspace_id": r.try_get::<Option<Uuid>, _>("wild_workspace_id").ok().flatten(),
        })
    }).collect();

    Ok(Json(json!({
        "creature_id": creature_id,
        "goals": goals,
        "total": goals.len(),
    })))
}

/// PATCH /api/creatures/:creature_id/goals/:goal_id
/// Update goal status or parameters.
pub async fn update_goal_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((creature_id, goal_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    let _ = verify_creature_ownership(pool, creature_id, &user_id).await?;

    // Only allow status and parameters updates
    let status = req.get("status").and_then(|v| v.as_str());
    if let Some(s) = status {
        if !["active", "achieved", "paused", "abandoned"].contains(&s) {
            return Err((StatusCode::BAD_REQUEST,
                format!("Invalid status '{}'", s)));
        }
        sqlx::query(
            "UPDATE creature_goals SET status = $1,
             achieved_at = CASE WHEN $1 = 'achieved' THEN NOW() ELSE achieved_at END
             WHERE goal_id = $2 AND creature_id = $3",
        )
        .bind(s)
        .bind(goal_id)
        .bind(creature_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    if let Some(params) = req.get("parameters") {
        sqlx::query(
            "UPDATE creature_goals SET parameters = $1
             WHERE goal_id = $2 AND creature_id = $3",
        )
        .bind(params)
        .bind(goal_id)
        .bind(creature_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    Ok(Json(json!({ "goal_id": goal_id, "updated": true })))
}
