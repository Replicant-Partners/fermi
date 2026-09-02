// src/agent_backend/tools/domains/rabble.rs
//
// Phase 2 domain migration: Rabble tools.
//
// Three tools:
//   mint_creature        — requires_workspace: true
//   activate_formation   — requires_workspace: false
//   scan_nearby_creatures — requires_workspace: false, is_llm_visible: false
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

/// All Rabble-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![
        Arc::new(MintCreature),
        Arc::new(ActivateFormation),
        Arc::new(ScanNearbyCreatures),
    ]
}

// ─── mint_creature ────────────────────────────────────────────────────────────

struct MintCreature;

#[async_trait]
impl PlatformTool for MintCreature {
    fn name(&self) -> &'static str {
        "mint_creature"
    }

    fn description(&self) -> &'static str {
        "Store a minted creature in the database. Creates the creature record with species data, asset path, variation notes, and generates a specimen name. Returns the creature ID and data card."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "scientific_name": {
                    "type": "string",
                    "description": "Scientific name of the species"
                },
                "common_name": {
                    "type": "string",
                    "description": "Common name (e.g. 'Red Admiral')"
                },
                "species_group": {
                    "type": "string",
                    "description": "Group: butterfly, dragonfly (default: butterfly)",
                    "default": "butterfly"
                },
                "gbif_key": {
                    "type": "integer",
                    "description": "GBIF species key for reference"
                },
                "taxonomy": {
                    "type": "object",
                    "description": "Full taxonomy object (kingdom through species)"
                },
                "asset_path": {
                    "type": "string",
                    "description": "Path to the specimen image in workspace files"
                },
                "flight_silhouette_path": {
                    "type": "string",
                    "description": "Path to the flight-pose image (optional)"
                },
                "specimen_name": {
                    "type": "string",
                    "description": "Unique name for this specimen (e.g. 'Twilight Admiral')"
                },
                "variation_notes": {
                    "type": "string",
                    "description": "Description of what makes this specimen unique"
                }
            },
            "required": ["scientific_name", "asset_path"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Rabble
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_mint_creature(input, ctx).await
    }
}

// ─── activate_formation ───────────────────────────────────────────────────────

struct ActivateFormation;

#[async_trait]
impl PlatformTool for ActivateFormation {
    fn name(&self) -> &'static str {
        "activate_formation"
    }

    fn description(&self) -> &'static str {
        "Activate a premium swarm formation algorithm for a rabble. Charges credits based on the algorithm's cost. Returns the formation spec JSON for client-side execution in the SwarmEngine. Idempotent: re-activating the same algorithm in the same session returns the spec without double-charging."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "algorithm_name": {
                    "type": "string",
                    "description": "Algorithm name (e.g. 'v_formation', 'echelon', 'encircle', 'patrol', 'search')"
                },
                "swarm_id": {
                    "type": "string",
                    "description": "Rabble/swarm session UUID"
                }
            },
            "required": ["algorithm_name", "swarm_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Rabble
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_activate_formation(input, ctx).await
    }
}

// ─── scan_nearby_creatures ────────────────────────────────────────────────────

struct ScanNearbyCreatures;

#[async_trait]
impl PlatformTool for ScanNearbyCreatures {
    fn name(&self) -> &'static str {
        "scan_nearby_creatures"
    }

    fn description(&self) -> &'static str {
        "Call this tool to find creatures near a given creature using H3 proximity. This tool is executed server-side against the live Rabble database — you do not need internet access to use it. Returns the target creature's species and all nearby creatures with taxonomy data."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "creature_id": {
                    "type": "string",
                    "description": "UUID of the creature to scan around"
                },
                "radius_rings": {
                    "type": "integer",
                    "description": "H3 grid ring radius (default: 1, i.e. 7 cells at res 12)",
                    "default": 1
                }
            },
            "required": ["creature_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Rabble
    }

    fn is_llm_visible(&self) -> bool {
        false
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_scan_nearby_creatures(input, ctx).await
    }
}

// ─── Private execute implementations ─────────────────────────────────────────

