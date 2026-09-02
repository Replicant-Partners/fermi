// src/agent_backend/tools/domains/spatial.rs
//
// Phase 2 domain migration: Spatial tools.
//
// Five tools:
//   h3_resolve        — requires_workspace: false
//   geocode           — requires_workspace: false
//   create_beacon     — requires_workspace: true
//   query_beacons     — requires_workspace: false
//   save_grid_map     — requires_workspace: true
//
// Each is a zero-size struct implementing PlatformTool. execute() calls the
// implementation directly without going through ToolRegistry dispatch.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;
use sqlx::Row;
use uuid::Uuid;

/// All Spatial-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![
        Arc::new(H3Resolve),
        Arc::new(Geocode),
        Arc::new(CreateBeacon),
        Arc::new(QueryBeacons),
        Arc::new(SaveGridMap),
    ]
}

// ─── h3_resolve ───────────────────────────────────────────────────────────────

struct H3Resolve;

#[async_trait]
impl PlatformTool for H3Resolve {
    fn name(&self) -> &'static str {
        "h3_resolve"
    }

    fn description(&self) -> &'static str {
        "Convert GPS coordinates to an H3 hexagonal grid cell ID, or convert an H3 cell ID back to GPS coordinates. Also computes k-ring neighbors and grid distance between cells. The foundation for all AR spatial operations."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["gps_to_h3", "h3_to_gps", "neighbors", "distance", "grid_disk"],
                    "description": "Operation: gps_to_h3 (lat/lng→cell), h3_to_gps (cell→lat/lng), neighbors (6 adjacent cells), distance (grid distance between 2 cells), grid_disk (all cells within k rings)"
                },
                "lat": {
                    "type": "number",
                    "description": "Latitude in decimal degrees (for gps_to_h3)"
                },
                "lng": {
                    "type": "number",
                    "description": "Longitude in decimal degrees (for gps_to_h3)"
                },
                "h3_cell": {
                    "type": "string",
                    "description": "H3 cell ID (for h3_to_gps, neighbors, distance)"
                },
                "h3_cell_b": {
                    "type": "string",
                    "description": "Second H3 cell ID (for distance operation)"
                },
                "resolution": {
                    "type": "integer",
                    "description": "H3 resolution 0-15 (default: 12, ~9m² hexes). Higher = more precise.",
                    "default": 12
                },
                "k": {
                    "type": "integer",
                    "description": "Ring count for grid_disk (default: 1). Total cells = 3k²+3k+1",
                    "default": 1
                }
            },
            "required": ["operation"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Spatial
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        execute_h3_resolve(input).await
    }
}

// ─── geocode ──────────────────────────────────────────────────────────────────

struct Geocode;

#[async_trait]
impl PlatformTool for Geocode {
    fn name(&self) -> &'static str {
        "geocode"
    }

    fn description(&self) -> &'static str {
        "Convert a street address or place name to GPS coordinates (lat/lng) using OpenStreetMap Nominatim. Free, no API key required. Rate limited to 1 request per second."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "address": {
                    "type": "string",
                    "description": "Street address or place name to geocode (e.g. '221B Baker Street, London' or 'Eiffel Tower')"
                }
            },
            "required": ["address"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Spatial
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        execute_geocode(input).await
    }
}

// ─── create_beacon ────────────────────────────────────────────────────────────

struct CreateBeacon;

