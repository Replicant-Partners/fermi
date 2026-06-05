//! Phase 2 — production `LlmJudge` implementation backed by Anthropic Haiku.
//!
//! `LlmJudgeAnthropic` accepts an `EpisodeBundle` (Phase 0 contract)
//! and produces a typed `JudgeOutcome` registered with the `EvaluatorRegistry`.
//!
//! See:
//! - `agent-bestiary/evaluators/src/judge.rs` for the `LlmJudge` trait
//! - `docs/architecture/OBSERVABILITY_IMPL.md` Phase 2

use agent_bestiary_evaluators::{EpisodeBundle, EvalError, JudgeOutcome, LlmJudge};
use async_trait::async_trait;
use serde_json::json;

/// Production `LlmJudge` impl — Anthropic Haiku via direct HTTP.
///
/// Mirrors the prompt and response shape of the legacy
/// `score_with_judge` so behaviour is unchanged for callers that
/// previously consumed that function. Reads `ANTHROPIC_API_KEY` from
/// the environment; absence is treated as a transient provider error
/// (registry will skip-aggregate it).
pub struct LlmJudgeAnthropic {
    /// Optional rubric text to inject into the prompt — usually pulled
    /// from `EvalTestCase.rubric` and threaded through the bundle's
    /// `goal_spec`. `None` → no rubric in the prompt.
    pub rubric: Option<String>,
    /// Optional expected output, again sourced from the test case.
    pub expected_output: Option<String>,
    /// Override the Anthropic model id. Defaults to `claude-haiku-4-5-20251001`.
    pub model: String,
}

impl Default for LlmJudgeAnthropic {
    fn default() -> Self {
        Self {
            rubric: None,
            expected_output: None,
            model: "claude-haiku-4-5-20251001".to_string(),
        }
    }
}

impl LlmJudgeAnthropic {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_rubric(mut self, rubric: Option<String>) -> Self {
        self.rubric = rubric;
        self
    }

    pub fn with_expected_output(mut self, expected: Option<String>) -> Self {
        self.expected_output = expected;
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Reproduce the legacy prompt verbatim, sourcing the agent
    /// response from the bundle's transcript or `context.reasoning`.
    fn build_prompt(&self, bundle: &EpisodeBundle) -> String {
        // Find the agent's response in the transcript. Prefer the
        // first `Agent` turn; fall back to context.reasoning; final
        // fallback is "(no output)" (matches legacy behaviour).
        let response = bundle
            .transcript
            .iter()
            .find(|t| matches!(t.role, agent_bestiary_evaluators::TranscriptRole::Agent))
            .map(|t| t.content.clone())
            .or_else(|| {
                bundle
                    .context
                    .get("reasoning")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "(no output)".to_string());

        let expected = self
            .expected_output
            .as_ref()
            .map(|e| format!("EXPECTED OUTPUT: {}\n", e))
            .unwrap_or_default();
        let rubric = self
            .rubric
            .as_ref()
            .map(|r| format!("SCORING RUBRIC: {}\n", r))
            .unwrap_or_default();

        format!(
            "You are an evaluation judge. Score the following agent output on three dimensions.\n\
             Each score is 1-5 (1=terrible, 5=excellent).\n\n\
             QUERY: {}\n\
             {}\
             {}\
             AGENT OUTPUT:\n{}\n\n\
             Respond with ONLY valid JSON:\n\
             {{\"relevance\": N, \"accuracy\": N, \"completeness\": N, \"overall\": N.N, \"reasoning\": \"...\"}}\n\
             where overall = average of the three scores.",
            bundle.query, expected, rubric, response,
        )
    }
}

#[async_trait]
impl LlmJudge for LlmJudgeAnthropic {
    fn model_id(&self) -> &str {
        &self.model
    }

    async fn score(&self, bundle: &EpisodeBundle) -> Result<JudgeOutcome, EvalError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| EvalError::Provider("ANTHROPIC_API_KEY not set".into()))?;

        let prompt = self.build_prompt(bundle);

        let client = reqwest::Client::new();
        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&json!({
                "model": self.model,
                "max_tokens": 300,
                "messages": [{"role": "user", "content": prompt}]
            }))
            .send()
            .await
            .map_err(|e| EvalError::Provider(format!("Anthropic request failed: {}", e)))?;

        if !resp.status().is_success() {
            return Err(EvalError::Provider(format!(
                "Anthropic returned status {}",
                resp.status()
            )));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| EvalError::Malformed(format!("Anthropic body not JSON: {}", e)))?;

        let text = body["content"][0]["text"]
            .as_str()
            .ok_or_else(|| EvalError::Malformed("missing content[0].text".into()))?;

        let parsed: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| EvalError::Malformed(format!("judge JSON parse: {}", e)))?;

        let read_score = |key: &str| -> Result<f64, EvalError> {
            parsed
                .get(key)
                .and_then(|v| v.as_f64())
                .ok_or_else(|| EvalError::Malformed(format!("missing field: {}", key)))
        };

        Ok(JudgeOutcome {
            relevance: read_score("relevance")?,
            accuracy: read_score("accuracy")?,
            completeness: read_score("completeness")?,
            overall: read_score("overall").unwrap_or_else(|_| {
                // overall is "N.N" not "N"; if the model wrote it as
                // an int we still want to read it. Fall back to mean.
                ((read_score("relevance").unwrap_or(0.0)
                    + read_score("accuracy").unwrap_or(0.0)
                    + read_score("completeness").unwrap_or(0.0))
                    / 3.0)
            }),
            reasoning: parsed
                .get("reasoning")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }
}
