//! LifelongBench — dimensional drift/retention evaluator (Track B).
//!
//! ## Design (EVALUATOR_DESIGN.md §LifelongBench)
//!
//! Tier: `Dimensional` — runs in parallel with other dimensional evaluators.
//!
//! ### Dimensions
//! - `persona_consistency` — does the agent's behaviour match prior sessions
//!   with the same dyad or on the same topic?
//! - `retention` — does the agent recall facts established in prior episodes?
//!
//! ### Approach
//!
//! **persona_consistency** — computed from the bundle's
//! `within_version_cosine` timeline field when available. When not
//! available (no embedding, first episode in persona_version), returns
//! `Inapplicable` for that dimension.
//!
//! Because the LifelongBench evaluator needs history that lives in the
//! memory store, it accepts an optional `PersonaConsistencySignal`
//! pre-computed by the caller (eval pipeline) rather than doing a DB
//! round-trip itself. This keeps the evaluator stateless and testable.
//!
//! **retention** — Phase 1 stub: returns `None` (skip aggregation).
//! Phase 2 will wire a fact-retrieval probe against the semantic memory.
//!
//! ### Inapplicability
//! - No prior history provided (first episode) → `Inapplicable`.
//! - No embedding available → `Inapplicable`.

use std::time::Instant;

use async_trait::async_trait;

use agent_bestiary_evaluators::{
    Dimension, EvalError, EvalModel, EvalResult, EvalTier, EpisodeBundle,
};

/// Pre-computed persona consistency signals from the memory store.
/// Injected by the eval pipeline to avoid DB calls inside the evaluator.
#[derive(Debug, Clone)]
pub struct PersonaConsistencySignal {
    /// Cosine similarity of this episode's embedding vs. the rolling
    /// mean embedding of the same persona_version (within-version cohesion).
    /// 1.0 = identical, 0.0 = orthogonal.
    pub within_version_cosine: f64,
    /// Number of prior episodes in this persona_version (confidence signal).
    pub n_prior_episodes: usize,
}

/// LifelongBench evaluator.
pub struct LifelongBenchEvaluator {
    /// Optionally injected prior-history signal.
    /// When `None`, evaluator returns `Inapplicable` (no history available).
    signal: Option<PersonaConsistencySignal>,
}

impl LifelongBenchEvaluator {
    /// No prior history (first episode, test mode).
    pub fn new() -> Self {
        Self { signal: None }
    }

    /// With pre-computed consistency signal from the eval pipeline.
    pub fn with_signal(signal: PersonaConsistencySignal) -> Self {
        Self { signal: Some(signal) }
    }
}

impl Default for LifelongBenchEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EvalModel for LifelongBenchEvaluator {
    fn name(&self) -> &'static str {
        "lifelongbench"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn tier(&self) -> EvalTier {
        EvalTier::Dimensional
    }

    fn dimensions(&self) -> Vec<Dimension> {
        vec![
            Dimension::new("persona_consistency"),
            // retention is planned for Phase 2 — declared but not scored yet.
            Dimension::new("retention"),
        ]
    }

