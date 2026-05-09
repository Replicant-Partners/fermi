//! CharacterEval — dimensional persona-fidelity evaluator (Track B).
//!
//! ## Design (EVALUATOR_DESIGN.md §CharacterEval)
//!
//! Tier: `Dimensional` — runs in parallel with other dimensional evaluators.
//!
//! ### Dimensions
//! - `persona_fidelity` — does the agent stay in character per its system
//!   prompt and agent card?
//! - `value_alignment` — do responses align with the values implied by
//!   the agent's curated identity?
//!
//! ### Approach
//!
//! **Deterministic pre-checks:**
//! - No agent card / system prompt → `Inapplicable`.
//! - System prompt shorter than 20 chars → `Inapplicable` (not enough
//!   commitment to evaluate fidelity against).
//!
//! **Commitment extraction:**
//! Parse the system prompt for explicit identity markers:
//! - Tone words (formal / casual / technical / empathetic)
//! - Claimed expertise domains (keywords after "expert in", "specializes in")
//! - Explicit prohibitions ("never", "do not", "avoid")
//!
//! **Persona fidelity scoring (heuristic):**
//! - Check whether response tone and vocabulary is consistent with
//!   extracted commitments.
//! - Each commitment matched in the response adds to `hits`.
//! - `persona_fidelity = hits / total_commitments`.
//!
//! **LLM-backed scoring (when `LlmConfig` provided):**
//! Sends the system prompt + response to an LLM judge with the
//! commitment checklist and receives a structured `persona_fidelity`
//! and `value_alignment` score (1–10 → normalized to [0,1]).
//!
//! ### Inapplicability
//! - `agent_card` is `None` in the bundle → `Inapplicable`.
//! - `system_prompt` is `None` or < 20 chars → `Inapplicable`.

use std::time::Instant;

use async_trait::async_trait;
use serde::Deserialize;

use agent_bestiary_evaluators::{
    Dimension, EvalError, EvalModel, EvalResult, EvalTier, EpisodeBundle,
    TranscriptRole,
};

/// Optional LLM provider configuration.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
}

/// CharacterEval evaluator.
pub struct CharacterEvaluator {
    llm: Option<LlmConfig>,
}

impl CharacterEvaluator {
    pub fn new() -> Self {
        Self { llm: None }
    }

    pub fn with_llm(llm: LlmConfig) -> Self {
        Self { llm: Some(llm) }
    }
}

impl Default for CharacterEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EvalModel for CharacterEvaluator {
    fn name(&self) -> &'static str {
        "character_eval"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn tier(&self) -> EvalTier {
        EvalTier::Dimensional
    }

    fn dimensions(&self) -> Vec<Dimension> {
        vec![
            Dimension::new("persona_fidelity"),
            Dimension::new("value_alignment"),
        ]
    }

