//! Faithfulness — pre-filter grounding evaluator (Track B).
//!
//! ## Design (EVALUATOR_DESIGN.md §Faithfulness)
//!
//! Tier: `PreFilter` — runs serially before dimensional evaluators.
//!
//! ### Approach
//!
//! 1. **Context extraction** — pull source material from
//!    `bundle.context` (tool outputs, retrieved documents, RAG hits).
//!    Key paths checked: `tool_outputs`, `documents`, `retrieved_chunks`.
//!
//! 2. **Claim extraction** — split the agent response into atomic
//!    factual sentences using a simple rule-based splitter (period /
//!    semicolon boundaries).
//!
//! 3. **Source matching** — for each claim, check whether any word
//!    n-gram from the claim appears verbatim in the source material.
//!    `supported` = ≥1 match, `unsupported` = 0 matches.
//!    (Entailment-model matching is the Phase 2 upgrade path.)
//!
//! 4. **Score** = `supported / (supported + unsupported)`.
//!    When there is no context to check against, returns `Inapplicable`.
//!
//! ### Opt-out
//! Agents with `capability_gates.skip_faithfulness = true` in their
//! card receive `Inapplicable` immediately (creative agents, etc.).
//!
//! ### Output
//! - Dimension `grounding` in `[0.0, 1.0]`
//! - When claims are contradicted (future: entailment): flag `groundedness:contradicted`

use std::time::Instant;

use async_trait::async_trait;

use agent_bestiary_evaluators::{
    Dimension, EpisodeBundle, EvalError, EvalFlag, EvalModel, EvalResult, EvalTier,
};

const MIN_CLAIMS: usize = 1;
// Minimum n-gram size for source matching.
const NGRAM_SIZE: usize = 4;

/// Faithfulness evaluator instance.
pub struct FaithfulnessEvaluator;

impl FaithfulnessEvaluator {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FaithfulnessEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EvalModel for FaithfulnessEvaluator {
    fn name(&self) -> &'static str {
        "faithfulness"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn tier(&self) -> EvalTier {
        // Dimensional, not PreFilter — grounding is a quality signal, not a
        // safety gate. PreFilter short-circuits the entire registry when score
        // < 0.5, which would prevent CharacterEval/Sotopia from running on
        // agents that have low-grounding responses.
        EvalTier::Dimensional
    }

    fn dimensions(&self) -> Vec<Dimension> {
        vec![Dimension::new("grounding")]
    }

