//! Rabble.world creature API handlers — public discovery + authenticated management.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use super::super::AppState;
use fermi_auth::AuthPrincipal;

// ─── Public endpoints ──────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreatureQuery {
    pub species_group: Option<String>,
    pub scientific_name: Option<String>,
    pub owner_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// GET /api/creatures — browse minted creatures
pub async fn list_creatures_handler(
    State(state): State<AppState>,
    Query(q): Query<CreatureQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(20).min(100);
    let offset = q.offset.unwrap_or(0);

    let mut sql = String::from(
        "SELECT creature_id, owner_id, scientific_name, common_name, species_group,
         gbif_key, specimen_name, variation_notes, asset_path, flight_silhouette_path,
         total_flights, unique_locations, created_at
         FROM creatures WHERE 1=1",
    );
    let mut bind_idx = 0u32;
    let mut binds_str: Vec<String> = Vec::new();

    if let Some(ref group) = q.species_group {
        bind_idx += 1;
        sql.push_str(&format!(" AND species_group = ${}", bind_idx));
        binds_str.push(group.clone());
    }
    if let Some(ref name) = q.scientific_name {
        bind_idx += 1;
        sql.push_str(&format!(" AND scientific_name ILIKE ${}", bind_idx));
        binds_str.push(format!("%{}%", name));
    }
    if let Some(ref owner) = q.owner_id {
        bind_idx += 1;
        sql.push_str(&format!(" AND owner_id = ${}", bind_idx));
        binds_str.push(owner.clone());
    }

    sql.push_str(" ORDER BY created_at DESC");
    sql.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

    // Build query with dynamic binds
    let mut query = sqlx::query(&sql);
    for s in &binds_str {
        query = query.bind(s);
    }

    let pool = state.memory_store.pool();
    match query.fetch_all(pool).await {
        Ok(rows) => {
            let creatures: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    json!({
                        "creature_id": row.get::<Uuid, _>("creature_id"),
                        "owner_id": row.get::<String, _>("owner_id"),
                        "scientific_name": row.get::<String, _>("scientific_name"),
                        "common_name": row.get::<Option<String>, _>("common_name"),
                        "species_group": row.get::<String, _>("species_group"),
                        "gbif_key": row.get::<Option<i64>, _>("gbif_key"),
                        "specimen_name": row.get::<Option<String>, _>("specimen_name"),
                        "variation_notes": row.get::<Option<String>, _>("variation_notes"),
                        "asset_path": row.get::<String, _>("asset_path"),
                        "flight_silhouette_path": row.get::<Option<String>, _>("flight_silhouette_path"),
                        "total_flights": row.get::<i32, _>("total_flights"),
                        "unique_locations": row.get::<i32, _>("unique_locations"),
                        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                    })
                })
                .collect();
            (
                StatusCode::OK,
                Json(json!({ "creatures": creatures, "count": creatures.len() })),
            )
                .into_response()
        }
        Err(e) => {
            eprintln!("Failed to list creatures: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to list creatures"})),
            )
                .into_response()
        }
    }
}

/// GET /api/creatures/:id — single creature with full data card
pub async fn get_creature_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let pool = state.memory_store.pool();
    match sqlx::query(
        "SELECT creature_id, owner_id, workspace_id, scientific_name, common_name,
         species_group, gbif_key, taxonomy, specimen_name, variation_notes,
         asset_path, flight_silhouette_path, generation_params,
         mint_number, total_flights, total_flight_time_seconds, unique_locations,
         data_card, created_at, updated_at
         FROM creatures WHERE creature_id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(row)) => {
            let creature = json!({
                "creature_id": row.get::<Uuid, _>("creature_id"),
                "owner_id": row.get::<String, _>("owner_id"),
                "workspace_id": row.get::<Option<Uuid>, _>("workspace_id"),
                "scientific_name": row.get::<String, _>("scientific_name"),
                "common_name": row.get::<Option<String>, _>("common_name"),
                "species_group": row.get::<String, _>("species_group"),
                "gbif_key": row.get::<Option<i64>, _>("gbif_key"),
                "taxonomy": row.get::<serde_json::Value, _>("taxonomy"),
                "specimen_name": row.get::<Option<String>, _>("specimen_name"),
                "variation_notes": row.get::<Option<String>, _>("variation_notes"),
                "asset_path": row.get::<String, _>("asset_path"),
                "flight_silhouette_path": row.get::<Option<String>, _>("flight_silhouette_path"),
                "generation_params": row.get::<serde_json::Value, _>("generation_params"),
                "mint_number": row.get::<i32, _>("mint_number"),
                "total_flights": row.get::<i32, _>("total_flights"),
                "total_flight_time_seconds": row.get::<i64, _>("total_flight_time_seconds"),
                "unique_locations": row.get::<i32, _>("unique_locations"),
                "data_card": row.get::<serde_json::Value, _>("data_card"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
            });
            (StatusCode::OK, Json(creature)).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Creature not found"})),
        )
            .into_response(),
        Err(e) => {
            eprintln!("Failed to get creature: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to get creature"})),
            )
                .into_response()
        }
    }
}

