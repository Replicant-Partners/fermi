//! Device pairing handlers.

use axum::{
    extract::{Path, State},
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


// ─── Device pairing handlers ──────────────────────────────────────

#[derive(Deserialize)]
pub struct PairDeviceRequest {
    pub device_type: String,
    pub device_identifier: String,
    pub device_name: Option<String>,
}


/// GET /api/creatures/:id/devices — list paired devices
pub async fn list_devices_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let rows = sqlx::query(
        "SELECT device_id, creature_id, device_type, device_identifier, device_name,
         is_active, last_lat, last_lng, last_seen_at, created_at
         FROM creature_devices WHERE creature_id = $1 AND owner_id = $2
         ORDER BY created_at DESC",
    )
    .bind(creature_id)
    .bind(&user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let devices: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            json!({
                "device_id": row.get::<Uuid, _>("device_id"),
                "creature_id": row.get::<Uuid, _>("creature_id"),
                "device_type": row.get::<String, _>("device_type"),
                "device_identifier": row.get::<String, _>("device_identifier"),
                "device_name": row.get::<Option<String>, _>("device_name"),
                "is_active": row.get::<bool, _>("is_active"),
                "last_lat": row.get::<Option<f64>, _>("last_lat"),
                "last_lng": row.get::<Option<f64>, _>("last_lng"),
                "last_seen_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_seen_at").map(|dt| dt.to_rfc3339()),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({ "devices": devices })))
}


/// POST /api/creatures/:id/devices — pair a device
pub async fn pair_device_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<PairDeviceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify creature ownership
    let owns = sqlx::query("SELECT 1 FROM creatures WHERE creature_id = $1 AND owner_id = $2")
        .bind(creature_id)
        .bind(&user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if owns.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            "Creature not found or not owned by you".to_string(),
        ));
    }

    let device_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO creature_devices (device_id, creature_id, owner_id, device_type, device_identifier, device_name)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (owner_id, device_identifier) DO UPDATE SET creature_id = $2, device_type = $4, device_name = $6",
    )
    .bind(device_id)
    .bind(creature_id)
    .bind(&user_id)
    .bind(&req.device_type)
    .bind(&req.device_identifier)
    .bind(&req.device_name)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "device_id": device_id,
        "creature_id": creature_id,
        "device_type": req.device_type,
        "paired": true,
    })))
}


#[derive(Deserialize)]
pub struct UpdateDeviceRequest {
    pub device_name: Option<String>,
    pub is_active: Option<bool>,
}


/// PUT /api/devices/:device_id — update device name/active
pub async fn update_device_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(device_id): Path<Uuid>,
    Json(req): Json<UpdateDeviceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let mut sets = Vec::new();
    let device_name = req.device_name;
    let is_active = req.is_active;
    let mut bind_idx = 0u32;

    if device_name.is_some() {
        bind_idx += 1;
        sets.push(format!("device_name = ${}", bind_idx));
    }
    if is_active.is_some() {
        bind_idx += 1;
        sets.push(format!("is_active = ${}", bind_idx));
    }

    if sets.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No fields to update".to_string()));
    }

    let device_bind = bind_idx + 1;
    let owner_bind = bind_idx + 2;
    let sql = format!(
        "UPDATE creature_devices SET {} WHERE device_id = ${} AND owner_id = ${}",
        sets.join(", "),
        device_bind,
        owner_bind
    );

    let mut query = sqlx::query(&sql);
    if let Some(ref name) = device_name {
        query = query.bind(name);
    }
    if let Some(active) = is_active {
        query = query.bind(active);
    }
    query = query.bind(device_id).bind(&user_id);

    let result = query
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Device not found".to_string()));
    }

    Ok(Json(json!({ "updated": true })))
}


/// DELETE /api/devices/:device_id — unpair device
pub async fn unpair_device_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(device_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let result = sqlx::query("DELETE FROM creature_devices WHERE device_id = $1 AND owner_id = $2")
        .bind(device_id)
        .bind(&user_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Device not found".to_string()));
    }

    Ok(Json(json!({ "unpaired": true })))
}


#[derive(Deserialize)]
pub struct ReportLocationRequest {
    pub lat: f64,
    pub lng: f64,
}


/// POST /api/devices/:device_id/location — report device location
pub async fn report_device_location_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(device_id): Path<Uuid>,
    Json(req): Json<ReportLocationRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let result = sqlx::query(
        "UPDATE creature_devices SET last_lat = $1, last_lng = $2, last_seen_at = NOW()
         WHERE device_id = $3 AND owner_id = $4",
    )
    .bind(req.lat)
    .bind(req.lng)
    .bind(device_id)
    .bind(&user_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Device not found".to_string()));
    }

    Ok(Json(json!({
        "device_id": device_id,
        "lat": req.lat,
        "lng": req.lng,
        "synced": true,
    })))
}

