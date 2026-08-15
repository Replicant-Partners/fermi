//! Agent-powered creature modules — enemy sensor, genome profiler, prey locator, dream.
//!
//! These are premium features that dispatch to LLM agents and cost gas credits.

/// Extract and parse the first valid JSON object from agent response text.
///
/// Handles all the ways Claude Haiku returns JSON:
///   - Pure JSON:          {"key": "value"}
///   - Fenced, no prose:   ```json\n{...}\n```
///   - Prose + fenced:     "Perfect! Here is the JSON:\n```json\n{...}\n```"
///   - Prose + bare JSON:  "Here is the result: {...}"
///
/// Falls back to `fallback` only if no parseable JSON object is found.
fn parse_agent_json(text: &str, fallback: serde_json::Value) -> serde_json::Value {
    // Try 1: bare JSON (most common after our fixes)
    let t = text.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(t) {
        if v.is_object() || v.is_array() {
            return v;
        }
    }

    // Try 2: extract from ```json ... ``` fence (with optional prose before/after)
    let fence_start = text
        .find("```json")
        .or_else(|| text.find("```JSON"))
        .or_else(|| text.find("```\n{"))
        .or_else(|| text.find("```\n["));
    if let Some(fs) = fence_start {
        let after_fence = text[fs..]
            .trim_start_matches('`')
            .trim_start_matches("json")
            .trim_start_matches("JSON")
            .trim_start();
        if let Some(fe) = after_fence.find("```") {
            let candidate = after_fence[..fe].trim();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(candidate) {
                if v.is_object() || v.is_array() {
                    return v;
                }
            }
        }
    }

    // Try 3: find first { ... } span in the text (prose + embedded JSON)
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            if end > start {
                let candidate = &text[start..=end];
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(candidate) {
                    if v.is_object() {
                        return v;
                    }
                }
            }
        }
    }

    fallback
}

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
use agent_bestiary_memory::{
    ConsolidationLock, ConsolidationWorker, LLMProviderConfig, LLMProviderFactory, ProviderType,
};
use fermi::gas::charge_gas;
use fermi_auth::{get_or_create_wallet, AuthPrincipal};
use std::sync::Arc;

#[derive(serde::Deserialize)]
pub struct ForageRequest {
    pub action: String, // "enable" | "disable" | "scout" | "log"
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub species: Option<String>,
    pub quantity: Option<String>,
    pub habitat: Option<String>,
    pub conditions: Option<serde_json::Value>,
    pub harvest_notes: Option<String>,
    pub flavor_notes: Option<String>,
    pub opted_in_shared: Option<bool>,
    pub goal_id: Option<String>,
    pub photo_urls: Option<Vec<String>>, // workspace git raw URLs
    pub photo_url: Option<String>,       // single photo URL for identify action
}

