/// LLM Executor - Real Claude API Integration
///
/// Calls Anthropic Claude API to generate evidence for forecasts.
use crate::agent_backend::executor::{
    AgentExecutor, AgentMetadata, AgentOutput, AgentStatus, ExecutionContext, ExecutionError,
};
use crate::ast::{AgentStmt, EvidenceStmt};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// LLM Executor using Anthropic Claude API.
///
/// Credential-stateless (SPEC_28): holds no API key. The key for each call
/// arrives on `ExecutionContext.credentials`, resolved per agent from the
/// `agent_credentials` store. This is what makes sharing one instance
/// process-wide compatible with per-agent funding.
pub struct LLMExecutor {
    client: reqwest::Client,
}

impl Default for LLMExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl LLMExecutor {
    pub fn new() -> Self {
        LLMExecutor {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(90))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Retained for call-site compatibility. No longer reads env: there is
    /// nothing to read, because keys are per-execution. Infallible.
    pub fn from_env() -> Result<Self, ExecutionError> {
        Ok(Self::new())
    }

    /// Universal preamble prepended to EVERY agent's system prompt.
    /// This ensures agents are always helpful and never refuse to analyze.
    const HELPFULNESS_PREAMBLE: &'static str = "\
CARDINAL RULES (override everything else):
1. NEVER refuse to help. NEVER say 'I cannot', 'I need more information before I can', or 'I don't have access to'.
2. ALWAYS provide your best analysis with whatever information you have.
3. When information is missing, STATE your assumptions and PROVIDE your analysis anyway.
4. A forecast with stated assumptions is infinitely more useful than a refusal.
5. Use your training knowledge. You know historical data, base rates, trends, and domain patterns.
6. Be QUANTITATIVE — specific numbers, percentages, and data points, not vague qualifiers.
7. If asked about something current you don't know, use the most recent data you have and note the date.

";

    /// Build the system prompt — use agent card's custom prompt if available.
    /// Treats empty strings as absent (Some("") → use default).
    /// Prepends helpfulness preamble for research agents, but SKIPS it for
    /// agents that demand a specific output format (JSON schema agents like fermi).
    /// The preamble's "ALWAYS provide analysis" instruction causes the LLM to
    /// ignore JSON format requirements and return narrative text instead.
    fn build_system_prompt(&self, context: &ExecutionContext) -> String {
        let base_prompt = if let Some(ref custom) = context.agent_card.system_prompt {
            if !custom.trim().is_empty() {
                custom.clone()
            } else {
                "You are a forecasting research agent helping to generate evidence for probabilistic forecasts.".to_string()
            }
        } else {
            "You are a forecasting research agent helping to generate evidence for probabilistic forecasts.".to_string()
        };

        // Skip the preamble for agents that enforce a specific output format.
        // The preamble's "ALWAYS provide your best analysis" causes the LLM to
        // return helpful narrative text instead of following the JSON schema.
        let demands_format = base_prompt.contains("ONLY")
            || base_prompt.contains("JSON")
            || base_prompt.contains("raw JSON")
            || base_prompt.contains("ONLY valid JSON");

        if demands_format {
            base_prompt
        } else {
            format!("{}{}", Self::HELPFULNESS_PREAMBLE, base_prompt)
        }
    }

    /// Returns true if the agent has a meaningful custom system prompt.
    fn has_custom_prompt(context: &ExecutionContext) -> bool {
        context
            .agent_card
            .system_prompt
            .as_ref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
    }

    /// Build the user message for the query.
    ///
    /// If the agent has a custom system prompt, pass the query through as-is.
    /// The system prompt already defines the response format — adding generic
    /// format instructions here would override the agent's own format and
    /// cause the LLM to ignore the system prompt's schema.
    ///
    /// The generic format wrapper is only used for agents WITHOUT a custom
    /// system prompt (legacy agents, simple research queries).
    fn build_prompt(&self, agent: &AgentStmt, context: &ExecutionContext) -> String {
        // If the agent has a custom system prompt, trust it to define the format.
        // Just pass the query with minimal context.
        if Self::has_custom_prompt(context) {
            let mut prompt = String::new();
            prompt.push_str(&agent.query);

            if !agent.driver_refs.is_empty() {
                prompt.push_str("\n\nRelevant forecast drivers:\n");
                for driver_ref in &agent.driver_refs {
                    prompt.push_str(&format!("  - {}\n", driver_ref));
                }
            }

            // Anthropic rejects empty user messages with:
            //   invalid_request_error: messages.0: user messages must have
            //   non-empty content
            // Custom-prompted agents whose query is blank AND have no driver
            // refs previously produced exactly that. Fall back to a nudge
            // that lets the system prompt do all the actual work.
            if prompt.trim().is_empty() {
                return "Begin.".to_string();
            }
            return prompt;
        }

        // Default format wrapper for agents without a custom system prompt.
        let mut prompt = String::new();

        prompt.push_str(&format!(
            "AGENT TYPE: {}\n",
            agent.agent_type.as_ref().unwrap_or(&"research".to_string())
        ));
        prompt.push_str(&format!("RESEARCH QUERY: {}\n\n", agent.query));

        if !agent.driver_refs.is_empty() {
            prompt.push_str("RELEVANT FORECAST DRIVERS:\n");
            for driver_ref in &agent.driver_refs {
                prompt.push_str(&format!("  - {}\n", driver_ref));
            }
            prompt.push_str("\n");
        }

        prompt.push_str("YOUR TASK:\n");
        prompt.push_str("1. Research and analyze information relevant to the query\n");
        prompt.push_str(
            "2. Provide 3-5 key findings that would help inform a probabilistic forecast\n",
        );
        prompt.push_str("3. Cite specific sources where possible\n");
        prompt.push_str("4. Be objective and focus on concrete, quantifiable information\n");
        prompt.push_str("5. Express your confidence level (0.0 to 1.0) in the findings\n\n");

        prompt.push_str("Respond in the following JSON format:\n");
        prompt.push_str("{\n");
        prompt.push_str("  \"key_findings\": [\"finding 1\", \"finding 2\", \"finding 3\"],\n");
        prompt.push_str("  \"summary\": \"Brief summary of research\",\n");
        prompt.push_str("  \"sources\": [\"source 1\", \"source 2\"],\n");
        prompt.push_str("  \"confidence\": 0.85,\n");
        prompt.push_str("  \"reasoning\": \"Why you have this confidence level\"\n");
        prompt.push_str("}\n");

        prompt
    }

    /// Parse Claude response into structured evidence
    fn parse_response(
        &self,
        response: &ClaudeResponse,
        agent_name: &str,
    ) -> Result<EvidenceStmt, ExecutionError> {
        // Extract text from response content blocks
        let text = extract_text_from_content(&response.content);
        if text.is_empty() {
            return Err(ExecutionError::ExecutionFailed(
                "Empty response".to_string(),
            ));
        }

        // Try to extract JSON from the response (it may be embedded in text)
        let json_text = if let Some(start) = text.find('{') {
            if let Some(end) = text.rfind('}') {
                &text[start..=end]
            } else {
                &text
            }
        } else {
            &text
        };

        // Try JSON parse first, fall back to plain text
        match serde_json::from_str::<EvidenceData>(json_text) {
            Ok(evidence_data) => {
                let confidence = if evidence_data.confidence > 0.0 {
                    evidence_data.confidence
                } else {
                    0.5
                };
                Ok(EvidenceStmt {
                    id: format!("{}_evidence_{}", agent_name, Utc::now().timestamp()),
                    source: format!("Agent: {} (Claude API)", agent_name),
                    summary: Some(evidence_data.summary),
                    url: None,
                    relevance: Some(confidence),
                    date: Some(Utc::now().format("%Y-%m-%d").to_string()),
                    strength: Some(confidence),
                    key_findings: evidence_data.key_findings,
                })
            }
            Err(_) => {
                // Fallback: treat as plain text evidence.
                //
                // Special case (issue #4): if the response is itself a
                // structured JSON object that simply didn't match the
                // EvidenceData shape (e.g. supply_chain_oracle's
                // `{items, risks, total_bom_cost, oracle_note}` contract),
                // do NOT stuff the entire JSON back into the `summary`
                // field. The downstream formatter would otherwise emit the
                // same JSON twice — once as the raw response and once as
                // an `**Evidence:**` addendum.
                //
                // We check `json_text` (already stripped of markdown fences
                // above) rather than the original `text` so that responses
                // wrapped in ```json …``` fences are also detected.
                //
                // The raw text remains accessible via
                // `AgentOutput.metadata.reasoning` (and from there
                // `execution_result.metadata.raw_response`), so no
                // information is lost; we just stop duplicating it into
                // the evidence channel where it doesn't belong.
                let looks_like_json_contract =
                    is_json_contract_text(&text) || is_json_contract_text(json_text);
                let (summary, findings): (Option<String>, Vec<String>) = if looks_like_json_contract
                {
                    // Safety net (ABW issue — "Success + empty payload"):
                    // even though this response is a structured JSON
                    // contract that didn't match EvidenceData, try to
                    // salvage a summary and key_findings from well-known
                    // fields (e.g. enemy_sensor's `summary`,
                    // prey_locator's `hunting_summary`, etc.).
                    extract_summary_from_json_contract(&text)
                } else {
                    let findings: Vec<String> = text
                        .lines()
                        .filter(|l| !l.trim().is_empty() && l.len() > 10)
                        .take(10)
                        .map(|l| l.trim().to_string())
                        .collect();
                    (Some(text.to_string()), findings)
                };

                Ok(EvidenceStmt {
                    id: format!("{}_evidence_{}", agent_name, Utc::now().timestamp()),
                    source: format!("Agent: {} (Claude API)", agent_name),
                    summary,
                    url: None,
                    relevance: Some(0.5),
                    date: Some(Utc::now().format("%Y-%m-%d").to_string()),
                    strength: Some(0.5),
                    key_findings: findings,
                })
            }
        }
    }

    /// Send a raw ClaudeRequest and return the parsed response.
    ///
    /// `api_key` is supplied per call from `ExecutionContext.credentials`
    /// (SPEC_28) rather than read from a field captured at construction —
    /// that field is what made this executor structurally incapable of
    /// per-agent funding.
    pub(crate) async fn send_request(
        &self,
        request: &ClaudeRequest,
        api_key: &str,
    ) -> Result<ClaudeResponse, ExecutionError> {
        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| ExecutionError::ExecutionFailed(format!("API request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ExecutionError::ExecutionFailed(format!(
                "API error: {}",
                error_text
            )));
        }

