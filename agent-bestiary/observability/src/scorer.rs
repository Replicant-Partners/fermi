//! `EpisodeScorer` — inline timeline-entry write.
//!
//! Per Q4 (c) — scorer runs on the hot path so the observatory
//! dashboard never lags behind reality. It produces the bare-minimum
//! row: episode/run/persona/dyad context, dim_scores from the
//! aggregated signal. Drift and anomaly fields are left null and
//! filled in by the background scanner (see `worker.rs`).

use std::sync::Arc;
use uuid::Uuid;

use agent_bestiary_evaluators::AggregatedSignal;
use agent_bestiary_memory::{Episode, MemoryStore, TimelineEntry};

use crate::error::ObservabilityError;

/// Inline scorer. Stateless wrapper over the memory store.
#[derive(Clone)]
pub struct EpisodeScorer {
    store: Arc<MemoryStore>,
}

impl EpisodeScorer {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }

    /// Project an `(Episode, AggregatedSignal)` pair into a timeline
    /// row and persist it. Returns the new `entry_id`.
    pub async fn write_inline(
        &self,
        episode: &Episode,
        signal: &AggregatedSignal,
        run_id: Option<Uuid>,
        session_id: Option<String>,
    ) -> Result<Uuid, ObservabilityError> {
        let dim_scores = serde_json::to_value(
            signal
                .per_dimension
                .iter()
                .map(|d| (d.dimension.as_str().to_string(), d.mean))
                .collect::<std::collections::BTreeMap<_, _>>(),
        )
        .map_err(|e| ObservabilityError::Storage(format!("dim_scores serialize: {}", e)))?;

        let entry = TimelineEntry {
            entry_id: Uuid::new_v4(),
            agent_id: episode.agent_id,
            episode_id: Some(episode.episode_id),
            run_id,
            persona_version: episode.persona_version_at_write.unwrap_or(1),
            dyad_id: episode.dyad_id.clone(),
            session_id,
            provenance: episode.provenance.to_string(),
            dim_scores,
            // Drift / anomaly fields filled by the background scanner.
            drift_norm: None,
            within_version_cosine: None,
            anomaly_flags: serde_json::json!([]),
            created_at: chrono::Utc::now(),
        };

        let entry_id = self
            .store
            .create_timeline_entry(&entry)
            .await
            .map_err(|e| ObservabilityError::Storage(e.to_string()))?;

        Ok(entry_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_bestiary_evaluators::{AggregatedSignal, Dimension, DimensionAggregate};

    #[test]
    fn entry_serializes_dim_scores_as_object() {
        let agg = AggregatedSignal {
            per_dimension: vec![
                DimensionAggregate {
                    dimension: Dimension::new("rapport"),
                    mean: 0.74,
                    contributions: vec![],
                    conflict: false,
                    spread: 0.0,
                },
                DimensionAggregate {
                    dimension: Dimension::new("forecast_calibration"),
                    mean: 0.61,
                    contributions: vec![],
                    conflict: false,
                    spread: 0.0,
                },
            ],
            conflicts: vec![],
            flags: vec![],
            active_evaluators: vec![],
            inapplicable_evaluators: vec![],
            failed_evaluators: vec![],
        };

        // Reproduce the dim_scores serialization the scorer would do.
        let dims: std::collections::BTreeMap<String, f64> = agg
            .per_dimension
            .iter()
            .map(|d| (d.dimension.as_str().to_string(), d.mean))
            .collect();
        let v = serde_json::to_value(&dims).unwrap();

        let obj = v.as_object().unwrap();
        assert_eq!(obj["rapport"].as_f64().unwrap(), 0.74);
        assert_eq!(obj["forecast_calibration"].as_f64().unwrap(), 0.61);
    }
}
