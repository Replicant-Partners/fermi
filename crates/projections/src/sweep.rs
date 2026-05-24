use serde::{Deserialize, Serialize};

/// How inputs are varied across the N runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SweepKind {
    /// Sample all declared variables jointly from their distributions.
    /// N runs total, each run varies all declared variables simultaneously.
    /// This is the default and covers the vast majority of use cases.
    MonteCarlo,

    /// Sample each variable uniformly from its declared range, one at a time,
    /// holding others at their nominal/midpoint value.
    /// Produces N runs per variable — useful for 1D sensitivity curves.
    SensitivityAxis,

    /// Use typical_range from the model config's field metadata.
    /// The engine walks the model config, finds every field with a
    /// `typical_range` annotation, and samples uniform within it.
    /// Requires no explicit variable declarations.
    FromTypicalRange,

    /// Walk forward through discrete timesteps.
    /// Each step advances the model by `step_size` time units.
    /// STUB — interface fixed, implementation deferred.
    TimeEvolution {
        steps: u32,
        step_size_seconds: f64,
    },
}

impl Default for SweepKind {
    fn default() -> Self {
        Self::MonteCarlo
    }
}

/// A single variable to vary in the sweep, identified by a JSON Pointer path
/// into the model config, with a declared sampling distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableSweep {
    /// JSON Pointer path into the model config.
    /// Examples:
    ///   "/stages/0/efficiency"
    ///   "/elec_price_per_kwh"
    ///   "/stages/2/opex_per_input_unit"
    pub path: String,

    /// Distribution to sample from for this variable.
    pub distribution: SamplingDistribution,

    /// Human-readable label for reporting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Sampling distributions available for sweep variables.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SamplingDistribution {
    Uniform { low: f64, high: f64 },
    Normal  { mean: f64, std: f64 },
    /// Triangular parameterised as (p5, p50, p95) — matches FPL convention.
    Triangular { p5: f64, p50: f64, p95: f64 },
    Beta    { alpha: f64, beta: f64 },
    /// Use the typical_range declared in the model config field.
    /// The engine resolves this at runtime from the config metadata.
    FromTypicalRange,
}

/// Full sweep configuration for a projection run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepConfig {
    #[serde(default)]
    pub kind: SweepKind,

    /// Variables to vary. Empty = use FromTypicalRange on all annotated fields.
    #[serde(default)]
    pub variables: Vec<VariableSweep>,
}

impl Default for SweepConfig {
    fn default() -> Self {
        Self {
            kind: SweepKind::MonteCarlo,
            variables: vec![],
        }
    }
}
