//! Tethering handlers — link creatures to live GPS/sensors, push telemetry,
//! track visualization, and presence management.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::handlers::rabble_workspace;
use crate::AppState;
use fermi::gas::charge_gas;
use fermi_auth::{get_or_create_wallet, AuthPrincipal};


#[derive(Deserialize)]
pub struct UpdatePresenceRequest {
    pub presence: String,
}

/// PUT /api/creatures/:creature_id/presence — set creature presence state.
/// Owner only. Dispatches keeper agent to log the transition.
pub async fn update_creature_presence_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<UpdatePresenceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    if !["active", "sleeping", "parked"].contains(&req.presence.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Presence must be 'active', 'sleeping', or 'parked'".to_string(),
        ));
    }

    let creature = sqlx::query(
        "SELECT owner_id, specimen_name, personal_workspace_id FROM creatures c
         JOIN users u ON u.user_id = c.owner_id
         WHERE c.creature_id = $1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    let owner: String = creature.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your creature".to_string()));
    }

    let specimen_name: String = creature.try_get("specimen_name").unwrap_or_default();
    let personal_ws: Option<Uuid> = creature
        .try_get::<Option<Uuid>, _>("personal_workspace_id")
        .ok()
        .flatten();

    let result = sqlx::query(
        "UPDATE creature_conditions SET presence = $1, updated_at = NOW()
         WHERE creature_id = $2",
    )
    .bind(&req.presence)
    .bind(creature_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Creature not found or conditions not initialized".to_string(),
        ));
    }

    // Dispatch keeper agent to log the transition (non-blocking)
    if let Some(ws_id) = personal_ws {
        let state2 = state.clone();
        let user_id2 = user_id.clone();
        let presence2 = req.presence.clone();
        let name2 = specimen_name.clone();
        tokio::spawn(async move {
            let query = format!(
                "Creature {} is now {}. Log the transition.",
                name2, presence2
            );
            let _ = rabble_workspace::dispatch_rabble_action(
                &state2,
                ws_id,
                "keeper",
                "presence_change",
                &query,
                &user_id2,
            )
            .await;
        });
    }

    // Broadcast creature SSE event
    crate::handlers::streams::emit_creature_event(
        &state,
        creature_id,
        "presence_changed",
        json!({
            "creature_id": creature_id,
            "presence": req.presence,
            "specimen_name": specimen_name,
        }),
    );

    Ok(Json(json!({
        "creature_id": creature_id,
        "presence": req.presence,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// Tethering — link creature to live GPS/sensor for real-time tracking
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct TetherRequest {
    pub tether_type: Option<String>, // phone_gps, meshtastic, gps_tracker, fixed_sensor
    pub device_label: Option<String>,
    #[serde(default)]
    pub config: serde_json::Value,
}

/// POST /api/creatures/:creature_id/tether — tether creature to a signal source (1cr)
pub async fn tether_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<TetherRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify ownership
    let creature =
        sqlx::query("SELECT owner_id, specimen_name FROM creatures WHERE creature_id = $1")
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    let owner: String = creature.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your creature".to_string()));
    }

    // Check not already tethered
    let existing = sqlx::query(
        "SELECT tether_id FROM creature_tethers WHERE creature_id = $1 AND active = true LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if existing.is_some() {
        return Err((StatusCode::CONFLICT, "Creature is already tethered".into()));
    }

    // End non-perch active flights (fly, solo). Keep perch flight alive so
    // creature retains its location after untether. Perch = "creature is here."
    sqlx::query(
        "UPDATE creature_flights SET ended_at = NOW(),
         duration_seconds = EXTRACT(EPOCH FROM (NOW() - started_at))::int
         WHERE creature_id = $1 AND ended_at IS NULL AND flight_pattern != 'perch'",
    )
    .bind(creature_id)
    .execute(pool)
    .await
    .ok();

    // Charge 1cr tether fee
    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        1,
        "tether",
        &format!(
            "Tether creature {} to {}",
            creature_id,
            req.tether_type.as_deref().unwrap_or("phone_gps")
        ),
        Some(&creature_id.to_string()),
    )
    .await?;

    let tether_type = req.tether_type.as_deref().unwrap_or("phone_gps");
    let tether_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO creature_tethers (tether_id, creature_id, owner_id, tether_type, device_label, config)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(tether_id)
    .bind(creature_id)
    .bind(&user_id)
    .bind(tether_type)
    .bind(&req.device_label)
    .bind(&req.config)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Set presence to tracking
    sqlx::query("UPDATE creature_conditions SET presence = 'tracking', updated_at = NOW() WHERE creature_id = $1")
        .bind(creature_id)
        .execute(pool)
        .await
        .ok();

    // Broadcast creature SSE event
    crate::handlers::streams::emit_creature_event(
        &state,
        creature_id,
        "state_changed",
        json!({
            "state": "tethered",
            "tether_id": tether_id,
            "tether_type": tether_type,
            "device_label": req.device_label,
        }),
    );

    Ok(Json(json!({
        "tether_id": tether_id,
        "creature_id": creature_id,
        "tether_type": tether_type,
        "device_label": req.device_label,
        "status": "active",
    })))
}

/// DELETE /api/creatures/:creature_id/tether — untether creature
pub async fn untether_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify ownership
    let owner: String = sqlx::query("SELECT owner_id FROM creatures WHERE creature_id = $1")
        .bind(creature_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?
        .get("owner_id");

    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your creature".to_string()));
    }

    let result = sqlx::query(
        "UPDATE creature_tethers SET active = false, deactivated_at = NOW()
         WHERE creature_id = $1 AND active = true",
    )
    .bind(creature_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "No active tether".into()));
    }

    // End tracking flights — but if the creature is in a rabble, keep it there.
    // Find the tether flight first to check if it's attached to a swarm.
    let tether_flight = sqlx::query(
        "SELECT flight_id, swarm_id, center_lat, center_lng, h3_cell
         FROM creature_flights
         WHERE creature_id = $1 AND ended_at IS NULL AND data_source = 'device' LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let stayed_in_rabble = if let Some(ref tf) = tether_flight {
        let swarm_id: Option<Uuid> = tf.try_get::<Option<Uuid>, _>("swarm_id").ok().flatten();
        let tf_id: Uuid = tf.get("flight_id");

        // End the tether flight
        sqlx::query(
            "UPDATE creature_flights SET ended_at = NOW(),
             duration_seconds = EXTRACT(EPOCH FROM (NOW() - started_at))::int
             WHERE flight_id = $1",
        )
        .bind(tf_id)
        .execute(pool)
        .await
        .ok();

        // If it was in a rabble, create a new static swarm flight at current position.
        // The rabble freezes at the last anchor location.
        if let Some(sid) = swarm_id {
            let lat: f64 = tf.try_get("center_lat").unwrap_or(0.0);
            let lng: f64 = tf.try_get("center_lng").unwrap_or(0.0);
            let h3: String = tf.try_get("h3_cell").unwrap_or_else(|_| String::new());
            sqlx::query(
                "INSERT INTO creature_flights (flight_id, creature_id, owner_id,
                 h3_cell, h3_resolution, center_lat, center_lng,
                 flight_pattern, swarm_id, started_at)
                 VALUES ($1, $2, $3, $4, 12, $5, $6, 'swarm', $7, NOW())",
            )
            .bind(Uuid::new_v4())
            .bind(creature_id)
            .bind(&user_id)
            .bind(&h3)
            .bind(lat)
            .bind(lng)
            .bind(sid)
            .execute(pool)
            .await
            .ok();
            true
        } else {
            false
        }
    } else {
        // No tether flight found — end any other device flights
        sqlx::query(
            "UPDATE creature_flights SET ended_at = NOW(),
             duration_seconds = EXTRACT(EPOCH FROM (NOW() - started_at))::int
             WHERE creature_id = $1 AND ended_at IS NULL AND data_source = 'device'",
        )
        .bind(creature_id)
        .execute(pool)
        .await
        .ok();
        false
    };

    // Set presence back to active
    sqlx::query("UPDATE creature_conditions SET presence = 'active', updated_at = NOW() WHERE creature_id = $1")
        .bind(creature_id)
        .execute(pool)
        .await
        .ok();

    // Broadcast creature SSE event
    crate::handlers::streams::emit_creature_event(
        &state,
        creature_id,
        "state_changed",
        json!({
            "state": "untethered",
            "creature_id": creature_id,
            "stayed_in_rabble": stayed_in_rabble,
        }),
    );

    Ok(Json(json!({
        "creature_id": creature_id,
        "status": "untethered",
        "stayed_in_rabble": stayed_in_rabble,
    })))
}