async fn execute_mint_creature(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = ctx
        .workspace_id
        .ok_or("mint_creature requires a workspace context")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("mint_creature requires a user context")?;

    let scientific_name = input
        .get("scientific_name")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: scientific_name")?;
    let asset_path = input
        .get("asset_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: asset_path")?;

    let common_name = input.get("common_name").and_then(|v| v.as_str());
    let species_group = input
        .get("species_group")
        .and_then(|v| v.as_str())
        .unwrap_or("butterfly");
    let gbif_key = input.get("gbif_key").and_then(|v| v.as_i64());
    let taxonomy = input
        .get("taxonomy")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let flight_silhouette_path = input.get("flight_silhouette_path").and_then(|v| v.as_str());
    let specimen_name = input.get("specimen_name").and_then(|v| v.as_str());
    let variation_notes = input.get("variation_notes").and_then(|v| v.as_str());

    let creature_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    // Generate a specimen name if not provided
    let final_specimen_name = specimen_name.map(|s| s.to_string()).unwrap_or_else(|| {
        let base = common_name.unwrap_or(scientific_name);
        format!("{} #{}", base, &creature_id.to_string()[..6])
    });

    let pool = ctx.memory_store.pool();
    sqlx::query(
        "INSERT INTO creatures (creature_id, owner_id, workspace_id,
         scientific_name, common_name, species_group, gbif_key,
         taxonomy, specimen_name, variation_notes,
         asset_path, flight_silhouette_path,
         created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)",
    )
    .bind(creature_id)
    .bind(user_id)
    .bind(workspace_id)
    .bind(scientific_name)
    .bind(common_name)
    .bind(species_group)
    .bind(gbif_key)
    .bind(&taxonomy)
    .bind(&final_specimen_name)
    .bind(variation_notes)
    .bind(asset_path)
    .bind(flight_silhouette_path)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to mint creature: {}", e))?;

    let result = serde_json::json!({
        "creature_id": creature_id,
        "specimen_name": final_specimen_name,
        "scientific_name": scientific_name,
        "common_name": common_name,
        "species_group": species_group,
        "gbif_key": gbif_key,
        "asset_path": asset_path,
        "variation_notes": variation_notes,
        "data_card": {
            "minted_at": now.to_rfc3339(),
            "minted_by": user_id,
            "workspace_id": workspace_id,
            "taxonomy": taxonomy,
        }
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_activate_formation(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let algorithm_name = input
        .get("algorithm_name")
        .and_then(|v| v.as_str())
        .ok_or("algorithm_name is required")?;
    let swarm_id_str = input
        .get("swarm_id")
        .and_then(|v| v.as_str())
        .ok_or("swarm_id is required")?;
    let swarm_id: uuid::Uuid = swarm_id_str
        .parse()
        .map_err(|_| "Invalid swarm_id UUID".to_string())?;

    let db = ctx.db.as_ref().ok_or("Database not available")?;
    let user_id = ctx
        .user_id
        .as_ref()
        .ok_or("User context required for formation activation")?;

    // Look up algorithm
    let algorithm = sqlx::query(
        "SELECT algorithm_id, name, display_name, formation_spec, tier, cost_credits \
         FROM swarm_algorithms WHERE name = $1",
    )
    .bind(algorithm_name)
    .fetch_optional(db)
    .await
    .map_err(|e| format!("DB error: {}", e))?
    .ok_or_else(|| format!("Algorithm '{}' not found", algorithm_name))?;

    let algorithm_id: uuid::Uuid = algorithm.get("algorithm_id");
    let display_name: String = algorithm.get("display_name");
    let formation_spec: serde_json::Value = algorithm.get("formation_spec");
    let tier: String = algorithm.get("tier");
    let cost: i32 = algorithm.get("cost_credits");

    // Free algorithms return spec directly
    if tier == "free" {
        let result = serde_json::json!({
            "algorithm_id": algorithm_id,
            "name": algorithm_name,
            "display_name": display_name,
            "formation_spec": formation_spec,
            "activated": true,
            "charged": false,
        });
        return serde_json::to_string_pretty(&result)
            .map_err(|e| format!("Serialization error: {}", e));
    }

    // Check idempotency
    let existing = sqlx::query(
        "SELECT activation_id FROM swarm_activations \
         WHERE user_id = $1 AND swarm_id = $2 AND algorithm_id = $3",
    )
    .bind(user_id)
    .bind(swarm_id)
    .bind(algorithm_id)
    .fetch_optional(db)
    .await
    .map_err(|e| format!("DB error: {}", e))?;

    if existing.is_some() {
        let result = serde_json::json!({
            "algorithm_id": algorithm_id,
            "name": algorithm_name,
            "display_name": display_name,
            "formation_spec": formation_spec,
            "activated": true,
            "charged": false,
            "message": "Already activated for this session",
        });
        return serde_json::to_string_pretty(&result)
            .map_err(|e| format!("Serialization error: {}", e));
    }

    // Charge credits
    let wallet = fermi_auth::get_or_create_wallet(db, "user", user_id)
        .await
        .map_err(|e| format!("Wallet error: {}", e))?;

    fermi_auth::credit_charge(
        db,
        wallet.wallet_id,
        cost,
        "formation_activate",
        &format!("Activate {} formation", display_name),
        Some(&algorithm_id.to_string()),
    )
    .await
    .map_err(|e| format!("Payment failed: {}", e))?;

    // Insert activation
    let activation_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO swarm_activations (activation_id, algorithm_id, user_id, swarm_id) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(activation_id)
    .bind(algorithm_id)
    .bind(user_id)
    .bind(swarm_id)
    .execute(db)
    .await
    .map_err(|e| format!("DB error: {}", e))?;

    let result = serde_json::json!({
        "algorithm_id": algorithm_id,
        "activation_id": activation_id,
        "name": algorithm_name,
        "display_name": display_name,
        "formation_spec": formation_spec,
        "activated": true,
        "charged": true,
        "cost_credits": cost,
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_scan_nearby_creatures(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    use h3o::CellIndex;

    let creature_id_str = input
        .get("creature_id")
        .and_then(|v| v.as_str())
        .ok_or("creature_id is required")?;
    let creature_id: Uuid = creature_id_str
        .parse()
        .map_err(|_| "Invalid creature_id UUID".to_string())?;
    let radius = input
        .get("radius_rings")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;

    let pool = ctx.memory_store.pool();

    // 1. Look up target creature's current state + species info
    //    LEFT JOIN creature_state — creature may not have a state row yet (pre-flight)
    //    Fallback: use latest creature_flights for location
    let target = sqlx::query(
        "SELECT c.creature_id, c.scientific_name, c.common_name, c.species_group,
                c.taxonomy,
                COALESCE(NULLIF(cs.h3_cell, ''), NULLIF(cf.h3_cell, '')) AS h3_cell,
                COALESCE(cs.location_lat, cf.center_lat) AS location_lat,
                COALESCE(cs.location_lng, cf.center_lng) AS location_lng,
                cs.rabble_id, cs.state
         FROM creatures c
         LEFT JOIN creature_state cs ON cs.creature_id = c.creature_id
         LEFT JOIN LATERAL (
             SELECT h3_cell, center_lat, center_lng FROM creature_flights
             WHERE creature_id = c.creature_id ORDER BY started_at DESC LIMIT 1
         ) cf ON true
         WHERE c.creature_id = $1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error: {}", e))?
    .ok_or("Creature not found")?;

    let h3_cell: Option<String> = target
        .try_get("h3_cell")
        .ok()
        .flatten()
        .filter(|s: &String| !s.is_empty());

    // Fallback: compute h3_cell from lat/lng if missing
    let h3_cell = match h3_cell {
        Some(c) => c,
        None => {
            let lat: Option<f64> = target.try_get("location_lat").ok().flatten();
            let lng: Option<f64> = target.try_get("location_lng").ok().flatten();
            match (lat, lng) {
                (Some(lat), Some(lng)) if lat != 0.0 || lng != 0.0 => {
                    use h3o::{LatLng, Resolution};
                    LatLng::new(lat, lng)
                        .map(|ll| ll.to_cell(Resolution::Twelve).to_string())
                        .map_err(|_| "Creature has no valid location".to_string())?
                }
                _ => return Err("Creature has no location — perch or fly first".to_string()),
            }
        }
    };

    let taxonomy: Option<serde_json::Value> = target.try_get("taxonomy").ok().flatten();
    let order = taxonomy
        .as_ref()
        .and_then(|t| t.get("order"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");
    let family = taxonomy
        .as_ref()
        .and_then(|t| t.get("family"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");

    let target_info = serde_json::json!({
        "creature_id": creature_id,
        "scientific_name": target.try_get::<Option<String>, _>("scientific_name").unwrap_or(None),
        "common_name": target.try_get::<Option<String>, _>("common_name").unwrap_or(None),
        "species_group": target.try_get::<Option<String>, _>("species_group").unwrap_or(None),
        "order": order,
        "family": family,
        "h3_cell": &h3_cell,
        "lat": target.try_get::<Option<f64>, _>("location_lat").unwrap_or(None),
        "lng": target.try_get::<Option<f64>, _>("location_lng").unwrap_or(None),
        "rabble_id": target.try_get::<Option<Uuid>, _>("rabble_id").ok().flatten(),
    });

    // 2. Compute H3 grid disk
    let center_cell: CellIndex = h3_cell
        .parse()
        .map_err(|e| format!("Invalid H3 cell '{}': {}", h3_cell, e))?;
    let disk: Vec<CellIndex> = center_cell.grid_disk::<Vec<_>>(radius);
    let cell_strings: Vec<String> = disk.iter().map(|c| c.to_string()).collect();

    // 3. Query nearby creatures (excluding target, excluding private)
    //    Use LATERAL fallback to creature_flights for creatures without creature_state
    let placeholders: Vec<String> = (1..=cell_strings.len())
        .map(|i| format!("${}", i))
        .collect();
    let in_clause = placeholders.join(", ");

    let sql = format!(
        "SELECT c.creature_id, c.scientific_name, c.common_name, c.species_group,
                c.taxonomy,
                COALESCE(NULLIF(cs.h3_cell, ''), NULLIF(cf.h3_cell, '')) AS h3_cell,
                cs.rabble_id,
                COALESCE(cc.visibility, 'public') AS visibility
         FROM creatures c
         LEFT JOIN creature_state cs ON cs.creature_id = c.creature_id
         LEFT JOIN LATERAL (
             SELECT h3_cell FROM creature_flights
             WHERE creature_id = c.creature_id ORDER BY started_at DESC LIMIT 1
         ) cf ON cs.h3_cell IS NULL
         LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
         WHERE COALESCE(NULLIF(cs.h3_cell, ''), NULLIF(cf.h3_cell, '')) IN ({})
           AND c.creature_id != ${}
           AND COALESCE(cc.visibility, 'public') != 'private'
         LIMIT 50",
        in_clause,
        cell_strings.len() + 1
    );

    let mut query = sqlx::query(&sql);
    for cs in &cell_strings {
        query = query.bind(cs);
    }
    query = query.bind(creature_id);

    let rows = query
        .fetch_all(pool)
        .await
        .map_err(|e| format!("DB error: {}", e))?;

    let target_rabble: Option<Uuid> = target
        .try_get::<Option<Uuid>, _>("rabble_id")
        .ok()
        .flatten();

    let nearby: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let tax: Option<serde_json::Value> = r.try_get("taxonomy").ok().flatten();
            let nearby_rabble: Option<Uuid> =
                r.try_get::<Option<Uuid>, _>("rabble_id").ok().flatten();
            let in_same_rabble = match (&target_rabble, &nearby_rabble) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            };
            serde_json::json!({
                "creature_id": r.get::<Uuid, _>("creature_id"),
                "scientific_name": r.try_get::<Option<String>, _>("scientific_name").unwrap_or(None),
                "common_name": r.try_get::<Option<String>, _>("common_name").unwrap_or(None),
                "species_group": r.try_get::<Option<String>, _>("species_group").unwrap_or(None),
                "order": tax.as_ref().and_then(|t| t.get("order")).and_then(|v| v.as_str()).unwrap_or("Unknown"),
                "family": tax.as_ref().and_then(|t| t.get("family")).and_then(|v| v.as_str()).unwrap_or("Unknown"),
                "h3_cell": r.try_get::<Option<String>, _>("h3_cell").unwrap_or(None),
                "in_same_rabble": in_same_rabble,
            })
        })
        .collect();

    let result = serde_json::json!({
        "target": target_info,
        "nearby_count": nearby.len(),
        "nearby": nearby,
        "radius_rings": radius,
        "cells_searched": cell_strings.len(),
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
    fn all_categories_are_rabble() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Rabble,
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
    fn tool_count_is_three() {
        assert_eq!(tools().len(), 3);
    }

    #[test]
    fn workspace_flags_are_correct() {
        let tools = tools();
        for tool in &tools {
            match tool.name() {
                "mint_creature" => {
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

    #[test]
    fn scan_nearby_creatures_is_not_llm_visible() {
        let tool = ScanNearbyCreatures;
        assert!(
            !tool.is_llm_visible(),
            "scan_nearby_creatures must not be visible to the LLM"
        );
    }
}
