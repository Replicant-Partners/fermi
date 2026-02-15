//! Read-only creature query handlers.

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

use crate::AppState;
use fermi_auth::AuthPrincipal;

// ─── Public endpoints ──────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreatureQuery {
    pub species_group: Option<String>,
    pub scientific_name: Option<String>,
    pub owner_id: Option<String>,
    pub status: Option<String>,
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
        "SELECT c.creature_id, c.owner_id, c.scientific_name, c.common_name, c.species_group,
         c.gbif_key, c.specimen_name, c.variation_notes, c.asset_path, c.flight_silhouette_path,
         c.total_flights, c.unique_locations, c.status, c.animation_status, c.created_at,
         COALESCE(cc.visibility, 'public') AS visibility,
         COALESCE(cc.presence, 'active') AS presence,
         (SELECT location_name FROM creature_flights WHERE creature_id = c.creature_id
          ORDER BY started_at DESC LIMIT 1) as last_location_name
         FROM creatures c
         LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
         WHERE 1=1",
    );
    let mut bind_idx = 0u32;
    let mut binds_str: Vec<String> = Vec::new();

    if let Some(ref group) = q.species_group {
        bind_idx += 1;
        sql.push_str(&format!(" AND c.species_group = ${}", bind_idx));
        binds_str.push(group.clone());
    }
    if let Some(ref name) = q.scientific_name {
        bind_idx += 1;
        sql.push_str(&format!(" AND c.scientific_name ILIKE ${}", bind_idx));
        binds_str.push(format!("%{}%", name));
    }
    if let Some(ref owner) = q.owner_id {
        bind_idx += 1;
        sql.push_str(&format!(" AND c.owner_id = ${}", bind_idx));
        binds_str.push(owner.clone());
    }

    // Status filter: default to 'active', use 'all' to see everything
    match q.status.as_deref() {
        Some("all") => {} // no filter
        Some(status) => {
            bind_idx += 1;
            sql.push_str(&format!(" AND c.status = ${}", bind_idx));
            binds_str.push(status.to_string());
        }
        None => {
            bind_idx += 1;
            sql.push_str(&format!(" AND c.status = ${}", bind_idx));
            binds_str.push("active".to_string());
        }
    }

    sql.push_str(" ORDER BY c.created_at DESC");
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
                        "status": row.try_get::<String, _>("status").unwrap_or_else(|_| "active".to_string()),
                        "animation_status": row.try_get::<Option<String>, _>("animation_status").unwrap_or(None),
                        "visibility": row.try_get::<String, _>("visibility").unwrap_or_else(|_| "public".to_string()),
                        "presence": row.try_get::<String, _>("presence").unwrap_or_else(|_| "active".to_string()),
                        "last_location_name": row.try_get::<Option<String>, _>("last_location_name").unwrap_or(None),
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
        "SELECT c.creature_id, c.owner_id, c.workspace_id, c.scientific_name, c.common_name,
         c.species_group, c.gbif_key, c.taxonomy, c.specimen_name, c.variation_notes,
         c.asset_path, c.flight_silhouette_path, c.generation_params,
         c.mint_number, c.total_flights, c.total_flight_time_seconds, c.unique_locations,
         c.data_card, c.animation_status,
         c.created_at, c.updated_at,
         cs.state AS creature_state, cs.location_lat, cs.location_lng, cs.h3_cell AS state_h3,
         cs.rabble_id, cs.version_id AS current_version_id,
         COALESCE(cc.visibility, 'public') AS visibility,
         COALESCE(cc.sosa_opt_in, false) AS sosa_opt_in,
         COALESCE(cc.presence, 'active') AS presence,
         cc.walk_in_price AS conditions_walk_in_price,
         cc.active_modules
         FROM creatures c
         LEFT JOIN creature_state cs ON cs.creature_id = c.creature_id
         LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
         WHERE c.creature_id = $1",
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
                "sosa_opt_in": row.try_get::<bool, _>("sosa_opt_in").unwrap_or(false),
                "animation_status": row.try_get::<Option<String>, _>("animation_status").unwrap_or(None),
                "visibility": row.try_get::<String, _>("visibility").unwrap_or_else(|_| "public".to_string()),
                "presence": row.try_get::<String, _>("presence").unwrap_or_else(|_| "active".to_string()),
                // Versioned state (Phase 2)
                "creature_state": row.try_get::<Option<String>, _>("creature_state").unwrap_or(None),
                "location_lat": row.try_get::<Option<f64>, _>("location_lat").unwrap_or(None),
                "location_lng": row.try_get::<Option<f64>, _>("location_lng").unwrap_or(None),
                "state_h3": row.try_get::<Option<String>, _>("state_h3").unwrap_or(None),
                "rabble_id": row.try_get::<Option<Uuid>, _>("rabble_id").unwrap_or(None),
                "current_version_id": row.try_get::<Option<Uuid>, _>("current_version_id").unwrap_or(None),
                "conditions_walk_in_price": row.try_get::<Option<i32>, _>("conditions_walk_in_price").unwrap_or(None),
                "active_modules": row.try_get::<Option<Vec<String>>, _>("active_modules").unwrap_or(None),
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
/// GET /api/creatures/:creature_id/versions — version history (state transitions)
pub async fn creature_versions_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<PaginationQuery>,
) -> impl IntoResponse {
    let pool = state.memory_store.pool();
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);

    match sqlx::query(
        "SELECT version_id, creature_id, version_number, state, previous_state,
         location_lat, location_lng, h3_cell, rabble_id,
         transition_type, triggered_by, episode_ids, workspace_id,
         valid_from, recorded_at, metadata
         FROM creature_versions
         WHERE creature_id = $1
         ORDER BY version_number DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => {
            let versions: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    json!({
                        "version_id": row.get::<Uuid, _>("version_id"),
                        "creature_id": row.get::<Uuid, _>("creature_id"),
                        "version_number": row.get::<i32, _>("version_number"),
                        "state": row.get::<String, _>("state"),
                        "previous_state": row.try_get::<Option<String>, _>("previous_state").unwrap_or(None),
                        "location_lat": row.try_get::<Option<f64>, _>("location_lat").unwrap_or(None),
                        "location_lng": row.try_get::<Option<f64>, _>("location_lng").unwrap_or(None),
                        "h3_cell": row.try_get::<Option<String>, _>("h3_cell").unwrap_or(None),
                        "rabble_id": row.try_get::<Option<Uuid>, _>("rabble_id").unwrap_or(None),
                        "transition_type": row.get::<String, _>("transition_type"),
                        "triggered_by": row.get::<String, _>("triggered_by"),
                        "episode_ids": row.try_get::<Option<Vec<Uuid>>, _>("episode_ids").unwrap_or(None),
                        "workspace_id": row.try_get::<Option<Uuid>, _>("workspace_id").unwrap_or(None),
                        "valid_from": row.get::<chrono::DateTime<chrono::Utc>, _>("valid_from").to_rfc3339(),
                        "recorded_at": row.get::<chrono::DateTime<chrono::Utc>, _>("recorded_at").to_rfc3339(),
                        "metadata": row.get::<serde_json::Value, _>("metadata"),
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "versions": versions }))).into_response()
        }
        Err(e) => {
            eprintln!("Failed to get creature versions: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to get creature versions"})),
            )
                .into_response()
        }
    }
}

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
         flight_pattern, swarm_id, started_at, ended_at, duration_seconds,
         path_samples, environment, data_source
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
                        "path_samples": row.get::<Option<serde_json::Value>, _>("path_samples"),
                        "environment": row.try_get::<Option<serde_json::Value>, _>("environment").unwrap_or(None),
                        "data_source": row.try_get::<String, _>("data_source").unwrap_or_else(|_| "synthetic".to_string()),
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