#[async_trait]
impl PlatformTool for CreateBeacon {
    fn name(&self) -> &'static str {
        "create_beacon"
    }

    fn description(&self) -> &'static str {
        "Create an AR beacon — place an AR asset at a physical location. Stores the beacon in the database with H3 cell, orientation, TTL, and interaction triggers. Returns the beacon record with its public asset URL."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "lat": {
                    "type": "number",
                    "description": "Latitude of placement"
                },
                "lng": {
                    "type": "number",
                    "description": "Longitude of placement"
                },
                "resolution": {
                    "type": "integer",
                    "description": "H3 resolution (default: 12)",
                    "default": 12
                },
                "asset_path": {
                    "type": "string",
                    "description": "Path to asset in workspace files (e.g. 'ar_assets/portal.png')"
                },
                "asset_type": {
                    "type": "string",
                    "description": "Asset type: image, model, video (default: image)",
                    "default": "image"
                },
                "azimuth_deg": {
                    "type": "number",
                    "description": "Compass bearing the asset faces, 0-360 (default: 0 = North)",
                    "default": 0
                },
                "elevation_deg": {
                    "type": "number",
                    "description": "Vertical tilt, -90 to 90 (default: 0 = eye level)",
                    "default": 0
                },
                "billboard": {
                    "type": "boolean",
                    "description": "If true, asset always faces the viewer (default: true)",
                    "default": true
                },
                "scale": {
                    "type": "number",
                    "description": "Scale factor (default: 1.0)",
                    "default": 1.0
                },
                "ttl_seconds": {
                    "type": "integer",
                    "description": "Time-to-live in seconds (default: 86400 = 24 hours)",
                    "default": 86400
                },
                "decay_style": {
                    "type": "string",
                    "description": "Decay style: fade, dissolve, instant, loop_decay (default: fade)",
                    "default": "fade"
                },
                "visibility": {
                    "type": "string",
                    "description": "Visibility: public, private, workspace (default: public)",
                    "default": "public"
                },
                "tags": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Tags for the beacon"
                },
                "interaction": {
                    "type": "object",
                    "description": "Interaction triggers: on_gaze, on_tap, on_proximity, on_dwell"
                }
            },
            "required": ["lat", "lng", "asset_path"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Spatial
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_create_beacon(input, ctx).await
    }
}

// ─── query_beacons ────────────────────────────────────────────────────────────

struct QueryBeacons;

#[async_trait]
impl PlatformTool for QueryBeacons {
    fn name(&self) -> &'static str {
        "query_beacons"
    }

    fn description(&self) -> &'static str {
        "Query AR beacons near a location. Returns all active (non-expired) beacons within k rings of the specified H3 cell. Used by renderers to discover nearby AR content."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "lat": {
                    "type": "number",
                    "description": "Latitude to search around"
                },
                "lng": {
                    "type": "number",
                    "description": "Longitude to search around"
                },
                "h3_cell": {
                    "type": "string",
                    "description": "H3 cell to search around (alternative to lat/lng)"
                },
                "radius_rings": {
                    "type": "integer",
                    "description": "Search radius in H3 rings (default: 3)",
                    "default": 3
                },
                "resolution": {
                    "type": "integer",
                    "description": "H3 resolution (default: 12)",
                    "default": 12
                },
                "include_expired": {
                    "type": "boolean",
                    "description": "Include expired beacons (default: false)",
                    "default": false
                }
            },
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Spatial
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_query_beacons(input, ctx).await
    }
}

// ─── save_grid_map ────────────────────────────────────────────────────────────

struct SaveGridMap;

#[async_trait]
impl PlatformTool for SaveGridMap {
    fn name(&self) -> &'static str {
        "save_grid_map"
    }

    fn description(&self) -> &'static str {
        "Save or update an AR grid map — a named spatial grid with quadrants and zones. Used by ar_cartographer to persist grid definitions to the database."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Human-readable name for the grid map"
                },
                "description": {
                    "type": "string",
                    "description": "Description of the space"
                },
                "center_lat": {
                    "type": "number",
                    "description": "Center latitude"
                },
                "center_lng": {
                    "type": "number",
                    "description": "Center longitude"
                },
                "grid_resolution": {
                    "type": "integer",
                    "description": "H3 resolution for placement grid (default: 12)",
                    "default": 12
                },
                "radius_rings": {
                    "type": "integer",
                    "description": "Grid radius in rings (default: 5)",
                    "default": 5
                },
                "quadrants": {
                    "type": "array",
                    "description": "Named quadrant definitions [{h3_cell, name, description, tags, color}]"
                },
                "zones": {
                    "type": "array",
                    "description": "Zone groupings [{name, description, quadrants: [names], color}]"
                }
            },
            "required": ["name", "center_lat", "center_lng"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Spatial
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_save_grid_map(input, ctx).await
    }
}

// ─── Private execute implementations ─────────────────────────────────────────

