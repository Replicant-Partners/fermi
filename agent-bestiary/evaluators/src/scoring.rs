//! `BrierEvaluator` — thin, **read-only** wrapper over the existing
//! Fermi forecast resolver.
//!
//! Per decision D8: this evaluator never *computes* a Brier score.
//! It looks up the latest already-computed `brier_score` for whatever
//! forecasts are tied to the bundle's agent / context and surfaces it
//! as the `forecast_calibration` dimension.
//!
//! Phase 1 keeps the lookup behind the [`BrierLookup`] trait so the
//! evaluator crate doesn't take a sqlx dependency. Phase 2 will plug
//! a real implementation backed by `src/handlers/forecasts.rs`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::model::EvalModel;
use crate::result::{Dimension, EvalResult};
use crate::tier::EvalTier;
use crate::EvalError;
use agent_bestiary_memory::EpisodeBundle;

/// One Brier observation — the read shape the evaluator needs from a
/// store. Phase 1 expects callers to map their resolver output into
/// this struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrierObservation {
    /// Raw Brier score in `[0.0, 1.0]` where 0.0 is perfect.
    pub brier_score: f64,
    /// Optional sample size used to compute the score (helpful for
    /// confidence-weighting).
    #[serde(default)]
    pub n_forecasts: Option<u32>,
    /// Wall-clock time the score was computed.
    #[serde(default)]
    pub computed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Lookup trait — Phase 2 wires this to the real forecasts table.
#[async_trait]
pub trait BrierLookup: Send + Sync {
    /// Return the most recent rolling Brier observation for an agent
    /// (or `None` when the agent has no resolved forecasts).
    async fn latest_for_agent(
        &self,
        agent_id: uuid::Uuid,
    ) -> Result<Option<BrierObservation>, EvalError>;
}

/// `BrierEvaluator` — wraps a [`BrierLookup`] and emits a single
/// `forecast_calibration` dimension score.
///
/// Score normalization: Brier is `[0, 1]` with 0 = perfect. We invert
/// to `[0, 1]` with 1 = perfect (matches the rest of the registry's
/// "higher is better" convention) using `score = 1.0 - brier`.
pub struct BrierEvaluator {
    lookup: Arc<dyn BrierLookup>,
    name: &'static str,
    version: &'static str,
}

impl BrierEvaluator {
    pub fn new(lookup: Arc<dyn BrierLookup>) -> Self {
        Self {
            lookup,
            name: "brier",
            version: "v1",
        }
    }

    pub fn with_name(mut self, name: &'static str) -> Self {
        self.name = name;
        self
    }

    pub fn with_version(mut self, version: &'static str) -> Self {
        self.version = version;
        self
    }
}

#[async_trait]
impl EvalModel for BrierEvaluator {
    fn name(&self) -> &'static str {
        self.name
    }

    fn version(&self) -> &'static str {
        self.version
    }

    fn tier(&self) -> EvalTier {
        EvalTier::Dimensional
    }

    fn dimensions(&self) -> Vec<Dimension> {
        vec![Dimension::new("forecast_calibration")]
    }

    async fn evaluate(&self, bundle: &EpisodeBundle) -> Result<EvalResult, EvalError> {
        let obs = self.lookup.latest_for_agent(bundle.agent_id).await?;
        let Some(obs) = obs else {
            return Err(EvalError::Inapplicable(
                "no resolved forecasts for this agent".into(),
            ));
        };

        // 1.0 - clipped Brier so higher = better.
        let brier_clamped = obs.brier_score.clamp(0.0, 1.0);
        let calibration = 1.0 - brier_clamped;

        // Confidence rises with sample size, with a soft saturation at
        // n=20. Below 5 samples we mark the result as low-confidence.
        let confidence = obs
            .n_forecasts
            .map(|n| {
                let n = n as f64;
                (n / 20.0).clamp(0.1, 1.0)
            })
            .unwrap_or(0.5);

        let result = EvalResult::new(self.name(), self.version())
            .with_score(Dimension::new("forecast_calibration"), calibration)
            .with_confidence(confidence)
            .with_rationale(format!(
                "Brier {:.3} over {} forecasts",
                brier_clamped,
                obs.n_forecasts.map(|n| n.to_string()).unwrap_or("?".into())
            ));

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_bestiary_memory::Episode;
    use chrono::Utc;
    use uuid::Uuid;

    struct StubLookup(Option<BrierObservation>);

    #[async_trait]
    impl BrierLookup for StubLookup {
        async fn latest_for_agent(
            &self,
            _agent_id: Uuid,
        ) -> Result<Option<BrierObservation>, EvalError> {
            Ok(self.0.clone())
        }
    }

    fn dummy_bundle() -> EpisodeBundle {
        let ep = Episode {
            episode_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            timestamp_ref: Utc::now(),
            query: "Will AMD hit $200M FY26?".into(),
            context: serde_json::json!({}),
            execution_status: agent_bestiary_memory::ExecutionStatus::Success,
            error_details: None,
            execution_time_ms: 0,
            tokens_used: None,
            cost_usd: None,
            embedding: None,
            consolidated: false,
            tags: vec![],
            provenance: agent_bestiary_memory::Provenance::AutoPass,
            authority_weight: 0.5,
            dyad_id: None,
            persona_version_at_write: None,
                provider_used: None,
                model_used: None,
        };
        EpisodeBundle::from_episode(&ep)
    }

    #[tokio::test]
    async fn perfect_brier_yields_full_calibration() {
        let lookup = Arc::new(StubLookup(Some(BrierObservation {
            brier_score: 0.0,
            n_forecasts: Some(40),
            computed_at: None,
        })));
        let ev = BrierEvaluator::new(lookup);
        let r = ev.evaluate(&dummy_bundle()).await.unwrap();
        let s = r
            .dimension_scores
            .get(&Dimension::new("forecast_calibration"))
            .unwrap();
        assert!((*s - 1.0).abs() < 1e-9);
        assert!((r.confidence - 1.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn coin_flip_brier_yields_half_calibration() {
        let lookup = Arc::new(StubLookup(Some(BrierObservation {
            brier_score: 0.5,
            n_forecasts: Some(10),
            computed_at: None,
        })));
        let ev = BrierEvaluator::new(lookup);
        let r = ev.evaluate(&dummy_bundle()).await.unwrap();
        let s = r
            .dimension_scores
            .get(&Dimension::new("forecast_calibration"))
            .unwrap();
        assert!((*s - 0.5).abs() < 1e-9);
        // n=10 / 20 = 0.5 confidence
        assert!((r.confidence - 0.5).abs() < 1e-9);
    }

    #[tokio::test]
    async fn missing_brier_is_inapplicable() {
        let lookup = Arc::new(StubLookup(None));
        let ev = BrierEvaluator::new(lookup);
        let err = ev.evaluate(&dummy_bundle()).await.unwrap_err();
        assert!(err.is_inapplicable());
    }
}