// ─── Image serving (persistent, from DB) ───────────────────────────

/// GET /api/creatures/:creature_id/image — serve creature art from database
///
/// Falls back to filesystem, then placeholder SVG.
/// Sets Cache-Control for browser caching.
pub async fn creature_image_handler(
    State(state): State<AppState>,
    Path(creature_id): Path<Uuid>,
) -> impl IntoResponse {
    let pool = state.memory_store.pool();

    // Try DB first
    if let Ok(Some(row)) =
        sqlx::query("SELECT image_bytes, mime_type FROM creature_images WHERE creature_id = $1")
            .bind(creature_id)
            .fetch_optional(pool)
            .await
    {
        let bytes: Vec<u8> = row.get("image_bytes");
        let mime: String = row.get("mime_type");
        return (
            StatusCode::OK,
            [
                (axum::http::header::CONTENT_TYPE, mime),
                (
                    axum::http::header::CACHE_CONTROL,
                    "public, max-age=86400".to_string(),
                ),
            ],
            bytes,
        )
            .into_response();
    }

    // Fallback: try filesystem (works during same deploy that generated it)
    let fs_path = format!("static/creatures/{}.png", creature_id);
    if let Ok(bytes) = std::fs::read(&fs_path) {
        // Also persist to DB for next deploy
        let _ = sqlx::query(
            "INSERT INTO creature_images (creature_id, image_bytes, mime_type, file_size)
             VALUES ($1, $2, 'image/png', $3)
             ON CONFLICT (creature_id) DO UPDATE
             SET image_bytes = $2, mime_type = 'image/png', file_size = $3, updated_at = NOW()",
        )
        .bind(creature_id)
        .bind(&bytes)
        .bind(bytes.len() as i32)
        .execute(pool)
        .await;

        return (
            StatusCode::OK,
            [
                (axum::http::header::CONTENT_TYPE, "image/png".to_string()),
                (
                    axum::http::header::CACHE_CONTROL,
                    "public, max-age=86400".to_string(),
                ),
            ],
            bytes,
        )
            .into_response();
    }

    // Final fallback: placeholder SVG
    let placeholder = std::fs::read("static/creatures/placeholder.svg")
        .unwrap_or_else(|_| b"<svg></svg>".to_vec());
    (
        StatusCode::OK,
        [
            (
                axum::http::header::CONTENT_TYPE,
                "image/svg+xml".to_string(),
            ),
            (
                axum::http::header::CACHE_CONTROL,
                "public, max-age=60".to_string(),
            ),
        ],
        placeholder,
    )
        .into_response()
}