/// GET /api/creatures/:id/flights — flight history for a creature
pub async fn creature_flights_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<PaginationQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);

    let pool = state.memory_store.pool();
    match sqlx::query(
        "SELECT flight_id, creature_id, beacon_id, owner_id,
         h3_cell, center_lat, center_lng, location_name, country_code,
         flight_pattern, swarm_id, started_at, ended_at, duration_seconds
         FROM creature_flights
         WHERE creature_id = $1
         ORDER BY started_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => {
            let flights: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    json!({
                        "flight_id": row.get::<Uuid, _>("flight_id"),
                        "creature_id": row.get::<Uuid, _>("creature_id"),
                        "beacon_id": row.get::<Option<Uuid>, _>("beacon_id"),
                        "owner_id": row.get::<String, _>("owner_id"),
                        "h3_cell": row.get::<String, _>("h3_cell"),
                        "center_lat": row.get::<f64, _>("center_lat"),
                        "center_lng": row.get::<f64, _>("center_lng"),
                        "location_name": row.get::<Option<String>, _>("location_name"),
                        "country_code": row.get::<Option<String>, _>("country_code"),
                        "flight_pattern": row.get::<String, _>("flight_pattern"),
                        "swarm_id": row.get::<Option<Uuid>, _>("swarm_id"),
                        "started_at": row.get::<chrono::DateTime<chrono::Utc>, _>("started_at").to_rfc3339(),
                        "ended_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("ended_at").map(|t| t.to_rfc3339()),
                        "duration_seconds": row.get::<Option<i32>, _>("duration_seconds"),
                    })
                })
                .collect();
            (
                StatusCode::OK,
                Json(json!({ "flights": flights, "count": flights.len() })),
            )
                .into_response()
        }
        Err(e) => {
            eprintln!("Failed to get flights: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to get flights"})),
            )
                .into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ─── Swarm endpoints (public read) ─────────────────────────────────

#[derive(Deserialize)]
pub struct SwarmQuery {
    pub h3_cell: Option<String>,
    pub status: Option<String>,
    pub species_filter: Option<String>,
    pub limit: Option<i64>,
}

/// GET /api/swarms — browse upcoming/active swarm events
pub async fn list_swarms_handler(
    State(state): State<AppState>,
    Query(q): Query<SwarmQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(20).min(50);

    let mut sql = String::from(
        "SELECT swarm_id, creator_id, h3_cell, center_lat, center_lng,
         location_name, name, description, species_filter, max_participants,
         starts_at, ends_at, status, participant_count, creature_count, created_at
         FROM swarm_events WHERE 1=1",
    );

    let mut binds: Vec<String> = Vec::new();
    let mut bind_idx = 0u32;

    if let Some(ref status) = q.status {
        bind_idx += 1;
        sql.push_str(&format!(" AND status = ${}", bind_idx));
        binds.push(status.clone());
    } else {
        // Default: show scheduled and active
        sql.push_str(" AND status IN ('scheduled', 'active')");
    }

    if let Some(ref h3) = q.h3_cell {
        bind_idx += 1;
        sql.push_str(&format!(" AND h3_cell = ${}", bind_idx));
        binds.push(h3.clone());
    }

    if let Some(ref species) = q.species_filter {
        bind_idx += 1;
        sql.push_str(&format!(" AND species_filter = ${}", bind_idx));
        binds.push(species.clone());
    }

    sql.push_str(&format!(" ORDER BY starts_at ASC LIMIT {}", limit));

    let mut query = sqlx::query(&sql);
    for s in &binds {
        query = query.bind(s);
    }

    let pool = state.memory_store.pool();
    match query.fetch_all(pool).await {
        Ok(rows) => {
            let swarms: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    json!({
                        "swarm_id": row.get::<Uuid, _>("swarm_id"),
                        "creator_id": row.get::<String, _>("creator_id"),
                        "h3_cell": row.get::<String, _>("h3_cell"),
                        "center_lat": row.get::<f64, _>("center_lat"),
                        "center_lng": row.get::<f64, _>("center_lng"),
                        "location_name": row.get::<Option<String>, _>("location_name"),
                        "name": row.get::<String, _>("name"),
                        "description": row.get::<Option<String>, _>("description"),
                        "species_filter": row.get::<Option<String>, _>("species_filter"),
                        "max_participants": row.get::<Option<i32>, _>("max_participants"),
                        "starts_at": row.get::<chrono::DateTime<chrono::Utc>, _>("starts_at").to_rfc3339(),
                        "ends_at": row.get::<chrono::DateTime<chrono::Utc>, _>("ends_at").to_rfc3339(),
                        "status": row.get::<String, _>("status"),
                        "participant_count": row.get::<i32, _>("participant_count"),
                        "creature_count": row.get::<i32, _>("creature_count"),
                        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                    })
                })
                .collect();
            (
                StatusCode::OK,
                Json(json!({ "swarms": swarms, "count": swarms.len() })),
            )
                .into_response()
        }
        Err(e) => {
            eprintln!("Failed to list swarms: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to list swarms"})),
            )
                .into_response()
        }
    }
}

