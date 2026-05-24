//! # Projections — Platform-level distributional projection engine
//!
//! Runs any registered deterministic executor N times with inputs sampled
//! from declared distributions, producing distribution summaries per output
//! dimension. Not SimOps-specific: any deterministic model can register as
//! a [`ModelExecutor`].
//!
//! ## Two primitives
//!
//! - [`project_distribution`] — N runs at logical "now", each with varied
//!   inputs. Output: a histogram/percentile summary per output dimension.
//!   Use case: "given uncertainty on my inputs, what is the distribution of
//!   outputs for this batch?"
//!
//! - [`project_timeseries`] — 1+ runs walking forward through discrete
//!   timesteps. Output: distribution per timestep. Use case: "synthesize a
//!   pH curve over 72 hours given kinetic model uncertainty."
//!   (Stub — implementation deferred, interface fixed.)
//!
//! ## Executor registration
//!
//! Implement [`ModelExecutor`] for your model. Register it with
//! [`ExecutorRegistry::register`]. The projection engine calls your executor
//! for each sample run; it knows nothing about your model's internals.

pub mod distribution;
pub mod error;
pub mod executor;
pub mod registry;
pub mod sweep;
pub mod types;

#[cfg(feature = "simops-executor")]
pub mod simops_executor;

pub use distribution::{DistributionSummary, ProjectionOutput};
pub use error::ProjectionError;
pub use executor::ModelExecutor;
pub use registry::ExecutorRegistry;
pub use sweep::{SamplingDistribution, SweepConfig, SweepKind, VariableSweep};
pub use types::{ModelConfig, OutputFormat, ProjectionRequest, ProjectionResponse};

use rand::SeedableRng;
use rand::rngs::StdRng;
use std::collections::HashMap;

// ─── Public API ──────────────────────────────────────────────────────────────

/// Run a distributional projection: execute the model N times with inputs
/// sampled from the declared sweep distributions.
///
/// # Arguments
/// - `request` — what to run, how many times, with what sweep
/// - `registry` — the executor registry (use `ExecutorRegistry::default()` for
///   built-in executors, or build your own)
///
/// # Returns
/// A [`ProjectionResponse`] with aggregate distribution summaries per output
/// dimension (and optionally the raw per-run data).
pub fn project_distribution(
    request: &ProjectionRequest,
    registry: &ExecutorRegistry,
) -> Result<ProjectionResponse, ProjectionError> {
    let executor = registry.get(&request.model.kind)?;
    let n_runs = request.n_runs.min(10_000).max(1);
    let output_dims = executor.output_dimensions();

    let mut rng: StdRng = match request.seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_entropy(),
    };

    // Accumulate per-dimension samples
    let mut accumulators: HashMap<String, Vec<f64>> = output_dims
        .iter()
        .map(|d| (d.clone(), Vec::with_capacity(n_runs)))
        .collect();
    let mut n_failed = 0usize;
    let mut raw_runs: Vec<HashMap<String, f64>> = Vec::new();
    let collect_raw = matches!(request.output_format, OutputFormat::RawRuns | OutputFormat::Both);

    for run_index in 0..n_runs {
        // Clone the base config and patch sampled variable values into it
        let mut config = request.model.config.clone();
        apply_sweep(&mut config, &request.sweep, &mut rng, executor.as_ref())?;

        match executor.run(&config, run_index) {
            Ok(outputs) => {
                for (dim, val) in &outputs {
                    accumulators.entry(dim.clone()).or_default().push(*val);
                }
                if collect_raw {
                    raw_runs.push(outputs);
                }
            }
            Err(_) => {
                n_failed += 1;
            }
        }
    }

    // Build distribution summaries
    let dimensions = output_dims
        .iter()
        .map(|dim| {
            let samples = accumulators.remove(dim).unwrap_or_default();
            DistributionSummary::from_samples(dim.clone(), samples, n_failed)
        })
        .collect();

    let sweep_kind_label = match &request.sweep.kind {
        SweepKind::MonteCarlo => "monte_carlo",
        SweepKind::SensitivityAxis => "sensitivity_axis",
        SweepKind::FromTypicalRange => "from_typical_range",
        SweepKind::TimeEvolution { .. } => "time_evolution",
    };

    Ok(ProjectionResponse {
        output: ProjectionOutput {
            dimensions,
            n_requested: n_runs,
            n_completed: n_runs - n_failed,
            seed: request.seed,
            executor_kind: request.model.kind.clone(),
            sweep_kind: sweep_kind_label.to_string(),
        },
        raw_runs: if collect_raw { raw_runs } else { vec![] },
    })
}

/// Stub for time-series projection. Interface fixed; implementation deferred.
/// Returns `ProjectionError::InvalidSweep` until implemented.
pub fn project_timeseries(
    _request: &ProjectionRequest,
    _registry: &ExecutorRegistry,
) -> Result<ProjectionResponse, ProjectionError> {
    Err(ProjectionError::InvalidSweep(
        "project_timeseries is not yet implemented. \
         Use project_distribution for distribution sampling at logical 'now'."
            .to_string(),
    ))
}

// ─── Sweep application ───────────────────────────────────────────────────────

