use std::collections::BTreeMap;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ModelManifest {
    pub uri: String,
    pub version: String,
    pub name: String,
    pub description: String,
    /// Property URIs that this model evolves together (SET semantics).
    pub applies_to_set: Vec<String>,
    pub state_schema: BTreeMap<String, StateFieldSchema>,
    pub params_schema: BTreeMap<String, ParamSchema>,
    pub context_schema: BTreeMap<String, ContextSchema>,
    pub default_params: BTreeMap<String, f64>,
    pub default_integrator: String,
    pub default_step_days: f64,
    pub citations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StateFieldSchema {
    pub label: String,
    pub units: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typical_range: Option<(f64, f64)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamSchema {
    pub label: String,
    pub units: String,
    pub description: String,
    pub default: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typical_range: Option<(f64, f64)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextSchema {
    pub label: String,
    pub units: String,
    pub description: String,
    pub source: ContextSource,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextSource {
    /// Sourced from a named field in the ProcessConfig.
    ProcessField { path: String },
    /// Operator provides it explicitly.
    OperatorInput,
    /// Scenario override value.
    ScenarioOverride,
}
