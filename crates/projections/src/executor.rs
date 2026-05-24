use std::collections::HashMap;
use serde_json::Value;
use crate::error::ProjectionError;

/// The contract every deterministic model must implement to participate
/// in distributional projection.
///
/// # Design
/// The executor receives a model config (as a JSON Value so the projection
/// engine stays model-agnostic) with some fields patched to the sampled
/// values for this run. It returns a flat map of output dimension names
/// to scalar values.
///
/// # Example outputs
/// SimOps cascade: `{"final_output_quantity": 4.2, "net_carbon_kg": -12.4, "total_opex_usd": 18.5}`
/// Fermi forecast: `{"outcome": 0.73}`
/// Predictor:      `{"yield_kg": 4.8}`
pub trait ModelExecutor: Send + Sync {
    /// Unique identifier for this executor.
    /// Used as the `kind` discriminator in [`ModelConfig`].
    fn kind(&self) -> &str;

    /// Run the model once with the given config (already patched with sampled
    /// input values for this run) and return scalar outputs.
    ///
    /// # Errors
    /// Return `ProjectionError::ExecutorFailed` if the model cannot produce
    /// valid outputs for the given config. The projection engine will record
    /// the failure and continue with remaining runs.
    fn run(
        &self,
        config: &Value,
        run_index: usize,
    ) -> Result<HashMap<String, f64>, ProjectionError>;

    /// Names of the output dimensions this executor produces.
    /// Used to pre-allocate the output accumulator and validate results.
    fn output_dimensions(&self) -> Vec<String>;

    /// Optional: apply a sampled variable value to the config.
    /// The default implementation uses JSON Pointer to set the value.
    /// Override if your config uses a non-standard patching strategy.
    fn apply_variable(
        &self,
        config: &mut Value,
        json_pointer_path: &str,
        value: f64,
    ) -> Result<(), ProjectionError> {
        // Navigate to parent, set the leaf value
        if let Some(target) = config.pointer_mut(json_pointer_path) {
            *target = Value::from(value);
            Ok(())
        } else {
            Err(ProjectionError::VariableNotFound {
                path: json_pointer_path.to_string(),
                message: format!(
                    "JSON Pointer '{}' does not resolve in the model config",
                    json_pointer_path
                ),
            })
        }
    }
}
