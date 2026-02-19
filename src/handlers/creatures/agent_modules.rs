//! Agent-powered creature modules — enemy sensor, genome profiler, prey locator, dream.
//!
//! These are premium features that dispatch to LLM agents and cost gas credits.

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

use crate::handlers::rabble_workspace;
use crate::AppState;
use fermi::gas::charge_gas;
use fermi_auth::{get_or_create_wallet, AuthPrincipal};

use super::helpers::{find_creature_workspace, get_current_state, record_transition, toggle_module};

#[derive(Deserialize)]
pub struct EnemySensorRequest {
    pub action: String,         // "enable" | "disable" | "check" | "strategy"
    pub prompt: Option<String>, // custom prompt for "strategy" action
}

/// POST /api/creatures/:creature_id/enemy-sensor
pub async fn enemy_sensor_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<EnemySensorRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();
    let gas = &state.gas_fees;

    // Verify ownership
    let creature = sqlx::query(
        "SELECT c.owner_id, c.scientific_name, c.common_name, c.species_group, c.workspace_id,
                c.taxonomy
         FROM creatures c WHERE c.creature_id = $1",
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

    match req.action.as_str() {
        "enable" => {
            // One-time unlock fee — check ledger for prior purchase
            let already_paid: bool = sqlx::query(
                "SELECT 1 FROM credit_ledger WHERE tx_type = 'enemy_sensor_enable' AND related_id = $1 LIMIT 1",
            )
            .bind(creature_id.to_string())
            .fetch_optional(pool)
            .await
            .map(|r| r.is_some())
            .unwrap_or(false);

            let cost = if already_paid {
                0
            } else {
                let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                charge_gas(
                    &state.db,
                    wallet.wallet_id,
                    gas.enemy_sensor_enable,
                    "enemy_sensor_enable",
                    &format!("Unlock enemy sensor for creature {}", creature_id),
                    Some(&creature_id.to_string()),
                )
                .await?;
                gas.enemy_sensor_enable
            };

            toggle_module(pool, creature_id, "enemy_sensor", true).await;

            Ok(Json(json!({
                "creature_id": creature_id,
                "enemy_sensor": "enabled",
                "cost": cost,
            })))
        }
        "disable" => {
            toggle_module(pool, creature_id, "enemy_sensor", false).await;

            Ok(Json(json!({
                "creature_id": creature_id,
                "enemy_sensor": "disabled",
            })))
        }
        "check" => {
            // Verify module is enabled
            let modules: Vec<String> = sqlx::query(
                "SELECT active_modules FROM creature_conditions WHERE creature_id = $1",
            )
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .and_then(|r| {
                r.try_get::<Option<Vec<String>>, _>("active_modules")
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();

            if !modules.contains(&"enemy_sensor".to_string()) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Enemy sensor not enabled — enable it first (5cr)".to_string(),
                ));
            }

            // Charge check fee
            let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            charge_gas(
                &state.db,
                wallet.wallet_id,
                gas.enemy_sensor_check,
                "enemy_sensor_check",
                &format!("Enemy sensor scan for creature {}", creature_id),
                Some(&creature_id.to_string()),
            )
            .await?;

            // Build query for the agent
            let scientific_name: String = creature
                .try_get("scientific_name")
                .unwrap_or_else(|_| "Unknown".to_string());
            let species_group: String = creature
                .try_get("species_group")
                .unwrap_or_else(|_| "insect".to_string());
            let taxonomy: Option<serde_json::Value> = creature.try_get("taxonomy").ok().flatten();
            let order = taxonomy
                .as_ref()
                .and_then(|t| t.get("order"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");

            let query = format!(
                "Scan for natural enemies near creature {} ({}, order {}). \
                 Use scan_nearby_creatures with creature_id \"{}\" to find who is nearby, \
                 then assess predation risk.",
                creature
                    .try_get::<Option<String>, _>("common_name")
                    .unwrap_or(None)
                    .unwrap_or_else(|| scientific_name.clone()),
                scientific_name,
                order,
                creature_id,
            );

            let workspace_id = find_creature_workspace(pool, creature_id).await?;

            // Synchronous dispatch with timeout to prevent Railway 502
            let assessment = tokio::time::timeout(
                std::time::Duration::from_secs(25),
                rabble_workspace::dispatch_rabble_action(
                    &state,
                    workspace_id,
                    "enemy_sensor",
                    "threat_scan",
                    &query,
                    &user_id,
                ),
            )
            .await
            .map_err(|_| {
                (
                    StatusCode::GATEWAY_TIMEOUT,
                    "Scan timed out — try again".to_string(),
                )
            })?
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Scan failed: {}", e),
                )
            })?;

            let parsed: serde_json::Value = serde_json::from_str(&assessment).unwrap_or_else(
                |_| json!({ "threat_level": "unknown", "summary": assessment, "threats": [] }),
            );

            // Record in creature log (background)
            let threat_level = parsed
                .get("threat_level")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let check_cost = gas.enemy_sensor_check;
            {
                let pool_bg = state.memory_store.pool().clone();
                let parsed_bg = parsed.clone();
                tokio::spawn(async move {
                    let _ = record_transition(
                        &pool_bg, creature_id, "active", None,
                        "enemy_scan", "enemy_sensor", 0.0, 0.0, "", None, None,
                        &json!({ "threat_level": threat_level, "cost": check_cost, "result": parsed_bg }),
                    ).await;
                });
            }

            Ok(Json(json!({
                "creature_id": creature_id,
                "species_group": species_group,
                "cost": gas.enemy_sensor_check,
                "assessment": parsed,
            })))
        }
        "strategy" => {
            // Follow-up query using enemy_sensor agent with custom prompt (1cr)
            let modules: Vec<String> = sqlx::query(
                "SELECT active_modules FROM creature_conditions WHERE creature_id = $1",
            )
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .and_then(|r| {
                r.try_get::<Option<Vec<String>>, _>("active_modules")
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();
            if !modules.contains(&"enemy_sensor".to_string()) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Enemy sensor not enabled".to_string(),
                ));
            }

            let custom_prompt = req
                .prompt
                .unwrap_or_else(|| "Analyze defensive strategies".to_string());
            let scientific_name: String = creature
                .try_get("scientific_name")
                .unwrap_or_else(|_| "Unknown".to_string());
            let taxonomy_val: Option<serde_json::Value> =
                creature.try_get("taxonomy").ok().flatten();
            let common_name: Option<String> = creature
                .try_get::<Option<String>, _>("common_name")
                .unwrap_or(None);
            let order = taxonomy_val
                .as_ref()
                .and_then(|t| t.get("order"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let label = common_name.unwrap_or_else(|| scientific_name.clone());

            let query = format!(
                "DEFENSE STRATEGY for {} ({}, order {}): {}. \
                 Use scan_nearby_creatures with creature_id \"{}\" for spatial context.",
                label, scientific_name, order, custom_prompt, creature_id,
            );

            let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            charge_gas(
                &state.db,
                wallet.wallet_id,
                gas.enemy_sensor_check,
                "enemy_sensor_strategy",
                &format!("Enemy sensor strategy for creature {}", creature_id),
                Some(&creature_id.to_string()),
            )
            .await?;

            let workspace_id = find_creature_workspace(pool, creature_id).await?;

            let result = tokio::time::timeout(
                std::time::Duration::from_secs(25),
                rabble_workspace::dispatch_rabble_action(
                    &state,
                    workspace_id,
                    "enemy_sensor",
                    "defense_strategy",
                    &query,
                    &user_id,
                ),
            )
            .await
            .map_err(|_| {
                (
                    StatusCode::GATEWAY_TIMEOUT,
                    "Strategy timed out — try again".to_string(),
                )
            })?
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Strategy failed: {}", e),
                )
            })?;

            let parsed: serde_json::Value =
                serde_json::from_str(&result).unwrap_or_else(|_| json!({ "strategy": result }));

            // Record in background
            let strategy_cost = gas.enemy_sensor_check;
            {
                let pool_bg = state.memory_store.pool().clone();
                let parsed_bg = parsed.clone();
                tokio::spawn(async move {
                    let _ = record_transition(
                        &pool_bg,
                        creature_id,
                        "active",
                        None,
                        "enemy_strategy",
                        "enemy_sensor",
                        0.0,
                        0.0,
                        "",
                        None,
                        None,
                        &json!({ "cost": strategy_cost, "result": parsed_bg }),
                    )
                    .await;
                });
            }

            Ok(Json(json!({
                "creature_id": creature_id,
                "cost": gas.enemy_sensor_check,
                "strategy": parsed,
            })))
        }
        other => Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Unknown action '{}' — use 'enable', 'disable', 'check', or 'strategy'",
                other
            ),
        )),
    }
}

