//! Sotopia — dimensional social-goals evaluator (Track B).
//!
//! ## Design (EVALUATOR_DESIGN.md §Sotopia)
//!
//! Tier: `Dimensional` — runs in parallel with other dimensional evaluators.
//!
//! ### Dimensions
//! - `goal_completion` — did the agent achieve the social goal in `goal_spec`?
//! - `social_capital`  — did the interaction increase relational standing?
//! - `rapport`         — affective tone and reciprocity within the exchange.
//!
//! ### Approach
//!
//! When `LlmConfig` is `Some`: structured LLM scoring against the Sotopia
//! rubric. The LLM is asked to score each dimension on a 1–10 Likert scale
//! and provide a one-line rationale. Scores are normalised to `[0, 1]`.
//!
//! When `LlmConfig` is `None` (offline / test mode): heuristic scoring
//! based on transcript length, presence of goal-relevant keywords, and
//! reciprocity signals.
//!
//! ### Inapplicability
//! Returns `EvalError::Inapplicable` when:
//! - `goal_spec` is `None` (Sotopia is undefined without a goal).
//! - Transcript has < 2 turns.

use std::time::Instant;

use async_trait::async_trait;
use serde::Deserialize;

use agent_bestiary_evaluators::{
    parse_llm_json, Dimension, EpisodeBundle, EvalError, EvalFlag, EvalModel, EvalResult, EvalTier,
    TranscriptRole,
};

/// Optional LLM provider configuration.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
}

/// Sotopia evaluator.
pub struct SotopiaEvaluator {
    llm: Option<LlmConfig>,
}

impl SotopiaEvaluator {
    /// Offline / heuristic-only mode.
    pub fn new() -> Self {
        Self { llm: None }
    }

    /// LLM-backed scoring mode.
    pub fn with_llm(llm: LlmConfig) -> Self {
        Self { llm: Some(llm) }
    }
}

impl Default for SotopiaEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EvalModel for SotopiaEvaluator {
    fn name(&self) -> &'static str {
        "sotopia"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn tier(&self) -> EvalTier {
        EvalTier::Dimensional
    }

    fn dimensions(&self) -> Vec<Dimension> {
        vec![
            Dimension::new("goal_completion"),
            Dimension::new("social_capital"),
            Dimension::new("rapport"),
        ]
    }

    async fn evaluate(&self, bundle: &EpisodeBundle) -> Result<EvalResult, EvalError> {
        let t0 = Instant::now();

        if bundle.goal_spec.is_none() {
            return Err(EvalError::Inapplicable(
                "goal_spec is required for Sotopia evaluation".into(),
            ));
        }

        if bundle.transcript.len() < 2 {
            return Err(EvalError::Inapplicable(
                "Sotopia requires ≥ 2 transcript turns".into(),
            ));
        }

        if let Some(llm) = &self.llm {
            match llm_score(llm, bundle, self.name(), self.version(), t0).await {
                Ok(r) => return Ok(r),
                Err(e) => {
                    // Degrade rather than disappear.
                    //
                    // Sotopia is the only producer of `rapport`,
                    // `social_capital` and `goal_completion`. Hard-failing on
                    // a provider hiccup removed the entire social axis from
                    // the platform — which is exactly what happened: zero
                    // sotopia signals were ever recorded, because a failed
                    // LLM call yielded nothing at all instead of the
                    // heuristic that was already sitting right here.
                    //
                    // A weaker signal that keeps the relationship loop moving
                    // beats a perfect one that never arrives. The flag and
                    // lowered confidence keep the degradation visible.
                    let mut fallback = heuristic_score(bundle, self.name(), self.version(), t0)?;
                    fallback = fallback
                        .with_confidence(0.40)
                        .with_flag(EvalFlag::new("sotopia", "llm_fallback"));
                    fallback.rationale = Some(match fallback.rationale.take() {
                        Some(r) => format!("LLM unavailable ({e}); {r}"),
                        None => format!("LLM unavailable ({e}); heuristic scoring"),
                    });
                    return Ok(fallback);
                }
            }
        }

        // Heuristic fallback.
        heuristic_score(bundle, self.name(), self.version(), t0)
    }
}

// ── Heuristic scorer ──────────────────────────────────────────────────────────