/// GET /api/creatures/:creature_id/animation/:layer_name — serve an animation layer from DB.
pub async fn creature_animation_layer_handler(
    State(state): State<AppState>,
    Path((creature_id, layer_name)): Path<(Uuid, String)>,
) -> impl IntoResponse {
    let pool = state.memory_store.pool();

    // Validate layer name
    if !["body", "left_wing", "right_wing"].contains(&layer_name.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            [
                (axum::http::header::CONTENT_TYPE, "text/plain".to_string()),
                (axum::http::header::CACHE_CONTROL, "no-cache".to_string()),
            ],
            b"Invalid layer name. Must be: body, left_wing, right_wing".to_vec(),
        )
            .into_response();
    }

    if let Ok(Some(row)) = sqlx::query(
        "SELECT image_bytes, mime_type FROM creature_animation_layers WHERE creature_id = $1 AND layer_name = $2",
    )
    .bind(creature_id)
    .bind(&layer_name)
    .fetch_optional(pool)
    .await
    {
        let bytes: Vec<u8> = row.get("image_bytes");
        let mime: String = row.get("mime_type");
        return (
            StatusCode::OK,
            [
                (axum::http::header::CONTENT_TYPE, mime),
                (axum::http::header::CACHE_CONTROL, "public, max-age=86400".to_string()),
            ],
            bytes,
        ).into_response();
    }

    (
        StatusCode::NOT_FOUND,
        [
            (axum::http::header::CONTENT_TYPE, "text/plain".to_string()),
            (axum::http::header::CACHE_CONTROL, "no-cache".to_string()),
        ],
        b"Animation layer not found".to_vec(),
    )
        .into_response()
}

