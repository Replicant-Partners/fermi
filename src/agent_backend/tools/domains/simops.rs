// src/agent_backend/tools/domains/simops.rs
//
// Phase 2 domain migration: SimOps tools.
//
// Fourteen tools:
//   simops_cascade_forward, simops_cascade_backward, simops_kpi_compute,
//   simops_predictor_train, simops_predictor_forecast,
//   simops_optimize_scale, simops_optimize_single_input,
//   simops_load_process, simops_write_observation,
//   simops_fetch_training_data, get_observations, describe_session,
//   simops_check_constraints, simops_write_actuation_plan.
//
// Each is a zero-size struct implementing PlatformTool. execute() delegates
// to the legacy ToolRegistry::standard() so that dispatch semantics are
// identical to the pre-migration path.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;

/// All SimOps-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![
        Arc::new(SimopsCascadeForward),
        Arc::new(SimopsCascadeBackward),
        Arc::new(SimopsKpiCompute),
        Arc::new(SimopsPredictorTrain),
        Arc::new(SimopsPredictorForecast),
        Arc::new(SimopsOptimizeScale),
        Arc::new(SimopsOptimizeSingleInput),
        Arc::new(SimopsLoadProcess),
        Arc::new(SimopsWriteObservation),
        Arc::new(SimopsFetchTrainingData),
        Arc::new(GetObservations),
        Arc::new(DescribeSession),
        Arc::new(SimopsCheckConstraints),
        Arc::new(SimopsWriteActuationPlan),
    ]
}

// ─── simops_cascade_forward ───────────────────────────────────────────────────

struct SimopsCascadeForward;

#[async_trait]
impl PlatformTool for SimopsCascadeForward {
    fn name(&self) -> &'static str {
        "simops_cascade_forward"
    }

    fn description(&self) -> &'static str {
        "Run a forward cascade through a multi-stage transformation process. Propagates input_quantity through all stages computing output quantities, energy, carbon delta (kg CO₂-eq), stage NER, and OPEX at each step. Returns a CascadeResult with system-level NER, total carbon, and LCC."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "process_name": {
                    "type": "string",
                    "description": "Named process config: 'ambu_bioreactor' or 'scoby_kombucha'. Omit to use ambu_bioreactor as default."
                },
                "process_json": {
                    "type": "object",
                    "description": "Inline process config JSON (overrides process_name). Full ProcessConfig schema."
                },
                "input_quantity": {
                    "type": "number",
                    "description": "Input quantity at stage 0 (in the units of the first stage's input resource)."
                }
            },
            "required": ["input_quantity"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::SimOps
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::simops_tools::execute_simops_cascade_forward(input).await
    }
}

// ─── simops_cascade_backward ──────────────────────────────────────────────────

struct SimopsCascadeBackward;

#[async_trait]
impl PlatformTool for SimopsCascadeBackward {
    fn name(&self) -> &'static str {
        "simops_cascade_backward"
    }

    fn description(&self) -> &'static str {
        "Run a backward cascade to determine the primary input required to produce a specified output. Given target_output at the final stage, back-calculates all intermediate quantities and the required stage-0 input."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "process_name": {
                    "type": "string",
                    "description": "Named process config: 'ambu_bioreactor' or 'scoby_kombucha'."
                },
                "process_json": {
                    "type": "object",
                    "description": "Inline process config JSON (overrides process_name)."
                },
                "target_output": {
                    "type": "number",
                    "description": "Desired output quantity at the final stage (in the final stage's output units)."
                }
            },
            "required": ["target_output"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::SimOps
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::simops_tools::execute_simops_cascade_backward(input).await
    }
}

// ─── simops_kpi_compute ───────────────────────────────────────────────────────

struct SimopsKpiCompute;

