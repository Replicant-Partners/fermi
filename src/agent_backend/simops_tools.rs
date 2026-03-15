/// SimOps tool handlers for the agent tool-use system.
///
/// These functions are called by the BuiltinToolRegistry dispatch when an agent
/// invokes a simops_* tool.  They bridge the JSON tool-call interface (agent side)
/// to the deterministic simops crate (compute side) and return JSON strings.
///
/// Tool registry entries live in `tools.rs` builtin_tools() and execute() match.
///
/// Process configs are resolved by name:
///   "ambu_bioreactor" → Chlorella cultivation in the Ambu photobioreactor
///   "scoby_kombucha"  → SCOBY kombucha primary + secondary fermentation
/// Custom configs can be passed inline as a `process_json` field.
use serde_json::json;
use simops::{
    cascade::{cascade_backward, cascade_forward},
    kpi::{compute_kpis, BatchObservation},
    optimizer::{scale_from_reference, single_input_solve},
    predictor::{Predictor, TrainingObservation},
    process::{CapexProfile, ProcessConfig, Resource, Stage},
};
use std::collections::HashMap;

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
