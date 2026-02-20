//! Dashboard handlers — spatial queries for situational awareness

use axum::{
    extract::{Path, Query, State},
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

// ═══════════════════════════════════════════════════════════════════════════
// SAVED LOCATIONS (favourite places, drop pins, creature waypoints)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct SaveLocationRequest {
    pub name: String,
    pub lat: f64,
    pub lng: f64,
    pub radius_meters: Option<i32>,
    pub source: Option<String>,
    pub source_id: Option<Uuid>,
    pub notes: Option<String>,
}

/// POST /api/locations — save a location (drop pin, rabble location, waypoint)
pub async fn save_location_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<SaveLocationRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.db.clone();

    // Enforce max 200 saved locations per user
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM saved_locations WHERE user_id = $1")
        .bind(&user_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

    if count >= 200 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Maximum 200 saved locations reached. Delete some to save more.".to_string(),
        ));
    }

    // Compute H3 cell
    let h3_cell = {
        use h3o::{LatLng, Resolution};
        LatLng::new(req.lat, req.lng)
            .map(|ll| ll.to_cell(Resolution::Twelve).to_string())
            .unwrap_or_default()
    };

    let source = req.source.as_deref().unwrap_or("pin");
    let radius = req.radius_meters.unwrap_or(500);

    let row = sqlx::query(
        "INSERT INTO saved_locations (user_id, name, lat, lng, radius_meters, h3_cell, source, source_id, notes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id, created_at",
    )
    .bind(&user_id)
    .bind(&req.name)
    .bind(req.lat)
    .bind(req.lng)
    .bind(radius)
    .bind(&h3_cell)
    .bind(source)
    .bind(req.source_id)
    .bind(&req.notes)
    .fetch_one(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "id": row.get::<Uuid, _>("id"),
        "name": req.name,
        "lat": req.lat,
        "lng": req.lng,
        "radius_meters": radius,
        "h3_cell": h3_cell,
        "source": source,
        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
    })))
}

/// GET /api/locations — list my saved locations
pub async fn list_locations_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.db.clone();

    let rows = sqlx::query("SELECT * FROM get_saved_locations($1, 200)")
        .bind(&user_id)
        .fetch_all(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let locations: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<Uuid, _>("id"),
                "name": row.get::<String, _>("name"),
                "lat": row.get::<f64, _>("lat"),
                "lng": row.get::<f64, _>("lng"),
                "radius_meters": row.get::<i32, _>("radius_meters"),
                "h3_cell": row.get::<Option<String>, _>("h3_cell"),
                "source": row.get::<String, _>("source"),
                "source_id": row.get::<Option<Uuid>, _>("source_id"),
                "notes": row.get::<Option<String>, _>("notes"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
                "rabble_name": row.get::<Option<String>, _>("rabble_name"),
                "rabble_status": row.get::<Option<String>, _>("rabble_status"),
            })
        })
        .collect();

    Ok(Json(json!({ "locations": locations })))
}

/// DELETE /api/locations/:id — remove a saved location
pub async fn delete_location_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(location_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.db.clone();

    let result = sqlx::query("DELETE FROM saved_locations WHERE id = $1 AND user_id = $2")
        .bind(location_id)
        .bind(&user_id)
        .execute(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Location not found".to_string()));
    }

    Ok(Json(json!({ "deleted": true, "id": location_id })))
}

#[derive(Deserialize)]
pub struct UpdateLocationRequest {
    pub name: Option<String>,
    pub radius_meters: Option<i32>,
    pub notes: Option<String>,
}

/// PATCH /api/locations/:id — rename, adjust radius, update notes
pub async fn update_location_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(location_id): Path<Uuid>,
    Json(req): Json<UpdateLocationRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.db.clone();

    let mut sets = Vec::new();
    let mut bind_idx = 3u32; // $1 = id, $2 = user_id

    if req.name.is_some() {
        sets.push(format!("name = ${bind_idx}"));
        bind_idx += 1;
    }
    if req.radius_meters.is_some() {
        sets.push(format!("radius_meters = ${bind_idx}"));
        bind_idx += 1;
    }
    if req.notes.is_some() {
        sets.push(format!("notes = ${bind_idx}"));
    }

    if sets.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No fields to update".to_string()));
    }

    sets.push("updated_at = NOW()".to_string());

    let sql = format!(
        "UPDATE saved_locations SET {} WHERE id = $1 AND user_id = $2 \
         RETURNING id, name, lat, lng, radius_meters, notes, updated_at",
        sets.join(", ")
    );

    let mut query = sqlx::query(&sql).bind(location_id).bind(&user_id);

    if let Some(ref name) = req.name {
        query = query.bind(name.as_str());
    }
    if let Some(radius) = req.radius_meters {
        query = query.bind(radius);
    }
    if let Some(ref notes) = req.notes {
        query = query.bind(notes.as_str());
    }

    let row = query
        .fetch_optional(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Location not found".to_string()))?;

    Ok(Json(json!({
        "id": row.get::<Uuid, _>("id"),
        "name": row.get::<String, _>("name"),
        "lat": row.get::<f64, _>("lat"),
        "lng": row.get::<f64, _>("lng"),
        "radius_meters": row.get::<i32, _>("radius_meters"),
        "notes": row.get::<Option<String>, _>("notes"),
        "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
    })))
}

// ═══════════════════════════════════════════════════════════════════════════
// NEARBY CREATURES (spatial discovery)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct NearbyCreaturesQuery {
    pub lat: f64,
    pub lng: f64,
    pub radius: Option<i32>,
    pub limit: Option<i32>,
}

/// GET /api/dashboard/nearby-creatures?lat=X&lng=Y&radius=Z
/// Spatial query for creatures near a point, respecting visibility settings.
pub async fn nearby_creatures_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<NearbyCreaturesQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.db.clone();
    let radius = q.radius.unwrap_or(1000);
    let limit = q.limit.unwrap_or(50);

    let rows = sqlx::query("SELECT * FROM get_nearby_creatures($1, $2, $3, $4, $5)")
        .bind(&user_id)
        .bind(q.lat)
        .bind(q.lng)
        .bind(radius)
        .bind(limit)
        .fetch_all(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let creatures: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "creature_id": row.get::<Uuid, _>("creature_id"),
                "owner_id": row.get::<String, _>("owner_id"),
                "specimen_name": row.get::<Option<String>, _>("specimen_name"),
                "scientific_name": row.get::<String, _>("scientific_name"),
                "species_group": row.get::<String, _>("species_group"),
                "asset_path": row.get::<String, _>("asset_path"),
                "creature_state": row.get::<Option<String>, _>("creature_state"),
                "rabble_id": row.get::<Option<Uuid>, _>("rabble_id"),
                "rabble_name": row.get::<Option<String>, _>("rabble_name"),
                "location_lat": row.get::<f64, _>("location_lat"),
                "location_lng": row.get::<f64, _>("location_lng"),
                "distance_meters": row.get::<f64, _>("distance_meters"),
                "is_contact": row.get::<bool, _>("is_contact"),
            })
        })
        .collect();

    Ok(Json(json!({
        "creatures": creatures,
        "center_lat": q.lat,
        "center_lng": q.lng,
        "radius_meters": radius,
    })))
}
