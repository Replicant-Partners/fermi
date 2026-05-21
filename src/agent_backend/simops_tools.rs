//! SimOps tool handlers — bridge between the JSON tool-call interface (agent side)
//! and the deterministic `simops` crate (compute side) + the ABW SOSA observation
//! store (persistence side).
//!
//! # Interface contract
//!
//! Every tool follows the same pattern:
//!   - Input: `&serde_json::Value` parsed from the LLM's tool call arguments
//!   - Output: `Result<String, String>` — JSON-serialised result or error message
//!   - Context: `Option<&ToolContext>` for DB-backed tools; pure tools take no context
//!
//! # ABW integration points
//!
//! SimOps agents read/write two ABW data stores:
//!
//! 1. **SOSA observation store** (`sosa_observations`, `observation_sessions`,
//!    `sosa_platforms`, `sosa_sensors`) — the universal W3C SSN/SOSA-aligned
//!    time-series store. All process measurements live here.
//!    See: `migrations/052_sosa_observations.sql`, `src/handlers/observations.rs`
//!
//! 2. **Agent episodic memory** (`episodes`) — process config snapshots and
//!    computed results are stored as episodes so the agent's dreaming cycle
//!    can consolidate patterns over time.
//!
//! # Process library
//!
//! Named process configs ("ambu_bioreactor", "scoby_kombucha") are built-in
//! defaults. Any agent can pass a `process_json` field to override with a
//! custom config. The `simops_load_process` tool loads a saved config from
//! agent episodic memory by process name.
//!
//! # Tool inventory
//!
//! ## Deterministic (no DB) — also registered as Skills
//! - simops_cascade_forward    Forward energy/mass cascade
//! - simops_cascade_backward   Backward target-tracing cascade
//! - simops_kpi_compute        NER, SEC, LCC, harvest intensity
//! - simops_predictor_train    Fit OLS regression from observations
//! - simops_predictor_forecast Predict yield from trained model
//! - simops_optimize_scale     Proportional scale to target output
//! - simops_optimize_single_input  Single free-variable optimiser
//!
//! ## ABW-integrated (require ToolContext with DB)
//! - simops_load_process       Load process config from agent memory or built-ins
//! - simops_write_observation  Write a SOSA observation to the platform store
//! - simops_fetch_training_data  Fetch observations as training data for predictor
//! - get_observations          Read recent observations for a session
//! - describe_session          Summarise a session (stats + config snapshot)
//! - simops_check_constraints  Validate optimizer output against process limits
//! - simops_write_actuation_plan  Persist an optimizer recommendation

use serde_json::json;
use simops::{
    cascade::{cascade_backward, cascade_forward},
    kpi::{compute_kpis, BatchObservation},
    optimizer::{scale_from_reference, single_input_solve},
    predictor::{Predictor, TrainingObservation},
    process::{CapexProfile, ProcessConfig, Resource, Stage},
};
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;

use crate::agent_backend::tools::ToolContext;

// ─── Process library ──────────────────────────────────────────────────────────

/// Resolve a named process config to a `ProcessConfig`.
/// Supports inline JSON override via `process_json` field.
pub fn resolve_process(input: &serde_json::Value) -> Result<ProcessConfig, String> {
    // Inline JSON takes priority
    if let Some(json_val) = input.get("process_json") {
        return serde_json::from_value(json_val.clone())
            .map_err(|e| format!("Invalid process_json: {e}"));
    }

    let name = input
        .get("process_name")
        .and_then(|v| v.as_str())
        .unwrap_or("ambu_bioreactor");

    match name {
        "ambu_bioreactor" => Ok(ambu_bioreactor_config()),
        "scoby_kombucha" => Ok(scoby_kombucha_config()),
        other => Err(format!(
            "Unknown process name '{other}'. Known: ambu_bioreactor, scoby_kombucha. \
             Pass process_json for a custom config."
        )),
    }
}

fn ambu_bioreactor_config() -> ProcessConfig {
    ProcessConfig {
        name: "Ambu Bioreactor — Chlorella Cultivation".into(),
        description: Some(
            "Single-stage photoautotrophic cultivation. Photons → dry biomass.".into(),
        ),
        feature_of_interest: Some("xid:platform/ambu-001".into()),
        elec_price_per_kwh: Some(0.22),
        maintenance_cost_usd: Some(480.0),
        stages: vec![Stage {
            id: "cultivation".into(),
            efficiency: 0.03,
            carbon_intensity: -1.8,
            input: Resource {
                name: "photons".into(),
                unit: "kWh".into(),
                energy_density: None,
                density_unit: None,
            },
            output: Resource {
                name: "biomass_dw".into(),
                unit: "kg".into(),
                energy_density: Some(5.5),
                density_unit: Some("kcal/g".into()),
            },
            capex: Some(CapexProfile {
                total_usd: 40.0,
                lifespan_years: 2.0,
            }),
            opex_per_input_unit: Some(0.22),
            sidestreams: None,
            sensors: None,
        }],
    }
}

