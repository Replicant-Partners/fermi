//! AR Beacon API handlers — public, read-only endpoints for beacon discovery and asset serving.

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use h3o::{CellIndex, LatLng, Resolution};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use super::super::AppState;

#[derive(Deserialize)]
pub struct NearbyQuery {
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub h3_cell: Option<String>,
    pub radius: Option<u32>,
    pub resolution: Option<u8>,
}

/// GET /api/beacons/nearby?lat=X&lng=Y&radius=3&resolution=12
/// Returns active beacons near a location.
pub async fn nearby_beacons_handler(
    State(state): State<AppState>,
    Query(q): Query<NearbyQuery>,
) -> impl IntoResponse {
    let res_num = q.resolution.unwrap_or(12);
    let radius = q.radius.unwrap_or(3).min(10); // cap at 10 rings

    let resolution = match Resolution::try_from(res_num) {
        Ok(r) => r,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Invalid resolution (0-15)"})),
            )
        }
    };

    let center_cell = if let Some(ref cs) = q.h3_cell {
        match cs.parse::<CellIndex>() {
            Ok(c) => c,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "Invalid H3 cell ID"})),
                )
            }
        }
    } else if let (Some(lat), Some(lng)) = (q.lat, q.lng) {
        match LatLng::new(lat, lng) {
            Ok(ll) => ll.to_cell(resolution),
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "Invalid coordinates"})),
                )
            }
        }
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Provide lat+lng or h3_cell"})),
        );
    };

    let disk: Vec<CellIndex> = center_cell.grid_disk::<Vec<_>>(radius);
    let cell_strings: Vec<String> = disk.iter().map(|c| c.to_string()).collect();

    let placeholders: Vec<String> = (1..=cell_strings.len())
        .map(|i| format!("${}", i))
        .collect();
    let in_clause = placeholders.join(", ");
    let time_param = cell_strings.len() + 1;

    let sql = format!(
        "SELECT beacon_id, workspace_id, h3_cell, h3_resolution, center_lat, center_lng,
                asset_path, asset_type, azimuth_deg, elevation_deg, billboard, scale,
                ttl_seconds, decay_style, expires_at, visibility, tags, interaction,
                created_at
         FROM ar_beacons
         WHERE h3_cell IN ({}) AND expires_at > ${} AND visibility = 'public'
         ORDER BY created_at DESC LIMIT 100",
        in_clause, time_param
    );

    let mut query = sqlx::query(&sql);
    for cs in &cell_strings {
        query = query.bind(cs);
    }
    query = query.bind(chrono::Utc::now());

    match query.fetch_all(&state.db).await {
        Ok(rows) => {
            let beacons: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    json!({
                        "beacon_id": row.get::<Uuid, _>("beacon_id"),
                        "h3_cell": row.get::<String, _>("h3_cell"),
                        "h3_resolution": row.get::<i32, _>("h3_resolution"),
                        "center_lat": row.get::<f64, _>("center_lat"),
                        "center_lng": row.get::<f64, _>("center_lng"),
                        "asset_url": format!("/api/beacons/{}/asset", row.get::<Uuid, _>("beacon_id")),
                        "asset_type": row.get::<String, _>("asset_type"),
                        "orientation": {
                            "azimuth_deg": row.get::<f64, _>("azimuth_deg"),
                            "elevation_deg": row.get::<f64, _>("elevation_deg"),
                            "billboard": row.get::<bool, _>("billboard"),
                        },
                        "scale": row.get::<f64, _>("scale"),
                        "decay_style": row.get::<String, _>("decay_style"),
                        "expires_at": row.get::<chrono::DateTime<chrono::Utc>, _>("expires_at").to_rfc3339(),
                        "tags": row.get::<serde_json::Value, _>("tags"),
                        "interaction": row.get::<serde_json::Value, _>("interaction"),
                        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                    })
                })
                .collect();

            (
                StatusCode::OK,
                Json(json!({
                    "center": center_cell.to_string(),
                    "radius_rings": radius,
                    "resolution": res_num,
                    "count": beacons.len(),
                    "beacons": beacons,
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Query failed: {}", e)})),
        ),
    }
}

/// GET /api/beacons/:beacon_id — Get a single beacon record
pub async fn get_beacon_handler(
    State(state): State<AppState>,
    Path(beacon_id): Path<Uuid>,
) -> impl IntoResponse {
    let row = sqlx::query(
        "SELECT beacon_id, workspace_id, h3_cell, h3_resolution, center_lat, center_lng,
                asset_path, asset_type, azimuth_deg, elevation_deg, billboard, scale,
                ttl_seconds, decay_style, expires_at, visibility, tags, interaction,
                metadata, created_at
         FROM ar_beacons WHERE beacon_id = $1",
    )
    .bind(beacon_id)
    .fetch_optional(&state.db)
    .await;

    match row {
        Ok(Some(row)) => {
            let visibility: String = row.get("visibility");
            if visibility != "public" {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({"error": "Beacon is not public"})),
                );
            }

            (
                StatusCode::OK,
                Json(json!({
                    "beacon_id": row.get::<Uuid, _>("beacon_id"),
                    "workspace_id": row.get::<Uuid, _>("workspace_id"),
                    "h3_cell": row.get::<String, _>("h3_cell"),
                    "h3_resolution": row.get::<i32, _>("h3_resolution"),
                    "center_lat": row.get::<f64, _>("center_lat"),
                    "center_lng": row.get::<f64, _>("center_lng"),
                    "asset_url": format!("/api/beacons/{}/asset", beacon_id),
                    "asset_type": row.get::<String, _>("asset_type"),
                    "orientation": {
                        "azimuth_deg": row.get::<f64, _>("azimuth_deg"),
                        "elevation_deg": row.get::<f64, _>("elevation_deg"),
                        "billboard": row.get::<bool, _>("billboard"),
                    },
                    "scale": row.get::<f64, _>("scale"),
                    "ttl_seconds": row.get::<i32, _>("ttl_seconds"),
                    "decay_style": row.get::<String, _>("decay_style"),
                    "expires_at": row.get::<chrono::DateTime<chrono::Utc>, _>("expires_at").to_rfc3339(),
                    "tags": row.get::<serde_json::Value, _>("tags"),
                    "interaction": row.get::<serde_json::Value, _>("interaction"),
                    "metadata": row.get::<serde_json::Value, _>("metadata"),
                    "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                })),
            )
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Beacon not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Query failed: {}", e)})),
        ),
    }
}

/// GET /api/beacons/:beacon_id/asset — Serve the beacon's asset file from workspace git
pub async fn beacon_asset_handler(
    State(state): State<AppState>,
    Path(beacon_id): Path<Uuid>,
) -> impl IntoResponse {
    // Look up the beacon to get workspace_id and asset_path
    let row = sqlx::query(
        "SELECT b.workspace_id, b.asset_path, b.visibility, w.slug
         FROM ar_beacons b
         JOIN workspaces w ON w.workspace_id = b.workspace_id
         WHERE b.beacon_id = $1",
    )
    .bind(beacon_id)
    .fetch_optional(&state.db)
    .await;

    let row = match row {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Beacon not found").into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("DB error: {}", e),
            )
                .into_response();
        }
    };

    let visibility: String = row.get("visibility");
    if visibility != "public" {
        return (StatusCode::FORBIDDEN, "Beacon is not public").into_response();
    }

    let slug: String = row.get("slug");
    let asset_path: String = row.get("asset_path");
    let asset_path_for_type = asset_path.clone();

    // Read from workspace git
    let git = state.workspace_git.clone();
    match tokio::task::spawn_blocking(move || git.read_file_bytes(&slug, &asset_path)).await {
        Ok(Ok(bytes)) => {
            // Guess content type from path
            let content_type = if asset_path_for_type.ends_with(".png") {
                "image/png"
            } else if asset_path_for_type.ends_with(".jpg")
                || asset_path_for_type.ends_with(".jpeg")
            {
                "image/jpeg"
            } else if asset_path_for_type.ends_with(".webp") {
                "image/webp"
            } else if asset_path_for_type.ends_with(".gif") {
                "image/gif"
            } else if asset_path_for_type.ends_with(".glb") {
                "model/gltf-binary"
            } else if asset_path_for_type.ends_with(".gltf") {
                "model/gltf+json"
            } else if asset_path_for_type.ends_with(".mp4") {
                "video/mp4"
            } else {
                "application/octet-stream"
            };

            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, content_type),
                    (header::CACHE_CONTROL, "public, max-age=3600"),
                ],
                bytes,
            )
                .into_response()
        }
        Ok(Err(e)) => (StatusCode::NOT_FOUND, format!("Asset not found: {}", e)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Read error: {}", e),
        )
            .into_response(),
    }
}

