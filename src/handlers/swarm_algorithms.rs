//! Swarm Algorithm Marketplace — purchasable Onto4MAT formation algorithms.
//!
//! Algorithms are declarative JSON specs (FormationSpec) stored in the database.
//! The Flutter client downloads a spec and applies it to the ring attractor
//! simulation at 60fps. Agents can invoke formations via the `activate_formation` tool.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use super::super::AppState;
use fermi::gas::charge_gas;
use fermi_auth::{get_or_create_wallet, AuthPrincipal};

// ─── Request / query types ─────────────────────────────────────────

#[derive(Deserialize)]
pub struct ListAlgorithmsQuery {
    pub category: Option<String>,
    pub tier: Option<String>,
}

#[derive(Deserialize)]
pub struct ActivateRequest {
    pub algorithm_name: Option<String>,
    pub algorithm_id: Option<String>,
    pub swarm_id: String,
}

// ─── GET /api/swarm-algorithms ─────────────────────────────────────

pub async fn list_algorithms_handler(
    State(state): State<AppState>,
    Query(query): Query<ListAlgorithmsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pool = &state.db;

    let mut sql = String::from(
        "SELECT algorithm_id, name, display_name, description, category, onto4mat_class, \
         formation_spec, tier, cost_credits, icon, created_at \
         FROM swarm_algorithms WHERE 1=1",
    );
    let mut bind_idx = 1;
    let mut binds: Vec<String> = Vec::new();

    if let Some(ref cat) = query.category {
        sql.push_str(&format!(" AND category = ${}", bind_idx));
        bind_idx += 1;
        binds.push(cat.clone());
    }
    if let Some(ref tier) = query.tier {
        sql.push_str(&format!(" AND tier = ${}", bind_idx));
        let _ = bind_idx;
        binds.push(tier.clone());
    }
    sql.push_str(" ORDER BY category, cost_credits, name");

    let mut q = sqlx::query(&sql);
    for b in &binds {
        q = q.bind(b);
    }

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let algorithms: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "algorithm_id": r.get::<Uuid, _>("algorithm_id"),
                "name": r.get::<String, _>("name"),
                "display_name": r.get::<String, _>("display_name"),
                "description": r.get::<Option<String>, _>("description"),
                "category": r.get::<String, _>("category"),
                "onto4mat_class": r.get::<String, _>("onto4mat_class"),
                "formation_spec": r.get::<serde_json::Value, _>("formation_spec"),
                "tier": r.get::<String, _>("tier"),
                "cost_credits": r.get::<i32, _>("cost_credits"),
                "icon": r.get::<Option<String>, _>("icon"),
            })
        })
        .collect();

    Ok(Json(json!({ "algorithms": algorithms })))
}

// ─── GET /api/swarm-algorithms/:id ─────────────────────────────────