#[async_trait]
impl PlatformTool for SimopsKpiCompute {
    fn name(&self) -> &'static str {
        "simops_kpi_compute"
    }

    fn description(&self) -> &'static str {
        "Compute batch KPIs for a fermentation or cultivation run: NER (Net Energy Ratio), SEC (Specific Energy Consumption kWh/kg), LCC (Levelized Cost of Calories $/million kcal), and Harvest Intensity %. Takes measured energy inputs and batch output."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "primary_energy_kwh": {
                    "type": "number",
                    "description": "Primary process energy input (e.g. LED lighting) in kWh."
                },
                "climate_energy_kwh": {
                    "type": "number",
                    "description": "Climate control energy (heating/cooling/Peltier) in kWh."
                },
                "delivery_energy_kwh": {
                    "type": "number",
                    "description": "Pumping and delivery energy in kWh."
                },
                "harvest_energy_kwh": {
                    "type": "number",
                    "description": "Harvest and post-processing energy in kWh."
                },
                "output_mass_kg": {
                    "type": "number",
                    "description": "Harvested output mass in kg (dry weight for biomass)."
                },
                "caloric_density_kcal_g": {
                    "type": "number",
                    "description": "Caloric density of the output in kcal/g."
                },
                "elec_price_per_kwh": {
                    "type": "number",
                    "description": "Electricity price in USD/kWh (e.g. 0.22 for German industrial)."
                },
                "consumables_cost_usd": {
                    "type": "number",
                    "description": "Total consumables cost for the batch in USD (nutrients, substrate, CO₂, etc.)."
                },
                "capex_contribution_usd": {
                    "type": "number",
                    "description": "Amortized CAPEX contribution for this batch in USD (optional, default 0)."
                }
            },
            "required": [
                "primary_energy_kwh",
                "climate_energy_kwh",
                "delivery_energy_kwh",
                "harvest_energy_kwh",
                "output_mass_kg",
                "caloric_density_kcal_g",
                "elec_price_per_kwh",
                "consumables_cost_usd"
            ]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::SimOps
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::simops_tools::execute_simops_kpi_compute(input).await
    }
}

// ─── simops_predictor_train ───────────────────────────────────────────────────

struct SimopsPredictorTrain;

#[async_trait]
impl PlatformTool for SimopsPredictorTrain {
    fn name(&self) -> &'static str {
        "simops_predictor_train"
    }

    fn description(&self) -> &'static str {
        "Fit an OLS linear regression model from historical observations. Takes an array of {features: {k: v, ...}, target: f64} records and returns model coefficients, intercept, R², and feature importance. Model JSON can be passed to simops_predictor_forecast or simops_optimize_* tools."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "observations": {
                    "type": "array",
                    "description": "Array of training observations. Each item must have 'features' (object of string→number) and 'target' (number).",
                    "items": {
                        "type": "object",
                        "properties": {
                            "features": {
                                "type": "object",
                                "additionalProperties": {"type": "number"}
                            },
                            "target": {"type": "number"}
                        },
                        "required": ["features", "target"]
                    },
                    "minItems": 4
                }
            },
            "required": ["observations"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::SimOps
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::simops_tools::execute_simops_predictor_train(input).await
    }
}

// ─── simops_predictor_forecast ────────────────────────────────────────────────

struct SimopsPredictorForecast;

#[async_trait]
impl PlatformTool for SimopsPredictorForecast {
    fn name(&self) -> &'static str {
        "simops_predictor_forecast"
    }

    fn description(&self) -> &'static str {
        "Predict yield or output for a planned operational batch using a trained OLS model. Takes a model_json (from simops_predictor_train) and a feature map. Returns predicted value, R², and caloric-positive/energy-sink status."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "model_json": {
                    "type": "object",
                    "description": "Trained predictor model returned by simops_predictor_train."
                },
                "features": {
                    "type": "object",
                    "description": "Feature map for the planned batch (same keys as training features, e.g. {lighting_kwh: 120, nutrients_g: 6.5, temp_c: 27}).",
                    "additionalProperties": {"type": "number"}
                }
            },
            "required": ["model_json", "features"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::SimOps
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::simops_tools::execute_simops_predictor_forecast(input).await
    }
}

// ─── simops_optimize_scale ────────────────────────────────────────────────────

struct SimopsOptimizeScale;

#[async_trait]
impl PlatformTool for SimopsOptimizeScale {
    fn name(&self) -> &'static str {
        "simops_optimize_scale"
    }

    fn description(&self) -> &'static str {
        "Proportionally scale a reference operating point to hit a target output. All inputs in the reference are scaled by the same factor. Returns scaled input values, predicted output, convergence status, and residual. Use for holistic scale-up planning."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "model_json": {
                    "type": "object",
                    "description": "Trained predictor model from simops_predictor_train."
                },
                "reference": {
                    "type": "object",
                    "description": "Reference operating point: feature map of current/baseline input values.",
                    "additionalProperties": {"type": "number"}
                },
                "target_output": {
                    "type": "number",
                    "description": "Target output value to achieve."
                },
                "max_scale": {
                    "type": "number",
                    "description": "Maximum scaling factor allowed (default: 5.0)."
                }
            },
            "required": ["model_json", "reference", "target_output"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::SimOps
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::simops_tools::execute_simops_optimize_scale(input).await
    }
}

// ─── simops_optimize_single_input ─────────────────────────────────────────────

struct SimopsOptimizeSingleInput;