        response.json().await.map_err(|e| {
            ExecutionError::ExecutionFailed(format!("Failed to parse response: {}", e))
        })
    }
}

#[async_trait]
impl AgentExecutor for LLMExecutor {
    async fn execute(
        &self,
        agent: &AgentStmt,
        context: &ExecutionContext,
    ) -> Result<AgentOutput, ExecutionError> {
        let start = Instant::now();

        // Build prompts
        let system_prompt = self.build_system_prompt(context);
        let user_prompt = self.build_prompt(agent, context);

        // Resolve sampling params — model_params JSONB overrides the legacy temperature f64.
        // Agents with custom system prompts (e.g. fermi decomposition) need more tokens.
        let default_max = if Self::has_custom_prompt(context) {
            4096
        } else {
            2048
        };
        let sp = context
            .agent_card
            .capabilities
            .resolve_sampling_params(default_max);

        let thinking = if sp.extended_thinking {
            sp.thinking_budget_tokens.map(|budget| ClaudeThinking {
                thinking_type: "enabled".to_string(),
                budget_tokens: budget,
            })
        } else {
            None
        };

        let request = ClaudeRequest {
            model: context.agent_card.capabilities.model.clone(),
            max_tokens: sp.max_tokens,
            temperature: sp.temperature,
            top_p: sp.top_p,
            top_k: sp.top_k,
            thinking,
            system: Some(system_prompt),
            messages: vec![Message {
                role: "user".to_string(),
                content: MessageContent::Text(user_prompt),
            }],
            tools: None,
            tool_choice: None,
        };

        // Call Claude API with THIS execution's credential. Resolved from
        // the agent's owning principal's store entry; never from env.
        let api_key = context.key_for("anthropic")?;
        // Stamp who paid, so the run is auditable after the fact.
        let funding = context.funding_provenance("anthropic");
        let claude_response = self.send_request(&request, api_key).await?;

        // Extract the full response text BEFORE parsing into evidence
        let full_response_text = extract_text_from_content(&claude_response.content);
        let stop_reason = claude_response.stop_reason.clone();

        // Extract evidence
        let evidence = self.parse_response(&claude_response, &agent.name)?;
        let confidence = evidence.strength.unwrap_or(0.5);

        let elapsed = start.elapsed();

        // Honest status: if the LLM hit max_tokens or produced no text,
        // don't claim Success (issue #3). The caller can still consume what
        // little there is via metadata.reasoning if useful.
        let (status, failure_reason) = if full_response_text.trim().is_empty() {
            (
                AgentStatus::Failed,
                Some(format!(
                    "llm produced empty text (stop_reason={})",
                    stop_reason.as_deref().unwrap_or("?")
                )),
            )
        } else if stop_reason.as_deref() == Some("max_tokens") {
            (
                AgentStatus::Failed,
                Some("llm hit max_tokens; response is truncated".to_string()),
            )
        } else {
            (AgentStatus::Success, None)
        };

        Ok(AgentOutput {
            agent_name: agent.name.clone(),
            agent_type: agent.agent_type.clone().unwrap_or_default(),
            timestamp: Utc::now(),
            status,
            evidence: vec![evidence],
            confidence,
            sources_consulted: vec!["claude-api".to_string()],
            execution_time_ms: elapsed.as_millis() as u64,
            tokens_used: Some(
                claude_response.usage.input_tokens + claude_response.usage.output_tokens,
            ),
            input_tokens: Some(claude_response.usage.input_tokens),
            output_tokens: Some(claude_response.usage.output_tokens),
            metadata: AgentMetadata {
                model_used: Some(claude_response.model),
                temperature: request.temperature,
                reasoning: Some(full_response_text),
                provider: Some("anthropic".to_string()),
                stop_reason,
                failure_reason,
                funding_principal: funding.0,
                credential_source: funding.1,
                ..Default::default()
            },
            tool_invocations: vec![],
            loop_iterations: 1,
        })
    }