fn heuristic_score(
    bundle: &EpisodeBundle,
    name: &str,
    version: &str,
    t0: Instant,
) -> Result<EvalResult, EvalError> {
    let goal = bundle.goal_spec.as_ref().unwrap();
    let goal_text = goal.to_string().to_lowercase();

    // Collect agent responses.
    let agent_turns: Vec<&str> = bundle
        .transcript
        .iter()
        .filter(|t| matches!(t.role, TranscriptRole::Agent))
        .map(|t| t.content.as_str())
        .collect();

    let response_text = agent_turns.join(" ").to_lowercase();

    // Goal completion: keyword overlap between goal and response.
    let goal_keywords: Vec<&str> = goal_text
        .split_whitespace()
        .filter(|w| w.len() > 4)
        .collect();
    let hits = goal_keywords
        .iter()
        .filter(|&&w| response_text.contains(w))
        .count();
    let goal_completion = if goal_keywords.is_empty() {
        0.5
    } else {
        (hits as f64 / goal_keywords.len() as f64).clamp(0.0, 1.0)
    };

    // Social capital: heuristic — longer, more substantive responses.
    let avg_response_len = if agent_turns.is_empty() {
        0.0
    } else {
        agent_turns
            .iter()
            .map(|t| t.split_whitespace().count())
            .sum::<usize>() as f64
            / agent_turns.len() as f64
    };
    let social_capital = (avg_response_len / 80.0).clamp(0.0, 1.0);

    // Rapport: turn-taking balance.
    let user_turns = bundle
        .transcript
        .iter()
        .filter(|t| matches!(t.role, TranscriptRole::User))
        .count();
    let agent_turn_count = agent_turns.len();
    let rapport = if user_turns + agent_turn_count == 0 {
        0.5
    } else {
        let ratio = agent_turn_count as f64 / (user_turns + agent_turn_count) as f64;
        // Ideal ratio ~0.5 (balanced); score peaks at 0.5, drops toward extremes.
        1.0 - (ratio - 0.5).abs() * 2.0
    };

    let latency = t0.elapsed().as_millis() as u64;
    Ok(EvalResult::new(name, version)
        .with_score("goal_completion", goal_completion)
        .with_score("social_capital", social_capital)
        .with_score("rapport", rapport)
        .with_confidence(0.55) // heuristic — lower confidence
        .with_latency_ms(latency)
        .with_rationale(format!(
            "Heuristic: goal_kw={}/{}, avg_words={:.0}, rapport_ratio={:.2}",
            hits,
            goal_keywords.len(),
            avg_response_len,
            rapport
        )))
}

// ── LLM scorer ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SotopiaScores {
    goal_completion: f64,
    social_capital: f64,
    rapport: f64,
    rationale: Option<String>,
}

