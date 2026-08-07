//! SimOps cascade executor — ModelExecutor for distributional projection.
//!
//! Wraps the SimOps cascade engine to participate in `project_distribution`.
//! Enabled via the `simops-executor` feature flag.
//!
//! ## Schema version
//!
//! As of spec 30 / 30.5, this executor accepts **schema_version: 2** only.
//! v1 configs (singular `input`/`output`, no `schema_version`) are rejected
//! with a clear error pointing to spec 30.
//!
//! ## Config shape (v2)
//!
//! The `config` field in [`ModelConfig`] must be a JSON-serialised
//! `simops::ProcessConfigV2`. Fields to sweep are referenced via JSON Pointer
//! into the ProcessConfigV2 structure. One virtual field is also supported:
//!
//! - `/throughput/qty_per_run` — sweep the basis input quantity directly
//!
//! ## Output dimensions
//!
//! Per-stage and process-total outputs from the v2 cascade response:
//! - `total_output_kg` — final stage total output (kg)
//! - `total_opex_per_run_eur` — process-level OPEX
//! - `total_revenue_per_run_eur` — process-level revenue
//! - `margin_per_run_eur` — revenue − OPEX
//! - `carbon_kg_co2_per_run` — net carbon

use crate::error::ProjectionError;
use crate::executor::ModelExecutor;
use serde_json::Value;
use simops::{
    cascade_v2::cascade_v2,
    process_v2::{CascadeRequestV2, ProcessConfigV2, ScaleRequest},
};
use std::collections::HashMap;

pub struct SimOpsCascadeExecutor;

impl ModelExecutor for SimOpsCascadeExecutor {
    fn kind(&self) -> &str {
        "simops_cascade"
    }

    fn output_dimensions(&self) -> Vec<String> {
        vec![
            "total_output_kg".into(),
            "total_opex_per_run_eur".into(),
            "total_revenue_per_run_eur".into(),
            "margin_per_run_eur".into(),
            "carbon_kg_co2_per_run".into(),
        ]
    }

    fn run(
        &self,
        config: &Value,
        run_index: usize,
    ) -> Result<HashMap<String, f64>, ProjectionError> {
        // Check schema version — reject v1 configs explicitly
        let schema_version = config
            .get("schema_version")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;

        if schema_version != 2 {
            return Err(ProjectionError::ExecutorFailed {
                kind: self.kind().to_string(),
                run_index,
                message: format!(
                    "ProcessConfig schema_version must be 2 (got: {schema_version}). \
                     See kask spec 30. POST /api/simops/project requires v2 ProcessConfig."
                ),
            });
        }

        // Deserialise as v2 ProcessConfig
        let process: ProcessConfigV2 = serde_json::from_value(config.clone()).map_err(|e| {
            ProjectionError::ExecutorFailed {
                kind: self.kind().to_string(),
                run_index,
                message: format!("Failed to deserialise ProcessConfigV2: {e}"),
            }
        })?;

        let req = CascadeRequestV2 {
            process,
            direction: "forward".into(),
            scale: ScaleRequest::FromThroughput,
            twin: None,
        };

        let result = cascade_v2(&req).map_err(|e| ProjectionError::ExecutorFailed {
            kind: self.kind().to_string(),
            run_index,
            message: e.to_string(),
        })?;

        // Extract key outputs for distributional analysis
        let last_stage = result.stages.last();
        let total_output_kg = last_stage
            .map(|s| s.mass_balance.total_output_kg)
            .unwrap_or(0.0);

        let mut outputs = HashMap::new();
        outputs.insert("total_output_kg".into(), total_output_kg);
        outputs.insert(
            "total_opex_per_run_eur".into(),
            result.process_totals.total_opex_per_run_eur,
        );
        outputs.insert(
            "total_revenue_per_run_eur".into(),
            result.process_totals.total_revenue_per_run_eur,
        );
        outputs.insert(
            "margin_per_run_eur".into(),
            result.process_totals.margin_per_run_eur,
        );
        outputs.insert(
            "carbon_kg_co2_per_run".into(),
            result.process_totals.carbon_kg_co2_per_run,
        );

        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use simops::process_v2::*;

    fn simple_v2_process() -> ProcessConfigV2 {
        ProcessConfigV2 {
            schema_version: 2,
            name: "test".into(),
            description: None,
            throughput: Throughput {
                basis_stage: Some("s1".into()),
                basis_input: Some("w".into()),
                qty_per_run: 10.0,
                qty_unit: "L".into(),
                runs_per_year: Some(10.0),
            },
            stages: vec![StageV2 {
                id: "s1".into(),
                name: None,
                description: None,
                inputs: vec![Input {
                    name: "w".into(),
                    role: InputRole::Principal,
                    qty: Some(1.0),
                    qty_unit: Some("L".into()),
                    per: None,
                    per_unit: None,
                    per_basis: Some(PerBasis::Principal),
                    from_stage: None,
                    from_output: None,
                    unit_cost: Some(0.001),
                    cost_unit: Some("eur_per_L".into()),
                    cost_source: None,
                    risk_flags: None,
                    density_kg_per_unit: Some(1.0),
                    mass_balance: None,
                }],
                outputs: vec![Output {
                    name: "output".into(),
                    role: OutputRole::Product,
                    qty_per_input_kg: None,
                    qty_unit: "kg".into(),
                    density_kg_per_unit: Some(1.0),
                    capture_fraction: None,
                    value_per_unit_usd: Some(2.0),
                    disposal_cost_per_unit_usd: None,
                }],
                efficiency: 0.80,
                power_kwh_per_input_kg: None,
                labor_hours_per_input_kg: None,
                carbon_intensity: None,
                duration_hours: None,
            }],
            elec_price_per_kwh: None,
            labor_cost_per_hour: None,
            carbon_price_per_tonne: None,
        }
    }

    #[test]
    fn runs_v2_cascade_and_returns_outputs() {
        let executor = SimOpsCascadeExecutor;
        let config = serde_json::to_value(simple_v2_process()).unwrap();
        let outputs = executor.run(&config, 0).unwrap();
        assert!(outputs.contains_key("total_output_kg"));
        // 10 kg in × 0.80 efficiency = 8 kg out
        assert!(
            (outputs["total_output_kg"] - 8.0).abs() < 1e-6,
            "expected 8.0, got {}",
            outputs["total_output_kg"]
        );
        assert!(outputs.contains_key("total_opex_per_run_eur"));
        assert!(outputs.contains_key("margin_per_run_eur"));
    }

    #[test]
    fn v1_config_rejected_with_clear_error() {
        let executor = SimOpsCascadeExecutor;
        // v1 config: no schema_version, singular input/output
        let v1_config = json!({
            "name": "v1_process",
            "stages": [{"id": "s1", "input": {"name":"w","unit":"L"}, "output": {"name":"o","unit":"L"}, "efficiency": 0.9}]
        });
        let result = executor.run(&v1_config, 0);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("schema_version must be 2"), "error was: {msg}");
    }

    #[test]
    fn kind_is_simops_cascade() {
        assert_eq!(SimOpsCascadeExecutor.kind(), "simops_cascade");
    }
}