/// GET /api/swarms/:id — single swarm with details
pub async fn get_swarm_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let pool = state.memory_store.pool();
    match sqlx::query(
        "SELECT swarm_id, creator_id, workspace_id, h3_cell, h3_resolution,
         center_lat, center_lng, location_name, grid_map_id,
         name, description, species_filter, max_participants,
         starts_at, ends_at, status, participant_count, creature_count,
         metadata, created_at
         FROM swarm_events WHERE swarm_id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(row)) => {
            let swarm = json!({
                "swarm_id": row.get::<Uuid, _>("swarm_id"),
                "creator_id": row.get::<String, _>("creator_id"),
                "workspace_id": row.get::<Option<Uuid>, _>("workspace_id"),
                "h3_cell": row.get::<String, _>("h3_cell"),
                "h3_resolution": row.get::<i32, _>("h3_resolution"),
                "center_lat": row.get::<f64, _>("center_lat"),
                "center_lng": row.get::<f64, _>("center_lng"),
                "location_name": row.get::<Option<String>, _>("location_name"),
                "grid_map_id": row.get::<Option<Uuid>, _>("grid_map_id"),
                "name": row.get::<String, _>("name"),
                "description": row.get::<Option<String>, _>("description"),
                "species_filter": row.get::<Option<String>, _>("species_filter"),
                "max_participants": row.get::<Option<i32>, _>("max_participants"),
                "starts_at": row.get::<chrono::DateTime<chrono::Utc>, _>("starts_at").to_rfc3339(),
                "ends_at": row.get::<chrono::DateTime<chrono::Utc>, _>("ends_at").to_rfc3339(),
                "status": row.get::<String, _>("status"),
                "participant_count": row.get::<i32, _>("participant_count"),
                "creature_count": row.get::<i32, _>("creature_count"),
                "metadata": row.get::<serde_json::Value, _>("metadata"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            });
            (StatusCode::OK, Json(swarm)).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Swarm not found"})),
        )
            .into_response(),
        Err(e) => {
            eprintln!("Failed to get swarm: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to get swarm"})),
            )
                .into_response()
        }
    }
}

// ─── Collections (authenticated) ───────────────────────────────────

/// GET /api/collections — user's creature collections
pub async fn list_collections_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> impl IntoResponse {
    let pool = state.memory_store.pool();
    match sqlx::query(
        "SELECT collection_id, owner_id, name, description, creature_ids, created_at, updated_at
         FROM creature_collections WHERE owner_id = $1
         ORDER BY updated_at DESC",
    )
    .bind(principal.user_id())
    .fetch_all(pool)
    .await
    {
        Ok(rows) => {
            let collections: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    json!({
                        "collection_id": row.get::<Uuid, _>("collection_id"),
                        "name": row.get::<String, _>("name"),
                        "description": row.get::<Option<String>, _>("description"),
                        "creature_ids": row.get::<serde_json::Value, _>("creature_ids"),
                        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                        "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "collections": collections }))).into_response()
        }
        Err(e) => {
            eprintln!("Failed to list collections: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to list collections"})),
            )
                .into_response()
        }
    }
}