/// Sample values for each declared sweep variable and patch them into the config.
fn apply_sweep(
    config: &mut serde_json::Value,
    sweep: &SweepConfig,
    rng: &mut StdRng,
    executor: &dyn ModelExecutor,
) -> Result<(), ProjectionError> {
    use rand_distr::{Beta, Distribution, Normal, Triangular, Uniform};

    for var in &sweep.variables {
        let value: f64 = match &var.distribution {
            SamplingDistribution::Uniform { low, high } => {
                // Uniform::new is infallible in rand_distr 0.4
                Uniform::new(*low, *high).sample(rng)
            }
            SamplingDistribution::Normal { mean, std } => {
                Normal::new(*mean, *std)
                    .map_err(|e| ProjectionError::InvalidSweep(e.to_string()))?
                    .sample(rng)
            }
            SamplingDistribution::Triangular { p5, p50, p95 } => {
                // Triangular::new(min, max, mode)
                Triangular::new(*p5, *p95, *p50)
                    .map_err(|e| ProjectionError::InvalidSweep(e.to_string()))?
                    .sample(rng)
            }
            SamplingDistribution::Beta { alpha, beta } => {
                Beta::new(*alpha, *beta)
                    .map_err(|e| ProjectionError::InvalidSweep(e.to_string()))?
                    .sample(rng)
            }
            SamplingDistribution::FromTypicalRange => {
                // TODO: resolve typical_range from config field metadata
                // Deferred until field annotation convention is agreed
                return Err(ProjectionError::InvalidSweep(
                    "FromTypicalRange requires field annotation support — not yet implemented"
                        .to_string(),
                ));
            }
        };

        executor.apply_variable(config, &var.path, value)?;
    }

    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    /// A trivial executor that returns whatever "value" field is in its config.
    struct EchoExecutor;
    impl ModelExecutor for EchoExecutor {
        fn kind(&self) -> &str { "echo" }
        fn output_dimensions(&self) -> Vec<String> { vec!["value".into()] }
        fn run(&self, config: &serde_json::Value, _: usize) -> Result<HashMap<String, f64>, ProjectionError> {
            let v = config["value"].as_f64().unwrap_or(0.0);
            Ok(HashMap::from([("value".into(), v)]))
        }
    }

    fn echo_registry() -> ExecutorRegistry {
        let mut r = ExecutorRegistry::new();
        r.register(std::sync::Arc::new(EchoExecutor));
        r
    }

    #[test]
    fn project_distribution_basic() {
        let request = ProjectionRequest {
            model: ModelConfig {
                kind: "echo".into(),
                config: json!({ "value": 1.0 }),
            },
            sweep: SweepConfig {
                kind: SweepKind::MonteCarlo,
                variables: vec![VariableSweep {
                    path: "/value".into(),
                    distribution: SamplingDistribution::Uniform { low: 1.0, high: 3.0 },
                    label: None,
                }],
            },
            n_runs: 500,
            seed: Some(42),
            output_format: OutputFormat::Aggregate,
        };

        let registry = echo_registry();
        let response = project_distribution(&request, &registry).unwrap();
        let summary = response.output.dimension("value").unwrap();

        assert_eq!(summary.n_runs, 500);
        assert!(summary.mean > 1.5 && summary.mean < 2.5, "mean={}", summary.mean);
        assert!(summary.p5 >= 1.0);
        assert!(summary.p95 <= 3.0);
    }

    #[test]
    fn unknown_executor_returns_error() {
        let request = ProjectionRequest {
            model: ModelConfig {
                kind: "nonexistent".into(),
                config: json!({}),
            },
            sweep: SweepConfig::default(),
            n_runs: 10,
            seed: None,
            output_format: OutputFormat::Aggregate,
        };
        let registry = ExecutorRegistry::new();
        let result = project_distribution(&request, &registry);
        assert!(matches!(result, Err(ProjectionError::UnknownExecutor(_))));
    }

    #[test]
    fn project_timeseries_returns_stub_error() {
        let request = ProjectionRequest {
            model: ModelConfig { kind: "echo".into(), config: json!({}) },
            sweep: SweepConfig::default(),
            n_runs: 1,
            seed: None,
            output_format: OutputFormat::Aggregate,
        };
        let registry = echo_registry();
        let result = project_timeseries(&request, &registry);
        assert!(matches!(result, Err(ProjectionError::InvalidSweep(_))));
    }

    #[test]
    fn reproducible_with_seed() {
        let request = ProjectionRequest {
            model: ModelConfig {
                kind: "echo".into(),
                config: json!({ "value": 1.0 }),
            },
            sweep: SweepConfig {
                kind: SweepKind::MonteCarlo,
                variables: vec![VariableSweep {
                    path: "/value".into(),
                    distribution: SamplingDistribution::Normal { mean: 5.0, std: 1.0 },
                    label: None,
                }],
            },
            n_runs: 200,
            seed: Some(99),
            output_format: OutputFormat::Aggregate,
        };

        let registry = echo_registry();
        let r1 = project_distribution(&request, &registry).unwrap();
        let r2 = project_distribution(&request, &registry).unwrap();
        let s1 = r1.output.dimension("value").unwrap();
        let s2 = r2.output.dimension("value").unwrap();
        assert_eq!(s1.mean.to_bits(), s2.mean.to_bits(), "same seed must produce identical output");
    }
}
