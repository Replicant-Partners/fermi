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

/// LLM Executor using Anthropic Claude API
pub struct LLMExecutor {
    api_key: String,
    client: reqwest::Client,
}

impl LLMExecutor {
    /// Create new LLM executor with API key
    pub fn new(api_key: String) -> Self {
        LLMExecutor {
            api_key,
            client: reqwest::Client::new(),
        }
    }

    /// Create from environment variable ANTHROPIC_API_KEY
    pub fn from_env() -> Result<Self, ExecutionError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
            ExecutionError::ExecutionFailed(
                "ANTHROPIC_API_KEY environment variable not set".to_string(),
            )
        })?;
        Ok(Self::new(api_key))
    }

    /// Build the system prompt — use agent card's custom prompt if available.
    /// Treats empty strings as absent (Some("") → use default).
    fn build_system_prompt(&self, context: &ExecutionContext) -> String {
        if let Some(ref custom) = context.agent_card.system_prompt {
            if !custom.trim().is_empty() {
                return custom.clone();
            }
        }
        // Default forecasting system prompt
        "You are a forecasting research agent helping to generate evidence for probabilistic forecasts.".to_string()
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
                // Fallback: treat as plain text evidence
                // IMPORTANT: preserve the FULL text as summary so downstream
                // consumers can parse structured data from it.
                // Also store key lines as findings for display.
                let summary = text.to_string();
                let findings: Vec<String> = text
                    .lines()
                    .filter(|l| !l.trim().is_empty() && l.len() > 10)
                    .take(10)
                    .map(|l| l.trim().to_string())
                    .collect();

                Ok(EvidenceStmt {
                    id: format!("{}_evidence_{}", agent_name, Utc::now().timestamp()),
                    source: format!("Agent: {} (Claude API)", agent_name),
                    summary: Some(summary),
                    url: None,
                    relevance: Some(0.5),
                    date: Some(Utc::now().format("%Y-%m-%d").to_string()),
                    strength: Some(0.5),
                    key_findings: findings,
                })
            }
        }
    }

    /// Send a raw ClaudeRequest and return the parsed response
    pub(crate) async fn send_request(
        &self,
        request: &ClaudeRequest,
    ) -> Result<ClaudeResponse, ExecutionError> {
        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
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

        // Prepare Claude API request
        // Agents with custom system prompts (e.g., fermi decomposition) need more
        // tokens for structured JSON output. Default agents use 2048.
        let max_tokens = if Self::has_custom_prompt(context) {
            4096
        } else {
            2048
        };
        let request = ClaudeRequest {
            model: context.agent_card.capabilities.model.clone(),
            max_tokens,
            temperature: context.agent_card.capabilities.temperature,
            system: Some(system_prompt),
            messages: vec![Message {
                role: "user".to_string(),
                content: MessageContent::Text(user_prompt),
            }],
            tools: None,
            tool_choice: None,
        };

        // Call Claude API
        let claude_response = self.send_request(&request).await?;

        // Extract the full response text BEFORE parsing into evidence
        let full_response_text = extract_text_from_content(&claude_response.content);

        // Extract evidence
        let evidence = self.parse_response(&claude_response, &agent.name)?;
        let confidence = evidence.strength.unwrap_or(0.5);

        let elapsed = start.elapsed();

        Ok(AgentOutput {
            agent_name: agent.name.clone(),
            agent_type: agent.agent_type.clone().unwrap_or_default(),
            timestamp: Utc::now(),
            status: AgentStatus::Success,
            evidence: vec![evidence],
            confidence,
            sources_consulted: vec!["claude-api".to_string()],
            execution_time_ms: elapsed.as_millis() as u64,
            tokens_used: Some(
                claude_response.usage.input_tokens + claude_response.usage.output_tokens,
            ),
            metadata: AgentMetadata {
                model_used: Some(claude_response.model),
                temperature: Some(request.temperature),
                // Store the full response text so downstream consumers
                // can parse structured data from it
                reasoning: Some(full_response_text),
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
    pub temperature: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ClaudeTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
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