    async fn evaluate(&self, bundle: &EpisodeBundle) -> Result<EvalResult, EvalError> {
        let t0 = Instant::now();

        // Opt-out check via capability_gates.
        if let Some(card) = &bundle.agent_card {
            // We don't have direct access to the full agent struct here, but
            // the system prompt can carry a `[faithfulness:skip]` tag as a
            // lightweight opt-out mechanism. Proper capability_gates
            // integration happens when the bundle starts carrying the full
            // capability_gates JSON (future work).
            if let Some(prompt) = &card.system_prompt {
                if prompt.contains("[faithfulness:skip]") {
                    return Err(EvalError::Inapplicable(
                        "agent opted out of faithfulness checking via system prompt tag".into(),
                    ));
                }
            }
        }

        // Extract source material from context.
        let sources = extract_sources(&bundle.context);
        if sources.is_empty() {
            return Err(EvalError::Inapplicable(
                "no context/tool outputs to check grounding against".into(),
            ));
        }
        let source_text = sources.join(" ").to_lowercase();

        // Extract response text from transcript.
        let response_text: String = bundle
            .transcript
            .iter()
            .filter(|t| matches!(t.role, agent_bestiary_evaluators::TranscriptRole::Agent))
            .map(|t| t.content.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        if response_text.trim().is_empty() {
            return Err(EvalError::Inapplicable("no agent response to check".into()));
        }

        // Split response into atomic claims.
        let claims = split_claims(&response_text);
        if claims.len() < MIN_CLAIMS {
            return Err(EvalError::Inapplicable(
                "response too short to extract claims".into(),
            ));
        }

        // Score each claim.
        let mut supported = 0usize;
        let mut unsupported = 0usize;
        let mut any_contradicted = false;

        for claim in &claims {
            let claim_lower = claim.to_lowercase();
            let ngrams = extract_ngrams(&claim_lower, NGRAM_SIZE);
            if ngrams.iter().any(|ng| source_text.contains(ng.as_str())) {
                supported += 1;
            } else {
                unsupported += 1;
                // Heuristic contradiction detection: response contains negation
                // of a source term. Very coarse — replaced by entailment in v2.
                if claim_lower.contains(" not ") || claim_lower.contains("never ") {
                    // Check if source says the opposite without negation.
                    let positive = claim_lower.replace(" not ", " ").replace("never ", "");
                    if source_text.contains(&positive[..positive.len().min(40)]) {
                        any_contradicted = true;
                    }
                }
            }
        }

        let total = (supported + unsupported) as f64;
        let score = if total > 0.0 {
            supported as f64 / total
        } else {
            1.0
        };
        let latency = t0.elapsed().as_millis() as u64;

        let mut result = EvalResult::new(self.name(), self.version())
            .with_score("grounding", score)
            .with_confidence(0.70) // rule-based only → modest confidence
            .with_latency_ms(latency)
            .with_rationale(format!(
                "{supported}/{} claims supported by context",
                supported + unsupported
            ));

        if any_contradicted {
            result = result.with_flag(EvalFlag::new("groundedness", "contradicted"));
        }

        Ok(result)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Pull text source material from the episode context JSONB.
/// Checks common keys: `tool_outputs`, `documents`, `retrieved_chunks`, `context`.
fn extract_sources(ctx: &serde_json::Value) -> Vec<String> {
    let mut sources = Vec::new();

    let try_key = |key: &str| -> Vec<String> {
        match &ctx[key] {
            serde_json::Value::String(s) => vec![s.clone()],
            serde_json::Value::Array(arr) => arr
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            serde_json::Value::Object(obj) => {
                // Flatten object values.
                obj.values()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            }
            _ => vec![],
        }
    };

    for key in &[
        "tool_outputs",
        "documents",
        "retrieved_chunks",
        "context",
        "sources",
    ] {
        sources.extend(try_key(key));
    }

    // ABW episodes store tool outputs under context.tool_invocations[].output
    // Extract each invocation's output string as a grounding source.
    if let serde_json::Value::Array(invocations) = &ctx["tool_invocations"] {
        for inv in invocations {
            if let Some(output) = inv.get("output").and_then(|v| v.as_str()) {
                if !output.trim().is_empty() {
                    sources.push(output.to_string());
                }
            }
        }
    }

    // Also treat evidence summaries as grounding sources.
    if let serde_json::Value::Array(evidence) = &ctx["evidence"] {
        for ev in evidence {
            for field in &["summary", "key_findings"] {
                if let Some(s) = ev.get(field).and_then(|v| v.as_str()) {
                    if !s.trim().is_empty() {
                        sources.push(s.to_string());
                    }
                }
            }
        }
    }

    sources
}

/// Very simple sentence splitter for claim extraction.
fn split_claims(text: &str) -> Vec<String> {
    text.split(['.', ';', '\n'])
        .map(str::trim)
        .filter(|s| s.split_whitespace().count() >= 4) // at least 4 words
        .map(str::to_string)
        .collect()
}

/// Generate all n-grams of size `n` (word-level) from a string.
fn extract_ngrams(text: &str, n: usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < n {
        return vec![text.to_string()];
    }
    words.windows(n).map(|w| w.join(" ")).collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use agent_bestiary_memory::{EpisodeBundle, Provenance, TranscriptRole, TranscriptTurn};
    use chrono::Utc;
    use uuid::Uuid;

    fn bundle_with_context(response: &str, context: serde_json::Value) -> EpisodeBundle {
        EpisodeBundle {
            episode_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            persona_version: 1,
            dyad_id: None,
            timestamp_ref: Utc::now(),
            query: "Tell me about Paris.".to_string(),
            transcript: vec![TranscriptTurn {
                role: TranscriptRole::Agent,
                content: response.to_string(),
                speaker_id: None,
            }],
            goal_spec: None,
            context,
            provenance: Provenance::AutoPass,
            authority_weight: 0.5,
            agent_card: None,
        }
    }

    #[tokio::test]
    async fn no_context_is_inapplicable() {
        let ev = FaithfulnessEvaluator::new();
        let b = bundle_with_context("Paris is the capital of France.", serde_json::json!({}));
        let err = ev.evaluate(&b).await.unwrap_err();
        assert!(err.is_inapplicable());
    }

    #[tokio::test]
    async fn grounded_response_scores_high() {
        let ev = FaithfulnessEvaluator::new();
        let ctx = serde_json::json!({
            "documents": ["Paris is the capital of France and a major European city."]
        });
        let response = "Paris is the capital of France and a major European city.";
        let b = bundle_with_context(response, ctx);
        let result = ev.evaluate(&b).await.unwrap();
        let score = result.dimension_scores[&Dimension::new("grounding")];
        assert!(score > 0.5, "expected grounding > 0.5, got {score}");
    }

    #[tokio::test]
    async fn ungrounded_response_scores_lower() {
        let ev = FaithfulnessEvaluator::new();
        let ctx = serde_json::json!({
            "documents": ["Berlin is the capital of Germany."]
        });
        // Response contains completely unrelated claims.
        let response = "The moon is made of cheese and orbits the Sun directly.";
        let b = bundle_with_context(response, ctx);
        let result = ev.evaluate(&b).await.unwrap();
        let score = result.dimension_scores[&Dimension::new("grounding")];
        assert!(
            score < 1.0,
            "ungrounded response should score < 1.0, got {score}"
        );
    }

    #[test]
    fn is_prefilter() {
        let ev = FaithfulnessEvaluator::new();
        assert_eq!(ev.tier(), EvalTier::PreFilter);
    }

    #[test]
    fn ngrams_work() {
        let ngs = extract_ngrams("the quick brown fox", 2);
        assert!(ngs.contains(&"the quick".to_string()));
        assert!(ngs.contains(&"quick brown".to_string()));
    }
}
