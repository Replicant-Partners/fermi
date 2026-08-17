//! `ProjectionScoringEvaluator` — deferred hard verifier for SimOps
//! dynamics projections (spec 20).
//!
//! When a real SOSA observation arrives (procedure ≠ simops_simulation),
//! this evaluator finds the prior synthetic prediction for the same
//! (observable_property, feature_of_interest) and computes the relative
//! error. The resulting score is written as an `EvalSignal` on the episode
//! that ran the projection, feeding the dreaming cycle's quality-weighted
//! consolidation.
//!
//! The signal is honest in a way operator-acceptance signals are not:
//! the batch completes independently of what the model predicted.
//!
//! ## Lookup contract (trait-injected, no sqlx in this crate)
//!
//! [`ProjectionLookup`] is the DB boundary. The main binary wires a
//! sqlx implementation; tests use stubs. Mirrors the BrierLookup pattern.
//!
//! ## Score formula
//!
//! ```text
//! relative_error = |predicted - actual| / max(|actual|, 1e-9)
//! score          = 1.0 - min(relative_error, 1.0)
//! ```
//!
//! 1.0 = exact match. 0.0 = ≥100% relative error.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::model::EvalModel;
use crate::result::{Dimension, EvalResult};
use crate::tier::EvalTier;
use crate::EvalError;
use agent_bestiary_memory::EpisodeBundle;

// ─── Lookup contract ──────────────────────────────────────────────────────────

/// A prior synthetic projection observation retrieved from the DB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionObservation {
    /// The predicted value (from the synthetic observation).
    pub predicted_value: f64,
    /// The actual measured value (from the real observation).
    pub actual_value: f64,
    /// Which model produced the prediction.
    pub model_uri: Option<String>,
    /// Stage the prediction was for.
    pub stage_id: Option<String>,
    /// The observable property (e.g. "bio:bc_yield_g_per_l").
    pub observable_property: String,
    /// How many prior observations exist for this (model_uri, observable_property) pair.
    /// Used to compute confidence — low n = low confidence.
    pub n_prior: u32,
    /// Temperature context when the projection was made (for rule flags).
    pub temperature_c: Option<f64>,
    /// Number of parallel instances (for rule flags).
    pub n_instances: Option<u32>,
}

/// DB boundary — wired to sqlx in the main binary, stubbed in tests.
#[async_trait]
pub trait ProjectionLookup: Send + Sync {
    /// Given the `projection_id` from `bundle.context->>'projection_id'`,
    /// look up the synthetic prediction and match it against real observations
    /// that reference it. Returns None when no match is found (evaluator
    /// returns Inapplicable).
    ///
    /// Fallback: when `projection_id` is absent, look up by
    /// `(observable_property, feature_of_interest)` within the last 30 days.
    async fn find_projection_match(
        &self,
        projection_id: Option<&str>,
        agent_id: uuid::Uuid,
    ) -> Result<Option<ProjectionObservation>, EvalError>;
}

// ─── Evaluator ────────────────────────────────────────────────────────────────

pub struct ProjectionScoringEvaluator {
    lookup: Arc<dyn ProjectionLookup>,
}

impl ProjectionScoringEvaluator {
    pub fn new(lookup: Arc<dyn ProjectionLookup>) -> Self {
        Self { lookup }
    }
}