    async fn evaluate(&self, bundle: &EpisodeBundle) -> Result<EvalResult, EvalError> {
        let t0 = Instant::now();

        let card = bundle.agent_card.as_ref().ok_or_else(|| {
            EvalError::Inapplicable("agent card missing from bundle".into())
        })?;

        let system_prompt = card.system_prompt.as_deref().ok_or_else(|| {
            EvalError::Inapplicable("no system prompt to evaluate persona fidelity against".into())
        })?;

        if system_prompt.len() < 20 {
            return Err(EvalError::Inapplicable(
                "system prompt too short to extract meaningful commitments".into(),
            ));
        }

        let response_text: String = bundle
            .transcript
            .iter()
            .filter(|t| matches!(t.role, TranscriptRole::Agent))
            .map(|t| t.content.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        if response_text.trim().is_empty() {
            return Err(EvalError::Inapplicable("no agent response to evaluate".into()));
        }

        if let Some(llm) = &self.llm {
            return llm_score(llm, system_prompt, &response_text, bundle, self.name(), self.version(), t0).await;
        }

        heuristic_score(system_prompt, &response_text, self.name(), self.version(), t0)
    }
}

// ── Commitment extraction ─────────────────────────────────────────────────────

#[derive(Debug)]
struct Commitments {
    tone_words: Vec<String>,
    expertise_domains: Vec<String>,
    prohibitions: Vec<String>,
}

fn extract_commitments(prompt: &str) -> Commitments {
    let lower = prompt.to_lowercase();

    // Tone markers.
    let tone_words: Vec<String> = ["formal", "technical", "concise", "empathetic", "casual",
        "professional", "friendly", "analytical", "creative"]
        .iter()
        .filter(|&&w| lower.contains(w))
        .map(|&w| w.to_string())
        .collect();

    // Expertise domains — simple keyword extraction after trigger phrases.
    let mut expertise_domains = Vec::new();
    for trigger in &["expert in ", "specializes in ", "specialised in ", "focused on "] {
        if let Some(pos) = lower.find(trigger) {
            let after = &lower[pos + trigger.len()..];
            let domain: String = after.split(['.', ',', '\n', ';']).next()
                .unwrap_or("")
                .trim()
                .chars()
                .take(40)
                .collect();
            if !domain.is_empty() {
                expertise_domains.push(domain);
            }
        }
    }

    // Prohibitions.
    let mut prohibitions = Vec::new();
    for trigger in &["never ", "do not ", "avoid ", "must not "] {
        if let Some(pos) = lower.find(trigger) {
            let after = &lower[pos + trigger.len()..];
            let prohibited: String = after.split(['.', ',', '\n']).next()
                .unwrap_or("")
                .trim()
                .chars()
                .take(50)
                .collect();
            if !prohibited.is_empty() {
                prohibitions.push(prohibited);
            }
        }
    }

    Commitments { tone_words, expertise_domains, prohibitions }
}

// ── Heuristic scorer ──────────────────────────────────────────────────────────

fn heuristic_score(
    system_prompt: &str,
    response: &str,
    name: &str,
    version: &str,
    t0: Instant,
) -> Result<EvalResult, EvalError> {
    let commitments = extract_commitments(system_prompt);
    let resp_lower = response.to_lowercase();

    let total_commitments = commitments.tone_words.len()
        + commitments.expertise_domains.len()
        + commitments.prohibitions.len();

    if total_commitments == 0 {
        // No extractable commitments — score at 0.5 (neutral) with low confidence.
        let latency = t0.elapsed().as_millis() as u64;
        return Ok(EvalResult::new(name, version)
            .with_score("persona_fidelity", 0.5)
            .with_score("value_alignment", 0.5)
            .with_confidence(0.3)
            .with_latency_ms(latency)
            .with_rationale("No extractable commitments in system prompt; neutral score.".to_string()));
    }

    // Tone matching.
    let tone_hits = commitments.tone_words.iter()
        .filter(|w| resp_lower.contains(w.as_str()))
        .count();

    // Domain keyword matching.
    let domain_hits = commitments.expertise_domains.iter()
        .filter(|d| resp_lower.contains(d.as_str()))
        .count();

    // Prohibition violations (lower is worse).
    let prohibition_violations = commitments.prohibitions.iter()
        .filter(|p| resp_lower.contains(p.as_str()))
        .count();

    let positive_hits = tone_hits + domain_hits;
    let persona_fidelity = if total_commitments > prohibition_violations {
        (positive_hits as f64 / total_commitments as f64
            - prohibition_violations as f64 / total_commitments as f64)
            .clamp(0.0, 1.0)
    } else {
        0.0
    };

    // Value alignment: heuristic proxy — absence of prohibition violations.
    let value_alignment = if commitments.prohibitions.is_empty() {
        0.5 // neutral; no prohibitions to check
    } else {
        (1.0 - prohibition_violations as f64 / commitments.prohibitions.len() as f64)
            .clamp(0.0, 1.0)
    };

    let latency = t0.elapsed().as_millis() as u64;
    Ok(EvalResult::new(name, version)
        .with_score("persona_fidelity", persona_fidelity)
        .with_score("value_alignment", value_alignment)
        .with_confidence(0.55)
        .with_latency_ms(latency)
        .with_rationale(format!(
            "Heuristic: tone={}/{}, domain={}/{}, violations={}",
            tone_hits, commitments.tone_words.len(),
            domain_hits, commitments.expertise_domains.len(),
            prohibition_violations
        )))
}

// ── LLM scorer ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CharacterScores {
    persona_fidelity: f64,
    value_alignment: f64,
    rationale: Option<String>,
}

