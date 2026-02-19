//! Shared helpers for creature handlers — ownership checks, flight management,
//! state transitions, H3 computation, and condition management.

use axum::http::StatusCode;
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════
// H3 computation
// ═══════════════════════════════════════════════════════════════════

/// Compute H3 cell index from lat/lng at resolution 12.
/// Returns the hex string or empty string on error.
pub(crate) fn compute_h3_cell(lat: f64, lng: f64) -> String {
    use h3o::{LatLng, Resolution};
    match LatLng::new(lat, lng) {
        Ok(ll) => ll.to_cell(Resolution::Twelve).to_string(),
        Err(_) => String::new(),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Ownership verification
// ═══════════════════════════════════════════════════════════════════

/// Verify that the given user owns the creature. Returns the full creature row on success.
pub(crate) async fn verify_creature_ownership(
    pool: &PgPool,
    creature_id: Uuid,
    user_id: &str,
) -> Result<sqlx::postgres::PgRow, (StatusCode, String)> {
    let creature = sqlx::query(
        "SELECT c.owner_id, c.specimen_name, c.scientific_name, c.species_group,
                c.common_name, c.gbif_key, c.asset_path,
                COALESCE(cc.visibility, 'public') AS visibility,
                u.personal_workspace_id
         FROM creatures c
         LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
         LEFT JOIN users u ON u.user_id = c.owner_id
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

    Ok(creature)
}

// ═══════════════════════════════════════════════════════════════════
// Flight management helpers
// ═══════════════════════════════════════════════════════════════════

/// Auto-end any active flight for a creature. Returns the old flight info if one was ended.
/// Used before perch, fly, record_flight, etc. to ensure clean state transitions.
pub(crate) async fn auto_end_active_flight(
    pool: &PgPool,
    creature_id: Uuid,
) -> Result<Option<EndedFlightInfo>, (StatusCode, String)> {
    let active_flight = sqlx::query(
        "SELECT flight_id, location_name, swarm_id FROM creature_flights
         WHERE creature_id = $1 AND ended_at IS NULL LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(row) = active_flight {
        let flight_id: Uuid = row.get("flight_id");
        let swarm_id: Option<Uuid> = row.try_get::<Option<Uuid>, _>("swarm_id").ok().flatten();
        let location_name: Option<String> = row.try_get("location_name").ok();

        sqlx::query(
            "UPDATE creature_flights SET ended_at = NOW(),
             duration_seconds = EXTRACT(EPOCH FROM (NOW() - started_at))::int
             WHERE flight_id = $1",
        )
        .bind(flight_id)
        .execute(pool)
        .await
        .ok();

        // Mark departure in swarm_participants if leaving a rabble
        if let Some(sid) = swarm_id {
            sqlx::query(
                "UPDATE swarm_participants SET left_at = NOW()
                 WHERE creature_id = $1 AND swarm_id = $2 AND left_at IS NULL",
            )
            .bind(creature_id)
            .bind(sid)
            .execute(pool)
            .await
            .ok();
        }

        Ok(Some(EndedFlightInfo {
            flight_id,
            swarm_id,
            location_name,
        }))
    } else {
        Ok(None)
    }
}

/// Info about a flight that was auto-ended.
pub(crate) struct EndedFlightInfo {
    pub flight_id: Uuid,
    pub swarm_id: Option<Uuid>,
    pub location_name: Option<String>,
}

/// Find the workspace associated with a creature (swarm workspace or personal).
pub(crate) async fn find_creature_workspace(
    pool: &PgPool,
    creature_id: Uuid,
) -> Result<Uuid, (StatusCode, String)> {
    // Try swarm workspace first
    let ws = sqlx::query(
        "SELECT se.workspace_id
         FROM creature_flights cf
         JOIN swarm_events se ON se.swarm_id = cf.swarm_id
         WHERE cf.creature_id = $1 AND cf.ended_at IS NULL AND se.workspace_id IS NOT NULL
         LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .and_then(|r| r.try_get::<Option<Uuid>, _>("workspace_id").ok().flatten());

    if let Some(ws_id) = ws {
        return Ok(ws_id);
    }

    // Fall back to owner's personal workspace
    let owner_id: String = sqlx::query("SELECT owner_id FROM creatures WHERE creature_id = $1")
        .bind(creature_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?
        .get("owner_id");

    sqlx::query("SELECT personal_workspace_id FROM users WHERE user_id = $1")
        .bind(&owner_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .and_then(|r| {
            r.try_get::<Option<Uuid>, _>("personal_workspace_id")
                .ok()
                .flatten()
        })
        .ok_or((
            StatusCode::BAD_REQUEST,
            "No workspace available — try again after placing your creature".to_string(),
        ))
}

// ═══════════════════════════════════════════════════════════════════
// Creature versioned state — dual-write helpers
// ═══════════════════════════════════════════════════════════════════

/// Record a state transition: insert a creature_version and update creature_state.
///
/// Returns the new version_id.
pub(crate) async fn record_transition(
    pool: &PgPool,
    creature_id: Uuid,
    state: &str,
    previous_state: Option<&str>,
    transition_type: &str,
    triggered_by: &str,
    location_lat: f64,
    location_lng: f64,
    h3_cell: &str,
    rabble_id: Option<Uuid>,
    workspace_id: Option<Uuid>,
    metadata: &serde_json::Value,
) -> Result<Uuid, String> {
    // Get next version number for this creature
    let next_vn: i64 = sqlx::query(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM creature_versions WHERE creature_id = $1",
    )
    .bind(creature_id)
    .fetch_one(pool)
    .await
    .map(|r| r.get::<i64, _>(0))
    .unwrap_or(1);

    // Insert version (immutable)
    let version_id = Uuid::new_v4();
    let result = sqlx::query(
        "INSERT INTO creature_versions (
            version_id, creature_id, version_number, state, previous_state,
            location_lat, location_lng, h3_cell, rabble_id,
            transition_type, triggered_by, workspace_id,
            valid_from, recorded_at, metadata
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW(), NOW(), $13)",
    )
    .bind(version_id)
    .bind(creature_id)
    .bind(next_vn)
    .bind(state)
    .bind(previous_state)
    .bind(location_lat)
    .bind(location_lng)
    .bind(h3_cell)
    .bind(rabble_id)
    .bind(transition_type)
    .bind(triggered_by)
    .bind(workspace_id)
    .bind(metadata)
    .execute(pool)
    .await;

    if let Err(e) = result {
        eprintln!("[creature_state] version insert failed: {}", e);
        return Err(e.to_string());
    }

    // Upsert creature_state (mutable pointer)
    let result = sqlx::query(
        "INSERT INTO creature_state (
            creature_id, state, location_lat, location_lng, h3_cell,
            rabble_id, workspace_id, version_id, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
        ON CONFLICT (creature_id) DO UPDATE SET
            state = $2, location_lat = $3, location_lng = $4, h3_cell = $5,
            rabble_id = $6, workspace_id = $7, version_id = $8, updated_at = NOW()",
    )
    .bind(creature_id)
    .bind(state)
    .bind(location_lat)
    .bind(location_lng)
    .bind(h3_cell)
    .bind(rabble_id)
    .bind(workspace_id)
    .bind(version_id)
    .execute(pool)
    .await;

    if let Err(e) = result {
        eprintln!("[creature_state] state upsert failed: {}", e);
        return Err(e.to_string());
    }

    Ok(version_id)
}

/// Initialize creature_conditions on mint.
pub(crate) async fn init_conditions(
    pool: &PgPool,
    creature_id: Uuid,
    visibility: &str,
    sosa_opt_in: bool,
) {
    let _ = sqlx::query(
        "INSERT INTO creature_conditions (creature_id, visibility, sosa_opt_in, presence, active_modules, updated_at)
         VALUES ($1, $2, $3, 'active', '{}', NOW())
         ON CONFLICT (creature_id) DO NOTHING",
    )
    .bind(creature_id)
    .bind(visibility)
    .bind(sosa_opt_in)
    .execute(pool)
    .await
    .map_err(|e| eprintln!("[creature_state] conditions init failed: {}", e));
}

/// Update a specific condition.
pub(crate) async fn update_condition_visibility(
    pool: &PgPool,
    creature_id: Uuid,
    visibility: &str,
) {
    let _ = sqlx::query(
        "UPDATE creature_conditions SET visibility = $1, updated_at = NOW() WHERE creature_id = $2",
    )
    .bind(visibility)
    .bind(creature_id)
    .execute(pool)
    .await
    .map_err(|e| eprintln!("[creature_state] visibility update failed: {}", e));
}

/// Toggle a module in active_modules array.
pub(crate) async fn toggle_module(pool: &PgPool, creature_id: Uuid, module: &str, active: bool) {
    let sql = if active {
        "UPDATE creature_conditions SET active_modules = array_append(
            array_remove(active_modules, $1), $1
         ), updated_at = NOW() WHERE creature_id = $2"
    } else {
        "UPDATE creature_conditions SET active_modules = array_remove(active_modules, $1),
         updated_at = NOW() WHERE creature_id = $2"
    };
    let _ = sqlx::query(sql)
        .bind(module)
        .bind(creature_id)
        .execute(pool)
        .await
        .map_err(|e| eprintln!("[creature_state] toggle_module failed: {}", e));
}

/// Get the current state for a creature. Returns (state, previous version_id) or None.
pub(crate) async fn get_current_state(
    pool: &PgPool,
    creature_id: Uuid,
) -> Option<(String, Option<Uuid>)> {
    sqlx::query("SELECT state, version_id FROM creature_state WHERE creature_id = $1")
        .bind(creature_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|r| {
            (
                r.get::<String, _>("state"),
                r.try_get::<Option<Uuid>, _>("version_id").unwrap_or(None),
            )
        })
}