#[async_trait]
impl EvalModel for ProjectionScoringEvaluator {
    fn name(&self) -> &'static str {
        "projection_scoring"
    }
    fn version(&self) -> &'static str {
        "v1"
    }
    fn tier(&self) -> EvalTier {
        EvalTier::Dimensional
    }
    fn dimensions(&self) -> Vec<Dimension> {
        vec![Dimension::new("projection_accuracy")]
    }

    async fn evaluate(&self, bundle: &EpisodeBundle) -> Result<EvalResult, EvalError> {
        // Only applies to simops dynamics/cascade agents
        let agent_name = bundle
            .agent_card
            .as_ref()
            .map(|c| c.agent_name.as_str())
            .unwrap_or("");
        if !agent_name.contains("dynamics_runner") && !agent_name.contains("simops_cascade") {
            return Err(EvalError::Inapplicable(
                "projection_scoring only applies to simops dynamics and cascade agents".into(),
            ));
        }

        // Extract projection_id from episode context
        let projection_id = bundle.context.get("projection_id").and_then(|v| v.as_str());

        let obs = self
            .lookup
            .find_projection_match(projection_id, bundle.agent_id)
            .await?;

        let Some(obs) = obs else {
            return Err(EvalError::Inapplicable(
                "no matching real observation found for this projection".into(),
            ));
        };

        // Score: 1 - relative_error, clamped [0, 1]
        let relative_error =
            (obs.predicted_value - obs.actual_value).abs() / obs.actual_value.abs().max(1e-9);
        let score = (1.0 - relative_error.min(1.0)).clamp(0.0, 1.0);

        // Confidence: rises with prior observation count, saturates at n=10
        let confidence = ((obs.n_prior as f64) / 10.0).clamp(0.1, 1.0);

        // Direction flag
        let delta_direction = if (obs.predicted_value - obs.actual_value).abs() < 1e-9 {
            "exact"
        } else if obs.predicted_value > obs.actual_value {
            "over"
        } else {
            "under"
        };

        use crate::result::EvalFlag;

        let rationale = format!(
            "projection {}: predicted {:.3}, actual {:.3} ({:.1}% {} — score {:.3})",
            obs.observable_property,
            obs.predicted_value,
            obs.actual_value,
            relative_error * 100.0,
            delta_direction,
            score,
        );

        let mut result = EvalResult::new(self.name(), self.version())
            .with_score(Dimension::new("projection_accuracy"), score)
            .with_confidence(confidence)
            .with_rationale(rationale)
            .with_flag(EvalFlag {
                kind: "delta_direction".into(),
                value: delta_direction.into(),
            })
            .with_flag(EvalFlag {
                kind: "observable_property".into(),
                value: obs.observable_property.clone(),
            })
            .with_flag(EvalFlag {
                kind: "relative_error".into(),
                value: format!("{relative_error:.4}"),
            })
            .with_flag(EvalFlag {
                kind: "predicted".into(),
                value: format!("{:.4}", obs.predicted_value),
            })
            .with_flag(EvalFlag {
                kind: "actual".into(),
                value: format!("{:.4}", obs.actual_value),
            });

        if let Some(ref mu) = obs.model_uri {
            result = result.with_flag(EvalFlag {
                kind: "model_uri".into(),
                value: mu.clone(),
            });
        }
        if let Some(ref sid) = obs.stage_id {
            result = result.with_flag(EvalFlag {
                kind: "stage_id".into(),
                value: sid.clone(),
            });
        }
        if let Some(pid) = projection_id {
            result = result.with_flag(EvalFlag {
                kind: "projection_id".into(),
                value: pid.into(),
            });
        }

        Ok(result)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use agent_bestiary_memory::{Episode, EpisodeBundle, ExecutionStatus, Provenance};
    use chrono::Utc;
    use uuid::Uuid;

    struct StubLookup(Option<ProjectionObservation>);

    #[async_trait]
    impl ProjectionLookup for StubLookup {
        async fn find_projection_match(
            &self,
            _projection_id: Option<&str>,
            _agent_id: Uuid,
        ) -> Result<Option<ProjectionObservation>, EvalError> {
            Ok(self.0.clone())
        }
    }

    fn dynamics_bundle(projection_id: Option<&str>, predicted: f64) -> EpisodeBundle {
        let ep = Episode {
            response_text: None,
            assertions: None,
            episode_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            timestamp_ref: Utc::now(),
            query: "run projection".into(),
            context: if let Some(pid) = projection_id {
                serde_json::json!({ "projection_id": pid, "predicted_value": predicted })
            } else {
                serde_json::json!({ "predicted_value": predicted })
            },
            execution_status: ExecutionStatus::Success,
            error_details: None,
            execution_time_ms: 100,
            tokens_used: None,
            cost_usd: None,
            input_tokens: None,
            output_tokens: None,
            cost_basis: None,
            cost_rate_key: None,
            parent_episode_id: None,
            embedding: None,
            consolidated: false,
            tags: vec![],
            provenance: Provenance::AutoPass,
            authority_weight: 0.5,
            dyad_id: None,
            persona_version_at_write: None,
            provider_used: None,
            model_used: None,
        };
        let mut bundle = EpisodeBundle::from_episode(&ep);
        // Simulate a dynamics_runner agent card
        bundle.agent_card = Some(agent_bestiary_memory::AgentCardSnapshot {
            agent_id: uuid::Uuid::new_v4(),
            agent_name: "simops_dynamics_runner".into(),
            agent_type: "research".into(),
            model: "claude-sonnet-4-6".into(),
            system_prompt: None,
            temperature: 0.1,
        });
        bundle
    }

    fn obs(predicted: f64, actual: f64, n: u32) -> ProjectionObservation {
        ProjectionObservation {
            predicted_value: predicted,
            actual_value: actual,
            model_uri: Some("kask:dynamics/kombucha_fermentation@v1".into()),
            stage_id: Some("primary_fermentation".into()),
            observable_property: "bio:bc_yield_g_per_l".into(),
            n_prior: n,
            temperature_c: Some(26.0),
            n_instances: Some(1),
        }
    }

    #[tokio::test]
    async fn perfect_prediction_scores_one() {
        let lookup = Arc::new(StubLookup(Some(obs(4.2, 4.2, 10))));
        let ev = ProjectionScoringEvaluator::new(lookup);
        let r = ev
            .evaluate(&dynamics_bundle(Some("proj-abc"), 4.2))
            .await
            .unwrap();
        let s = r.dimension_scores[&Dimension::new("projection_accuracy")];
        assert!(
            (s - 1.0).abs() < 1e-9,
            "exact match should score 1.0, got {s}"
        );
    }

    #[tokio::test]
    async fn hundred_percent_error_scores_zero() {
        let lookup = Arc::new(StubLookup(Some(obs(8.4, 4.2, 10))));
        let ev = ProjectionScoringEvaluator::new(lookup);
        let r = ev
            .evaluate(&dynamics_bundle(Some("proj-abc"), 8.4))
            .await
            .unwrap();
        let s = r.dimension_scores[&Dimension::new("projection_accuracy")];
        assert!(
            (s - 0.0).abs() < 1e-9,
            "100% error should score 0.0, got {s}"
        );
    }

    #[tokio::test]
    async fn ten_percent_error_scores_ninety() {
        let lookup = Arc::new(StubLookup(Some(obs(4.62, 4.2, 10)))); // 10% over
        let ev = ProjectionScoringEvaluator::new(lookup);
        let r = ev
            .evaluate(&dynamics_bundle(Some("proj-abc"), 4.62))
            .await
            .unwrap();
        let s = r.dimension_scores[&Dimension::new("projection_accuracy")];
        assert!(
            (s - 0.9).abs() < 0.01,
            "10% error should score ~0.9, got {s}"
        );
    }

    #[tokio::test]
    async fn no_match_is_inapplicable() {
        let lookup = Arc::new(StubLookup(None));
        let ev = ProjectionScoringEvaluator::new(lookup);
        let err = ev
            .evaluate(&dynamics_bundle(Some("proj-abc"), 4.2))
            .await
            .unwrap_err();
        assert!(err.is_inapplicable(), "no match should be Inapplicable");
    }

    #[tokio::test]
    async fn confidence_rises_with_n() {
        let lookup_low = Arc::new(StubLookup(Some(obs(4.2, 4.2, 2))));
        let lookup_high = Arc::new(StubLookup(Some(obs(4.2, 4.2, 10))));
        let ev_low = ProjectionScoringEvaluator::new(lookup_low);
        let ev_high = ProjectionScoringEvaluator::new(lookup_high);
        let r_low = ev_low
            .evaluate(&dynamics_bundle(Some("p"), 4.2))
            .await
            .unwrap();
        let r_high = ev_high
            .evaluate(&dynamics_bundle(Some("p"), 4.2))
            .await
            .unwrap();
        assert!(
            r_high.confidence > r_low.confidence,
            "more observations should yield higher confidence"
        );
    }

    #[tokio::test]
    async fn over_prediction_flagged() {
        let lookup = Arc::new(StubLookup(Some(obs(5.0, 4.0, 5)))); // 25% over
        let ev = ProjectionScoringEvaluator::new(lookup);
        let r = ev.evaluate(&dynamics_bundle(Some("p"), 5.0)).await.unwrap();
        let direction_flag = r
            .flags
            .iter()
            .find(|f| f.kind == "delta_direction")
            .map(|f| f.value.as_str());
        assert_eq!(
            direction_flag,
            Some("over"),
            "25% over-prediction should be flagged as 'over'"
        );
    }
}