fn scoby_kombucha_config() -> ProcessConfig {
    ProcessConfig {
        name: "SCOBY Kombucha — Primary + Secondary Fermentation".into(),
        description: Some("Substrate sugars → organic acids → carbonated product.".into()),
        feature_of_interest: Some("xid:platform/ambu-001".into()),
        elec_price_per_kwh: Some(0.22),
        maintenance_cost_usd: Some(120.0),
        stages: vec![
            Stage {
                id: "primary_fermentation".into(),
                efficiency: 0.60,
                carbon_intensity: 0.05,
                input: Resource {
                    name: "substrate_sugars".into(),
                    unit: "kg".into(),
                    energy_density: Some(3.94),
                    density_unit: Some("kcal/g".into()),
                },
                output: Resource {
                    name: "organic_acids".into(),
                    unit: "kg".into(),
                    energy_density: Some(2.5),
                    density_unit: Some("kcal/g".into()),
                },
                capex: Some(CapexProfile {
                    total_usd: 15.0,
                    lifespan_years: 5.0,
                }),
                opex_per_input_unit: Some(0.60),
                sidestreams: None,
                sensors: None,
            },
            Stage {
                id: "secondary_fermentation".into(),
                efficiency: 0.98,
                carbon_intensity: 0.12,
                input: Resource {
                    name: "organic_acids".into(),
                    unit: "kg".into(),
                    energy_density: Some(2.5),
                    density_unit: Some("kcal/g".into()),
                },
                output: Resource {
                    name: "kombucha_product".into(),
                    unit: "kg".into(),
                    energy_density: Some(0.17),
                    density_unit: Some("kcal/g".into()),
                },
                capex: Some(CapexProfile {
                    total_usd: 8.0,
                    lifespan_years: 3.0,
                }),
                opex_per_input_unit: Some(0.10),
                sidestreams: None,
                sensors: None,
            },
        ],
    }
}

// ─── Tool handlers ────────────────────────────────────────────────────────────

/// simops_cascade_forward
/// Input: { process_name?, process_json?, input_quantity: f64 }
pub async fn execute_simops_cascade_forward(
    input: &serde_json::Value,
) -> Result<String, String> {
    let process = resolve_process(input)?;
    process.validate().map_err(|e| e.to_string())?;

    let qty = input
        .get("input_quantity")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: input_quantity (f64)")?;

    let result = cascade_forward(&process, qty);
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

/// simops_cascade_backward
/// Input: { process_name?, process_json?, target_output: f64 }
pub async fn execute_simops_cascade_backward(
    input: &serde_json::Value,
) -> Result<String, String> {
    let process = resolve_process(input)?;
    process.validate().map_err(|e| e.to_string())?;

    let target = input
        .get("target_output")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: target_output (f64)")?;

    let result = cascade_backward(&process, target);
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

/// simops_kpi_compute
/// Input: { primary_energy_kwh, climate_energy_kwh, delivery_energy_kwh,
///          harvest_energy_kwh, output_mass_kg, caloric_density_kcal_g,
///          elec_price_per_kwh, consumables_cost_usd, capex_contribution_usd? }
pub async fn execute_simops_kpi_compute(input: &serde_json::Value) -> Result<String, String> {
    macro_rules! get_f64 {
        ($key:expr) => {
            input
                .get($key)
                .and_then(|v| v.as_f64())
                .ok_or_else(|| format!("Missing required parameter: {}", $key))?
        };
    }

    let obs = BatchObservation {
        primary_energy_kwh:       get_f64!("primary_energy_kwh"),
        climate_energy_kwh:       get_f64!("climate_energy_kwh"),
        delivery_energy_kwh:      get_f64!("delivery_energy_kwh"),
        harvest_energy_kwh:       get_f64!("harvest_energy_kwh"),
        output_mass_kg:           get_f64!("output_mass_kg"),
        caloric_density_kcal_g:   get_f64!("caloric_density_kcal_g"),
        elec_price_per_kwh:       get_f64!("elec_price_per_kwh"),
        consumables_cost_usd:     get_f64!("consumables_cost_usd"),
        capex_contribution_usd:   input
            .get("capex_contribution_usd")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
    };

    let report = compute_kpis(&obs);
    serde_json::to_string_pretty(&report).map_err(|e| e.to_string())
}

/// simops_predictor_train
/// Input: { observations: [ { features: {k:v,...}, target: f64 }, ... ] }
pub async fn execute_simops_predictor_train(
    input: &serde_json::Value,
) -> Result<String, String> {
    let raw = input
        .get("observations")
        .and_then(|v| v.as_array())
        .ok_or("Missing required parameter: observations (array)")?;

    let training: Vec<TrainingObservation> = raw
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let features_val = item
                .get("features")
                .ok_or_else(|| format!("observations[{i}] missing 'features'"))?;
            let target = item
                .get("target")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| format!("observations[{i}] missing 'target'"))?;

            let features: HashMap<String, f64> = features_val
                .as_object()
                .ok_or_else(|| format!("observations[{i}].features must be an object"))?
                .iter()
                .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f)))
                .collect();

            Ok(TrainingObservation { features, target })
        })
        .collect::<Result<_, String>>()?;

    let model = Predictor::fit(&training).map_err(|e| e.to_string())?;

    // Return model summary + serialised coefficients
    let response = json!({
        "feature_names": model.feature_names,
        "coefficients": model.coefficients,
        "intercept": model.intercept,
        "r_squared": model.r_squared,
        "n_samples": model.n_samples,
        "model_json": serde_json::to_value(&model).unwrap_or(json!(null)),
    });
    serde_json::to_string_pretty(&response).map_err(|e| e.to_string())
}