    fn name(&self) -> &str {
        "llm"
    }
}

// ─── Claude API types (tool-aware) ─────────────────────────────────

/// Claude API request structure
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ClaudeRequest {
    pub model: String,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ClaudeThinking>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ClaudeTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
}

/// Anthropic extended thinking block — requires temperature = 1.0.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ClaudeThinking {
    #[serde(rename = "type")]
    pub thinking_type: String,
    pub budget_tokens: u32,
}

/// A tool definition for the Anthropic API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ClaudeTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Message in the conversation
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Message {
    pub role: String,
    pub content: MessageContent,
}

/// Message content — either a plain text string or an array of content blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum MessageContent {
    Text(String),
    Blocks(Vec<MessageBlock>),
}

/// A content block within a message (for multi-block messages)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum MessageBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

/// Claude API response structure
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ClaudeResponse {
    pub id: String,
    pub model: String,
    pub content: Vec<ContentBlock>,
    pub usage: Usage,
    #[serde(default)]
    pub stop_reason: Option<String>,
}

/// A content block in the response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Heuristic — does this text look like a JSON-contract response (an
/// object or array that parses as valid JSON), possibly wrapped in a
/// markdown ```json … ``` fence? Used to suppress the legacy Evidence
/// addendum when an agent's primary answer is structured JSON
/// (issue #4 / docs/specs/10_RESEARCH_AGENTS_EMPTY_LLM_OUTPUT.md
/// follow-up).
pub(crate) fn is_json_contract_text(text: &str) -> bool {
    let t = text.trim();
    // Strip an optional ```json … ``` fence.
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```JSON"))
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t)
        .trim_start();
    let t = t.strip_suffix("```").unwrap_or(t).trim();
    if !(t.starts_with('{') || t.starts_with('[')) {
        return false;
    }
    serde_json::from_str::<serde_json::Value>(t).is_ok()
}

