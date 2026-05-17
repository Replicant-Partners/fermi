//! `LLMJudgeEvaluator` — reference dimensional evaluator.
//!
//! Provider-agnostic: the actual LLM call is abstracted behind the
//! [`LlmJudge`] trait. The production implementation is
//! `LlmJudgeAnthropic` in `src/handlers/eval_judge.rs`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::model::EvalModel;
use crate::result::{Dimension, EvalResult};
use crate::tier::EvalTier;
use crate::EvalError;
use agent_bestiary_memory::EpisodeBundle;

/// Outcome returned by a [`LlmJudge`] implementation. Mirrors the
/// existing `score_with_judge` JSON contract (relevance / accuracy /
/// completeness / overall on a 1–5 scale, plus reasoning).
///
/// Phase 1 normalizes these to `[0.0, 1.0]` inside the evaluator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeOutcome {
    pub relevance: f64,
    pub accuracy: f64,
    pub completeness: f64,
    pub overall: f64,
    #[serde(default)]
    pub reasoning: Option<String>,
}

/// Provider-agnostic LLM judge interface. The concrete implementor
/// owns the prompt construction, HTTP call, and JSON parsing.
#[async_trait]
pub trait LlmJudge: Send + Sync {
    /// Score the bundle. Implementations should return scores on the
    /// same 1–5 scale as the legacy `score_with_judge`; the
    /// `LlmJudgeEvaluator` normalizes to `[0.0, 1.0]`.
    async fn score(&self, bundle: &EpisodeBundle) -> Result<JudgeOutcome, EvalError>;

    /// The model identifier used (for `EvalResult::model_used`).
    fn model_id(&self) -> &str {
        "unknown-judge-model"
    }
}

/// Test/no-op judge — returns a fixed outcome. Used by the integration
/// tests and as a placeholder until Phase 2 wires the real provider.
pub struct NoopLlmJudge {
    pub fixed_outcome: JudgeOutcome,
    pub model: &'static str,
}

impl NoopLlmJudge {
    pub fn new(fixed_outcome: JudgeOutcome) -> Self {
        Self {
            fixed_outcome,
            model: "noop-judge",
        }
    }
}

#[async_trait]
impl LlmJudge for NoopLlmJudge {
    async fn score(&self, _bundle: &EpisodeBundle) -> Result<JudgeOutcome, EvalError> {
        Ok(self.fixed_outcome.clone())
    }
    fn model_id(&self) -> &str {
        self.model
    }
}

/// Configuration for the [`LlmJudgeEvaluator`].
#[derive(Debug, Clone)]
pub struct LlmJudgeConfig {
    /// Stable evaluator name. Defaults to `"llm_judge"`.
    pub name: &'static str,
    /// Bumps when the prompt template changes. Defaults to `"v1"`.
    pub version: &'static str,
}

impl Default for LlmJudgeConfig {
    fn default() -> Self {
        Self {
            name: "llm_judge",
            version: "v1",
        }
    }
}

/// Reference dimensional evaluator. Three dimensions:
/// `relevance`, `accuracy`, `completeness`. Scores are normalized
/// from the legacy 1–5 scale into `[0.0, 1.0]`.
pub struct LlmJudgeEvaluator {
    judge: Arc<dyn LlmJudge>,
    config: LlmJudgeConfig,
}

impl LlmJudgeEvaluator {
    pub fn new(judge: Arc<dyn LlmJudge>) -> Self {
        Self {
            judge,
            config: LlmJudgeConfig::default(),
        }
    }

    pub fn with_config(mut self, config: LlmJudgeConfig) -> Self {
        self.config = config;
        self
    }
}

/// Normalize a 1–5 Likert score to `[0.0, 1.0]`. Inputs outside
/// `[1.0, 5.0]` are clamped first.
fn normalize_likert(s: f64) -> f64 {
    let clamped = s.clamp(1.0, 5.0);
    (clamped - 1.0) / 4.0
}

#[async_trait]
impl EvalModel for LlmJudgeEvaluator {
    fn name(&self) -> &'static str {
        self.config.name
    }

    fn version(&self) -> &'static str {
        self.config.version
    }

    fn tier(&self) -> EvalTier {
        EvalTier::Dimensional
    }

    fn dimensions(&self) -> Vec<Dimension> {
        vec![
            Dimension::new("relevance"),
            Dimension::new("accuracy"),
            Dimension::new("completeness"),
        ]
    }

    async fn evaluate(&self, bundle: &EpisodeBundle) -> Result<EvalResult, EvalError> {
        // Inapplicable when there's no transcript at all — the legacy
        // judge needs a response to score.
        if bundle.transcript.is_empty() && bundle.query.trim().is_empty() {
            return Err(EvalError::Inapplicable(
                "empty transcript and query".into(),
            ));
        }

        let outcome = self.judge.score(bundle).await?;

        let mut result = EvalResult::new(self.name(), self.version())
            .with_score(Dimension::new("relevance"), normalize_likert(outcome.relevance))
            .with_score(Dimension::new("accuracy"), normalize_likert(outcome.accuracy))
            .with_score(
                Dimension::new("completeness"),
                normalize_likert(outcome.completeness),
            )
            .with_model(self.judge.model_id().to_string());

        if let Some(reasoning) = outcome.reasoning {
            result = result.with_rationale(reasoning);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_bestiary_memory::Episode;
    use chrono::Utc;
    use uuid::Uuid;

    fn dummy_bundle() -> EpisodeBundle {
        let ep = Episode {
            episode_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            timestamp_ref: Utc::now(),
            query: "What is 2+2?".into(),
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
    async fn judge_scores_normalized_into_unit_interval() {
        let judge = Arc::new(NoopLlmJudge::new(JudgeOutcome {
            relevance: 5.0,
            accuracy: 3.0,
            completeness: 1.0,
            overall: 3.0,
            reasoning: Some("solid".into()),
        }));
        let ev = LlmJudgeEvaluator::new(judge);
        let result = ev.evaluate(&dummy_bundle()).await.unwrap();

        let scores = &result.dimension_scores;
        assert!(
            (scores.get(&Dimension::new("relevance")).unwrap() - 1.0).abs() < 1e-9
        );
        assert!(
            (scores.get(&Dimension::new("accuracy")).unwrap() - 0.5).abs() < 1e-9
        );
        assert!(
            (scores.get(&Dimension::new("completeness")).unwrap() - 0.0).abs() < 1e-9
        );
        assert_eq!(result.evaluator_name, "llm_judge");
        assert_eq!(result.evaluator_version, "v1");
        assert_eq!(result.model_used.as_deref(), Some("noop-judge"));
    }

    #[tokio::test]
    async fn judge_clamps_out_of_range_inputs() {
        let judge = Arc::new(NoopLlmJudge::new(JudgeOutcome {
            relevance: 9.0,    // out of range high
            accuracy: -1.0,    // out of range low
            completeness: 4.0, // mid
            overall: 4.0,
            reasoning: None,
        }));
        let ev = LlmJudgeEvaluator::new(judge);
        let result = ev.evaluate(&dummy_bundle()).await.unwrap();
        let scores = &result.dimension_scores;
        // 9 → clamp to 5 → 1.0
        assert!((scores.get(&Dimension::new("relevance")).unwrap() - 1.0).abs() < 1e-9);
        // -1 → clamp to 1 → 0.0
        assert!((scores.get(&Dimension::new("accuracy")).unwrap() - 0.0).abs() < 1e-9);
        // 4 → 0.75
        assert!(
            (scores.get(&Dimension::new("completeness")).unwrap() - 0.75).abs() < 1e-9
        );
    }
}