pub async fn get_algorithm_handler(
    State(state): State<AppState>,
    Path(algorithm_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pool = &state.db;

    let row = sqlx::query(
        "SELECT algorithm_id, name, display_name, description, category, onto4mat_class, \
         formation_spec, tier, cost_credits, icon, created_at \
         FROM swarm_algorithms WHERE algorithm_id = $1",
    )
    .bind(algorithm_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Algorithm not found".to_string()))?;

    Ok(Json(json!({
        "algorithm_id": row.get::<Uuid, _>("algorithm_id"),
        "name": row.get::<String, _>("name"),
        "display_name": row.get::<String, _>("display_name"),
        "description": row.get::<Option<String>, _>("description"),
        "category": row.get::<String, _>("category"),
        "onto4mat_class": row.get::<String, _>("onto4mat_class"),
        "formation_spec": row.get::<serde_json::Value, _>("formation_spec"),
        "tier": row.get::<String, _>("tier"),
        "cost_credits": row.get::<i32, _>("cost_credits"),
        "icon": row.get::<Option<String>, _>("icon"),
    })))
}

// ─── POST /api/swarm-algorithms/activate ───────────────────────────

pub async fn activate_algorithm_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<ActivateRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    let swarm_id: Uuid = req
        .swarm_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid swarm_id".to_string()))?;

    // Look up algorithm by name or id
    let algorithm = if let Some(ref name) = req.algorithm_name {
        sqlx::query(
            "SELECT algorithm_id, name, display_name, formation_spec, tier, cost_credits \
             FROM swarm_algorithms WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else if let Some(ref id_str) = req.algorithm_id {
        let aid: Uuid = id_str
            .parse()
            .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid algorithm_id".to_string()))?;
        sqlx::query(
            "SELECT algorithm_id, name, display_name, formation_spec, tier, cost_credits \
             FROM swarm_algorithms WHERE algorithm_id = $1",
        )
        .bind(aid)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            "Provide algorithm_name or algorithm_id".to_string(),
        ));
    };

    let algorithm = algorithm.ok_or((StatusCode::NOT_FOUND, "Algorithm not found".to_string()))?;

    let algorithm_id: Uuid = algorithm.get("algorithm_id");
    let algo_name: String = algorithm.get("name");
    let display_name: String = algorithm.get("display_name");
    let formation_spec: serde_json::Value = algorithm.get("formation_spec");
    let tier: String = algorithm.get("tier");
    let cost: i32 = algorithm.get("cost_credits");

    // Free algorithms don't need activation — return spec directly
    if tier == "free" {
        return Ok(Json(json!({
            "algorithm_id": algorithm_id,
            "name": algo_name,
            "display_name": display_name,
            "formation_spec": formation_spec,
            "activated": true,
            "charged": false,
            "message": "Free algorithm — no activation required",
        })));
    }

    // Check idempotency: already activated for this session?
    let existing = sqlx::query(
        "SELECT activation_id FROM swarm_activations \
         WHERE user_id = $1 AND swarm_id = $2 AND algorithm_id = $3",
    )
    .bind(&user_id)
    .bind(swarm_id)
    .bind(algorithm_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if existing.is_some() {
        return Ok(Json(json!({
            "algorithm_id": algorithm_id,
            "name": algo_name,
            "display_name": display_name,
            "formation_spec": formation_spec,
            "activated": true,
            "charged": false,
            "message": "Already activated for this session",
        })));
    }

    // Charge credits
    let wallet = get_or_create_wallet(pool, "user", &user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    charge_gas(
        pool,
        wallet.wallet_id,
        cost,
        "formation_activate",
        &format!("Activate {} formation", display_name),
        Some(&algorithm_id.to_string()),
    )
    .await?;

    // Insert activation
    let activation_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO swarm_activations (activation_id, algorithm_id, user_id, swarm_id) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(activation_id)
    .bind(algorithm_id)
    .bind(&user_id)
    .bind(swarm_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "algorithm_id": algorithm_id,
        "activation_id": activation_id,
        "name": algo_name,
        "display_name": display_name,
        "formation_spec": formation_spec,
        "activated": true,
        "charged": true,
        "cost_credits": cost,
    })))
}

// ─── GET /api/swarm-algorithms/activations/:swarm_id ───────────────

pub async fn list_activations_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(swarm_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    let rows = sqlx::query(
        "SELECT sa.activation_id, sa.activated_at, \
                a.algorithm_id, a.name, a.display_name, a.category, a.onto4mat_class, \
                a.formation_spec, a.tier, a.cost_credits, a.icon \
         FROM swarm_activations sa \
         JOIN swarm_algorithms a ON a.algorithm_id = sa.algorithm_id \
         WHERE sa.user_id = $1 AND sa.swarm_id = $2 \
         ORDER BY sa.activated_at ASC",
    )
    .bind(&user_id)
    .bind(swarm_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let activations: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "activation_id": r.get::<Uuid, _>("activation_id"),
                "activated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("activated_at").to_rfc3339(),
                "algorithm_id": r.get::<Uuid, _>("algorithm_id"),
                "name": r.get::<String, _>("name"),
                "display_name": r.get::<String, _>("display_name"),
                "category": r.get::<String, _>("category"),
                "onto4mat_class": r.get::<String, _>("onto4mat_class"),
                "formation_spec": r.get::<serde_json::Value, _>("formation_spec"),
                "tier": r.get::<String, _>("tier"),
                "cost_credits": r.get::<i32, _>("cost_credits"),
                "icon": r.get::<Option<String>, _>("icon"),
            })
        })
        .collect();

    Ok(Json(json!({
        "swarm_id": swarm_id,
        "activations": activations,
    })))
}