async fn execute_h3_resolve(input: &serde_json::Value) -> Result<String, String> {
    use h3o::{CellIndex, LatLng, Resolution};

    let operation = input
        .get("operation")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: operation")?;

    let parse_resolution = |input: &serde_json::Value| -> Result<Resolution, String> {
        let res = input
            .get("resolution")
            .and_then(|v| v.as_u64())
            .unwrap_or(12) as u8;
        Resolution::try_from(res).map_err(|_| format!("Invalid resolution: {}. Must be 0-15.", res))
    };

    let parse_cell = |s: &str| -> Result<CellIndex, String> {
        s.parse::<CellIndex>()
            .map_err(|e| format!("Invalid H3 cell '{}': {}", s, e))
    };

    match operation {
        "gps_to_h3" => {
            let lat = input
                .get("lat")
                .and_then(|v| v.as_f64())
                .ok_or("gps_to_h3 requires 'lat'")?;
            let lng = input
                .get("lng")
                .and_then(|v| v.as_f64())
                .ok_or("gps_to_h3 requires 'lng'")?;
            let resolution = parse_resolution(input)?;

            let ll = LatLng::new(lat, lng).map_err(|e| format!("Invalid coordinates: {}", e))?;
            let cell = ll.to_cell(resolution);
            let center = LatLng::from(cell);

            let result = serde_json::json!({
                "h3_cell": cell.to_string(),
                "resolution": u8::from(resolution),
                "center_lat": f64::from(center.lat()),
                "center_lng": f64::from(center.lng()),
                "input_lat": lat,
                "input_lng": lng,
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        "h3_to_gps" => {
            let cell_str = input
                .get("h3_cell")
                .and_then(|v| v.as_str())
                .ok_or("h3_to_gps requires 'h3_cell'")?;
            let cell = parse_cell(cell_str)?;
            let center = LatLng::from(cell);

            let result = serde_json::json!({
                "h3_cell": cell.to_string(),
                "resolution": u8::from(cell.resolution()),
                "lat": f64::from(center.lat()),
                "lng": f64::from(center.lng()),
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        "neighbors" => {
            let cell_str = input
                .get("h3_cell")
                .and_then(|v| v.as_str())
                .ok_or("neighbors requires 'h3_cell'")?;
            let cell = parse_cell(cell_str)?;

            // grid_disk(1) returns center + 6 neighbors
            let disk: Vec<CellIndex> = cell.grid_disk::<Vec<_>>(1);
            let neighbors: Vec<serde_json::Value> = disk
                .iter()
                .filter(|c| **c != cell)
                .map(|c| {
                    let ll = LatLng::from(*c);
                    serde_json::json!({
                        "h3_cell": c.to_string(),
                        "lat": f64::from(ll.lat()),
                        "lng": f64::from(ll.lng()),
                    })
                })
                .collect();

            let result = serde_json::json!({
                "center": cell.to_string(),
                "neighbors": neighbors,
                "count": neighbors.len(),
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        "distance" => {
            let cell_a_str = input
                .get("h3_cell")
                .and_then(|v| v.as_str())
                .ok_or("distance requires 'h3_cell'")?;
            let cell_b_str = input
                .get("h3_cell_b")
                .and_then(|v| v.as_str())
                .ok_or("distance requires 'h3_cell_b'")?;
            let cell_a = parse_cell(cell_a_str)?;
            let cell_b = parse_cell(cell_b_str)?;

            let distance = cell_a
                .grid_distance(cell_b)
                .map_err(|_| "Cannot compute distance between cells at different resolutions or too far apart")?;

            let result = serde_json::json!({
                "cell_a": cell_a.to_string(),
                "cell_b": cell_b.to_string(),
                "grid_distance": distance,
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        "grid_disk" => {
            let lat = input.get("lat").and_then(|v| v.as_f64());
            let lng = input.get("lng").and_then(|v| v.as_f64());
            let cell_str = input.get("h3_cell").and_then(|v| v.as_str());
            let k = input.get("k").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let resolution = parse_resolution(input)?;

            let center_cell = if let Some(cs) = cell_str {
                parse_cell(cs)?
            } else if let (Some(lat), Some(lng)) = (lat, lng) {
                let ll = LatLng::new(lat, lng)
                    .map_err(|e| format!("Invalid coordinates: {}", e))?;
                ll.to_cell(resolution)
            } else {
                return Err("grid_disk requires either 'h3_cell' or 'lat'+'lng'".to_string());
            };

            let disk: Vec<CellIndex> = center_cell.grid_disk::<Vec<_>>(k);
            let cells: Vec<serde_json::Value> = disk
                .iter()
                .map(|c| {
                    let ll = LatLng::from(*c);
                    serde_json::json!({
                        "h3_cell": c.to_string(),
                        "lat": f64::from(ll.lat()),
                        "lng": f64::from(ll.lng()),
                    })
                })
                .collect();

            let total = 3 * k * k + 3 * k + 1;
            let result = serde_json::json!({
                "center": center_cell.to_string(),
                "k": k,
                "resolution": u8::from(resolution),
                "total_cells": total,
                "cells": cells,
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        other => Err(format!(
            "Unknown h3_resolve operation: '{}'. Use: gps_to_h3, h3_to_gps, neighbors, distance, grid_disk",
            other
        )),
    }
}

async fn execute_geocode(input: &serde_json::Value) -> Result<String, String> {
    let address = input
        .get("address")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: address")?;

    let client = reqwest::Client::new();
    let response = client
        .get("https://nominatim.openstreetmap.org/search")
        .query(&[
            ("q", address),
            ("format", "json"),
            ("limit", "3"),
            ("addressdetails", "1"),
        ])
        .header("User-Agent", "AgentBestiary/1.0 (AR Spatial Suite)")
        .send()
        .await
        .map_err(|e| format!("Geocoding request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Nominatim error: {}", response.status()));
    }

    let results: Vec<serde_json::Value> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse geocoding response: {}", e))?;

    if results.is_empty() {
        return Ok(serde_json::json!({
            "status": "not_found",
            "message": format!("No results for '{}'. Try a more specific address or use GPS coordinates directly.", address)
        }).to_string());
    }

    let formatted: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            let lat: f64 = r
                .get("lat")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            let lng: f64 = r
                .get("lon")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);

            serde_json::json!({
                "lat": lat,
                "lng": lng,
                "display_name": r.get("display_name").and_then(|v| v.as_str()).unwrap_or(""),
                "type": r.get("type").and_then(|v| v.as_str()).unwrap_or(""),
                "importance": r.get("importance").and_then(|v| v.as_f64()).unwrap_or(0.0),
            })
        })
        .collect();

    let result = serde_json::json!({
        "query": address,
        "results": formatted,
        "best_match": formatted.first(),
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_create_beacon(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    use h3o::{LatLng, Resolution};

    let workspace_id = ctx
        .workspace_id
        .ok_or("create_beacon requires a workspace context")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("create_beacon requires a user context")?;

    let lat = input
        .get("lat")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: lat")?;
    let lng = input
        .get("lng")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: lng")?;
    let asset_path = input
        .get("asset_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: asset_path")?;

    let res_num = input
        .get("resolution")
        .and_then(|v| v.as_u64())
        .unwrap_or(12) as u8;
    let resolution =
        Resolution::try_from(res_num).map_err(|_| format!("Invalid resolution: {}", res_num))?;

    let ll = LatLng::new(lat, lng).map_err(|e| format!("Invalid coordinates: {}", e))?;
    let cell = ll.to_cell(resolution);
    let center = LatLng::from(cell);

    let asset_type = input
        .get("asset_type")
        .and_then(|v| v.as_str())
        .unwrap_or("image");
    let azimuth = input
        .get("azimuth_deg")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let elevation = input
        .get("elevation_deg")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let billboard = input
        .get("billboard")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let scale = input.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let ttl_seconds = input
        .get("ttl_seconds")
        .and_then(|v| v.as_i64())
        .unwrap_or(86400) as i32;
    let decay_style = input
        .get("decay_style")
        .and_then(|v| v.as_str())
        .unwrap_or("fade");
    let visibility = input
        .get("visibility")
        .and_then(|v| v.as_str())
        .unwrap_or("public");
    let tags = input.get("tags").cloned().unwrap_or(serde_json::json!([]));
    let interaction = input
        .get("interaction")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::seconds(ttl_seconds as i64);
    let beacon_id = Uuid::new_v4();

    let pool = ctx.memory_store.pool();
    sqlx::query(
        "INSERT INTO ar_beacons (beacon_id, workspace_id, creator_id, agent_name,
         h3_cell, h3_resolution, center_lat, center_lng,
         asset_path, asset_type,
         azimuth_deg, elevation_deg, billboard, scale,
         ttl_seconds, decay_style, expires_at,
         visibility, tags, interaction, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $21)"
    )
    .bind(beacon_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind("ar_beacon")
    .bind(cell.to_string())
    .bind(res_num as i32)
    .bind(f64::from(center.lat()))
    .bind(f64::from(center.lng()))
    .bind(asset_path)
    .bind(asset_type)
    .bind(azimuth)
    .bind(elevation)
    .bind(billboard)
    .bind(scale)
    .bind(ttl_seconds)
    .bind(decay_style)
    .bind(expires_at)
    .bind(visibility)
    .bind(&tags)
    .bind(&interaction)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create beacon: {}", e))?;

    let result = serde_json::json!({
        "beacon_id": beacon_id,
        "h3_cell": cell.to_string(),
        "h3_resolution": res_num,
        "center_lat": f64::from(center.lat()),
        "center_lng": f64::from(center.lng()),
        "asset_path": asset_path,
        "asset_url": format!("/api/beacons/{}/asset", beacon_id),
        "expires_at": expires_at.to_rfc3339(),
        "ttl_seconds": ttl_seconds,
        "decay_style": decay_style,
        "visibility": visibility,
        "orientation": {
            "azimuth_deg": azimuth,
            "elevation_deg": elevation,
            "billboard": billboard,
        },
        "scale": scale,
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_query_beacons(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    use h3o::{CellIndex, LatLng, Resolution};

    let radius = input
        .get("radius_rings")
        .and_then(|v| v.as_u64())
        .unwrap_or(3) as u32;
    let res_num = input
        .get("resolution")
        .and_then(|v| v.as_u64())
        .unwrap_or(12) as u8;
    let include_expired = input
        .get("include_expired")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let resolution =
        Resolution::try_from(res_num).map_err(|_| format!("Invalid resolution: {}", res_num))?;

    // Resolve center cell from h3_cell or lat/lng
    let center_cell = if let Some(cs) = input.get("h3_cell").and_then(|v| v.as_str()) {
        cs.parse::<CellIndex>()
            .map_err(|e| format!("Invalid H3 cell: {}", e))?
    } else {
        let lat = input
            .get("lat")
            .and_then(|v| v.as_f64())
            .ok_or("query_beacons requires 'h3_cell' or 'lat'+'lng'")?;
        let lng = input
            .get("lng")
            .and_then(|v| v.as_f64())
            .ok_or("query_beacons requires 'lng'")?;
        let ll = LatLng::new(lat, lng).map_err(|e| format!("Invalid coordinates: {}", e))?;
        ll.to_cell(resolution)
    };

    // Compute all cells in the search radius
    let disk: Vec<CellIndex> = center_cell.grid_disk::<Vec<_>>(radius);
    let cell_strings: Vec<String> = disk.iter().map(|c| c.to_string()).collect();

    let pool = ctx.memory_store.pool();

    // Build query with IN clause for H3 cells
    let placeholders: Vec<String> = (1..=cell_strings.len())
        .map(|i| format!("${}", i))
        .collect();
    let in_clause = placeholders.join(", ");

    let time_filter = if include_expired {
        "".to_string()
    } else {
        format!(" AND expires_at > ${}", cell_strings.len() + 1)
    };

    let sql = format!(
        "SELECT beacon_id, workspace_id, h3_cell, h3_resolution, center_lat, center_lng,
                asset_path, asset_type, azimuth_deg, elevation_deg, billboard, scale,
                ttl_seconds, decay_style, expires_at, visibility, tags, interaction,
                created_at
         FROM ar_beacons WHERE h3_cell IN ({}){}\n         ORDER BY created_at DESC LIMIT 100",
        in_clause, time_filter
    );

    let mut query = sqlx::query(&sql);
    for cs in &cell_strings {
        query = query.bind(cs);
    }
    if !include_expired {
        query = query.bind(chrono::Utc::now());
    }

    let rows = query
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Beacon query failed: {}", e))?;

    let beacons: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "beacon_id": row.get::<Uuid, _>("beacon_id"),
                "workspace_id": row.get::<Uuid, _>("workspace_id"),
                "h3_cell": row.get::<String, _>("h3_cell"),
                "center_lat": row.get::<f64, _>("center_lat"),
                "center_lng": row.get::<f64, _>("center_lng"),
                "asset_path": row.get::<String, _>("asset_path"),
                "asset_type": row.get::<String, _>("asset_type"),
                "asset_url": format!("/api/beacons/{}/asset", row.get::<Uuid, _>("beacon_id")),
                "orientation": {
                    "azimuth_deg": row.get::<f64, _>("azimuth_deg"),
                    "elevation_deg": row.get::<f64, _>("elevation_deg"),
                    "billboard": row.get::<bool, _>("billboard"),
                },
                "scale": row.get::<f64, _>("scale"),
                "expires_at": row.get::<chrono::DateTime<chrono::Utc>, _>("expires_at").to_rfc3339(),
                "visibility": row.get::<String, _>("visibility"),
                "tags": row.get::<serde_json::Value, _>("tags"),
                "interaction": row.get::<serde_json::Value, _>("interaction"),
            })
        })
        .collect();

    let result = serde_json::json!({
        "center": center_cell.to_string(),
        "radius_rings": radius,
        "total_beacons": beacons.len(),
        "beacons": beacons,
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_save_grid_map(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    use h3o::{LatLng, Resolution};

    let workspace_id = ctx
        .workspace_id
        .ok_or("save_grid_map requires a workspace context")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("save_grid_map requires a user context")?;

    let name = input
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: name")?;
    let center_lat = input
        .get("center_lat")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: center_lat")?;
    let center_lng = input
        .get("center_lng")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: center_lng")?;

    let description = input.get("description").and_then(|v| v.as_str());
    let grid_res = input
        .get("grid_resolution")
        .and_then(|v| v.as_u64())
        .unwrap_or(12) as u8;
    let radius_rings = input
        .get("radius_rings")
        .and_then(|v| v.as_i64())
        .unwrap_or(5) as i32;
    let quadrants = input
        .get("quadrants")
        .cloned()
        .unwrap_or(serde_json::json!([]));
    let zones = input.get("zones").cloned().unwrap_or(serde_json::json!([]));

    let resolution =
        Resolution::try_from(grid_res).map_err(|_| format!("Invalid resolution: {}", grid_res))?;
    // Center resolution is 3 levels above grid resolution (or 0 if grid_res < 3)
    let center_res_num = if grid_res >= 3 { grid_res - 3 } else { 0 };
    let center_resolution = Resolution::try_from(center_res_num)
        .map_err(|_| format!("Invalid center resolution: {}", center_res_num))?;

    let ll =
        LatLng::new(center_lat, center_lng).map_err(|e| format!("Invalid coordinates: {}", e))?;
    let center_cell = ll.to_cell(center_resolution);

    let k = radius_rings as u32;
    let total_cells = (3 * k * k + 3 * k + 1) as i32;
    let map_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let pool = ctx.memory_store.pool();
    sqlx::query(
        "INSERT INTO ar_grid_maps (map_id, workspace_id, creator_id, name, description,
         center_lat, center_lng, center_h3, center_resolution,
         grid_resolution, radius_rings, total_cells,
         quadrants, zones, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $15)",
    )
    .bind(map_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(name)
    .bind(description)
    .bind(center_lat)
    .bind(center_lng)
    .bind(center_cell.to_string())
    .bind(center_res_num as i32)
    .bind(grid_res as i32)
    .bind(radius_rings)
    .bind(total_cells)
    .bind(&quadrants)
    .bind(&zones)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to save grid map: {}", e))?;

    let result = serde_json::json!({
        "map_id": map_id,
        "name": name,
        "center_h3": center_cell.to_string(),
        "center_resolution": center_res_num,
        "grid_resolution": grid_res,
        "radius_rings": radius_rings,
        "total_cells": total_cells,
        "quadrants_count": quadrants.as_array().map(|a| a.len()).unwrap_or(0),
        "zones_count": zones.as_array().map(|a| a.len()).unwrap_or(0),
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_names_are_dispatchable() {
        for tool in tools() {
            assert!(!tool.name().is_empty(), "tool has empty name");
        }
    }

    #[test]
    fn all_categories_are_spatial() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Spatial,
                "tool `{}` has wrong category",
                tool.name()
            );
        }
    }

    #[test]
    fn input_schemas_are_objects() {
        for tool in tools() {
            let schema = tool.input_schema();
            assert_eq!(
                schema["type"],
                "object",
                "tool `{}` input_schema missing \"type\": \"object\"",
                tool.name()
            );
        }
    }

    #[test]
    fn tool_count_is_five() {
        assert_eq!(tools().len(), 5);
    }

    #[test]
    fn workspace_flags_are_correct() {
        let tools = tools();
        for tool in &tools {
            match tool.name() {
                "create_beacon" | "save_grid_map" => {
                    assert!(
                        tool.requires_workspace(),
                        "tool `{}` should require workspace",
                        tool.name()
                    );
                }
                _ => {
                    assert!(
                        !tool.requires_workspace(),
                        "tool `{}` should NOT require workspace",
                        tool.name()
                    );
                }
            }
        }
    }
}
