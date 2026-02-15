//! Creature versioned state — dual-write helpers.
//!
//! These functions write to the new creature_state / creature_versions /
//! creature_conditions tables alongside the existing creatures + creature_flights
//! columns. Once the migration is complete and handlers read from the new tables,
//! the old writes can be removed.

use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Record a state transition: insert a creature_version and update creature_state.
///
/// Returns the new version_id.
pub async fn record_transition(
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
pub async fn init_conditions(
    pool: &PgPool,
    creature_id: Uuid,
    visibility: &str,
    sosa_opt_in: bool,
) {
    let _ = sqlx::query(
        "INSERT INTO creature_conditions (creature_id, visibility, sosa_opt_in, active_modules, updated_at)
         VALUES ($1, $2, $3, '{}', NOW())
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
pub async fn update_condition_visibility(pool: &PgPool, creature_id: Uuid, visibility: &str) {
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
pub async fn toggle_module(pool: &PgPool, creature_id: Uuid, module: &str, active: bool) {
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
pub async fn get_current_state(pool: &PgPool, creature_id: Uuid) -> Option<(String, Option<Uuid>)> {
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
