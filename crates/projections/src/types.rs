use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::sweep::SweepConfig;
use crate::distribution::ProjectionOutput;

/// Which model to run and its configuration.
/// The `kind` string must match a registered [`ModelExecutor`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Executor kind string. E.g. "simops_cascade", "fermi_forecast", "predictor".
    pub kind: String,
    /// Model-specific configuration. For cascade: ProcessConfig JSON.
    /// For fermi forecast: FPL program string wrapped as `{"fpl": "..."}`.
    pub config: Value,
}

/// Full request to the projection engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionRequest {
    pub model: ModelConfig,
    pub sweep: SweepConfig,
    /// Number of runs (samples). Default 100. Max 10000.
    #[serde(default = "default_n_runs")]
    pub n_runs: usize,
    /// Optional seed for reproducibility. Same {model, sweep, n_runs, seed} → identical output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Output format. Default "aggregate".
    #[serde(default)]
    pub output_format: OutputFormat,
}

fn default_n_runs() -> usize { 100 }

/// What the projection engine returns.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    /// Return distribution summaries (percentiles + histogram) per output dimension.
    /// No per-run data. Fast. Default.
    #[default]
    Aggregate,
    /// Return all N run outputs as a flat array of maps.
    /// Useful for downstream analysis; expensive over the wire at high N.
    RawRuns,
    /// Both aggregate and raw runs.
    Both,
}

/// Full response from the projection engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionResponse {
    pub output: ProjectionOutput,
    /// Raw per-run results. Only populated when OutputFormat is RawRuns or Both.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub raw_runs: Vec<std::collections::HashMap<String, f64>>,
}