/// Extract all text from content blocks
pub(crate) fn extract_text_from_content(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Try to extract a human-readable summary and key findings from a JSON
/// contract response whose shape doesn't match `EvidenceData`.
///
/// Creature / research agents (enemy_sensor, genome_profiler, prey_locator, …)
/// embed a `"summary"` or `"hunting_summary"` field directly in their response
/// JSON.  We also harvest top-level string arrays (threats, prey_targets, …) as
/// key_findings so the evidence card is never entirely blank.
///
/// This is the safety-net used by both `LLMExecutor::parse_response` and
/// `tool_executor::parse_evidence_text` (ABW issue — "Success + empty payload").
pub(crate) fn extract_summary_from_json_contract(text: &str) -> (Option<String>, Vec<String>) {
    // Strip optional ```json … ``` fence before parsing.
    let t = text.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```JSON"))
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t)
        .trim_start();
    let stripped = t.strip_suffix("```").unwrap_or(t).trim();

    let value: serde_json::Value = match serde_json::from_str(stripped) {
        Ok(v) => v,
        Err(_) => return (None, Vec::new()),
    };

    let obj = match value.as_object() {
        Some(o) => o,
        // Arrays have no obvious summary field — leave empty.
        None => return (None, Vec::new()),
    };

    // --- summary string ---
    // Prefer the conventional "summary" key, then agent-specific variants.
    let summary_keys = [
        "summary",
        "hunting_summary",
        "oracle_note",
        "note",
        "description",
        "assessment",
    ];
    let summary = summary_keys
        .iter()
        .find_map(|k| obj.get(*k)?.as_str().map(|s| s.to_string()))
        .filter(|s| !s.is_empty());

    // --- key findings ---
    // Collect short string descriptions from well-known array / scalar fields.
    let array_keys = [
        "threats",
        "prey_targets",
        "key_findings",
        "findings",
        "items",
        "risks",
        "notable_genes",
        "defining_traits",
        "sister_taxa",
    ];
    let mut findings: Vec<String> = Vec::new();
    for key in &array_keys {
        if let Some(arr) = obj.get(*key).and_then(|v| v.as_array()) {
            for item in arr.iter().take(10) {
                if let Some(s) = item.as_str() {
                    if !s.is_empty() {
                        findings.push(s.to_string());
                    }
                } else if let Some(sub) = item.as_object() {
                    // For structured items, build a short description from
                    // common fields: species, relationship, reasoning, name, etc.
                    let desc_keys = [
                        "species",
                        "name",
                        "relationship",
                        "reasoning",
                        "risk",
                        "vulnerability",
                    ];
                    let parts: Vec<&str> = desc_keys
                        .iter()
                        .filter_map(|dk| sub.get(*dk)?.as_str())
                        .collect();
                    if !parts.is_empty() {
                        findings.push(parts.join(" — "));
                    }
                }
            }
            if findings.len() >= 10 {
                break;
            }
        }
    }

    // Fallback: if we have no summary but the response carries a top-level
    // "threat_level" string, synthesise a minimal human-readable one.
    let summary = summary.or_else(|| {
        obj.get("threat_level")
            .and_then(|v| v.as_str())
            .map(|tl| format!("Threat level: {}", tl))
    });

    (summary, findings)
}

/// Parsed evidence data from LLM response
#[derive(Debug, Deserialize)]
struct EvidenceData {
    key_findings: Vec<String>,
    summary: String,
    #[serde(default)]
    sources: Vec<String>,
    confidence: f64,
    #[serde(default)]
    reasoning: String,
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_response(text: &str) -> ClaudeResponse {
        ClaudeResponse {
            id: "msg_test".to_string(),
            model: "claude-test".to_string(),
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            usage: Usage {
                input_tokens: 0,
                output_tokens: 0,
            },
            stop_reason: Some("end_turn".to_string()),
        }
    }

    /// Regression for issue #4: when the LLM returns a JSON object that
    /// doesn't match the EvidenceData shape (e.g. supply_chain_oracle's
    /// `{items, risks, total_bom_cost, oracle_note}` contract), the fallback
    /// branch must NOT stuff the whole raw JSON blob into `summary`.
    ///
    /// Updated for ABW fix: `oracle_note` is now harvested into `summary`
    /// and item `name` fields into `key_findings`. The raw JSON must NOT
    /// appear verbatim in `summary` (that was the original issue #4 bug).
    #[test]
    fn parse_response_does_not_stuff_json_contract_into_summary() {
        let executor = LLMExecutor::new();
        let oracle_response = r#"```json
{
  "items": [{"name": "Tea", "unit_cost": 42}],
  "risks": [],
  "total_bom_cost": 42,
  "oracle_note": "ok"
}
```"#;
        let response = mk_response(oracle_response);
        let evidence = executor
            .parse_response(&response, "supply_chain_oracle")
            .expect("parse_response should succeed");

        // `oracle_note` is a recognised summary-key — it must be extracted.
        assert_eq!(
            evidence.summary.as_deref(),
            Some("ok"),
            "oracle_note must be harvested as summary, was {:?}",
            evidence.summary
        );
        // Critically, the raw JSON blob must NOT be the summary value.
        if let Some(ref s) = evidence.summary {
            assert!(
                !s.contains("total_bom_cost"),
                "raw JSON must not be stuffed into summary; got: {:?}",
                s
            );
        }
    }

    /// Confirm the JSON-array contract case is also detected, not just objects.
    #[test]
    fn parse_response_recognises_array_contract() {
        let executor = LLMExecutor::new();
        let response = mk_response(r#"[{"name": "a"}, {"name": "b"}]"#);
        let evidence = executor
            .parse_response(&response, "test_agent")
            .expect("parse_response should succeed");
        assert!(evidence.summary.is_none());
    }

    /// Free-form text responses should still get extracted into summary
    /// + findings — the fix is targeted at JSON-contract shapes only.
    #[test]
    fn parse_response_preserves_summary_for_free_form_text() {
        let executor = LLMExecutor::new();
        let response = mk_response(
            "Analysis of the situation:\n\
             - The market is volatile.\n\
             - Lead times are extended.\n\
             Forecast: 6-9 months recovery.",
        );
        let evidence = executor
            .parse_response(&response, "test_agent")
            .expect("parse_response should succeed");
        assert!(
            evidence.summary.is_some(),
            "free-form text must keep its summary"
        );
        let s = evidence.summary.unwrap();
        assert!(s.contains("Analysis"));
        assert!(!evidence.key_findings.is_empty());
    }

    /// EvidenceData-shaped JSON (the proper contract) goes through the
    /// happy path, not the fallback — summary comes from the JSON field.
    /// Then the JSON-contract detection below the Err branch fires too,
    /// since the response IS a JSON contract; the happy path still wins
    /// because it returns Ok first.
    #[test]
    fn parse_response_uses_summary_field_when_evidence_data_shape() {
        let executor = LLMExecutor::new();
        let response = mk_response(
            r#"{"key_findings": ["a", "b"], "summary": "headline finding", "confidence": 0.8}"#,
        );
        let evidence = executor
            .parse_response(&response, "test_agent")
            .expect("parse_response should succeed");
        assert_eq!(evidence.summary.as_deref(), Some("headline finding"));
        assert_eq!(evidence.key_findings.len(), 2);
    }

    // ─── is_json_contract_text helper ────────────────────────────────

    #[test]
    fn is_json_contract_text_detects_bare_object() {
        assert!(is_json_contract_text(r#"{"items":[]}"#));
        assert!(is_json_contract_text(r#"  {"items":[]}  "#));
    }

    #[test]
    fn is_json_contract_text_detects_bare_array() {
        assert!(is_json_contract_text(r#"[1, 2, 3]"#));
        assert!(is_json_contract_text(r#"[{"a":1}]"#));
    }

    #[test]
    fn is_json_contract_text_detects_markdown_fenced_json() {
        assert!(is_json_contract_text(
            "```json\n{\"items\":[{\"name\":\"Tea\"}]}\n```"
        ));
        assert!(is_json_contract_text("```\n{\"a\":1}\n```"));
        assert!(is_json_contract_text("```JSON\n[1,2]\n```"));
    }

    #[test]
    fn is_json_contract_text_rejects_invalid_json() {
        // Looks like JSON but isn't parseable.
        assert!(!is_json_contract_text("{not valid json}"));
        assert!(!is_json_contract_text("{\"items\":,}"));
    }

    #[test]
    fn is_json_contract_text_rejects_free_form_text() {
        assert!(!is_json_contract_text(
            "Analysis: the market is up.\n- bullet"
        ));
        assert!(!is_json_contract_text("# Heading\n\nParagraph."));
        assert!(!is_json_contract_text(""));
    }

    #[test]
    fn is_json_contract_text_rejects_partial_json() {
        // Free-form text that quotes a JSON snippet inline shouldn't trigger
        // — we only suppress addenda when the entire response is structured.
        assert!(!is_json_contract_text(
            "The agent emitted {\"items\":[]}.\n\nAnalysis follows."
        ));
    }
}