#[async_trait]
impl PlatformTool for SimopsOptimizeSingleInput {
    fn name(&self) -> &'static str {
        "simops_optimize_single_input"
    }

    fn description(&self) -> &'static str {
        "Solve analytically for a single free input variable to hit a target output, holding all other inputs fixed. Use for questions like 'how much more LED power do I need to produce 5 kg biomass?'. Returns the required value and convergence report."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "model_json": {
                    "type": "object",
                    "description": "Trained predictor model from simops_predictor_train."
                },
                "fixed_inputs": {
                    "type": "object",
                    "description": "Fixed input feature values (all features except the free one).",
                    "additionalProperties": {"type": "number"}
                },
                "free_feature": {
                    "type": "string",
                    "description": "Name of the single input feature to solve for."
                },
                "target_output": {
                    "type": "number",
                    "description": "Target output value to achieve."
                },
                "min_value": {
                    "type": "number",
                    "description": "Minimum allowed value for the free feature (default: 0)."
                },
                "max_value": {
                    "type": "number",
                    "description": "Maximum allowed value for the free feature (default: 1,000,000)."
                }
            },
            "required": ["model_json", "fixed_inputs", "free_feature", "target_output"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::SimOps
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::simops_tools::execute_simops_optimize_single_input(input).await
    }
}

// ─── simops_load_process ──────────────────────────────────────────────────────

struct SimopsLoadProcess;

#[async_trait]
impl PlatformTool for SimopsLoadProcess {
    fn name(&self) -> &'static str {
        "simops_load_process"
    }

    fn description(&self) -> &'static str {
        "Load a SimOps process configuration by name or from agent memory. Returns the full ProcessConfig JSON. Sources: built-ins (ambu_bioreactor, scoby_kombucha), inline process_json, or a config saved in agent episodic memory."
    }

    fn input_schema(&self) -> Value {
        // No "required" field — both inputs are optional.
        json!({
            "type": "object",
            "properties": {
                "process_name": {
                    "type": "string",
                    "description": "Named process: ambu_bioreactor | scoby_kombucha | any custom name saved in memory."
                },
                "process_json": {
                    "type": "object",
                    "description": "Inline ProcessConfig JSON (takes priority over process_name)."
                }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::SimOps
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::simops_tools::execute_simops_load_process(input, ctx).await
    }
}

// ─── simops_write_observation ─────────────────────────────────────────────────

struct SimopsWriteObservation;

#[async_trait]
impl PlatformTool for SimopsWriteObservation {
    fn name(&self) -> &'static str {
        "simops_write_observation"
    }

    fn description(&self) -> &'static str {
        "Write a SOSA observation to the platform store. Use after each measurement cycle to build training data for simops_predictor and the session history."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "UUID of the observation session."
                },
                "observable_property": {
                    "type": "string",
                    "description": "What was measured: e.g. 'biomass_dw_g', 'od600', 'titratable_acidity'."
                },
                "result_value": {
                    "type": "number",
                    "description": "Measured value."
                },
                "result_unit": {
                    "type": "string",
                    "description": "Unit of measurement: g/L, kg, pH, etc."
                },
                "feature_of_interest": {
                    "type": "string",
                    "description": "SOSA FeatureOfInterest URI, e.g. xid:platform/ambu-001."
                },
                "phenomenon_time": {
                    "type": "integer",
                    "description": "Unix milliseconds of measurement (defaults to now)."
                },
                "extra": {
                    "type": "object",
                    "description": "Any additional metadata."
                }
            },
            "required": ["session_id", "observable_property", "result_value"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::SimOps
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::simops_tools::execute_simops_write_observation(input, ctx).await
    }
}

// ─── simops_fetch_training_data ───────────────────────────────────────────────

struct SimopsFetchTrainingData;

#[async_trait]
impl PlatformTool for SimopsFetchTrainingData {
    fn name(&self) -> &'static str {
        "simops_fetch_training_data"
    }

    fn description(&self) -> &'static str {
        "Fetch SOSA observations for a session as structured training data. Groups by phenomenon_time into feature vectors ready for simops_predictor_train."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "UUID of the observation session."
                },
                "feature_properties": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "observable_property names to use as X input features."
                },
                "target_property": {
                    "type": "string",
                    "description": "observable_property name to use as y prediction target."
                },
                "limit": {
                    "type": "integer",
                    "default": 1000,
                    "description": "Max observations per property."
                }
            },
            "required": ["session_id", "feature_properties", "target_property"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::SimOps
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::simops_tools::execute_simops_fetch_training_data(input, ctx).await
    }
}

// ─── get_observations ─────────────────────────────────────────────────────────

struct GetObservations;