/// GET /api/creatures/:creature_id/animation-status — check animation readiness.
pub async fn creature_animation_status_handler(
    State(state): State<AppState>,
    Path(creature_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pool = state.memory_store.pool();

    let row =
        sqlx::query("SELECT animation_status, species_group FROM creatures WHERE creature_id = $1")
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    let status: Option<String> = row.try_get("animation_status").unwrap_or(None);
    let species_group: String = row.get("species_group");

    let mut result = json!({
        "creature_id": creature_id,
        "species_group": species_group,
        "animation_status": status,
    });

    if status.as_deref() == Some("ready") {
        result["layers"] = json!({
            "body": format!("/api/creatures/{}/animation/body", creature_id),
            "left_wing": format!("/api/creatures/{}/animation/left_wing", creature_id),
            "right_wing": format!("/api/creatures/{}/animation/right_wing", creature_id),
        });
    }

    Ok(Json(result))
}

// ─── Visible flights endpoint ───────────────────────────────────────

#[derive(Deserialize)]
pub struct VisibleFlightsQuery {
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub radius: Option<f64>, // km, unused for now but reserved
}

/// GET /api/flights/visible — active flights visible to the current user
/// Returns public flights + contacts-only flights where viewer is a contact.
pub async fn list_visible_flights_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<VisibleFlightsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Get active flights that the viewer can see:
    // - public flights (anyone)
    // - contacts-only flights where the viewer is in the owner's contacts
    // - the viewer's own flights (always visible to self)
    // Excludes private flights from others.
    let rows = sqlx::query(
        "SELECT f.flight_id, f.creature_id, f.owner_id, f.center_lat, f.center_lng,
                f.location_name, f.flight_pattern, f.visibility, f.started_at, f.swarm_id,
                c.scientific_name, c.common_name, c.specimen_name, c.species_group,
                c.asset_path
         FROM creature_flights f
         JOIN creatures c ON c.creature_id = f.creature_id
         WHERE f.ended_at IS NULL
           AND (
             f.owner_id = $1
             OR f.visibility = 'public'
             OR (f.visibility = 'contacts'
                 AND EXISTS (
                   SELECT 1 FROM contacts
                   WHERE user_id = f.owner_id AND contact_id = $1
                 ))
           )
         ORDER BY f.started_at DESC
         LIMIT 100",
    )
    .bind(&user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let flights: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let owner_id: String = row.get("owner_id");
            json!({
                "flight_id": row.get::<Uuid, _>("flight_id"),
                "creature_id": row.get::<Uuid, _>("creature_id"),
                "owner_id": &owner_id,
                "is_mine": owner_id == user_id,
                "center_lat": row.get::<f64, _>("center_lat"),
                "center_lng": row.get::<f64, _>("center_lng"),
                "location_name": row.try_get::<Option<String>, _>("location_name").unwrap_or(None),
                "flight_pattern": row.get::<String, _>("flight_pattern"),
                "visibility": row.get::<String, _>("visibility"),
                "swarm_id": row.try_get::<Option<Uuid>, _>("swarm_id").unwrap_or(None),
                "started_at": row.get::<chrono::DateTime<chrono::Utc>, _>("started_at").to_rfc3339(),
                "scientific_name": row.get::<String, _>("scientific_name"),
                "common_name": row.try_get::<Option<String>, _>("common_name").unwrap_or(None),
                "specimen_name": row.try_get::<Option<String>, _>("specimen_name").unwrap_or(None),
                "species_group": row.get::<String, _>("species_group"),
                "asset_path": row.get::<String, _>("asset_path"),
            })
        })
        .collect();

    Ok(Json(json!({
        "flights": flights,
        "count": flights.len(),
    })))
}
