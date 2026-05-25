use serde::Serialize;
use crate::{ModelManifest, SkillInput};

#[derive(Debug, Clone, Serialize)]
pub struct Provenance {
    pub model_uri: String,
    pub model_version: String,
    pub params_used: serde_json::Value,
    pub context_used: serde_json::Value,
    pub initial_state: serde_json::Value,
    pub integrator: String,
    pub step_size_days: f64,
    pub generated_at: String,
    pub projection_id: String,
    pub generated_by: String,
}

pub fn build(manifest: &ModelManifest, input: &SkillInput, step_days: f64) -> Provenance {
    // Merge default params with overrides
    let mut params = manifest.default_params.clone();
    for (k, v) in &input.params_override {
        params.insert(k.clone(), *v);
    }

    Provenance {
        model_uri: manifest.uri.clone(),
        model_version: manifest.version.clone(),
        params_used: serde_json::to_value(&params).unwrap_or_default(),
        context_used: input.process_context.clone(),
        initial_state: serde_json::to_value(&input.initial_state).unwrap_or_default(),
        integrator: input.integrator.clone().unwrap_or_else(|| manifest.default_integrator.clone()),
        step_size_days: step_days,
        generated_at: chrono::Utc::now().to_rfc3339(),
        projection_id: uuid::Uuid::new_v4().to_string(),
        generated_by: input.generated_by.clone().unwrap_or_else(|| "system".into()),
    }
}