async fn llm_score(
    llm: &LlmConfig,
    bundle: &EpisodeBundle,
    name: &str,
    version: &str,
    t0: Instant,
) -> Result<EvalResult, EvalError> {
    let goal = bundle.goal_spec.as_ref().unwrap();
    let transcript_text: String = bundle
        .transcript
        .iter()
        .map(|t| format!("{}: {}", format!("{:?}", t.role).to_lowercase(), t.content))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        r#"You are a social intelligence evaluator using the Sotopia framework.

Goal: {goal}

Transcript:
{transcript}

Score the agent on three dimensions (1–10 each):
- goal_completion: did the agent achieve the stated goal?
- social_capital: did the interaction increase the agent's relational standing?
- rapport: was the affective tone reciprocal and warm?

Return ONLY valid JSON:
{{
  "goal_completion": <1-10>,
  "social_capital": <1-10>,
  "rapport": <1-10>,
  "rationale": "<one sentence>"
}}"#,
        goal = goal,
        transcript = transcript_text.chars().take(3000).collect::<String>(),
    );

    let body = serde_json::json!({
        "model": llm.model,
        "max_tokens": 256,
        "messages": [{ "role": "user", "content": prompt }]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&llm.endpoint)
        .header("x-api-key", &llm.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| EvalError::Provider(e.to_string()))?;

    let raw: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| EvalError::Malformed(e.to_string()))?;
    let content = raw["content"][0]["text"]
        .as_str()
        .ok_or_else(|| EvalError::Malformed("no text in response".into()))?;

    let scores: SotopiaScores =
        parse_llm_json(content).map_err(|e| EvalError::Malformed(format!("JSON: {e}")))?;

    // Normalise 1–10 → 0–1.
    let norm = |v: f64| ((v - 1.0) / 9.0).clamp(0.0, 1.0);

    let latency = t0.elapsed().as_millis() as u64;
    let mut result = EvalResult::new(name, version)
        .with_score("goal_completion", norm(scores.goal_completion))
        .with_score("social_capital", norm(scores.social_capital))
        .with_score("rapport", norm(scores.rapport))
        .with_confidence(0.82)
        .with_latency_ms(latency)
        .with_model(llm.model.clone())
        .with_cost(2);

    if let Some(r) = scores.rationale {
        result = result.with_rationale(r);
    }

    Ok(result)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use agent_bestiary_memory::{EpisodeBundle, Provenance, TranscriptRole, TranscriptTurn};
    use chrono::Utc;
    use uuid::Uuid;

    fn bundle(
        goal: Option<serde_json::Value>,
        turns: Vec<(&str, TranscriptRole)>,
    ) -> EpisodeBundle {
        EpisodeBundle {
            episode_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            persona_version: 1,
            dyad_id: None,
            timestamp_ref: Utc::now(),
            query: "Hello".to_string(),
            transcript: turns
                .into_iter()
                .map(|(c, r)| TranscriptTurn {
                    role: r,
                    content: c.to_string(),
                    speaker_id: None,
                })
                .collect(),
            goal_spec: goal,
            context: serde_json::json!({}),
            provenance: Provenance::AutoPass,
            authority_weight: 0.5,
            agent_card: None,
        }
    }

    #[tokio::test]
    async fn no_goal_is_inapplicable() {
        let ev = SotopiaEvaluator::new();
        let b = bundle(
            None,
            vec![
                ("hi", TranscriptRole::User),
                ("hello", TranscriptRole::Agent),
            ],
        );
        assert!(ev.evaluate(&b).await.unwrap_err().is_inapplicable());
    }

    #[tokio::test]
    async fn too_few_turns_is_inapplicable() {
        let ev = SotopiaEvaluator::new();
        let b = bundle(Some(serde_json::json!({"goal": "make a friend"})), vec![]);
        assert!(ev.evaluate(&b).await.unwrap_err().is_inapplicable());
    }

    /// The regression that mattered: an unreachable or misbehaving LLM used to
    /// make Sotopia fail outright, which silently removed `rapport`,
    /// `social_capital` and `goal_completion` from the entire platform. It must
    /// now degrade to the heuristic and still emit all three dimensions.
    #[tokio::test]
    async fn llm_failure_degrades_to_heuristic_instead_of_failing() {
        // Unroutable endpoint — guarantees a provider error without a network
        // dependency or an API key.
        let ev = SotopiaEvaluator::with_llm(LlmConfig {
            endpoint: "http://127.0.0.1:1/v1/messages".into(),
            api_key: "not-a-key".into(),
            model: "test-model".into(),
        });
        let b = bundle(
            Some(serde_json::json!({"goal": "discuss the weather forecast"})),
            vec![
                ("What is the weather forecast?", TranscriptRole::User),
                (
                    "The forecast shows sunny skies for outdoor activities.",
                    TranscriptRole::Agent,
                ),
            ],
        );

        let result = ev
            .evaluate(&b)
            .await
            .expect("must degrade to heuristic, not fail");

        for dim in ["goal_completion", "social_capital", "rapport"] {
            assert!(
                result.dimension_scores.contains_key(&Dimension::new(dim)),
                "missing {dim} after fallback"
            );
        }
        // Degradation must be visible, not silent.
        assert!(
            result.confidence < 0.55,
            "fallback confidence should be low"
        );
        assert!(
            result.flags.iter().any(|f| f.value == "llm_fallback"),
            "fallback must be flagged"
        );
        assert!(result
            .rationale
            .as_deref()
            .unwrap_or_default()
            .contains("LLM unavailable"));
    }

    #[tokio::test]
    async fn heuristic_scores_all_dimensions() {
        let ev = SotopiaEvaluator::new();
        let b = bundle(
            Some(serde_json::json!({"goal": "discuss the weather forecast"})),
            vec![
                ("What is the weather forecast?", TranscriptRole::User),
                ("The weather forecast shows sunny skies with some clouds. The forecast looks great for outdoor activities.", TranscriptRole::Agent),
                ("Great, thanks!", TranscriptRole::User),
                ("You are welcome! Enjoy the weather.", TranscriptRole::Agent),
            ],
        );
        let result = ev.evaluate(&b).await.unwrap();
        assert!(result
            .dimension_scores
            .contains_key(&Dimension::new("goal_completion")));
        assert!(result
            .dimension_scores
            .contains_key(&Dimension::new("social_capital")));
        assert!(result
            .dimension_scores
            .contains_key(&Dimension::new("rapport")));
    }
}