/// GET /api/grid-maps/:map_id — Get a grid map definition
pub async fn get_grid_map_handler(
    State(state): State<AppState>,
    Path(map_id): Path<Uuid>,
) -> impl IntoResponse {
    let row = sqlx::query(
        "SELECT map_id, workspace_id, name, description,
                center_lat, center_lng, center_h3, center_resolution,
                grid_resolution, radius_rings, total_cells,
                quadrants, zones, metadata, created_at, updated_at
         FROM ar_grid_maps WHERE map_id = $1",
    )
    .bind(map_id)
    .fetch_optional(&state.db)
    .await;

    match row {
        Ok(Some(row)) => (
            StatusCode::OK,
            Json(json!({
                "map_id": row.get::<Uuid, _>("map_id"),
                "workspace_id": row.get::<Uuid, _>("workspace_id"),
                "name": row.get::<String, _>("name"),
                "description": row.get::<Option<String>, _>("description"),
                "center": {
                    "lat": row.get::<f64, _>("center_lat"),
                    "lng": row.get::<f64, _>("center_lng"),
                    "h3_cell": row.get::<String, _>("center_h3"),
                    "resolution": row.get::<i32, _>("center_resolution"),
                },
                "grid_resolution": row.get::<i32, _>("grid_resolution"),
                "radius_rings": row.get::<i32, _>("radius_rings"),
                "total_cells": row.get::<i32, _>("total_cells"),
                "quadrants": row.get::<serde_json::Value, _>("quadrants"),
                "zones": row.get::<serde_json::Value, _>("zones"),
                "metadata": row.get::<serde_json::Value, _>("metadata"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
            })),
        )
            .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Grid map not found"})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Query failed: {}", e)})),
        )
            .into_response(),
    }
}