    async fn evaluate(&self, bundle: &EpisodeBundle) -> Result<EvalResult, EvalError> {
        let t0 = Instant::now();

        let sig = self.signal.as_ref().ok_or_else(|| {
            EvalError::Inapplicable(
                "no prior episode history for persona consistency scoring".into(),
            )
        })?;

        // Minimum history requirement (EVALUATOR_DESIGN.md §LifelongBench).
        if sig.n_prior_episodes < 5 {
            return Err(EvalError::Inapplicable(format!(
                "need ≥ 5 prior episodes; only {} available",
                sig.n_prior_episodes
            )));
        }

        // persona_consistency = within_version_cosine, already in [0, 1].
        let persona_consistency = sig.within_version_cosine.clamp(0.0, 1.0);

        // Confidence rises with sample size, saturating around n = 50.
        let confidence = (sig.n_prior_episodes as f64 / 50.0).clamp(0.1, 0.9);

        // Drift flag: high variance in consistency across the window.
        // Here we use a simple threshold — low cosine = significant drift.
        let drift_flag = persona_consistency < 0.5;

        let latency = t0.elapsed().as_millis() as u64;

        let mut result = EvalResult::new(self.name(), self.version())
            .with_score("persona_consistency", persona_consistency)
            // retention is not scored in v0.1 — intentionally omitted so
            // the aggregator treats it as inapplicable for this dimension.
            .with_confidence(confidence)
            .with_latency_ms(latency)
            .with_rationale(format!(
                "within-version cosine={:.3}, n_prior={}, drift={}",
                sig.within_version_cosine,
                sig.n_prior_episodes,
                if drift_flag { "⚠" } else { "ok" }
            ));

        if drift_flag {
            result = result.with_flag(agent_bestiary_evaluators::EvalFlag::new(
                "drift",
                format!("cosine={:.3}", persona_consistency),
            ));
        }

        // Drift also flag if persona_version > 1 and cosine dropped sharply.
        if bundle.persona_version > 1 && persona_consistency < 0.3 {
            result = result.with_flag(agent_bestiary_evaluators::EvalFlag::new(
                "drift",
                "inter_version:significant",
            ));
        }

        Ok(result)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use agent_bestiary_memory::{EpisodeBundle, Provenance, TranscriptRole, TranscriptTurn};
    use chrono::Utc;
    use uuid::Uuid;

    fn minimal_bundle() -> EpisodeBundle {
        EpisodeBundle {
            episode_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            persona_version: 2,
            dyad_id: Some("eval:agent:user".to_string()),
            timestamp_ref: Utc::now(),
            query: "How are you?".to_string(),
            transcript: vec![TranscriptTurn {
                role: TranscriptRole::Agent,
                content: "I am well, thank you.".to_string(),
                speaker_id: None,
            }],
            goal_spec: None,
            context: serde_json::json!({}),
            provenance: Provenance::AutoPass,
            authority_weight: 0.5,
            agent_card: None,
        }
    }

    #[tokio::test]
    async fn no_signal_is_inapplicable() {
        let ev = LifelongBenchEvaluator::new();
        assert!(ev.evaluate(&minimal_bundle()).await.unwrap_err().is_inapplicable());
    }

    #[tokio::test]
    async fn too_few_episodes_is_inapplicable() {
        let ev = LifelongBenchEvaluator::with_signal(PersonaConsistencySignal {
            within_version_cosine: 0.9,
            n_prior_episodes: 3,
        });
        assert!(ev.evaluate(&minimal_bundle()).await.unwrap_err().is_inapplicable());
    }

    #[tokio::test]
    async fn stable_persona_scores_high() {
        let ev = LifelongBenchEvaluator::with_signal(PersonaConsistencySignal {
            within_version_cosine: 0.92,
            n_prior_episodes: 20,
        });
        let result = ev.evaluate(&minimal_bundle()).await.unwrap();
        let score = result.dimension_scores[&Dimension::new("persona_consistency")];
        assert!(score > 0.85);
        assert!(result.flags.is_empty());
    }

    #[tokio::test]
    async fn drifting_persona_flags() {
        let ev = LifelongBenchEvaluator::with_signal(PersonaConsistencySignal {
            within_version_cosine: 0.30,
            n_prior_episodes: 15,
        });
        let mut b = minimal_bundle();
        b.persona_version = 3;
        let result = ev.evaluate(&b).await.unwrap();
        let score = result.dimension_scores[&Dimension::new("persona_consistency")];
        assert!(score < 0.5);
        assert!(!result.flags.is_empty());
    }

    #[test]
    fn declares_two_dimensions() {
        let ev = LifelongBenchEvaluator::new();
        assert_eq!(ev.dimensions().len(), 2);
    }
}