// ── Genome Profiler ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct GenomeProfilerRequest {
    pub action: String, // "enable", "disable", "check"
}

/// POST /api/creatures/:creature_id/genome-profiler
pub async fn genome_profiler_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<GenomeProfilerRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();
    let gas = &state.gas_fees;

    let creature = sqlx::query(
        "SELECT c.owner_id, c.scientific_name, c.common_name, c.species_group,
                c.taxonomy, c.gbif_key
         FROM creatures c WHERE c.creature_id = $1",
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

    match req.action.as_str() {
        "enable" => {
            // One-time unlock fee — check ledger for prior purchase
            let already_paid: bool = sqlx::query(
                "SELECT 1 FROM credit_ledger WHERE tx_type = 'genome_profiler_enable' AND related_id = $1 LIMIT 1",
            )
            .bind(creature_id.to_string())
            .fetch_optional(pool)
            .await
            .map(|r| r.is_some())
            .unwrap_or(false);

            let cost = if already_paid {
                0
            } else {
                let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                charge_gas(
                    &state.db,
                    wallet.wallet_id,
                    gas.genome_profiler_enable,
                    "genome_profiler_enable",
                    &format!("Unlock genome profiler for creature {}", creature_id),
                    Some(&creature_id.to_string()),
                )
                .await?;
                gas.genome_profiler_enable
            };

            toggle_module(pool, creature_id, "genome_profiler", true).await;

            Ok(Json(json!({
                "creature_id": creature_id,
                "genome_profiler": "enabled",
                "cost": cost,
            })))
        }
        "disable" => {
            toggle_module(pool, creature_id, "genome_profiler", false).await;

            Ok(Json(json!({
                "creature_id": creature_id,
                "genome_profiler": "disabled",
            })))
        }
        "check" => {
            // Check module is enabled + look for cached profile in one query
            let cond_row = sqlx::query(
                "SELECT active_modules, genome_profile FROM creature_conditions WHERE creature_id = $1",
            )
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            let modules: Vec<String> = cond_row
                .as_ref()
                .and_then(|r| {
                    r.try_get::<Option<Vec<String>>, _>("active_modules")
                        .ok()
                        .flatten()
                })
                .unwrap_or_default();

            if !modules.contains(&"genome_profiler".to_string()) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Genome profiler not enabled — enable it first (5cr)".to_string(),
                ));
            }

            // Return cached profile for free if it exists
            let cached: Option<serde_json::Value> = cond_row.as_ref().and_then(|r| {
                r.try_get::<Option<serde_json::Value>, _>("genome_profile")
                    .ok()
                    .flatten()
            });

            if let Some(profile) = cached {
                return Ok(Json(json!({
                    "creature_id": creature_id,
                    "cost": 0,
                    "cached": true,
                    "profile": profile,
                })));
            }

            // First time — charge and generate
            let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            charge_gas(
                &state.db,
                wallet.wallet_id,
                gas.genome_profiler_check,
                "genome_profiler_check",
                &format!("Genome profile for creature {}", creature_id),
                Some(&creature_id.to_string()),
            )
            .await?;

            let scientific_name: String = creature
                .try_get("scientific_name")
                .unwrap_or_else(|_| "Unknown".to_string());
            let gbif_key: Option<i64> = creature.try_get("gbif_key").ok().flatten();
            let taxonomy: Option<serde_json::Value> = creature.try_get("taxonomy").ok().flatten();
            let order = taxonomy
                .as_ref()
                .and_then(|t| t.get("order"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");

            let query = format!(
                "Build a phylogenetic profile for {} (order {}). \
                 Use gbif_taxonomy_tree with {} to get the full taxonomy hierarchy, \
                 then analyze its genomic context and evolutionary relationships.",
                scientific_name,
                order,
                if let Some(key) = gbif_key {
                    format!("gbif_key {}", key)
                } else {
                    format!("scientific_name \"{}\"", scientific_name)
                },
            );

            let workspace_id = find_creature_workspace(pool, creature_id).await?;

            // Synchronous dispatch with timeout
            let profile = tokio::time::timeout(
                std::time::Duration::from_secs(25),
                rabble_workspace::dispatch_rabble_action(
                    &state,
                    workspace_id,
                    "genome_profiler",
                    "phylogenetic_profile",
                    &query,
                    &user_id,
                ),
            )
            .await
            .map_err(|_| {
                (
                    StatusCode::GATEWAY_TIMEOUT,
                    "Profile timed out — try again".to_string(),
                )
            })?
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Profile failed: {}", e),
                )
            })?;

            let parsed: serde_json::Value = serde_json::from_str(&profile).unwrap_or_else(
                |_| json!({ "summary": profile, "taxonomy": {}, "genome": {}, "phylogeny": {} }),
            );

            // Cache + record in background
            let profile_cost = gas.genome_profiler_check;
            {
                let pool_bg = state.memory_store.pool().clone();
                let parsed_bg = parsed.clone();
                tokio::spawn(async move {
                    // Cache the profile
                    let summary_str = parsed_bg
                        .get("summary")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let has_content = !summary_str.is_empty()
                        || parsed_bg
                            .get("taxonomy")
                            .and_then(|v| v.as_object())
                            .map(|m| !m.is_empty())
                            .unwrap_or(false)
                        || parsed_bg
                            .get("genome")
                            .and_then(|v| v.as_object())
                            .map(|m| !m.is_empty())
                            .unwrap_or(false);
                    if has_content {
                        let _ = sqlx::query(
                            "UPDATE creature_conditions SET genome_profile = $1 WHERE creature_id = $2",
                        ).bind(&parsed_bg).bind(creature_id).execute(&pool_bg).await;
                    }
                    let _ = record_transition(
                        &pool_bg,
                        creature_id,
                        "active",
                        None,
                        "genome_profile",
                        "genome_profiler",
                        0.0,
                        0.0,
                        "",
                        None,
                        None,
                        &json!({ "cost": profile_cost, "result": parsed_bg }),
                    )
                    .await;
                });
            }

            Ok(Json(json!({
                "creature_id": creature_id,
                "cost": gas.genome_profiler_check,
                "cached": false,
                "profile": parsed,
            })))
        }
        other => Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Unknown action '{}' — use 'enable', 'disable', or 'check'",
                other
            ),
        )),
    }
}