/// simops_predictor_forecast
/// Input: { model_json: {...}, features: {k: v, ...} }
pub async fn execute_simops_predictor_forecast(
    input: &serde_json::Value,
) -> Result<String, String> {
    let model: Predictor = serde_json::from_value(
        input
            .get("model_json")
            .cloned()
            .ok_or("Missing required parameter: model_json")?,
    )
    .map_err(|e| format!("Invalid model_json: {e}"))?;

    let features_val = input
        .get("features")
        .ok_or("Missing required parameter: features")?;

    let features: HashMap<String, f64> = features_val
        .as_object()
        .ok_or("features must be an object")?
        .iter()
        .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f)))
        .collect();

    let predicted = model.predict(&features).map_err(|e| e.to_string())?;

    let response = json!({
        "predicted_value": predicted,
        "r_squared": model.r_squared,
        "n_training_samples": model.n_samples,
        "status": if predicted > 0.0 { "valid" } else { "floored_at_zero" },
    });
    serde_json::to_string_pretty(&response).map_err(|e| e.to_string())
}

/// simops_optimize_scale
/// Input: { model_json, reference: {k:v,...}, target_output: f64, max_scale?: f64 }
pub async fn execute_simops_optimize_scale(
    input: &serde_json::Value,
) -> Result<String, String> {
    let model: Predictor = serde_json::from_value(
        input
            .get("model_json")
            .cloned()
            .ok_or("Missing required parameter: model_json")?,
    )
    .map_err(|e| format!("Invalid model_json: {e}"))?;

    let reference: HashMap<String, f64> = input
        .get("reference")
        .and_then(|v| v.as_object())
        .ok_or("Missing required parameter: reference (object)")?
        .iter()
        .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f)))
        .collect();

    let target = input
        .get("target_output")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: target_output")?;

    let max_scale = input
        .get("max_scale")
        .and_then(|v| v.as_f64())
        .unwrap_or(5.0);

    let result = scale_from_reference(&model, &reference, target, max_scale)
        .map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

/// simops_optimize_single_input
/// Input: { model_json, fixed_inputs: {k:v,...}, free_feature: str,
///          target_output: f64, min_value?: f64, max_value?: f64 }
pub async fn execute_simops_optimize_single_input(
    input: &serde_json::Value,
) -> Result<String, String> {
    let model: Predictor = serde_json::from_value(
        input
            .get("model_json")
            .cloned()
            .ok_or("Missing required parameter: model_json")?,
    )
    .map_err(|e| format!("Invalid model_json: {e}"))?;

    let fixed: HashMap<String, f64> = input
        .get("fixed_inputs")
        .and_then(|v| v.as_object())
        .ok_or("Missing required parameter: fixed_inputs (object)")?
        .iter()
        .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f)))
        .collect();

    let free_feature = input
        .get("free_feature")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: free_feature")?;

    let target = input
        .get("target_output")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: target_output")?;

    let min_val = input.get("min_value").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let max_val = input
        .get("max_value")
        .and_then(|v| v.as_f64())
        .unwrap_or(1_000_000.0);

    let result =
        single_input_solve(&model, &fixed, free_feature, target, min_val, max_val)
            .map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

// ─── ABW-integrated tools ─────────────────────────────────────────────────────
//
// These tools require a ToolContext with a live DB connection. They bridge
// the SimOps compute engine to the ABW SOSA observation store.

