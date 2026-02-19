//! Dashboard handlers — spatial queries for situational awareness

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use fermi_auth::AuthPrincipal;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;

// ═══════════════════════════════════════════════════════════════════════════
// QUERY STRUCTS
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct NearbyQuery {
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub radius: Option<i32>, // meters, default 1000
}

#[derive(Deserialize)]
pub struct CreaturesQuery {
    pub status: Option<String>,
    pub limit: Option<i32>,
}

// ═══════════════════════════════════════════════════════════════════════════
// HANDLERS
// ═══════════════════════════════════════════════════════════════════════════

/// GET /api/dashboard/my-rabbles
/// Returns rabbles where user has creatures, with distance from center
pub async fn my_rabbles_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.db.clone();

    let rows = sqlx::query("SELECT * FROM get_my_rabbles_with_status($1, 50)")
        .bind(&user_id)
        .fetch_all(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let rabbles: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "swarm_id": row.get::<Uuid, _>("swarm_id"),
                "name": row.get::<String, _>("name"),
                "location_name": row.get::<Option<String>, _>("location_name"),
                "center_lat": row.get::<f64, _>("center_lat"),
                "center_lng": row.get::<f64, _>("center_lng"),
                "radius_meters": row.get::<i32, _>("radius_meters"),
                "creature_count": row.get::<i32, _>("creature_count"),
                "participant_count": row.get::<i32, _>("participant_count"),
                "starts_at": row.get::<chrono::DateTime<chrono::Utc>, _>("starts_at").to_rfc3339(),
                "ends_at": row.get::<chrono::DateTime<chrono::Utc>, _>("ends_at").to_rfc3339(),
                "status": row.get::<String, _>("status"),
                "anchor_creature_id": row.get::<Option<Uuid>, _>("anchor_creature_id"),
                "anchor_creature_name": row.get::<Option<String>, _>("anchor_creature_name"),
                "anchor_creature_image": row.get::<Option<String>, _>("anchor_creature_image"),
                "my_creatures": row.get::<Option<Value>, _>("my_creatures"),
            })
        })
        .collect();

    Ok(Json(json!({ "rabbles": rabbles })))
}

/// GET /api/dashboard/nearby?lat=X&lng=Y&radius=Z
/// Returns rabbles near user location, with "in area" indicator
pub async fn nearby_rabbles_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<NearbyQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.db.clone();

    let lat = q.lat.ok_or((
        StatusCode::BAD_REQUEST,
        "Missing 'lat' parameter".to_string(),
    ))?;
    let lng = q.lng.ok_or((
        StatusCode::BAD_REQUEST,
        "Missing 'lng' parameter".to_string(),
    ))?;
    let radius = q.radius.unwrap_or(1000);

    let rows = sqlx::query("SELECT * FROM get_nearby_rabbles($1, $2, $3, 50)")
        .bind(lat)
        .bind(lng)
        .bind(radius)
        .fetch_all(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let rabbles: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "swarm_id": row.get::<Uuid, _>("swarm_id"),
                "name": row.get::<String, _>("name"),
                "location_name": row.get::<Option<String>, _>("location_name"),
                "center_lat": row.get::<f64, _>("center_lat"),
                "center_lng": row.get::<f64, _>("center_lng"),
                "radius_meters": row.get::<i32, _>("radius_meters"),
                "creature_count": row.get::<i32, _>("creature_count"),
                "participant_count": row.get::<i32, _>("participant_count"),
                "starts_at": row.get::<chrono::DateTime<chrono::Utc>, _>("starts_at").to_rfc3339(),
                "ends_at": row.get::<chrono::DateTime<chrono::Utc>, _>("ends_at").to_rfc3339(),
                "status": row.get::<String, _>("status"),
                "anchor_creature_id": row.get::<Option<Uuid>, _>("anchor_creature_id"),
                "anchor_creature_name": row.get::<Option<String>, _>("anchor_creature_name"),
                "anchor_creature_image": row.get::<Option<String>, _>("anchor_creature_image"),
                "distance_meters": row.get::<f64, _>("distance_meters"),
                "user_in_area": row.get::<bool, _>("user_in_area"),
            })
        })
        .collect();

    Ok(Json(json!({ "rabbles": rabbles })))
}

/// GET /api/dashboard/creatures
/// Returns user's creatures with deployment status
pub async fn creatures_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<CreaturesQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.db.clone();

    let status = q.status.unwrap_or_else(|| "active".to_string());
    let limit = q.limit.unwrap_or(200);

    let rows = sqlx::query("SELECT * FROM get_creatures_with_deployment($1, $2, $3)")
        .bind(&user_id)
        .bind(&status)
        .bind(limit)
        .fetch_all(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let creatures: Vec<Value> = rows.iter().map(|row| {
        json!({
            "creature_id": row.get::<Uuid, _>("creature_id"),
            "specimen_name": row.get::<Option<String>, _>("specimen_name"),
            "scientific_name": row.get::<String, _>("scientific_name"),
            "species_group": row.get::<String, _>("species_group"),
            "asset_path": row.get::<String, _>("asset_path"),
            "rabble_id": row.get::<Option<Uuid>, _>("rabble_id"),
            "rabble_name": row.get::<Option<String>, _>("rabble_name"),
            "location_lat": row.get::<Option<f64>, _>("location_lat"),
            "location_lng": row.get::<Option<f64>, _>("location_lng"),
            "h3_cell": row.get::<Option<String>, _>("h3_cell"),
            "state": row.get::<String, _>("state"),
            "presence": row.get::<String, _>("presence"),
            "distance_from_rabble_center": row.get::<Option<f64>, _>("distance_from_rabble_center"),
            "in_rabble_area": row.get::<Option<bool>, _>("in_rabble_area"),
        })
    }).collect();

    Ok(Json(json!({ "creatures": creatures })))
}

/// GET /api/dashboard/boundary-violations
/// Returns creatures that have left their rabble area
pub async fn boundary_violations_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.db.clone();

    let rows = sqlx::query("SELECT * FROM check_boundary_violations($1)")
        .bind(&user_id)
        .fetch_all(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let violations: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "creature_id": row.get::<Uuid, _>("creature_id"),
                "specimen_name": row.get::<Option<String>, _>("specimen_name"),
                "rabble_id": row.get::<Uuid, _>("rabble_id"),
                "rabble_name": row.get::<String, _>("rabble_name"),
                "distance_meters": row.get::<f64, _>("distance_meters"),
                "rabble_radius": row.get::<i32, _>("rabble_radius"),
            })
        })
        .collect();

    Ok(Json(json!({ "violations": violations })))
}