#[derive(Debug, Deserialize)]
pub struct TelemetryPoint {
    pub lat: f64,
    pub lng: f64,
    pub altitude: Option<f64>,
    pub accuracy: Option<f64>,
    pub speed: Option<f64>,
    pub heading: Option<f64>,
    pub recorded_at: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct PushTelemetryRequest {
    pub points: Vec<TelemetryPoint>,
}

/// POST /api/creatures/:creature_id/telemetry — push position points from tethered device
pub async fn push_telemetry_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<PushTelemetryRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Get active tether
    let tether = sqlx::query(
        "SELECT tether_id, owner_id FROM creature_tethers
         WHERE creature_id = $1 AND active = true LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((
        StatusCode::NOT_FOUND,
        "No active tether for this creature".into(),
    ))?;

    let tether_owner: String = tether.get("owner_id");
    if tether_owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your tether".into()));
    }

    let tether_id: Uuid = tether.get("tether_id");
    let mut inserted = 0;

    for point in &req.points {
        let recorded_at = point
            .recorded_at
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);

        sqlx::query(
            "INSERT INTO telemetry_points
             (tether_id, creature_id, lat, lng, altitude, accuracy, speed, heading, metadata, recorded_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(tether_id)
        .bind(creature_id)
        .bind(point.lat)
        .bind(point.lng)
        .bind(point.altitude)
        .bind(point.accuracy)
        .bind(point.speed)
        .bind(point.heading)
        .bind(&point.metadata)
        .bind(recorded_at)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        inserted += 1;
    }

    // Update creature's flight record with latest position (for map display)
    if let Some(last) = req.points.last() {
        // Upsert: update existing tracking flight or create one
        let existing_flight = sqlx::query(
            "SELECT flight_id FROM creature_flights
             WHERE creature_id = $1 AND ended_at IS NULL AND data_source = 'device' LIMIT 1",
        )
        .bind(creature_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        if let Some(row) = existing_flight {
            let flight_id: Uuid = row.get("flight_id");
            sqlx::query(
                "UPDATE creature_flights SET center_lat = $1, center_lng = $2 WHERE flight_id = $3",
            )
            .bind(last.lat)
            .bind(last.lng)
            .bind(flight_id)
            .execute(pool)
            .await
            .ok();
        } else {
            // Create a tracking flight record
            sqlx::query(
                "INSERT INTO creature_flights
                 (flight_id, creature_id, owner_id, h3_cell, h3_resolution,
                  center_lat, center_lng, flight_pattern, data_source, started_at)
                 VALUES ($1, $2, $3, '', 12, $4, $5, 'tracking', 'device', NOW())",
            )
            .bind(Uuid::new_v4())
            .bind(creature_id)
            .bind(&user_id)
            .bind(last.lat)
            .bind(last.lng)
            .execute(pool)
            .await
            .ok();
        }

        // Anchor propagation: if this creature is a rabble's anchor, move the rabble
        let h3 = h3o::LatLng::new(last.lat, last.lng)
            .ok()
            .map(|ll| ll.to_cell(h3o::Resolution::Twelve).to_string())
            .unwrap_or_default();

        let updated = sqlx::query(
            "UPDATE swarm_events SET center_lat = $1, center_lng = $2, h3_cell = $3
             WHERE anchor_creature_id = $4 AND status IN ('scheduled', 'active')",
        )
        .bind(last.lat)
        .bind(last.lng)
        .bind(&h3)
        .bind(creature_id)
        .execute(pool)
        .await
        .ok()
        .map(|r| r.rows_affected())
        .unwrap_or(0);

        if updated > 0 {
            eprintln!(
                "[tether] Anchor creature {} moved rabble to ({}, {})",
                creature_id, last.lat, last.lng
            );
        }
    }

    // Broadcast creature SSE event — send latest position to subscribers
    if let Some(last) = req.points.last() {
        crate::handlers::streams::emit_creature_event(
            &state,
            creature_id,
            "location_update",
            json!({
                "lat": last.lat,
                "lng": last.lng,
                "altitude": last.altitude,
                "speed": last.speed,
                "heading": last.heading,
                "points_count": inserted,
            }),
        );
    }

    Ok(Json(json!({
        "inserted": inserted,
        "creature_id": creature_id,
    })))
}

#[derive(Debug, Deserialize)]
pub struct TrackQuery {
    pub since: Option<String>, // ISO 8601, defaults to last 24h
    pub limit: Option<i64>,    // max points, defaults to 1000
}

/// GET /api/creatures/:creature_id/track — get telemetry track for visualization
pub async fn get_track_handler(
    State(state): State<AppState>,
    Path(creature_id): Path<Uuid>,
    Query(q): Query<TrackQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pool = state.memory_store.pool();

    let since = q
        .since
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::hours(24));

    let limit = q.limit.unwrap_or(1000).min(5000);

    let rows = sqlx::query(
        "SELECT lat, lng, altitude, accuracy, speed, heading, metadata, recorded_at
         FROM telemetry_points
         WHERE creature_id = $1 AND recorded_at >= $2
         ORDER BY recorded_at ASC
         LIMIT $3",
    )
    .bind(creature_id)
    .bind(since)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let points: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "lat": r.get::<f64, _>("lat"),
                "lng": r.get::<f64, _>("lng"),
                "altitude": r.try_get::<Option<f64>, _>("altitude").unwrap_or(None),
                "accuracy": r.try_get::<Option<f64>, _>("accuracy").unwrap_or(None),
                "speed": r.try_get::<Option<f64>, _>("speed").unwrap_or(None),
                "heading": r.try_get::<Option<f64>, _>("heading").unwrap_or(None),
                "metadata": r.try_get::<serde_json::Value, _>("metadata").unwrap_or(json!({})),
                "recorded_at": r.get::<chrono::DateTime<chrono::Utc>, _>("recorded_at").to_rfc3339(),
            })
        })
        .collect();

    // Get active tether info
    let tether = sqlx::query(
        "SELECT tether_id, tether_type, device_label, created_at
         FROM creature_tethers WHERE creature_id = $1 AND active = true LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let tether_info = tether.map(|t| {
        json!({
            "tether_id": t.get::<Uuid, _>("tether_id"),
            "tether_type": t.get::<String, _>("tether_type"),
            "device_label": t.try_get::<Option<String>, _>("device_label").unwrap_or(None),
            "since": t.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
        })
    });

    Ok(Json(json!({
        "creature_id": creature_id,
        "points": points,
        "count": points.len(),
        "tether": tether_info,
    })))
}