#[async_trait]
impl PlatformTool for GetObservations {
    fn name(&self) -> &'static str {
        "get_observations"
    }

    fn description(&self) -> &'static str {
        "Read recent SOSA observations for a session. Returns per-property summary statistics and the raw observation list."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "UUID of the observation session."
                },
                "observable_property": {
                    "type": "string",
                    "description": "Filter to one property (optional)."
                },
                "limit": {
                    "type": "integer",
                    "default": 100,
                    "description": "Max observations to return."
                },
                "from_ms": {
                    "type": "integer",
                    "description": "Unix ms lower bound (optional)."
                },
                "to_ms": {
                    "type": "integer",
                    "description": "Unix ms upper bound (optional)."
                }
            },
            "required": ["session_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::SimOps
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::simops_tools::execute_get_observations(input, ctx).await
    }
}

// ─── describe_session ─────────────────────────────────────────────────────────

struct DescribeSession;

#[async_trait]
impl PlatformTool for DescribeSession {
    fn name(&self) -> &'static str {
        "describe_session"
    }

    fn description(&self) -> &'static str {
        "Summarise an observation session: metadata, per-property statistics, and any saved process config snapshot. Used by simops_advisor and simops_narrator for context."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "UUID of the observation session."
                }
            },
            "required": ["session_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::SimOps
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::simops_tools::execute_describe_session(input, ctx).await
    }
}

// ─── simops_check_constraints ─────────────────────────────────────────────────

struct SimopsCheckConstraints;

#[async_trait]
impl PlatformTool for SimopsCheckConstraints {
    fn name(&self) -> &'static str {
        "simops_check_constraints"
    }

    fn description(&self) -> &'static str {
        "Validate that an optimizer or cascade result is physically feasible: non-negative quantities, stage efficiency bounds, unit compatibility, and any user-supplied min/max constraints."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "process_name": {
                    "type": "string"
                },
                "process_json": {
                    "type": "object"
                },
                "optimizer_result": {
                    "type": "object",
                    "description": "Output of simops_optimize_* or simops_cascade_*."
                },
                "constraints": {
                    "type": "object",
                    "description": "Optional bounds: { feature_name: { min?: f64, max?: f64 } }",
                    "additionalProperties": {"type": "object"}
                }
            },
            "required": ["optimizer_result"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::SimOps
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::simops_tools::execute_simops_check_constraints(input, ctx).await
    }
}

// ─── simops_write_actuation_plan ──────────────────────────────────────────────

struct SimopsWriteActuationPlan;

#[async_trait]
impl PlatformTool for SimopsWriteActuationPlan {
    fn name(&self) -> &'static str {
        "simops_write_actuation_plan"
    }

    fn description(&self) -> &'static str {
        "Persist an optimizer recommendation as an actuation plan in agent episodic memory. Creates a durable record of what was recommended, the rationale, and the operator decision. Feeds the agent's dreaming/consolidation cycle."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "UUID of the observation session this plan addresses."
                },
                "process_name": {
                    "type": "string"
                },
                "optimizer_result": {
                    "type": "object",
                    "description": "The optimizer output being recorded."
                },
                "rationale": {
                    "type": "string",
                    "description": "Why this plan was chosen."
                },
                "decision": {
                    "type": "string",
                    "enum": ["proposed", "accept", "reject", "modify"],
                    "default": "proposed"
                },
                "modifications": {
                    "type": "object",
                    "description": "Operator-applied changes, if any."
                },
                "target_output": {
                    "type": "number",
                    "description": "The production target this plan addresses."
                }
            },
            "required": ["session_id", "optimizer_result", "rationale"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::SimOps
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::simops_tools::execute_simops_write_actuation_plan(input, ctx).await
    }
}

// ─── Tests ─────────────────────────────────────────────────────

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
    fn all_categories_are_simops() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::SimOps,
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
    fn tool_count_is_fourteen() {
        assert_eq!(tools().len(), 14);
    }

    #[test]
    fn names_are_unique() {
        let all = tools();
        let mut seen = std::collections::HashSet::new();
        for tool in &all {
            assert!(
                seen.insert(tool.name()),
                "duplicate tool name: {}",
                tool.name()
            );
        }
    }

    #[test]
    fn requires_workspace_is_false_for_all() {
        for tool in tools() {
            assert!(
                !tool.requires_workspace(),
                "tool `{}` unexpectedly requires workspace",
                tool.name()
            );
        }
    }

    #[test]
    fn response_shapes_are_none() {
        for tool in tools() {
            assert!(
                tool.response_shape().is_none(),
                "tool `{}` declared a response_shape but none were specified in Phase 2",
                tool.name()
            );
        }
    }
}
