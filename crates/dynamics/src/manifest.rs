use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

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

/// How a model contributes to a shared state variable in a coupled run.
///
/// When multiple models are integrated as a coupled system, each model that
/// declares a variable in its `state_schema` contributes a `dy/dt` term.
/// This enum controls how those contributions are combined.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContributionMode {
    /// Model's dy/dt is summed with other models' contributions. Default.
    #[default]
    Additive,
    /// Model's dy/dt replaces all other contributions for this variable.
    /// At most one model per variable may declare Replacement — validated
    /// before integration starts.
    Replacement,
    /// Model reads the variable but contributes nothing to its derivative.
    /// Used for substrates that influence rates but aren't depleted by THIS model.
    ReadOnly,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct StateFieldSchema {
    pub label: String,
    pub units: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typical_range: Option<(f64, f64)>,
    /// Contribution mode in a coupled multi-model run. Default: Additive.
    #[serde(default)]
    pub contribution: ContributionMode,
}

impl StateFieldSchema {
    /// Convenience constructor — all fields explicit, contribution defaults to Additive.
    pub fn new(
        label: impl Into<String>,
        units: impl Into<String>,
        description: impl Into<String>,
        typical_range: Option<(f64, f64)>,
    ) -> Self {
        Self {
            label: label.into(),
            units: units.into(),
            description: description.into(),
            typical_range,
            contribution: ContributionMode::Additive,
        }
    }
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
