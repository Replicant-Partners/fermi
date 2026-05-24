//! SimOps cascade executor — the first registered ModelExecutor.
//!
//! Wraps `simops::cascade_forward` to participate in distributional projection.
//! Enabled via the `simops-executor` feature flag.
//!
//! # Config shape
//! The `config` field in [`ModelConfig`] must be a JSON-serialised
//! `simops::ProcessConfig`. Use `serde_json::to_value(&process_config)` to
//! produce it.
//!
//! # Output dimensions
//! - `final_output_quantity` — quantity at the final stage output (e.g. kg biomass)
//! - `net_carbon_kg` — net CO₂-equivalent across all stages
//! - `total_opex_usd` — total operational cost
//! - `system_ner` — system-level Net Energy Ratio (None → NaN, filtered from summaries)

use std::collections::HashMap;
use serde_json::Value;
use simops::{cascade_forward, ProcessConfig};
use crate::executor::ModelExecutor;
use crate::error::ProjectionError;

pub struct SimOpsCascadeExecutor;

impl ModelExecutor for SimOpsCascadeExecutor {
    fn kind(&self) -> &str {
        "simops_cascade"
    }

    fn output_dimensions(&self) -> Vec<String> {
        vec![
            "final_output_quantity".into(),
            "net_carbon_kg".into(),
            "total_opex_usd".into(),
            "system_ner".into(),
        ]
    }

    fn run(
        &self,
        config: &Value,
        run_index: usize,
    ) -> Result<HashMap<String, f64>, ProjectionError> {
        // Deserialise the ProcessConfig from the patched JSON
        let process: ProcessConfig = serde_json::from_value(config.clone())
            .map_err(|e| ProjectionError::ExecutorFailed {
                kind: self.kind().to_string(),
                run_index,
                message: format!("Failed to deserialise ProcessConfig: {e}"),
            })?;

        // Determine input quantity — use the first stage's input as 1.0
        // unit by default (the cascade scales linearly).
        // Callers can sweep the primary input via path "/primary_input_quantity"
        // which is a virtual field resolved here.
        let input_quantity = config
            .get("primary_input_quantity")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0);

        let result = cascade_forward(&process, input_quantity);

        let mut outputs = HashMap::new();
        outputs.insert("final_output_quantity".into(), result.final_output_quantity);
        outputs.insert("net_carbon_kg".into(), result.net_carbon_kg);
        outputs.insert("total_opex_usd".into(), result.total_opex_usd);
        // system_ner is Option<f64> — use NaN to signal "undefined"
        // DistributionSummary filters NaN values automatically
        outputs.insert(
            "system_ner".into(),
            result.system_ner.unwrap_or(f64::NAN),
        );

        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simops::{ProcessConfig, Stage, Resource};
    use serde_json::json;

    fn simple_process() -> ProcessConfig {
        ProcessConfig {
            name: "test".into(),
            description: None,
            feature_of_interest: None,
            elec_price_per_kwh: Some(0.12),
            maintenance_cost_usd: None,
            stages: vec![
                Stage {
                    id: "step1".into(),
                    efficiency: 0.80,
                    carbon_intensity: 0.1,
                    input: Resource {
                        name: "input".into(),
                        unit: "kg".into(),
                        energy_density: None,
                        density_unit: None,
                    },
                    output: Resource {
                        name: "output".into(),
                        unit: "kg".into(),
                        energy_density: None,
                        density_unit: None,
                    },
                    capex: None,
                    opex_per_input_unit: Some(1.0),
                    sidestreams: None,
                    sensors: None,
                },
            ],
        }
    }

    #[test]
    fn runs_cascade_and_returns_outputs() {
        let executor = SimOpsCascadeExecutor;
        let mut config = serde_json::to_value(simple_process()).unwrap();
        config["primary_input_quantity"] = json!(10.0);

        let outputs = executor.run(&config, 0).unwrap();
        assert!(outputs.contains_key("final_output_quantity"));
        // 10 kg × 0.80 efficiency = 8 kg
        assert!((outputs["final_output_quantity"] - 8.0).abs() < 1e-6);
        assert!(outputs.contains_key("net_carbon_kg"));
        assert!(outputs.contains_key("total_opex_usd"));
    }

    #[test]
    fn kind_is_simops_cascade() {
        assert_eq!(SimOpsCascadeExecutor.kind(), "simops_cascade");
    }
}