use super::helpers::{
    find_creature_workspace, get_current_state, record_transition, toggle_module,
    verify_creature_ownership,
};

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
    let creature = verify_creature_ownership(pool, creature_id, &user_id).await?;

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
                std::time::Duration::from_secs(55),
                rabble_workspace::dispatch_rabble_action(
                    &state,
                    workspace_id,
                    "enemy_sensor",
                    "threat_scan",
                    &query,
                    &user_id,
                    Some(creature_id),
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

            let mut parsed: serde_json::Value = parse_agent_json(
                &assessment,
                json!({ "threat_level": "unknown", "summary": assessment, "threats": [] }),
            );

            // Grounding contract. Unlike genome_profiler this agent is
            // well-formed — `scan_nearby_creatures` returns the creatures it
            // reports on, and the risk rating is a judgement it is asked to
            // make. Enforcement here stamps `model_inference` on the
            // judgement blocks rather than stripping them, and catches the
            // one thing that would be a fabrication: a threat naming a
            // creature the scan never returned.
            let grounding = crate::grounding_trust::enforce("enemy_sensor", &mut parsed);
            if !grounding.is_clean() {
                tracing::warn!(
                    creature_id = %creature_id,
                    violations = grounding.violations.len(),
                    paths = ?grounding.violations.iter().map(|v| v.path.as_str()).collect::<Vec<_>>(),
                    "enemy_sensor produced ungrounded fields; stripped before use"
                );
            }

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
                std::time::Duration::from_secs(55),
                rabble_workspace::dispatch_rabble_action(
                    &state,
                    workspace_id,
                    "enemy_sensor",
                    "defense_strategy",
                    &query,
                    &user_id,
                    Some(creature_id),
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
                parse_agent_json(&result, json!({ "strategy": result }));

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

    // Verify ownership
    let creature = verify_creature_ownership(pool, creature_id, &user_id).await?;

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

            // Return cached profile for free if it exists AND has real content.
            // An empty taxonomy map means the profile was written during a
            // pre-fix run when the agent hallucinated or returned empty data —
            // treat it as a cache miss and re-run the agent.
            let cached: Option<serde_json::Value> = cond_row.as_ref().and_then(|r| {
                r.try_get::<Option<serde_json::Value>, _>("genome_profile")
                    .ok()
                    .flatten()
            });

            let cache_is_valid = cached
                .as_ref()
                .map(|p| {
                    p.get("taxonomy")
                        .and_then(|v| v.as_object())
                        .map(|m| !m.is_empty())
                        .unwrap_or(false)
                })
                .unwrap_or(false);

            if cache_is_valid {
                // Enforce grounding on READ as well as on write.
                //
                // 13 cached profiles predate this contract and carry
                // fabricated genome sizes, karyotypes, divergence dates and
                // IUCN statuses. `cache_is_valid` cannot see them: it asks
                // only whether `taxonomy` is non-empty, and taxonomy is the
                // one block that HAS a tool — so a profile with real
                // taxonomy and invented genome data stays "valid" forever.
                // That predicate was written for a previous fix, against a
                // symptom (empty profiles) rather than this cause.
                //
                // Enforcing on read means the 13 stop being served
                // immediately, without re-running the agent at 2 credits a
                // call. Migration 200 quarantines the stored copies.
                let mut profile = cached.unwrap();
                let report = crate::grounding_trust::enforce("genome_profiler", &mut profile);
                if !report.is_clean() {
                    tracing::warn!(
                        creature_id = %creature_id,
                        violations = report.violations.len(),
                        paths = ?report.violations.iter().map(|v| v.path.as_str()).collect::<Vec<_>>(),
                        "cached genome profile carried ungrounded fields; stripped on read"
                    );
                }
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
                "Build a phylogenetic profile for {} (order: {}{}). \
                 Call gbif_taxonomy_tree first, then gbif_species_search. \
                 Return the JSON profile described in your instructions.",
                scientific_name,
                order,
                if let Some(key) = gbif_key {
                    format!(", gbif_key: {}", key)
                } else {
                    String::new()
                },
            );

            let workspace_id = find_creature_workspace(pool, creature_id).await?;

            // Synchronous dispatch with timeout
            let profile = tokio::time::timeout(
                std::time::Duration::from_secs(55),
                rabble_workspace::dispatch_rabble_action(
                    &state,
                    workspace_id,
                    "genome_profiler",
                    "phylogenetic_profile",
                    &query,
                    &user_id,
                    Some(creature_id),
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

            let mut parsed: serde_json::Value = parse_agent_json(
                &profile,
                json!({
                    "summary": profile,
                    "taxonomy": {},
                    "genome": {},
                    "phylogeny": {},
                    // `conservation` was absent from this fallback, so a
                    // parse failure produced a document missing a block the
                    // schema requires — and nothing downstream noticed,
                    // because nothing validated the shape.
                    "conservation": {},
                }),
            );

            // Enforce the grounding contract before anything caches, records
            // or renders this. The agent has two GBIF tools and is asked for
            // four blocks; three of them have no possible source, so any
            // value in them came from the model's weights rather than from a
            // lookup. `enforce` nulls those, stamps `<block>_provenance`, and
            // hands back what it removed.
            //
            // Placed here rather than inside `parse_agent_json` because that
            // function is shared by every creature module (enemy_sensor,
            // prey_locator, dream) and is a parser with a fallback, not a
            // validator. Mixing the two would make the grounding rules
            // invisible to anyone reading either call site.
            let grounding = crate::grounding_trust::enforce("genome_profiler", &mut parsed);
            if !grounding.is_clean() {
                // WARN not ERROR: the run itself succeeded and the taxonomy
                // is real. What failed is the prompt's ability to stop the
                // model answering questions it has no source for, which is a
                // card defect, not a request failure.
                tracing::warn!(
                    creature_id = %creature_id,
                    scientific_name = %scientific_name,
                    violations = grounding.violations.len(),
                    paths = ?grounding
                        .violations
                        .iter()
                        .map(|v| v.path.as_str())
                        .collect::<Vec<_>>(),
                    "genome_profiler produced ungrounded fields; stripped before caching"
                );
            }

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

    // Verify ownership
    let creature = verify_creature_ownership(pool, creature_id, &user_id).await?;

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
                std::time::Duration::from_secs(55),
                rabble_workspace::dispatch_rabble_action(
                    &state,
                    workspace_id,
                    "prey_locator",
                    "prey_scan",
                    &query,
                    &user_id,
                    Some(creature_id),
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

            let mut parsed: serde_json::Value = parse_agent_json(
                &result,
                json!({ "prey_targets": [], "hunting_summary": result }),
            );

            // `distance_cells` is the guess here: the scan returns `h3_cell`
            // per neighbour and no distance of any kind. Exactly computable
            // with `h3o`, which is already a dependency — see the contract
            // entry, which names this the cheapest Unsourced field in the
            // corpus to retire.
            let grounding = crate::grounding_trust::enforce("prey_locator", &mut parsed);
            if !grounding.is_clean() {
                tracing::warn!(
                    creature_id = %creature_id,
                    mode = "scan",
                    violations = grounding.violations.len(),
                    paths = ?grounding.violations.iter().map(|v| v.path.as_str()).collect::<Vec<_>>(),
                    "prey_locator produced ungrounded fields; stripped before use"
                );
            }

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
                std::time::Duration::from_secs(55),
                rabble_workspace::dispatch_rabble_action(
                    &state,
                    workspace_id,
                    "prey_locator",
                    "stalk_plan",
                    &query,
                    &user_id,
                    Some(creature_id),
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

            let mut parsed: serde_json::Value = parse_agent_json(
                &plan,
                json!({ "flight_plan": { "approach": plan }, "tactical_notes": "" }),
            );

            // The stalk mode is the sharpest case in the corpus. Nearby
            // creatures reach this agent as an `h3_cell` and nothing else —
            // no latitude, no longitude, no distance — so every waypoint
            // coordinate in the flight plan is a number it was never given,
            // in a document meant to be flown rather than read. The strategy
            // and the difficulty rating survive; the geometry does not.
            let grounding = crate::grounding_trust::enforce("prey_locator", &mut parsed);
            if !grounding.is_clean() {
                tracing::warn!(
                    creature_id = %creature_id,
                    mode = "stalk",
                    violations = grounding.violations.len(),
                    paths = ?grounding.violations.iter().map(|v| v.path.as_str()).collect::<Vec<_>>(),
                    "prey_locator produced ungrounded flight geometry; stripped before use"
                );
            }

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
                std::time::Duration::from_secs(55),
                rabble_workspace::dispatch_rabble_action(
                    &state,
                    workspace_id,
                    "prey_locator",
                    "prey_strategy",
                    &query,
                    &user_id,
                    Some(creature_id),
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
                parse_agent_json(&result, json!({ "strategy": result }));

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
    let creature = verify_creature_ownership(pool, creature_id, &user_id).await?;

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
            Some(creature_id),
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
                eprintln!("[dream] narrative completed for creature {}", creature_id);

                // ── Consolidation: the actual learning step ──────────────
                // The narrative is the story; consolidation is the memory.
                // Run ConsolidationWorker for each active creature agent
                // (enemy_sensor, genome_profiler, prey_locator) that has
                // accumulated unconsolidated episodes. This extracts rules,
                // entities, and facts into the knowledge graph so that
                // enrich_with_kg_context can surface them on future runs.
                let active_agents = ["enemy_sensor", "genome_profiler", "prey_locator"];
                let llm_opt = std::env::var("ANTHROPIC_API_KEY").ok().and_then(|key| {
                    LLMProviderFactory::create(&LLMProviderConfig {
                        provider_type: ProviderType::Anthropic,
                        api_key: key,
                        model: "claude-haiku-4-5-20251001".to_string(),
                        base_url: None,
                    })
                    .ok()
                });

                for agent_name in &active_agents {
                    // Look up the agent's DB UUID
                    let agent_uuid: Option<uuid::Uuid> = sqlx::query_scalar(
                        "SELECT agent_id FROM agents WHERE agent_name = $1 LIMIT 1",
                    )
                    .bind(agent_name)
                    .fetch_optional(pool_bg)
                    .await
                    .ok()
                    .flatten();

                    let agent_uuid = match agent_uuid {
                        Some(id) => id,
                        None => {
                            eprintln!(
                                "[dream] agent {} not found in DB, skipping consolidation",
                                agent_name
                            );
                            continue;
                        }
                    };

                    // Check if there are unconsolidated episodes first (avoid
                    // creating a lock + job for empty queues)
                    let episode_count: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM episodes WHERE agent_id = $1 AND consolidated = false",
                    )
                    .bind(agent_uuid)
                    .fetch_optional(pool_bg)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or(0);

                    if episode_count == 0 {
                        eprintln!(
                            "[dream] {} has no unconsolidated episodes, skipping",
                            agent_name
                        );
                        continue;
                    }

                    eprintln!(
                        "[dream] consolidating {} ({} episodes)",
                        agent_name, episode_count
                    );

                    let lock = Arc::new(ConsolidationLock::new(
                        Arc::new(pool_bg.clone()),
                        format!("dream-{}", creature_id),
                    ));

                    let worker = match &llm_opt {
                        Some(llm) => ConsolidationWorker::with_llm(
                            spawn_state.memory_store.clone(),
                            lock,
                            spawn_state.embedder.clone(),
                            llm.clone(),
                            format!("dream-{}", creature_id),
                        ),
                        None => ConsolidationWorker::new(
                            spawn_state.memory_store.clone(),
                            lock,
                            spawn_state.embedder.clone(),
                            format!("dream-{}", creature_id),
                        ),
                    };

                    match worker.consolidate_agent(agent_uuid, 0.5, 2).await {
                        Ok(result) => {
                            eprintln!(
                                "[dream] {} consolidated: {} episodes, {} rules, {} entities",
                                agent_name,
                                result.episodes_processed,
                                result.rules_extracted,
                                result.entities_created,
                            );
                            // Update last_consolidated_at and dreaming credits
                            let _ = sqlx::query(
                                "UPDATE agents SET last_consolidated_at = NOW(),
                                 dreaming_credits_used = dreaming_credits_used + 1
                                 WHERE agent_id = $1",
                            )
                            .bind(agent_uuid)
                            .execute(pool_bg)
                            .await;

                            // Update the in-memory registry's ontology_stats so
                            // enrich_with_kg_context stops fast-pathing this agent.
                            // We fetch the live entity count from DB rather than
                            // using result.entities_created (which is the delta,
                            // not the total).
                            if result.entities_created > 0 || result.rules_extracted > 0 {
                                let total_entities: i64 = sqlx::query_scalar(
                                    "SELECT COUNT(*) FROM kg_entities WHERE agent_id = $1",
                                )
                                .bind(agent_uuid)
                                .fetch_optional(pool_bg)
                                .await
                                .ok()
                                .flatten()
                                .unwrap_or(0);

                                if let Ok(mut card) = spawn_state.registry.get(agent_name) {
                                    card.ontology_stats.entities = total_entities as u32;
                                    card.ontology_stats.relationships = result.facts_created as u32;
                                    card.ontology_stats.evolution_commits += 1;
                                    let _ = spawn_state.registry.update(card);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[dream] consolidation failed for {}: {}", agent_name, e)
                        }
                    }
                }
                eprintln!("[dream] cycle complete for creature {}", creature_id);
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

// ─── Forage Module ────────────────────────────────────────────────────────────
//
// Bridges the Rabble creature to kask-app-wild via cross-workspace delegation.
// The creature provides spatial context (location, species, creature_id);
// wild_companion provides foraging intelligence using its full tool suite.

/// POST /api/creatures/:creature_id/forage
pub async fn forage_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<uuid::Uuid>,
    Json(req): Json<ForageRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;
    let gas = &state.gas_fees;

    let creature = verify_creature_ownership(pool, creature_id, &user_id).await?;

    match req.action.as_str() {
        "enable" => {
            toggle_module(pool, creature_id, "forage", true).await;
            Ok(Json(
                json!({ "creature_id": creature_id, "forage": "enabled" }),
            ))
        }
        "disable" => {
            toggle_module(pool, creature_id, "forage", false).await;
            Ok(Json(
                json!({ "creature_id": creature_id, "forage": "disabled" }),
            ))
        }
        "scout" => {
            // Check module enabled
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

            if !modules.contains(&"forage".to_string()) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Forage module not enabled".to_string(),
                ));
            }

            // Charge gas
            let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            charge_gas(
                &state.db,
                wallet.wallet_id,
                gas.enemy_sensor_check, // reuse same gas cost for now
                "forage_scout",
                &format!("Forage scout for creature {}", creature_id),
                Some(&creature_id.to_string()),
            )
            .await?;

            // Get creature location — use request lat if provided,
            // otherwise look up from creature_state
            let lat = if req.lat.is_some() {
                req.lat
            } else {
                sqlx::query("SELECT location_lat FROM creature_state WHERE creature_id = $1")
                    .bind(creature_id)
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|row| row.try_get::<f64, _>("location_lat").ok())
            };

            let scientific_name: String = creature
                .try_get("scientific_name")
                .unwrap_or_else(|_| "Unknown".to_string());

            let location_hint = match (lat, req.lng) {
                (Some(la), Some(ln)) => format!("at coordinates {:.4}, {:.4}", la, ln),
                _ => "at current location".to_string(),
            };

            let query = format!(
                "Forage scout for creature {} ({}). {}. \
                 Use inat_observations and openweather_forecast to assess what is likely fruiting nearby. \
                 Return structured foraging intelligence.",
                creature_id, scientific_name, location_hint,
            );

            let workspace_id = find_creature_workspace(pool, creature_id).await?;

            // Find wild workspace for this creature (if any)
            let wild_workspace_id: Option<uuid::Uuid> = sqlx::query_scalar(
                "SELECT wild_workspace_id FROM creature_goals
                 WHERE creature_id = $1 AND status = 'active'
                 AND wild_workspace_id IS NOT NULL LIMIT 1",
            )
            .bind(creature_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

            let result = tokio::time::timeout(
                std::time::Duration::from_secs(55),
                rabble_workspace::dispatch_rabble_action(
                    &state,
                    workspace_id,
                    "forage_scout",
                    "forage_scout",
                    &query,
                    &user_id,
                    Some(creature_id),
                ),
            )
            .await
            .map_err(|_| {
                (
                    StatusCode::GATEWAY_TIMEOUT,
                    "Scout timed out — try again".to_string(),
                )
            })?
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Scout failed: {}", e),
                )
            })?;

            let parsed = parse_agent_json(
                &result,
                json!({ "summary": result, "species_likely": [], "foraging_signal": "unknown" }),
            );

            Ok(Json(json!({
                "creature_id": creature_id,
                "cost": gas.enemy_sensor_check,
                "wild_workspace_id": wild_workspace_id,
                "scout": parsed,
            })))
        }
        "log" => {
            // Log a foraging observation directly (no agent call needed)
            let species = req.species.as_deref().ok_or((
                StatusCode::BAD_REQUEST,
                "species is required for log action".to_string(),
            ))?;

            let flavor_profile = if let Some(ref notes) = req.flavor_notes {
                json!({ "tasting_notes": notes })
            } else {
                json!({})
            };

            let goal_uuid: Option<uuid::Uuid> = req.goal_id.as_deref().and_then(|s| s.parse().ok());

            // Collect photo URLs — from array or single field
            let photo_urls: Vec<String> = req.photo_urls.clone().unwrap_or_else(|| {
                req.photo_url
                    .as_ref()
                    .map(|u| vec![u.clone()])
                    .unwrap_or_default()
            });
            let photo_urls_val: Option<serde_json::Value> = if photo_urls.is_empty() {
                None
            } else {
                Some(json!(photo_urls))
            };

            let obs_id: uuid::Uuid = sqlx::query(
                r#"INSERT INTO forage_observations (
                    creature_id, goal_id, owner_id,
                    species_name, taxa_group, quantity,
                    location_lat, location_lng,
                    habitat_type, conditions,
                    harvest_notes, flavor_profile, opted_in_shared,
                    photo_urls
                ) VALUES ($1,$2,$3,$4,'fungi',$5,$6,$7,$8,$9,$10,$11,$12,$13)
                RETURNING observation_id"#,
            )
            .bind(creature_id)
            .bind(goal_uuid)
            .bind(&user_id)
            .bind(species)
            .bind(req.quantity.as_deref())
            .bind(req.lat)
            .bind(req.lng)
            .bind(req.habitat.as_deref())
            .bind(req.conditions.clone().unwrap_or_else(|| json!({})))
            .bind(req.harvest_notes.as_deref())
            .bind(&flavor_profile)
            .bind(req.opted_in_shared.unwrap_or(false))
            .bind(if photo_urls.is_empty() {
                None
            } else {
                Some(&photo_urls as &Vec<String>)
            })
            .fetch_one(&state.db)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to log observation: {}", e),
                )
            })?
            .try_get("observation_id")
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            // Update goal progress
            if let Some(gid) = goal_uuid {
                let _ = sqlx::query(
                    "UPDATE creature_goals SET
                     progress = jsonb_set(progress, '{observations_logged}',
                         to_jsonb(COALESCE((progress->>'observations_logged')::int, 0) + 1)),
                     last_evaluated_at = NOW()
                     WHERE goal_id = $1",
                )
                .bind(gid)
                .execute(&state.db)
                .await;
            }

            Ok(Json(json!({
                "creature_id": creature_id,
                "observation_id": obs_id,
                "species": species,
                "photo_urls": photo_urls_val,
                "logged": true,
            })))
        }

        "identify" => {
            // Photo-based species identification via Claude vision API.
            // The client uploads the photo to the workspace git first, then
            // passes the raw URL here. We build a vision message directly
            // using the Anthropic messages API with an image URL content block.
            let photo_url = req.photo_url.as_deref().ok_or((
                StatusCode::BAD_REQUEST,
                "photo_url is required for identify action".to_string(),
            ))?;

            let habitat_hint = req.habitat.as_deref().unwrap_or("unknown habitat");
            let location_hint = match (req.lat, req.lng) {
                (Some(la), Some(ln)) => format!("{:.4}, {:.4}", la, ln),
                _ => "unknown".to_string(),
            };

            let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "ANTHROPIC_API_KEY not set".to_string(),
                )
            })?;

            // Build a vision request: text instruction + image URL block
            let request_body = json!({
                "model": "claude-haiku-4-5-20251001",
                "max_tokens": 1024,
                "system": "You are an expert field mycologist and foraging safety advisor. \
                           When shown a photo of a wild specimen, you identify the species \
                           with scientific rigour and provide a clear safety assessment. \
                           You ALWAYS flag toxic look-alikes. When uncertain, you say so. \
                           Never recommend harvesting an unidentified specimen. \
                           Respond in JSON only.",
                "messages": [{
                    "role": "user",
                    "content": [
                        {
                            "type": "image",
                            "source": {
                                "type": "url",
                                "url": photo_url
                            }
                        },
                        {
                            "type": "text",
                            "text": format!(
                                "Identify this specimen. Location: {}. Habitat: {}.\n\n\
                                 Respond with JSON only:\n\
                                 {{\n\
                                   \"species\": \"scientific name or null if uncertain\",\n\
                                   \"common_name\": \"common name\",\n\
                                   \"edibility\": \"choice|edible|inedible|toxic|unknown\",\n\
                                   \"confidence\": \"high|medium|low\",\n\
                                   \"identification_notes\": \"key visual features used\",\n\
                                   \"look_alikes\": [\n\
                                     {{\"species\": \"name\", \"danger\": \"fatal|toxic|inedible\", \"distinguishing\": \"how to tell apart\"}}\n\
                                   ],\n\
                                   \"harvest_window\": \"now|1-2 days|not prime|do not harvest\",\n\
                                   \"processing_recommendation\": \"brief processing note\",\n\
                                   \"safety_note\": \"critical safety information — especially if toxic look-alikes exist\"\n\
                                 }}",
                                location_hint, habitat_hint
                            )
                        }
                    ]
                }]
            });

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default();

            let resp = client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&request_body)
                .send()
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Vision API request failed: {}", e),
                    )
                })?;

            if !resp.status().is_success() {
                let err = resp.text().await.unwrap_or_default();
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Vision API error: {}", err),
                ));
            }

            let claude_resp: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            let raw_text = claude_resp
                .pointer("/content/0/text")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let identification = parse_agent_json(
                raw_text,
                json!({
                    "species": null,
                    "edibility": "unknown",
                    "confidence": "low",
                    "safety_note": "Could not identify specimen. Do not harvest unidentified fungi.",
                }),
            );

            Ok(Json(json!({
                "creature_id": creature_id,
                "photo_url": photo_url,
                "identification": identification,
            })))
        }

        other => Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Unknown action '{}' — use enable|disable|scout|log|identify",
                other
            ),
        )),
    }
}