// ── Prey Locator (premium hunting) ──────────────────────────────

#[derive(Deserialize)]
pub struct PreyLocatorRequest {
    pub action: String, // "enable", "disable", "scan", "stalk", "strategy"
    pub target_creature_id: Option<Uuid>, // required for "stalk"
    pub prompt: Option<String>, // custom prompt for "strategy" action
}

/// POST /api/creatures/:creature_id/prey-locator
///
/// Premium tactical hunting: scan for prey (2cr), then stalk with flight plan (5cr).
/// One-time unlock: 5cr.
pub async fn prey_locator_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<PreyLocatorRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();
    let gas = &state.gas_fees;

    let creature = sqlx::query(
        "SELECT c.owner_id, c.scientific_name, c.common_name, c.species_group, c.taxonomy
         FROM creatures c WHERE c.creature_id = $1",
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

    match req.action.as_str() {
        "enable" => {
            let already_paid: bool = sqlx::query(
                "SELECT 1 FROM credit_ledger WHERE tx_type = 'prey_locator_enable' AND related_id = $1 LIMIT 1",
            )
            .bind(creature_id.to_string())
            .fetch_optional(pool)
            .await
            .map(|r| r.is_some())
            .unwrap_or(false);

            let cost = if already_paid {
                0
            } else {
                let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                charge_gas(
                    &state.db,
                    wallet.wallet_id,
                    gas.prey_locator_enable,
                    "prey_locator_enable",
                    &format!("Unlock prey locator for creature {}", creature_id),
                    Some(&creature_id.to_string()),
                )
                .await?;
                gas.prey_locator_enable
            };

            toggle_module(pool, creature_id, "prey_locator", true).await;

            Ok(Json(json!({
                "creature_id": creature_id,
                "prey_locator": "enabled",
                "cost": cost,
            })))
        }
        "disable" => {
            toggle_module(pool, creature_id, "prey_locator", false).await;
            Ok(Json(
                json!({ "creature_id": creature_id, "prey_locator": "disabled" }),
            ))
        }
        "scan" => {
            // Verify module is enabled
            let modules: Vec<String> = sqlx::query(
                "SELECT active_modules FROM creature_conditions WHERE creature_id = $1",
            )
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .and_then(|r| {
                r.try_get::<Option<Vec<String>>, _>("active_modules")
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();

            if !modules.contains(&"prey_locator".to_string()) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Prey locator not enabled".to_string(),
                ));
            }

            let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            charge_gas(
                &state.db,
                wallet.wallet_id,
                gas.prey_locator_scan,
                "prey_locator_scan",
                &format!("Prey scan for creature {}", creature_id),
                Some(&creature_id.to_string()),
            )
            .await?;

            let scientific_name: String = creature
                .try_get("scientific_name")
                .unwrap_or_else(|_| "Unknown".to_string());
            let taxonomy: Option<serde_json::Value> = creature.try_get("taxonomy").ok().flatten();
            let order = taxonomy
                .as_ref()
                .and_then(|t| t.get("order"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let common_name: Option<String> = creature
                .try_get::<Option<String>, _>("common_name")
                .unwrap_or(None);
            let label = common_name.unwrap_or_else(|| scientific_name.clone());

            let query = format!(
                "SCAN MODE: Identify viable prey near {} ({}, order {}). \
                 Use scan_nearby_creatures with creature_id \"{}\" to find who is nearby, \
                 then assess which creatures this predator could hunt. \
                 Rank by vulnerability and accessibility.",
                label, scientific_name, order, creature_id,
            );

            let workspace_id = find_creature_workspace(pool, creature_id).await?;

            // Synchronous dispatch with timeout
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(25),
                rabble_workspace::dispatch_rabble_action(
                    &state,
                    workspace_id,
                    "prey_locator",
                    "prey_scan",
                    &query,
                    &user_id,
                ),
            )
            .await
            .map_err(|_| {
                (
                    StatusCode::GATEWAY_TIMEOUT,
                    "Scan timed out — try again".to_string(),
                )
            })?
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Scan failed: {}", e),
                )
            })?;

            let parsed: serde_json::Value = serde_json::from_str(&result)
                .unwrap_or_else(|_| json!({ "prey_targets": [], "hunting_summary": result }));

            // Record in background
            let scan_cost = gas.prey_locator_scan;
            {
                let pool_bg = state.memory_store.pool().clone();
                let parsed_bg = parsed.clone();
                let target_count = parsed
                    .get("prey_targets")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                tokio::spawn(async move {
                    let _ = record_transition(
                        &pool_bg, creature_id, "active", None,
                        "prey_scan", "prey_locator", 0.0, 0.0, "", None, None,
                        &json!({ "targets_found": target_count, "cost": scan_cost, "result": parsed_bg }),
                    ).await;
                });
            }

            Ok(Json(json!({
                "creature_id": creature_id,
                "cost": gas.prey_locator_scan,
                "scan": parsed,
            })))
        }
        "stalk" => {
            let target_id = req.target_creature_id.ok_or((
                StatusCode::BAD_REQUEST,
                "target_creature_id required for stalk".to_string(),
            ))?;

            // Verify module enabled
            let modules: Vec<String> = sqlx::query(
                "SELECT active_modules FROM creature_conditions WHERE creature_id = $1",
            )
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .and_then(|r| {
                r.try_get::<Option<Vec<String>>, _>("active_modules")
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();

            if !modules.contains(&"prey_locator".to_string()) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Prey locator not enabled".to_string(),
                ));
            }

            // Look up target creature
            let target = sqlx::query(
                "SELECT c.scientific_name, c.common_name, cs.location_lat, cs.location_lng, cs.h3_cell
                 FROM creatures c LEFT JOIN creature_state cs ON cs.creature_id = c.creature_id
                 WHERE c.creature_id = $1",
            )
            .bind(target_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Target creature not found".to_string()))?;

            let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            charge_gas(
                &state.db,
                wallet.wallet_id,
                gas.prey_locator_stalk,
                "prey_locator_stalk",
                &format!(
                    "Stalk flight plan for creature {} → {}",
                    creature_id, target_id
                ),
                Some(&creature_id.to_string()),
            )
            .await?;

            let scientific_name: String = creature
                .try_get("scientific_name")
                .unwrap_or_else(|_| "Unknown".to_string());
            let target_name: String = target
                .try_get("scientific_name")
                .unwrap_or_else(|_| "Unknown".to_string());
            let target_lat: f64 = target.try_get("location_lat").unwrap_or(0.0);
            let target_lng: f64 = target.try_get("location_lng").unwrap_or(0.0);

            let query = format!(
                "STALK MODE: Generate a tactical intercept flight plan for {} to hunt {} \
                 at position ({}, {}). Plan approach vectors, ambush positions, \
                 and waypoints. Consider prey escape behaviors and optimal intercept strategy.",
                scientific_name, target_name, target_lat, target_lng,
            );

            let workspace_id = find_creature_workspace(pool, creature_id).await?;

            // Synchronous dispatch with timeout
            let plan = tokio::time::timeout(
                std::time::Duration::from_secs(25),
                rabble_workspace::dispatch_rabble_action(
                    &state,
                    workspace_id,
                    "prey_locator",
                    "stalk_plan",
                    &query,
                    &user_id,
                ),
            )
            .await
            .map_err(|_| {
                (
                    StatusCode::GATEWAY_TIMEOUT,
                    "Stalk timed out — try again".to_string(),
                )
            })?
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Stalk failed: {}", e),
                )
            })?;

            let parsed: serde_json::Value = serde_json::from_str(&plan).unwrap_or_else(
                |_| json!({ "flight_plan": { "approach": plan }, "tactical_notes": "" }),
            );

            // Record in background
            let stalk_cost = gas.prey_locator_stalk;
            {
                let pool_bg = state.memory_store.pool().clone();
                let parsed_bg = parsed.clone();
                tokio::spawn(async move {
                    let _ = record_transition(
                        &pool_bg, creature_id, "active", None, "prey_stalk", "prey_locator",
                        0.0, 0.0, "", None, None,
                        &json!({ "target_creature_id": target_id.to_string(), "cost": stalk_cost, "result": parsed_bg }),
                    ).await;
                });
            }

            Ok(Json(json!({
                "creature_id": creature_id,
                "target_creature_id": target_id,
                "cost": gas.prey_locator_stalk,
                "stalk": parsed,
            })))
        }
        "strategy" => {
            let modules: Vec<String> = sqlx::query(
                "SELECT active_modules FROM creature_conditions WHERE creature_id = $1",
            )
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .and_then(|r| {
                r.try_get::<Option<Vec<String>>, _>("active_modules")
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();
            if !modules.contains(&"prey_locator".to_string()) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Prey locator not enabled".to_string(),
                ));
            }

            let custom_prompt = req
                .prompt
                .unwrap_or_else(|| "Analyze hunting opportunities".to_string());
            let scientific_name: String = creature
                .try_get("scientific_name")
                .unwrap_or_else(|_| "Unknown".to_string());
            let taxonomy_val: Option<serde_json::Value> =
                creature.try_get("taxonomy").ok().flatten();
            let common_name: Option<String> = creature
                .try_get::<Option<String>, _>("common_name")
                .unwrap_or(None);
            let order = taxonomy_val
                .as_ref()
                .and_then(|t| t.get("order"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let label = common_name.unwrap_or_else(|| scientific_name.clone());

            let query = format!(
                "STRATEGY MODE for {} ({}, order {}): {}. \
                 Use scan_nearby_creatures with creature_id \"{}\" for spatial context.",
                label, scientific_name, order, custom_prompt, creature_id,
            );

            let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            charge_gas(
                &state.db,
                wallet.wallet_id,
                gas.prey_locator_scan,
                "prey_locator_strategy",
                &format!("Prey strategy for creature {}", creature_id),
                Some(&creature_id.to_string()),
            )
            .await?;

            let workspace_id = find_creature_workspace(pool, creature_id).await?;

            // Synchronous dispatch with timeout
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(25),
                rabble_workspace::dispatch_rabble_action(
                    &state,
                    workspace_id,
                    "prey_locator",
                    "prey_strategy",
                    &query,
                    &user_id,
                ),
            )
            .await
            .map_err(|_| {
                (
                    StatusCode::GATEWAY_TIMEOUT,
                    "Strategy timed out — try again".to_string(),
                )
            })?
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Strategy failed: {}", e),
                )
            })?;

            let parsed: serde_json::Value =
                serde_json::from_str(&result).unwrap_or_else(|_| json!({ "strategy": result }));

            // Record in background
            let strategy_cost = gas.prey_locator_scan;
            {
                let pool_bg = state.memory_store.pool().clone();
                let parsed_bg = parsed.clone();
                tokio::spawn(async move {
                    let _ = record_transition(
                        &pool_bg,
                        creature_id,
                        "active",
                        None,
                        "prey_strategy",
                        "prey_locator",
                        0.0,
                        0.0,
                        "",
                        None,
                        None,
                        &json!({ "cost": strategy_cost, "result": parsed_bg }),
                    )
                    .await;
                });
            }

            Ok(Json(json!({
                "creature_id": creature_id,
                "cost": gas.prey_locator_scan,
                "strategy": parsed,
            })))
        }
        other => Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Unknown action '{}' — use 'enable', 'disable', 'scan', 'stalk', or 'strategy'",
                other
            ),
        )),
    }
}

