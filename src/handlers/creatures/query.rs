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
          ORDER BY started_at DESC LIMIT 1) as last_location_name,
         FLOOR(LOG(2, 1.0
           + COALESCE((SELECT COUNT(*) FROM creature_versions WHERE creature_id = c.creature_id), 0) * 1.0
           + COALESCE((SELECT COUNT(*) FROM creature_versions WHERE creature_id = c.creature_id AND transition_type = 'dream'), 0) * 5.0
           + c.total_flights * 0.2
           + c.unique_locations * 0.3
           + COALESCE((SELECT COUNT(DISTINCT swarm_id) FROM creature_flights WHERE creature_id = c.creature_id AND swarm_id IS NOT NULL), 0) * 2.0
           + COALESCE(array_length(cc.active_modules, 1), 0) * 1.0
         ))::int AS cognition_level,
         cs.state AS creature_state,
         cs.rabble_id,
         sw.name AS rabble_name,
         -- Anchor = host. One creature per rabble is the host; user manages by proxy.
         (CASE WHEN sw.anchor_creature_id = c.creature_id THEN true ELSE false END) AS is_anchor
         FROM creatures c
         LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
         LEFT JOIN creature_state cs ON cs.creature_id = c.creature_id
         LEFT JOIN swarm_events sw ON sw.swarm_id = cs.rabble_id
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
                    let is_anchor = row.try_get::<bool, _>("is_anchor").unwrap_or(false);
                    let rabble_id: Option<Uuid> = row.try_get::<Option<Uuid>, _>("rabble_id").ok().flatten();
                    // Anchor creature = host. User is proxy only.
                    let rabble_role = if rabble_id.is_some() {
                        if is_anchor { Some("host") } else { Some("participant") }
                    } else {
                        None
                    };
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
                        "cognition_level": row.try_get::<Option<i32>, _>("cognition_level").unwrap_or(Some(0)),
                        "creature_state": row.try_get::<Option<String>, _>("creature_state").unwrap_or(None),
                        "rabble_id": rabble_id,
                        "rabble_name": row.try_get::<Option<String>, _>("rabble_name").unwrap_or(None),
                        "is_anchor": is_anchor,
                        "rabble_role": rabble_role,
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
         sw.name AS rabble_name,
         sw.creator_id AS rabble_creator_id,
         COALESCE(cc.visibility, 'public') AS visibility,
         COALESCE(cc.sosa_opt_in, false) AS sosa_opt_in,
         COALESCE(cc.presence, 'active') AS presence,
         cc.walk_in_price AS conditions_walk_in_price,
         cc.active_modules,
         -- Social context: friendship count
         (SELECT COUNT(*) FROM creature_friendships cf
          WHERE cf.status = 'accepted'
            AND (cf.creature_a = c.creature_id OR cf.creature_b = c.creature_id)
         ) AS friend_count,
         -- Social context: pending inbound friendship requests
         (SELECT COUNT(*) FROM creature_friendships cf
          WHERE cf.status = 'pending'
            AND cf.initiated_by != c.creature_id
            AND (cf.creature_a = c.creature_id OR cf.creature_b = c.creature_id)
         ) AS pending_friend_requests,
         -- Is this creature the anchor of a rabble it's in?
         (CASE WHEN sw.anchor_creature_id = c.creature_id THEN true ELSE false END) AS is_anchor,
         -- Active flight info (tether status, data source)
         cf_active.flight_id AS active_flight_id,
         cf_active.data_source AS active_flight_data_source,
         cf_active.started_at AS active_flight_started_at,
         cf_active.location_name AS active_flight_location,
         -- Owner display name (for viewing other users' creatures)
         u_owner.display_name AS owner_display_name,
         u_owner.social_visibility AS owner_social_visibility
         FROM creatures c
         LEFT JOIN creature_state cs ON cs.creature_id = c.creature_id
         LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
         LEFT JOIN swarm_events sw ON sw.swarm_id = cs.rabble_id
         LEFT JOIN LATERAL (
             SELECT flight_id, data_source, started_at, location_name
             FROM creature_flights
             WHERE creature_id = c.creature_id AND ended_at IS NULL
             ORDER BY started_at DESC LIMIT 1
         ) cf_active ON true
         LEFT JOIN users u_owner ON u_owner.user_id = c.owner_id
         WHERE c.creature_id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(row)) => {
            let creature_id_val = row.get::<Uuid, _>("creature_id");
            let owner_id_val = row.get::<String, _>("owner_id");
            let rabble_id_val: Option<Uuid> =
                row.try_get::<Option<Uuid>, _>("rabble_id").unwrap_or(None);
            let rabble_creator: Option<String> = row
                .try_get::<Option<String>, _>("rabble_creator_id")
                .unwrap_or(None);
            let is_anchor: bool = row.try_get::<bool, _>("is_anchor").unwrap_or(false);
            let data_source: Option<String> = row
                .try_get::<Option<String>, _>("active_flight_data_source")
                .unwrap_or(None);

            // Derive the creature's role in its current rabble.
            // The anchor creature IS the host — the user only manages by proxy.
            // A user may have multiple creatures in their own rabble, but only
            // the anchor shows as "host"; the rest are "participant".
            let rabble_role = if rabble_id_val.is_some() {
                if is_anchor {
                    Some("host")
                } else {
                    Some("participant")
                }
            } else {
                None
            };

            // Derive tether status from active flight data source
            let is_tethered = data_source.as_deref() == Some("device");

            // Owner visibility respects social_visibility setting
            let owner_vis: String = row
                .try_get::<String, _>("owner_social_visibility")
                .unwrap_or_else(|_| "public".to_string());
            let owner_display = match owner_vis.as_str() {
                "public" => row
                    .try_get::<Option<String>, _>("owner_display_name")
                    .unwrap_or(None),
                _ => None, // creature-only or private: hide owner name
            };

            let creature = json!({
                "creature_id": creature_id_val,
                "owner_id": owner_id_val,
                "owner_display_name": owner_display,
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
                "rabble_id": rabble_id_val,
                "rabble_name": row.try_get::<Option<String>, _>("rabble_name").unwrap_or(None),
                "current_version_id": row.try_get::<Option<Uuid>, _>("current_version_id").unwrap_or(None),
                "conditions_walk_in_price": row.try_get::<Option<i32>, _>("conditions_walk_in_price").unwrap_or(None),
                "active_modules": row.try_get::<Option<Vec<String>>, _>("active_modules").unwrap_or(None),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
                // ─── Social context (new) ───────────────────────────
                "social": {
                    "friend_count": row.try_get::<i64, _>("friend_count").unwrap_or(0),
                    "pending_friend_requests": row.try_get::<i64, _>("pending_friend_requests").unwrap_or(0),
                    "rabble_role": rabble_role,      // "host" | "participant" | null  (host = anchor creature only)
                    "is_tethered": is_tethered,
                    "is_anchor": is_anchor,
                },
                // ─── Active flight info (new) ───────────────────────
                "active_flight": {
                    "flight_id": row.try_get::<Option<Uuid>, _>("active_flight_id").unwrap_or(None),
                    "data_source": data_source,       // "device" = tethered, "synthetic" = manual
                    "started_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("active_flight_started_at")
                        .unwrap_or(None).map(|t| t.to_rfc3339()),
                    "location_name": row.try_get::<Option<String>, _>("active_flight_location").unwrap_or(None),
                },
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

/// GET /api/creatures/:creature_id/versions/latest?transition_type=enemy_scan&after=<ISO8601>
/// Poll endpoint: returns the most recent version matching the given transition_type
/// created after the specified timestamp. Used by pills to poll for fire-and-forget results.
// ═══════════════════════════════════════════════════════════════════
// Per-creature activity feed (Phase 4, Task 4.3)
// ═══════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct CreatureActivityQuery {
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

/// GET /api/creatures/:creature_id/activity — per-creature activity feed.
/// Filters activity_events to those referencing this creature.
/// Same response shape as /api/feed/events but scoped to one creature.
pub async fn creature_activity_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(creature_id): Path<String>,
    Query(q): Query<CreatureActivityQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pool = state.memory_store.pool();
    let cid: Uuid = creature_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid creature ID".to_string()))?;

    let limit = q.limit.unwrap_or(30).min(100);
    let offset = q.offset.unwrap_or(0);

    let rows = sqlx::query(
        "SELECT ae.event_id, ae.event_type, ae.title, ae.body, ae.metadata,
                ae.rabble_id, ae.created_at,
                se.name AS rabble_name,
                c.specimen_name AS actor_creature_name,
                c.species_group AS actor_species_group
         FROM activity_events ae
         LEFT JOIN swarm_events se ON se.swarm_id = ae.rabble_id
         LEFT JOIN creatures c ON c.creature_id = ae.creature_id
         WHERE ae.creature_id = $1
         ORDER BY ae.created_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(cid)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let events: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "event_id": row.get::<Uuid, _>("event_id"),
                "event_type": row.get::<String, _>("event_type"),
                "title": row.get::<String, _>("title"),
                "body": row.get::<Option<String>, _>("body"),
                "metadata": row.get::<Option<serde_json::Value>, _>("metadata"),
                "rabble_id": row.get::<Option<Uuid>, _>("rabble_id"),
                "rabble_name": row.get::<Option<String>, _>("rabble_name"),
                "actor_creature_name": row.get::<Option<String>, _>("actor_creature_name"),
                "actor_species_group": row.get::<Option<String>, _>("actor_species_group"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    let has_more = events.len() as i32 == limit;

    Ok(Json(serde_json::json!({
        "events": events,
        "creature_id": cid,
        "has_more": has_more,
        "limit": limit,
        "offset": offset,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// Stitched flight path as GeoJSON (Phase 4, Task 4.5)
// ═══════════════════════════════════════════════════════════════════

/// GET /api/creatures/:creature_id/flight-path/:flight_id — flight path as GeoJSON.
///
/// Takes raw telemetry points + path_samples from the flight and returns a
/// GeoJSON Feature with LineString geometry, plus computed stats (distance,
/// speed, waypoints, rabbles crossed).
pub async fn creature_flight_path_handler(
    State(state): State<AppState>,
    Path((creature_id, flight_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pool = state.memory_store.pool();
    let cid: Uuid = creature_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid creature ID".to_string()))?;
    let fid: Uuid = flight_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid flight ID".to_string()))?;

    // Fetch the flight record
    let flight = sqlx::query(
        "SELECT flight_id, creature_id, owner_id, center_lat, center_lng,
                h3_cell, location_name, flight_pattern, swarm_id,
                started_at, ended_at, duration_seconds, path_samples, data_source
         FROM creature_flights
         WHERE flight_id = $1 AND creature_id = $2",
    )
    .bind(fid)
    .bind(cid)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Flight not found".to_string()))?;

    let started_at: chrono::DateTime<chrono::Utc> = flight.get("started_at");
    let ended_at: Option<chrono::DateTime<chrono::Utc>> = flight.try_get("ended_at").ok();
    let duration_seconds: Option<i32> = flight.try_get("duration_seconds").ok();
    let path_samples: Option<serde_json::Value> = flight.try_get("path_samples").ok().flatten();
    let swarm_id: Option<Uuid> = flight.try_get::<Option<Uuid>, _>("swarm_id").ok().flatten();
    let location_name: Option<String> = flight.try_get("location_name").ok();
    let data_source: Option<String> = flight.try_get("data_source").ok();

    // Also fetch telemetry points if available
    let telemetry_rows = sqlx::query(
        "SELECT lat, lng, altitude, speed, heading, recorded_at
         FROM creature_telemetry
         WHERE creature_id = $1
           AND recorded_at >= $2
           AND ($3::timestamptz IS NULL OR recorded_at <= $3)
         ORDER BY recorded_at ASC
         LIMIT 5000",
    )
    .bind(cid)
    .bind(started_at)
    .bind(ended_at)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Build coordinates array from telemetry or path_samples
    let mut coordinates: Vec<serde_json::Value> = Vec::new();
    let mut waypoints: Vec<serde_json::Value> = Vec::new();
    let mut total_distance_meters: f64 = 0.0;

    if !telemetry_rows.is_empty() {
        // Use real telemetry
        let mut prev_lat: Option<f64> = None;
        let mut prev_lng: Option<f64> = None;

        for row in &telemetry_rows {
            let lat: f64 = row.get("lat");
            let lng: f64 = row.get("lng");
            let recorded_at: chrono::DateTime<chrono::Utc> = row.get("recorded_at");

            coordinates.push(serde_json::json!([lng, lat, recorded_at.to_rfc3339()]));

            // Compute distance from previous point (Haversine)
            if let (Some(plat), Some(plng)) = (prev_lat, prev_lng) {
                total_distance_meters += haversine_meters(plat, plng, lat, lng);
            }

            // Add waypoint every ~10 points for enrichment hooks
            if coordinates.len() % 10 == 0 || coordinates.len() == 1 {
                let h3 = {
                    use h3o::{LatLng as H3LatLng, Resolution};
                    H3LatLng::new(lat, lng)
                        .map(|ll| ll.to_cell(Resolution::Twelve).to_string())
                        .unwrap_or_default()
                };
                waypoints.push(serde_json::json!({
                    "lat": lat,
                    "lng": lng,
                    "timestamp": recorded_at.to_rfc3339(),
                    "h3_cell": h3,
                    "enrichment_hook": "waypoint_context",
                }));
            }

            prev_lat = Some(lat);
            prev_lng = Some(lng);
        }
    } else if let Some(ref samples) = path_samples {
        // Fall back to path_samples from the flight record
        if let Some(arr) = samples.as_array() {
            for sample in arr {
                let lat = sample.get("lat").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let lng = sample.get("lng").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let t = sample
                    .get("t")
                    .or_else(|| sample.get("timestamp"))
                    .and_then(|v| v.as_f64());
                coordinates.push(serde_json::json!([lng, lat, t]));
            }
        }
    } else {
        // No telemetry — just use the flight's center point
        let lat: f64 = flight.get("center_lat");
        let lng: f64 = flight.get("center_lng");
        if lat != 0.0 || lng != 0.0 {
            coordinates.push(serde_json::json!([lng, lat, started_at.to_rfc3339()]));
        }
    }

    // Compute average speed
    let dur_secs = duration_seconds.unwrap_or(0) as f64;
    let avg_speed = if dur_secs > 0.0 {
        total_distance_meters / dur_secs
    } else {
        0.0
    };

    // Find rabbles crossed (swarms whose area intersects the flight path)
    let rabbles_crossed: Vec<Uuid> = if swarm_id.is_some() {
        vec![swarm_id.unwrap()]
    } else {
        vec![]
    };

    // Build GeoJSON Feature
    let geojson = serde_json::json!({
        "type": "Feature",
        "geometry": {
            "type": if coordinates.len() > 1 { "LineString" } else { "Point" },
            "coordinates": if coordinates.len() > 1 {
                serde_json::Value::Array(coordinates.clone())
            } else if coordinates.len() == 1 {
                coordinates[0].clone()
            } else {
                serde_json::json!([0, 0])
            },
        },
        "properties": {
            "total_distance_meters": (total_distance_meters * 10.0).round() / 10.0,
            "duration_seconds": duration_seconds,
            "average_speed_mps": (avg_speed * 100.0).round() / 100.0,
            "rabbles_crossed": rabbles_crossed,
            "waypoints": waypoints,
            "data_source": data_source,
            "point_count": coordinates.len(),
        },
    });

    let share_token = format!("{:x}", fid.as_u128() & 0xFFFFFFFFFFFF);

    Ok(Json(serde_json::json!({
        "flight_id": fid,
        "creature_id": cid,
        "started_at": started_at.to_rfc3339(),
        "ended_at": ended_at.map(|t| t.to_rfc3339()),
        "location_name": location_name,
        "geojson": geojson,
        "share_url": format!("/flights/{}", share_token),
    })))
}

/// Haversine distance in meters between two lat/lng pairs.
fn haversine_meters(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    const R: f64 = 6_371_000.0;
    let d_lat = (lat2 - lat1).to_radians();
    let d_lng = (lng2 - lng1).to_radians();
    let a = (d_lat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lng / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    R * c
}

// ═══════════════════════════════════════════════════════════════════

#[allow(dead_code)]
pub async fn creature_version_poll_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<VersionPollQuery>,
) -> impl IntoResponse {
    let pool = state.memory_store.pool();

    let row = sqlx::query(
        "SELECT version_id, version_number, state, transition_type, triggered_by,
                recorded_at, metadata
         FROM creature_versions
         WHERE creature_id = $1 AND transition_type = $2 AND recorded_at > $3
         ORDER BY recorded_at DESC LIMIT 1",
    )
    .bind(id)
    .bind(&q.transition_type)
    .bind(q.after)
    .fetch_optional(pool)
    .await;

    match row {
        Ok(Some(r)) => (
            StatusCode::OK,
            Json(json!({
                "found": true,
                "version_id": r.get::<Uuid, _>("version_id"),
                "version_number": r.get::<i32, _>("version_number"),
                "state": r.get::<String, _>("state"),
                "transition_type": r.get::<String, _>("transition_type"),
                "triggered_by": r.get::<String, _>("triggered_by"),
                "recorded_at": r.get::<chrono::DateTime<chrono::Utc>, _>("recorded_at").to_rfc3339(),
                "metadata": r.get::<serde_json::Value, _>("metadata"),
            })),
        )
            .into_response(),
        Ok(None) => (
            StatusCode::OK,
            Json(json!({ "found": false })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{}", e) })),
        )
            .into_response(),
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
         path_samples, environment, metadata, data_source
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
                        "metadata": row.try_get::<Option<serde_json::Value>, _>("metadata").unwrap_or(None),
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
pub struct VersionPollQuery {
    pub transition_type: String,
    pub after: chrono::DateTime<chrono::Utc>,
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
                "owner_id": row.get::<String, _>("owner_id"),
            })
        })
        .collect();

    Ok(Json(json!({
        "flights": flights,
        "count": flights.len(),
    })))
}

// ─── Activity Feed ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct FeedQuery {
    pub filter: Option<String>,  // "all" | "nearby" | "friends" | "starred"
    pub h3_cell: Option<String>, // for nearby filter
    pub limit: Option<i64>,      // default 50, max 200
    pub before: Option<String>,  // cursor: ISO timestamp for pagination
}

/// GET /api/feed — community activity feed from creature_versions
pub async fn feed_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<FeedQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();
    let limit = q.limit.unwrap_or(50).min(200);
    let filter = q.filter.as_deref().unwrap_or("all");

    // Parse cursor
    let before: Option<chrono::DateTime<chrono::Utc>> = q.before.as_ref().and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });

    // Build dynamic SQL based on filter
    // Base query: creature_versions JOIN creatures + users + swarm_events
    // Visibility: own creatures always, public, contacts-only if viewer is contact
    let mut sql = String::from(
        "SELECT cv.version_id, cv.creature_id, cv.version_number,
                cv.state, cv.previous_state, cv.transition_type,
                cv.location_lat, cv.location_lng, cv.h3_cell,
                cv.rabble_id, cv.metadata, cv.valid_from,
                c.specimen_name, c.common_name, c.scientific_name,
                c.species_group, c.asset_path, c.owner_id,
                u.display_name AS owner_name,
                se.name AS rabble_name, se.creature_count, se.walk_in_price,
                se.location_name AS rabble_location, se.creator_id AS rabble_creator_id
         FROM creature_versions cv
         JOIN creatures c ON c.creature_id = cv.creature_id
         LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
         LEFT JOIN users u ON u.user_id = c.owner_id
         LEFT JOIN swarm_events se ON se.swarm_id = cv.rabble_id
         WHERE cv.valid_from > NOW() - INTERVAL '14 days'
           AND (
             c.owner_id = $1
             OR COALESCE(cc.visibility, 'public') = 'public'
             OR (COALESCE(cc.visibility, 'public') = 'contacts'
                 AND EXISTS (SELECT 1 FROM contacts WHERE user_id = c.owner_id AND contact_id = $1))
           )",
    );

    let mut bind_idx = 1u32; // $1 = user_id

    // Cursor pagination
    bind_idx += 1; // $2 = before timestamp
    sql.push_str(&format!(
        " AND (${bind_idx}::timestamptz IS NULL OR cv.valid_from < ${bind_idx})"
    ));

    // Filter-specific clauses
    match filter {
        "nearby" => {
            if let Some(ref h3) = q.h3_cell {
                bind_idx += 1;
                sql.push_str(&format!(" AND cv.h3_cell = ${bind_idx}"));
                // We'll bind h3 later
                let _ = h3; // used in bind section
            }
        }
        "friends" => {
            sql.push_str(
                " AND EXISTS (SELECT 1 FROM contacts WHERE user_id = $1 AND contact_id = c.owner_id)",
            );
        }
        "starred" => {
            sql.push_str(
                " AND EXISTS (SELECT 1 FROM creature_favourites WHERE user_id = $1 AND creature_id = cv.creature_id)",
            );
        }
        _ => {} // "all" — no extra filter
    }

    sql.push_str(&format!(
        " ORDER BY cv.valid_from DESC LIMIT {}",
        limit + 1 // fetch one extra to detect has_more
    ));

    // Bind parameters dynamically
    let mut query = sqlx::query(&sql).bind(&user_id).bind(before);
    if filter == "nearby" {
        if let Some(ref h3) = q.h3_cell {
            query = query.bind(h3);
        }
    }

    let rows = query
        .fetch_all(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let has_more = rows.len() as i64 > limit;
    let rows_to_use = if has_more {
        &rows[..limit as usize]
    } else {
        &rows
    };

    // Batch-lookup starred creatures for this user
    let creature_ids: Vec<Uuid> = rows_to_use
        .iter()
        .map(|r| r.get::<Uuid, _>("creature_id"))
        .collect();
    let mut starred: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    if !creature_ids.is_empty() {
        let unique_ids: Vec<Uuid> = creature_ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let placeholders: Vec<String> = (1..=unique_ids.len())
            .map(|i| format!("${}", i + 1))
            .collect();
        let star_sql = format!(
            "SELECT creature_id FROM creature_favourites WHERE user_id = $1 AND creature_id IN ({})",
            placeholders.join(", ")
        );
        let mut star_query = sqlx::query(&star_sql).bind(&user_id);
        for cid in &unique_ids {
            star_query = star_query.bind(cid);
        }
        if let Ok(star_rows) = star_query.fetch_all(pool).await {
            for r in &star_rows {
                starred.insert(r.get::<Uuid, _>("creature_id"));
            }
        }
    }

    let events: Vec<serde_json::Value> = rows_to_use
        .iter()
        .map(|row| {
            let creature_id: Uuid = row.get("creature_id");
            let owner_id: String = row.get("owner_id");
            let is_mine = owner_id == user_id;
            let rabble_creator: Option<String> = row
                .try_get::<Option<String>, _>("rabble_creator_id")
                .ok()
                .flatten();
            let is_rabble_creator = rabble_creator
                .as_ref()
                .map(|rc| rc == &user_id)
                .unwrap_or(false);
            let creature_count: Option<i32> = row
                .try_get::<Option<i32>, _>("creature_count")
                .ok()
                .flatten();
            let rabble_id: Option<Uuid> = row
                .try_get::<Option<Uuid>, _>("rabble_id")
                .ok()
                .flatten();
            let can_join = rabble_id.is_some()
                && !is_mine
                && creature_count.unwrap_or(0) > 0;
            let creature_name = row
                .try_get::<Option<String>, _>("specimen_name")
                .ok()
                .flatten()
                .or_else(|| {
                    row.try_get::<Option<String>, _>("common_name")
                        .ok()
                        .flatten()
                })
                .unwrap_or_else(|| row.get::<String, _>("scientific_name"));
            let location = row
                .try_get::<Option<String>, _>("rabble_location")
                .ok()
                .flatten();

            json!({
                "version_id": row.get::<Uuid, _>("version_id"),
                "creature_id": creature_id,
                "creature_name": creature_name,
                "scientific_name": row.get::<String, _>("scientific_name"),
                "species_group": row.get::<String, _>("species_group"),
                "asset_path": row.get::<String, _>("asset_path"),
                "owner_id": owner_id,
                "owner_name": row.try_get::<Option<String>, _>("owner_name").ok().flatten(),
                "is_mine": is_mine,
                "is_rabble_creator": is_rabble_creator,
                "transition_type": row.get::<String, _>("transition_type"),
                "state": row.get::<String, _>("state"),
                "previous_state": row.try_get::<Option<String>, _>("previous_state").ok().flatten(),
                "location_name": location,
                "location_lat": row.try_get::<Option<f64>, _>("location_lat").ok().flatten().unwrap_or(0.0),
                "location_lng": row.try_get::<Option<f64>, _>("location_lng").ok().flatten().unwrap_or(0.0),
                "h3_cell": row.try_get::<Option<String>, _>("h3_cell").ok().flatten(),
                "rabble_id": rabble_id,
                "rabble_name": row.try_get::<Option<String>, _>("rabble_name").ok().flatten(),
                "rabble_creature_count": creature_count,
                "walk_in_price": row.try_get::<Option<i32>, _>("walk_in_price").ok().flatten(),
                "can_join": can_join,
                "is_starred": starred.contains(&creature_id),
                "timestamp": row.get::<chrono::DateTime<chrono::Utc>, _>("valid_from").to_rfc3339(),
                "metadata": row.try_get::<serde_json::Value, _>("metadata").unwrap_or_else(|_| json!({})),
            })
        })
        .collect();

    Ok(Json(json!({
        "events": events,
        "count": events.len(),
        "has_more": has_more,
    })))
}

// ─── ADR-011: Cognition endpoint ────────────────────────────────────

/// GET /api/creatures/:creature_id/cognition
///
/// Returns both cognition axes for a creature:
/// - cognition_level: earned knowledge (computed from activity history, never degrades)
/// - cognition_tier:  bandwidth (set by owner, determines which model runs)
pub async fn creature_cognition_handler(
    State(state): State<AppState>,
    Path(creature_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let row = sqlx::query(
        "SELECT
           c.creature_id,
           c.owner_id,
           c.specimen_name,
           c.scientific_name,
           COALESCE(cc.cognition_tier, 'free') AS cognition_tier,
           FLOOR(LOG(2, 1.0
             + COALESCE((SELECT COUNT(*) FROM creature_versions WHERE creature_id = c.creature_id), 0) * 1.0
             + COALESCE((SELECT COUNT(*) FROM creature_versions WHERE creature_id = c.creature_id AND transition_type = 'dream'), 0) * 5.0
             + c.total_flights * 0.2
             + c.unique_locations * 0.3
             + COALESCE((SELECT COUNT(DISTINCT swarm_id) FROM creature_flights WHERE creature_id = c.creature_id AND swarm_id IS NOT NULL), 0) * 2.0
             + COALESCE(array_length(cc.active_modules, 1), 0) * 1.0
           ))::int AS cognition_level
         FROM creatures c
         LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
         WHERE c.creature_id = $1",
    )
    .bind(creature_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "Creature not found".into()))?;

    let cognition_level: i32 = row.try_get("cognition_level").unwrap_or(0);
    let cognition_tier: String = row
        .try_get("cognition_tier")
        .unwrap_or_else(|_| "free".into());

    // Tier thresholds for upgrade nudge: if level suggests the creature has
    // outgrown free, surface a hint. Thresholds are intentionally generous.
    let upgrade_available = match cognition_tier.as_str() {
        "free" => cognition_level >= 3,
        "standard" => cognition_level >= 7,
        _ => false,
    };

    Ok(Json(json!({
        "creature_id": creature_id,
        "specimen_name": row.try_get::<String, _>("specimen_name").unwrap_or_default(),
        "scientific_name": row.try_get::<String, _>("scientific_name").unwrap_or_default(),
        "cognition_level": cognition_level,
        "cognition_tier": cognition_tier,
        "upgrade_available": upgrade_available,
        "tier_description": match cognition_tier.as_str() {
            "free"     => "Basic retrieval and generation. Compound orchestrations gated.",
            "standard" => "Moderate synthesis, reliable tool use, richer narration.",
            "premium"  => "Deep reasoning, full choreography, complex compound agents.",
            _          => "Unknown tier.",
        },
    })))
}