/// simops_load_process
///
/// Loads a process configuration. Priority:
///   1. Inline `process_json` in the call arguments (highest priority)
///   2. Named built-in ("ambu_bioreactor", "scoby_kombucha")
///   3. Agent episodic memory: searches recent episodes tagged "simops_process"
///      with episode.context.process_name == requested name
///
/// Input: { process_name?: str, process_json?: object }
/// Output: { config: ProcessConfig, source: "inline"|"builtin"|"memory" }
pub async fn execute_simops_load_process(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    // 1. Inline override
    if let Some(pj) = input.get("process_json") {
        let config: ProcessConfig = serde_json::from_value(pj.clone())
            .map_err(|e| format!("Invalid process_json: {e}"))?;
        config.validate().map_err(|e| e.to_string())?;
        let result = json!({
            "config": serde_json::to_value(&config).unwrap_or_default(),
            "source": "inline",
        });
        return serde_json::to_string_pretty(&result).map_err(|e| e.to_string());
    }

    let name = input
        .get("process_name")
        .and_then(|v| v.as_str())
        .unwrap_or("ambu_bioreactor");

    // 2. Built-in library
    if let Ok(config) = resolve_process(input) {
        let result = json!({
            "config": serde_json::to_value(&config).unwrap_or_default(),
            "source": "builtin",
            "available_builtins": ["ambu_bioreactor", "scoby_kombucha"],
        });
        return serde_json::to_string_pretty(&result).map_err(|e| e.to_string());
    }

    // 3. Agent episodic memory — look for episodes tagged "simops_process"
    //    where context.process_name matches
    if let Some(agent_id) = ctx.current_agent_id {
        let pool = ctx.memory_store.pool();
        let rows = sqlx::query(
            "SELECT context FROM episodes
             WHERE agent_id = $1
               AND $2 = ANY(tags)
               AND context->>'process_name' = $3
             ORDER BY timestamp_ref DESC
             LIMIT 1",
        )
        .bind(agent_id)
        .bind("simops_process")
        .bind(name)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("DB error: {e}"))?;

        if let Some(row) = rows.first() {
            let ctx_val: serde_json::Value = row
                .try_get("context")
                .map_err(|e| format!("DB error: {e}"))?;
            if let Some(config_val) = ctx_val.get("process_config") {
                let config: ProcessConfig = serde_json::from_value(config_val.clone())
                    .map_err(|e| format!("Stored config invalid: {e}"))?;
                config.validate().map_err(|e| e.to_string())?;
                let result = json!({
                    "config": serde_json::to_value(&config).unwrap_or_default(),
                    "source": "memory",
                });
                return serde_json::to_string_pretty(&result).map_err(|e| e.to_string());
            }
        }
    }

    Err(format!(
        "Process '{}' not found. Available built-ins: ambu_bioreactor, scoby_kombucha. \
         Pass process_json for a custom config, or store one via simops_write_observation \
         with tags=[\"simops_process\"] and context.process_name set.",
        name
    ))
}