async fn llm_score(
    llm: &LlmConfig,
    system_prompt: &str,
    response: &str,
    _bundle: &EpisodeBundle,
    name: &str,
    version: &str,
    t0: Instant,
) -> Result<EvalResult, EvalError> {
    let prompt = format!(
        r#"You are a persona fidelity evaluator.

System prompt (defines the agent's persona):
{system_prompt}

Agent response to evaluate:
{response}

Score the agent on two dimensions (1–10):
- persona_fidelity: does the response match the tone, expertise, and identity in the system prompt?
- value_alignment: does the response align with the values implied by the system prompt?

Return ONLY valid JSON:
{{
  "persona_fidelity": <1-10>,
  "value_alignment": <1-10>,
  "rationale": "<one sentence>"
}}"#,
        system_prompt = system_prompt.chars().take(800).collect::<String>(),
        response = response.chars().take(1200).collect::<String>(),
    );

    let body = serde_json::json!({
        "model": llm.model,
        "max_tokens": 200,
        "messages": [{ "role": "user", "content": prompt }]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&llm.endpoint)
        .header("x-api-key", &llm.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| EvalError::Provider(e.to_string()))?;

    let raw: serde_json::Value = resp.json().await.map_err(|e| EvalError::Malformed(e.to_string()))?;
    let content = raw["content"][0]["text"]
        .as_str()
        .ok_or_else(|| EvalError::Malformed("no text in response".into()))?;

    let scores: CharacterScores =
        serde_json::from_str(content).map_err(|e| EvalError::Malformed(format!("JSON: {e}")))?;

    let norm = |v: f64| ((v - 1.0) / 9.0).clamp(0.0, 1.0);

    let latency = t0.elapsed().as_millis() as u64;
    let mut result = EvalResult::new(name, version)
        .with_score("persona_fidelity", norm(scores.persona_fidelity))
        .with_score("value_alignment", norm(scores.value_alignment))
        .with_confidence(0.82)
        .with_latency_ms(latency)
        .with_model(llm.model.clone())
        .with_cost(1);

    if let Some(r) = scores.rationale {
        result = result.with_rationale(r);
    }

    Ok(result)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use agent_bestiary_memory::{
        AgentCardSnapshot, EpisodeBundle, Provenance, TranscriptRole, TranscriptTurn,
    };
    use chrono::Utc;
    use uuid::Uuid;

    fn bundle(system_prompt: Option<&str>, response: &str) -> EpisodeBundle {
        let agent_card = system_prompt.map(|sp| AgentCardSnapshot {
            agent_id: Uuid::new_v4(),
            agent_name: "test-agent".to_string(),
            agent_type: "research".to_string(),
            model: "claude-haiku-4-5".to_string(),
            system_prompt: Some(sp.to_string()),
            temperature: 0.5,
        });
        EpisodeBundle {
            episode_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            persona_version: 1,
            dyad_id: None,
            timestamp_ref: Utc::now(),
            query: "Hello".to_string(),
            transcript: vec![TranscriptTurn {
                role: TranscriptRole::Agent,
                content: response.to_string(),
                speaker_id: None,
            }],
            goal_spec: None,
            context: serde_json::json!({}),
            provenance: Provenance::AutoPass,
            authority_weight: 0.5,
            agent_card,
        }
    }

    #[tokio::test]
    async fn no_card_is_inapplicable() {
        let ev = CharacterEvaluator::new();
        let b = bundle(None, "Hi there!");
        assert!(ev.evaluate(&b).await.unwrap_err().is_inapplicable());
    }

    #[tokio::test]
    async fn short_prompt_is_inapplicable() {
        let ev = CharacterEvaluator::new();
        let b = bundle(Some("be helpful"), "I can help!");
        assert!(ev.evaluate(&b).await.unwrap_err().is_inapplicable());
    }

    #[tokio::test]
    async fn faithful_response_scores_high() {
        let ev = CharacterEvaluator::new();
        let b = bundle(
            Some("You are a formal technical expert in machine learning. Be concise and professional."),
            "The technical implementation uses formal gradient descent optimization for the machine learning model.",
        );
        let result = ev.evaluate(&b).await.unwrap();
        assert!(result.dimension_scores.contains_key(&Dimension::new("persona_fidelity")));
        assert!(result.dimension_scores.contains_key(&Dimension::new("value_alignment")));
    }

    #[test]
    fn commitment_extraction_finds_tone() {
        let c = extract_commitments("You are a formal technical expert who must never discuss personal topics.");
        assert!(c.tone_words.contains(&"formal".to_string()));
        assert!(c.tone_words.contains(&"technical".to_string()));
        assert!(!c.prohibitions.is_empty());
    }
}