/// Helper: find workspace for a creature via its active flight's swarm
pub async fn creature_level_handler(
    State(state): State<AppState>,
    Path(creature_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let pool = &state.db;

    // Verify creature exists
    let creature = sqlx::query(
        "SELECT c.creature_id, c.owner_id, c.specimen_name, c.scientific_name,
                u.personal_workspace_id
         FROM creatures c
         JOIN users u ON u.user_id = c.owner_id
         WHERE c.creature_id = $1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    let workspace_id: Option<Uuid> = creature
        .try_get::<Option<Uuid>, _>("personal_workspace_id")
        .ok()
        .flatten();

    // Count workspace messages (proxy for embedding_count)
    let message_count: i64 = if let Some(ws_id) = workspace_id {
        sqlx::query("SELECT COUNT(*) FROM workspace_messages WHERE workspace_id = $1")
            .bind(ws_id)
            .fetch_one(pool)
            .await
            .map(|r| r.get::<i64, _>(0))
            .unwrap_or(0)
    } else {
        0
    };

    // Count unique flight locations
    let unique_locations: i64 = sqlx::query(
        "SELECT COUNT(DISTINCT h3_cell) FROM creature_flights
         WHERE creature_id = $1 AND h3_cell IS NOT NULL AND h3_cell != ''",
    )
    .bind(creature_id)
    .fetch_one(pool)
    .await
    .map(|r| r.get::<i64, _>(0))
    .unwrap_or(0);

    // Count state transitions (agent_interactions proxy)
    let version_count: i64 =
        sqlx::query("SELECT COUNT(*) FROM creature_versions WHERE creature_id = $1")
            .bind(creature_id)
            .fetch_one(pool)
            .await
            .map(|r| r.get::<i64, _>(0))
            .unwrap_or(0);

    // Count total flights
    let flight_count: i64 =
        sqlx::query("SELECT COUNT(*) FROM creature_flights WHERE creature_id = $1")
            .bind(creature_id)
            .fetch_one(pool)
            .await
            .map(|r| r.get::<i64, _>(0))
            .unwrap_or(0);

    // Count rabbles joined (distinct swarm_ids)
    let rabbles_joined: i64 = sqlx::query(
        "SELECT COUNT(DISTINCT swarm_id) FROM creature_flights
         WHERE creature_id = $1 AND swarm_id IS NOT NULL",
    )
    .bind(creature_id)
    .fetch_one(pool)
    .await
    .map(|r| r.get::<i64, _>(0))
    .unwrap_or(0);

    // Count active modules (sensors enabled)
    let active_modules: Vec<String> =
        sqlx::query("SELECT active_modules FROM creature_conditions WHERE creature_id = $1")
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .and_then(|r| r.try_get::<Vec<String>, _>("active_modules").ok())
            .unwrap_or_default();

    // Count dream cycles (creature_versions with transition_type = 'dream')
    let dream_cycles: i64 = sqlx::query(
        "SELECT COUNT(*) FROM creature_versions
         WHERE creature_id = $1 AND transition_type = 'dream'",
    )
    .bind(creature_id)
    .fetch_one(pool)
    .await
    .map(|r| r.get::<i64, _>(0))
    .unwrap_or(0);

    // Total credits spent on this creature (from credit_ledger via related_id)
    let credits_spent: i64 = sqlx::query(
        "SELECT COALESCE(SUM(ABS(amount)), 0) FROM credit_ledger
         WHERE related_id = $1 AND amount < 0",
    )
    .bind(creature_id.to_string())
    .fetch_one(pool)
    .await
    .map(|r| r.get::<i64, _>(0))
    .unwrap_or(0);

    // Level formula: floor(log2(1 + weighted_score))
    let weighted_score: f64 = message_count as f64 * 0.5
        + version_count as f64 * 1.0
        + unique_locations as f64 * 0.3
        + dream_cycles as f64 * 5.0
        + flight_count as f64 * 0.2
        + rabbles_joined as f64 * 2.0
        + active_modules.len() as f64 * 1.0;

    let level = (1.0 + weighted_score).log2().floor() as i32;

    Ok(Json(json!({
        "creature_id": creature_id,
        "level": level,
        "weighted_score": weighted_score,
        "metrics": {
            "message_count": message_count,
            "unique_locations": unique_locations,
            "version_count": version_count,
            "flight_count": flight_count,
            "rabbles_joined": rabbles_joined,
            "dream_cycles": dream_cycles,
            "active_modules": active_modules,
            "credits_spent": credits_spent,
        },
        "weights": {
            "messages": 0.5,
            "versions": 1.0,
            "locations": 0.3,
            "dreams": 5.0,
            "flights": 0.2,
            "rabbles": 2.0,
            "modules": 1.0,
        },
    })))
}

// ─── Dream request ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreatureDreamRequest {
    // empty for now — future: dream_type, focus_topic, etc.
}

/// POST /api/creatures/:creature_id/dream — trigger ADM consolidation cycle.
///
/// Chains: coherence evaluation → consolidation → dream narrator.
/// Records a "dream" transition in creature_versions for leveling.
pub async fn creature_dream_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(_req): Json<CreatureDreamRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;
    let gas = &state.gas_fees;

    // 1. Verify ownership
    let creature = sqlx::query(
        "SELECT c.owner_id, c.specimen_name, c.scientific_name, c.species_group,
                u.personal_workspace_id
         FROM creatures c
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
    let scientific_name: String = creature.try_get("scientific_name").unwrap_or_default();

    // 2. Get creature's workspace (personal menagerie)
    let workspace_id: Uuid = creature
        .try_get::<Option<Uuid>, _>("personal_workspace_id")
        .ok()
        .flatten()
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Creature has no workspace — nothing to dream about".to_string(),
        ))?;

    // 3. Check minimum dream interval (1 hour)
    let last_dream = sqlx::query(
        "SELECT recorded_at FROM creature_versions
         WHERE creature_id = $1 AND transition_type = 'dream'
         ORDER BY recorded_at DESC LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(row) = last_dream {
        let last_at: chrono::DateTime<chrono::Utc> = row.get("recorded_at");
        let elapsed = chrono::Utc::now() - last_at;
        if elapsed < chrono::Duration::hours(1) {
            let mins_left = 60 - elapsed.num_minutes();
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                format!(
                    "Too soon to dream again — wait {} more minute{}",
                    mins_left,
                    if mins_left == 1 { "" } else { "s" }
                ),
            ));
        }
    }

    // 4. Check workspace has messages (something to dream about)
    let msg_count: i64 =
        sqlx::query("SELECT COUNT(*) FROM workspace_messages WHERE workspace_id = $1")
            .bind(workspace_id)
            .fetch_one(pool)
            .await
            .map(|r| r.get::<i64, _>(0))
            .unwrap_or(0);

    if msg_count == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Nothing to dream about — interact with your creature first".to_string(),
        ));
    }

    // 5. Daily dream bonus: first dream each day is free (wellness reward)
    let dreams_today: i64 = sqlx::query(
        "SELECT COUNT(*) FROM creature_versions cv
         JOIN creatures c ON c.creature_id = cv.creature_id
         WHERE c.owner_id = $1 AND cv.transition_type = 'dream'
         AND cv.recorded_at > NOW() - INTERVAL '24 hours'",
    )
    .bind(&user_id)
    .fetch_one(pool)
    .await
    .map(|r| r.get::<i64, _>(0))
    .unwrap_or(0);

    let is_dream_bonus = dreams_today == 0;
    let dream_cost = if is_dream_bonus {
        0
    } else {
        gas.creature_dream
    };

    let wallet = fermi_auth::get_or_create_wallet(pool, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if dream_cost > 0 {
        charge_gas(
            pool,
            wallet.wallet_id,
            dream_cost,
            "gas_fee",
            &format!("Dream cycle for {}", specimen_name),
            Some(&creature_id.to_string()),
        )
        .await?;
    }

    // 6. Dispatch dream_narrator agent via workspace to generate the dream
    let dream_query = format!(
        "Dream cycle for creature {} ({}). \
         Consolidate recent interactions, observations, and flights into a coherent dream narrative. \
         Reflect on what this creature has learned, encountered, and experienced. \
         Synthesize patterns and insights from its {} workspace messages. \
         Output a vivid, evocative dream narrative (2-3 paragraphs) that captures \
         the creature's growing understanding of its world.",
        specimen_name, scientific_name, msg_count,
    );

    // Fire-and-forget: spawn dream agent dispatch, return immediately
    let spawn_state = state.clone();
    let spawn_user = user_id.clone();
    tokio::spawn(async move {
        let pool_bg = spawn_state.memory_store.pool();
        match rabble_workspace::dispatch_rabble_action(
            &spawn_state,
            workspace_id,
            "dream_narrator",
            "dream",
            &dream_query,
            &spawn_user,
        )
        .await
        {
            Ok(narrative) => {
                let current_state = get_current_state(pool_bg, creature_id).await;
                let sn = current_state
                    .as_ref()
                    .map(|(s, _)| s.as_str())
                    .unwrap_or("perched");
                let dream_metadata = json!({
                    "dream_narrative": &narrative,
                    "messages_consolidated": msg_count,
                    "trigger": "user_action",
                });
                let loc = sqlx::query(
                    "SELECT location_lat, location_lng, h3_cell, rabble_id
                     FROM creature_state WHERE creature_id = $1",
                )
                .bind(creature_id)
                .fetch_optional(pool_bg)
                .await
                .ok()
                .flatten();
                let (lat, lng, h3, rabble_id) = loc
                    .map(|r| {
                        (
                            r.try_get::<f64, _>("location_lat").unwrap_or(0.0),
                            r.try_get::<f64, _>("location_lng").unwrap_or(0.0),
                            r.try_get::<String, _>("h3_cell").unwrap_or_default(),
                            r.try_get::<Option<Uuid>, _>("rabble_id").ok().flatten(),
                        )
                    })
                    .unwrap_or((0.0, 0.0, String::new(), None));
                let _ = record_transition(
                    pool_bg,
                    creature_id,
                    sn,
                    Some(sn),
                    "dream",
                    &spawn_user,
                    lat,
                    lng,
                    &h3,
                    rabble_id,
                    Some(workspace_id),
                    &dream_metadata,
                )
                .await;
                eprintln!("[dream] completed for creature {}", creature_id);
            }
            Err(e) => eprintln!("[dream] failed for creature {}: {}", creature_id, e),
        }
    });

    // Return immediately — dream runs in background
    let dream_cycles: i64 = sqlx::query(
        "SELECT COUNT(*) FROM creature_versions
         WHERE creature_id = $1 AND transition_type = 'dream'",
    )
    .bind(creature_id)
    .fetch_one(pool)
    .await
    .map(|r| r.get::<i64, _>(0))
    .unwrap_or(0);

    Ok(Json(json!({
        "creature_id": creature_id,
        "status": "dreaming",
        "cost": dream_cost,
        "dream_bonus": is_dream_bonus,
        "dream_cycles": dream_cycles,
        "message": "Dream dispatched — narrative will appear in creature log",
    })))
}