/// simops_write_observation
///
/// Write a SOSA observation to the platform observation store. Used by
/// simops_cascade and simops_predictor agents to persist measurements.
///
/// Input: {
///   session_id: UUID,
///   observable_property: str,       e.g. "biomass_dw_g", "od600", "titratable_acidity"
///   result_value: f64,
///   result_unit?: str,              e.g. "g/L", "kg", "g"
///   feature_of_interest?: str,      e.g. "xid:platform/ambu-001"
///   phenomenon_time?: i64,          Unix ms; defaults to now
///   extra?: object                  Any additional metadata
/// }
pub async fn execute_simops_write_observation(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let pool = ctx.db.as_ref().ok_or("simops_write_observation requires database context")?;

    let session_id: Uuid = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: session_id")?
        .parse()
        .map_err(|e| format!("Invalid session_id UUID: {e}"))?;

    let observable_property = input
        .get("observable_property")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: observable_property")?;

    let result_value = input
        .get("result_value")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: result_value (f64)")?;

    let result_unit = input.get("result_unit").and_then(|v| v.as_str());
    let feature_of_interest = input.get("feature_of_interest").and_then(|v| v.as_str());
    let phenomenon_time = input
        .get("phenomenon_time")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    let extra = input.get("extra").cloned().unwrap_or(json!({}));

    // Verify session exists and get platform_id
    let session_row = sqlx::query(
        "SELECT platform_id FROM observation_sessions WHERE session_id = $1",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error checking session: {e}"))?
    .ok_or_else(|| format!("Session {} not found", session_id))?;

    let platform_id: Uuid = session_row
        .try_get("platform_id")
        .map_err(|e| format!("DB error: {e}"))?;

    let observation_id = Uuid::new_v4();

    // Doc 12 § Capability 1 — resolve the current agent's identity and
    // version stamp them onto the row. ToolContext.current_agent_id is the
    // canonical UUID; we look up the human-readable name (for the
    // denormalised string column) and the latest version row.
    //
    // Best-effort: if the agent_id is missing or the lookups fail, the
    // columns are left NULL and the write still succeeds. Observation
    // provenance is observability, not correctness.
    let (produced_by_agent_id, produced_by_version_id, produced_by_version_number): (
        Option<String>,
        Option<Uuid>,
        Option<i32>,
    ) = match ctx.current_agent_id {
        Some(agent_uuid) => {
            let name: Option<String> = sqlx::query(
                "SELECT name FROM agents WHERE agent_id = $1 LIMIT 1",
            )
            .bind(agent_uuid)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .and_then(|row| row.try_get("name").ok());

            // Resolve current version directly (no MemoryStore handle in this
            // tool context). Mirrors the body of
            // MemoryStore::get_current_agent_version.
            let (vid, vnum): (Option<Uuid>, Option<i32>) = sqlx::query(
                "SELECT version_id, version_number FROM agent_versions \
                 WHERE agent_id = $1 ORDER BY version_number DESC LIMIT 1",
            )
            .bind(agent_uuid)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .map(|row| (row.try_get("version_id").ok(), row.try_get("version_number").ok()))
            .unwrap_or((None, None));

            (name, vid, vnum)
        }
        None => (None, None, None),
    };

    sqlx::query(
        "INSERT INTO sosa_observations
         (observation_id, session_id, platform_id, observable_property,
          feature_of_interest, result_value, result_unit,
          phenomenon_time, result_time, extra,
          produced_by_agent_id, produced_by_version_id, produced_by_version_number)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8, $9, $10, $11, $12)",
    )
    .bind(observation_id)
    .bind(session_id)
    .bind(platform_id)
    .bind(observable_property)
    .bind(feature_of_interest)
    .bind(result_value)
    .bind(result_unit)
    .bind(phenomenon_time)
    .bind(&extra)
    .bind(&produced_by_agent_id)
    .bind(produced_by_version_id)
    .bind(produced_by_version_number)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to write observation: {e}"))?;

    let result = json!({
        "observation_id": observation_id,
        "session_id": session_id,
        "observable_property": observable_property,
        "result_value": result_value,
        "result_unit": result_unit,
        "phenomenon_time": phenomenon_time,
        "produced_by_agent_id": produced_by_agent_id,
        "produced_by_version_id": produced_by_version_id,
        "produced_by_version_number": produced_by_version_number,
        "status": "written",
    });
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

/// simops_fetch_training_data
///
/// Fetch SOSA observations for a session as structured training data for
/// simops_predictor_train. Groups observations by phenomenon_time into
/// feature vectors ready for OLS regression.
///
/// Input: {
///   session_id: UUID,
///   feature_properties: [str],   observable_property names to use as X features
///   target_property: str,        observable_property name to use as y target
///   limit?: int                  Max observations per property (default 1000)
/// }
pub async fn execute_simops_fetch_training_data(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let pool = ctx.db.as_ref().ok_or("simops_fetch_training_data requires database context")?;

    let session_id: Uuid = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: session_id")?
        .parse()
        .map_err(|e| format!("Invalid session_id: {e}"))?;

    let feature_properties: Vec<String> = input
        .get("feature_properties")
        .and_then(|v| v.as_array())
        .ok_or("Missing required parameter: feature_properties (array of strings)")?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    if feature_properties.is_empty() {
        return Err("feature_properties must be a non-empty array of observable_property names".into());
    }

    let target_property = input
        .get("target_property")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: target_property")?;

    let limit = input
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(1000)
        .clamp(1, 10000);

    // Fetch all relevant properties for the session, ordered by time
    let mut all_props = feature_properties.clone();
    all_props.push(target_property.to_string());

    let rows = sqlx::query(
        "SELECT observable_property, result_value, phenomenon_time
         FROM sosa_observations
         WHERE session_id = $1
           AND observable_property = ANY($2)
         ORDER BY phenomenon_time ASC
         LIMIT $3",
    )
    .bind(session_id)
    .bind(&all_props)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("DB error: {e}"))?;

    // Group by phenomenon_time — each time bucket is one training sample
    let mut by_time: std::collections::BTreeMap<i64, HashMap<String, f64>> =
        std::collections::BTreeMap::new();

    for row in &rows {
        let property: String = row.try_get("observable_property").unwrap_or_default();
        let value: f64 = row.try_get("result_value").unwrap_or(0.0);
        let time: i64 = row.try_get("phenomenon_time").unwrap_or(0);
        by_time.entry(time).or_default().insert(property, value);
    }

    // Build training samples: only include time buckets that have all required fields
    let mut training_samples: Vec<serde_json::Value> = Vec::new();
    let mut skipped = 0usize;

    for (time, props) in &by_time {
        let has_all_features = feature_properties.iter().all(|f| props.contains_key(f));
        let has_target = props.contains_key(target_property);
        if has_all_features && has_target {
            let features: HashMap<String, f64> = feature_properties
                .iter()
                .filter_map(|f| props.get(f).map(|v| (f.clone(), *v)))
                .collect();
            training_samples.push(json!({
                "features": features,
                "target": props[target_property],
                "phenomenon_time": time,
            }));
        } else {
            skipped += 1;
        }
    }

    let result = json!({
        "session_id": session_id,
        "feature_properties": feature_properties,
        "target_property": target_property,
        "total_time_buckets": by_time.len(),
        "complete_samples": training_samples.len(),
        "skipped_incomplete": skipped,
        "observations": training_samples,
        "ready_for_training": training_samples.len() >= 2,
        "note": if training_samples.len() < 2 {
            "Not enough complete samples for training (need ≥ 2). Check that feature and target properties are being written at the same phenomenon_time."
        } else {
            "Pass observations to simops_predictor_train."
        },
    });
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

/// get_observations
///
/// Read recent observations for a session, optionally filtered by property.
/// Used by simops_advisor and simops_narrator to provide context.
///
/// Input: {
///   session_id: UUID,
///   observable_property?: str,   filter to one property
///   limit?: int                  (default 100)
///   from_ms?: i64               Unix ms lower bound
///   to_ms?: i64                 Unix ms upper bound
/// }
pub async fn execute_get_observations(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let pool = ctx.db.as_ref().ok_or("get_observations requires database context")?;

    let session_id: Uuid = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: session_id")?
        .parse()
        .map_err(|e| format!("Invalid session_id: {e}"))?;

    let limit = input
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(100)
        .clamp(1, 2000);

    let property_filter = input.get("observable_property").and_then(|v| v.as_str());
    let from_ms = input.get("from_ms").and_then(|v| v.as_i64());
    let to_ms = input.get("to_ms").and_then(|v| v.as_i64());

    let mut sql = String::from(
        "SELECT observable_property, result_value, result_unit, phenomenon_time, feature_of_interest, extra
         FROM sosa_observations WHERE session_id = $1",
    );
    let mut params_count = 1i32;

    if property_filter.is_some() { params_count += 1; sql.push_str(&format!(" AND observable_property = ${}", params_count)); }
    if from_ms.is_some() { params_count += 1; sql.push_str(&format!(" AND phenomenon_time >= ${}", params_count)); }
    if to_ms.is_some() { params_count += 1; sql.push_str(&format!(" AND phenomenon_time <= ${}", params_count)); }

    sql.push_str(&format!(" ORDER BY phenomenon_time DESC LIMIT ${}", params_count + 1));

    let mut query = sqlx::query(&sql).bind(session_id);
    if let Some(p) = property_filter { query = query.bind(p); }
    if let Some(f) = from_ms { query = query.bind(f); }
    if let Some(t) = to_ms { query = query.bind(t); }
    query = query.bind(limit);

    let rows = query.fetch_all(pool).await
        .map_err(|e| format!("DB error: {e}"))?;

    let observations: Vec<serde_json::Value> = rows.iter().map(|r| json!({
        "observable_property": r.try_get::<String,_>("observable_property").unwrap_or_default(),
        "result_value": r.try_get::<f64,_>("result_value").unwrap_or(0.0),
        "result_unit": r.try_get::<Option<String>,_>("result_unit").unwrap_or(None),
        "phenomenon_time": r.try_get::<i64,_>("phenomenon_time").unwrap_or(0),
        "feature_of_interest": r.try_get::<Option<String>,_>("feature_of_interest").unwrap_or(None),
        "extra": r.try_get::<serde_json::Value,_>("extra").unwrap_or(json!({})),
    })).collect();

    // Compute per-property summary stats
    let mut prop_stats: HashMap<String, (f64, f64, usize)> = HashMap::new(); // (sum, sum_sq, count)
    for obs in &observations {
        if let (Some(prop), Some(val)) = (
            obs["observable_property"].as_str(),
            obs["result_value"].as_f64(),
        ) {
            let e = prop_stats.entry(prop.to_string()).or_insert((0.0, 0.0, 0));
            e.0 += val; e.1 += val * val; e.2 += 1;
        }
    }
    let summaries: Vec<serde_json::Value> = prop_stats.iter().map(|(prop, (sum, sum_sq, n))| {
        let mean = sum / *n as f64;
        let variance = (sum_sq / *n as f64) - mean * mean;
        json!({ "property": prop, "count": n, "mean": mean, "std_dev": variance.sqrt() })
    }).collect();

    let result = json!({
        "session_id": session_id,
        "count": observations.len(),
        "property_summaries": summaries,
        "observations": observations,
    });
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

/// describe_session
///
/// Summarise an observation session for the simops_advisor and simops_narrator.
/// Returns session metadata + per-property statistics + process config snapshot
/// if one was stored as an episode.
///
/// Input: { session_id: UUID }
pub async fn execute_describe_session(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let pool = ctx.db.as_ref().ok_or("describe_session requires database context")?;

    let session_id: Uuid = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: session_id")?
        .parse()
        .map_err(|e| format!("Invalid session_id: {e}"))?;

    // Session metadata
    let session_row = sqlx::query(
        "SELECT s.name, s.description, s.status, s.started_at, s.ended_at, s.metadata,
                p.name as platform_name, p.platform_type, p.description as platform_desc
         FROM observation_sessions s
         JOIN sosa_platforms p ON p.platform_id = s.platform_id
         WHERE s.session_id = $1",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error: {e}"))?
    .ok_or_else(|| format!("Session {} not found", session_id))?;

    // Per-property stats
    let stat_rows = sqlx::query(
        "SELECT observable_property, result_unit,
                COUNT(*) as n,
                MIN(result_value) as min_val,
                MAX(result_value) as max_val,
                AVG(result_value) as mean_val,
                STDDEV(result_value) as std_val,
                MIN(phenomenon_time) as first_t,
                MAX(phenomenon_time) as last_t
         FROM sosa_observations
         WHERE session_id = $1
         GROUP BY observable_property, result_unit
         ORDER BY observable_property",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("DB error: {e}"))?;

    let property_stats: Vec<serde_json::Value> = stat_rows.iter().map(|r| json!({
        "property": r.try_get::<String,_>("observable_property").unwrap_or_default(),
        "unit": r.try_get::<Option<String>,_>("result_unit").unwrap_or(None),
        "count": r.try_get::<i64,_>("n").unwrap_or(0),
        "min": r.try_get::<f64,_>("min_val").ok(),
        "max": r.try_get::<f64,_>("max_val").ok(),
        "mean": r.try_get::<f64,_>("mean_val").ok(),
        "std_dev": r.try_get::<f64,_>("std_val").ok(),
        "first_observation_ms": r.try_get::<i64,_>("first_t").unwrap_or(0),
        "last_observation_ms": r.try_get::<i64,_>("last_t").unwrap_or(0),
    })).collect();

    // Look for a process config snapshot in agent memory
    let process_config_snapshot = if let Some(agent_id) = ctx.current_agent_id {
        sqlx::query(
            "SELECT context FROM episodes
             WHERE agent_id = $1
               AND 'simops_process' = ANY(tags)
             ORDER BY timestamp_ref DESC LIMIT 1",
        )
        .bind(agent_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get::<serde_json::Value,_>("context").ok())
        .and_then(|ctx| ctx.get("process_config").cloned())
    } else {
        None
    };

    let session_meta: serde_json::Value = json!({
        "session_id": session_id,
        "name": session_row.try_get::<String,_>("name").unwrap_or_default(),
        "description": session_row.try_get::<Option<String>,_>("description").unwrap_or(None),
        "status": session_row.try_get::<String,_>("status").unwrap_or_default(),
        "platform": {
            "name": session_row.try_get::<String,_>("platform_name").unwrap_or_default(),
            "type": session_row.try_get::<String,_>("platform_type").unwrap_or_default(),
            "description": session_row.try_get::<Option<String>,_>("platform_desc").unwrap_or(None),
        },
        "metadata": session_row.try_get::<serde_json::Value,_>("metadata").unwrap_or(json!({})),
    });

    let result = json!({
        "session": session_meta,
        "property_count": property_stats.len(),
        "property_stats": property_stats,
        "process_config_snapshot": process_config_snapshot,
    });
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

/// simops_check_constraints
///
/// Validate that an optimizer result respects process constraints declared
/// in the process config (stage efficiency bounds, resource unit compatibility,
/// non-negative quantities).
///
/// Input: {
///   process_name?: str, process_json?: object,
///   optimizer_result: object,    the output of simops_optimize_* or cascade_*
///   constraints?: {              optional additional bounds
///     [feature_name]: { min?: f64, max?: f64 }
///   }
/// }
pub async fn execute_simops_check_constraints(
    input: &serde_json::Value,
    _ctx: &ToolContext,
) -> Result<String, String> {
    let process = resolve_process(input)?;
    process.validate().map_err(|e| e.to_string())?;

    let optimizer_result = input
        .get("optimizer_result")
        .ok_or("Missing required parameter: optimizer_result")?;

    let user_constraints = input.get("constraints").and_then(|v| v.as_object());

    let mut violations: Vec<serde_json::Value> = Vec::new();
    let mut warnings: Vec<serde_json::Value> = Vec::new();

    // Check stage efficiency bounds (efficiency must be 0–1)
    for stage in &process.stages {
        if stage.efficiency <= 0.0 || stage.efficiency > 1.0 {
            violations.push(json!({
                "stage": stage.id,
                "type": "invalid_efficiency",
                "message": format!("Stage '{}' efficiency {} is outside (0, 1]", stage.id, stage.efficiency),
            }));
        }
    }

    // Check stage unit compatibility (already done by validate() but surface it clearly)
    if let Err(e) = process.validate() {
        violations.push(json!({
            "type": "unit_mismatch",
            "message": e.to_string(),
        }));
    }

    // Check optimizer result quantities are non-negative
    if let Some(obj) = optimizer_result.as_object() {
        for (key, val) in obj {
            if let Some(v) = val.as_f64() {
                if v < 0.0 {
                    violations.push(json!({
                        "field": key,
                        "type": "negative_quantity",
                        "value": v,
                        "message": format!("Field '{}' = {} is negative — physical quantities must be ≥ 0", key, v),
                    }));
                }
            }
        }
    }

    // Check user-supplied constraints
    if let Some(constraints) = user_constraints {
        for (feature, bounds) in constraints {
            let val = optimizer_result
                .get(feature)
                .and_then(|v| v.as_f64());
            if let Some(v) = val {
                if let Some(min) = bounds.get("min").and_then(|b| b.as_f64()) {
                    if v < min {
                        violations.push(json!({
                            "field": feature,
                            "type": "below_min",
                            "value": v,
                            "min": min,
                            "message": format!("'{}' = {:.4} is below minimum {:.4}", feature, v, min),
                        }));
                    }
                }
                if let Some(max) = bounds.get("max").and_then(|b| b.as_f64()) {
                    if v > max {
                        warnings.push(json!({
                            "field": feature,
                            "type": "above_max",
                            "value": v,
                            "max": max,
                            "message": format!("'{}' = {:.4} exceeds maximum {:.4} — may be physically infeasible", feature, v, max),
                        }));
                    }
                }
            }
        }
    }

    let feasible = violations.is_empty();
    let result = json!({
        "feasible": feasible,
        "violation_count": violations.len(),
        "warning_count": warnings.len(),
        "violations": violations,
        "warnings": warnings,
        "process_name": process.name,
        "stage_count": process.stages.len(),
        "recommendation": if feasible {
            "Optimizer result is physically consistent. Proceed to simops_write_actuation_plan."
        } else {
            "Violations detected. Review and correct before writing an actuation plan."
        },
    });
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

/// simops_write_actuation_plan
///
/// Persist an optimizer recommendation as an actuation plan in agent episodic
/// memory. This creates a durable record of what the optimizer recommended,
/// why (rationale), and what the operator decided to do with it.
///
/// Input: {
///   session_id: UUID,
///   process_name?: str,
///   optimizer_result: object,      the output of simops_optimize_*
///   rationale: str,                why this plan was chosen
///   decision?: "accept"|"reject"|"modify",   operator decision (default: "proposed")
///   modifications?: object,        any operator-applied changes
///   target_output?: f64,           the production target this plan addresses
/// }
pub async fn execute_simops_write_actuation_plan(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let session_id_str = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: session_id")?;
    let session_id: Uuid = session_id_str.parse()
        .map_err(|e| format!("Invalid session_id: {e}"))?;

    let optimizer_result = input
        .get("optimizer_result")
        .ok_or("Missing required parameter: optimizer_result")?
        .clone();

    let rationale = input
        .get("rationale")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: rationale")?;

    let decision = input.get("decision").and_then(|v| v.as_str()).unwrap_or("proposed");
    let process_name = input.get("process_name").and_then(|v| v.as_str()).unwrap_or("unknown");
    let target_output = input.get("target_output").and_then(|v| v.as_f64());
    let modifications = input.get("modifications").cloned();

    let plan_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    // Store as an episode in agent memory — tagged for dreaming consolidation
    if let Some(agent_id) = ctx.current_agent_id {
        let plan_context = json!({
            "plan_id": plan_id,
            "session_id": session_id,
            "process_name": process_name,
            "optimizer_result": optimizer_result,
            "rationale": rationale,
            "decision": decision,
            "target_output": target_output,
            "modifications": modifications,
            "created_at": now.to_rfc3339(),
            "plan_type": "simops_actuation",
        });

        let episode = agent_bestiary_memory::Episode {
            episode_id: plan_id,
            agent_id,
            timestamp_ref: now,
            query: format!(
                "Actuation plan for process '{}' targeting session {}. Decision: {}.",
                process_name, session_id, decision
            ),
            context: plan_context,
            execution_status: agent_bestiary_memory::ExecutionStatus::Success,
            error_details: None,
            execution_time_ms: 0,
            tokens_used: None,
            cost_usd: None,
            embedding: None,
            consolidated: false,
            tags: vec!["simops_actuation".into(), format!("process:{}", process_name), decision.into()],
            provenance: agent_bestiary_memory::Provenance::AutoPass,
            authority_weight: 0.5,
            dyad_id: None,
            persona_version_at_write: None,
                provider_used: None,
                model_used: None,
        };

        ctx.memory_store.store_episode(episode).await
            .map_err(|e| format!("Failed to store actuation plan: {e}"))?;
    }

    let result = json!({
        "plan_id": plan_id,
        "session_id": session_id,
        "process_name": process_name,
        "decision": decision,
        "target_output": target_output,
        "status": "written",
        "note": "Actuation plan stored in agent episodic memory. It will be consolidated into the agent's knowledge graph during the next dreaming cycle.",
    });
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}
