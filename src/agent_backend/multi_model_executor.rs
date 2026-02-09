/// Multi-Model Executor — dispatches to different LLM providers
/// based on the agent's `capabilities.provider` field.
///
/// Supported providers:
///   - anthropic (default, Claude API)
///   - mistral (OpenAI-compatible)
///   - qwen (OpenAI-compatible)
///   - openrouter (OpenAI-compatible proxy)
use crate::agent_backend::executor::{
    AgentExecutor, AgentMetadata, AgentOutput, AgentStatus, ExecutionContext, ExecutionError,
};
use crate::agent_backend::llm_executor::LLMExecutor;
use crate::ast::{AgentStmt, EvidenceStmt};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

/// Provider configuration
struct ProviderConfig {
    api_key: String,
    base_url: String,
}

/// Multi-model executor that dispatches to the right provider
pub struct MultiModelExecutor {
    /// Anthropic executor (handles Claude natively)
    anthropic: LLMExecutor,
    /// OpenAI-compatible providers keyed by name
    providers: HashMap<String, ProviderConfig>,
    client: reqwest::Client,
}

impl MultiModelExecutor {
    /// Discover available providers from environment variables
    pub fn from_env() -> Result<Self, ExecutionError> {
        let anthropic = LLMExecutor::from_env()?;

        let mut providers = HashMap::new();

        if let Ok(key) = std::env::var("MISTRAL_API_KEY") {
            providers.insert(
                "mistral".to_string(),
                ProviderConfig {
                    api_key: key,
                    base_url: "https://api.mistral.ai/v1".to_string(),
                },
            );
            println!("  Multi-model: Mistral provider available");
        }

        if let Ok(key) = std::env::var("QWEN_API_KEY") {
            providers.insert(
                "qwen".to_string(),
                ProviderConfig {
                    api_key: key,
                    base_url: std::env::var("QWEN_BASE_URL").unwrap_or_else(|_| {
                        "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string()
                    }),
                },
            );
            println!("  Multi-model: Qwen provider available");
        }

        if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
            providers.insert(
                "openrouter".to_string(),
                ProviderConfig {
                    api_key: key,
                    base_url: "https://openrouter.ai/api/v1".to_string(),
                },
            );
            println!("  Multi-model: OpenRouter provider available");
        }

        println!(
            "  Multi-model: {} additional provider(s) configured",
            providers.len()
        );

        Ok(Self {
            anthropic,
            providers,
            client: reqwest::Client::new(),
        })
    }

    /// Execute via OpenAI-compatible API (Mistral, Qwen, OpenRouter)
    async fn execute_openai_compatible(
        &self,
        agent: &AgentStmt,
        context: &ExecutionContext,
        config: &ProviderConfig,
    ) -> Result<AgentOutput, ExecutionError> {
        let start = Instant::now();

        let system_prompt = context
            .agent_card
            .system_prompt
            .clone()
            .unwrap_or_else(|| "You are a research agent.".to_string());

        let user_prompt = format!(
            "AGENT TYPE: {}\nRESEARCH QUERY: {}\n\n\
             Respond in JSON: {{\"key_findings\": [...], \"summary\": \"...\", \
             \"sources\": [...], \"confidence\": 0.85, \"reasoning\": \"...\"}}",
            agent.agent_type.as_deref().unwrap_or("research"),
            agent.query
        );

        let request = OpenAIRequest {
            model: context.agent_card.capabilities.model.clone(),
            messages: vec![
                OpenAIMessage::chat("system", &system_prompt),
                OpenAIMessage::chat("user", &user_prompt),
            ],
            temperature: Some(context.agent_card.capabilities.temperature),
            max_tokens: Some(2048),
            tools: None,
            tool_choice: None,
        };

        let oai_response = self.send_openai_request(&request, config).await?;

        let text = oai_response
            .choices
            .first()
            .map(|c| c.message.content.clone().unwrap_or_default())
            .unwrap_or_default();

        let tokens_used = oai_response.usage.as_ref().map(|u| u.total_tokens);
        let elapsed = start.elapsed();

        // Try to parse JSON evidence
        let (evidence, confidence, reasoning) = parse_evidence_json(&text, &agent.name);

        Ok(AgentOutput {
            agent_name: agent.name.clone(),
            agent_type: agent
                .agent_type
                .clone()
                .unwrap_or_else(|| "research".to_string()),
            timestamp: Utc::now(),
            status: AgentStatus::Success,
            evidence,
            confidence,
            sources_consulted: vec![],
            execution_time_ms: elapsed.as_millis() as u64,
            tokens_used,
            metadata: AgentMetadata {
                model_used: Some(context.agent_card.capabilities.model.clone()),
                temperature: Some(context.agent_card.capabilities.temperature),
                reasoning,
            },
            tool_invocations: vec![],
            loop_iterations: 1,
        })
    }

    /// Send a request to an OpenAI-compatible endpoint
    pub(crate) async fn send_openai_request(
        &self,
        request: &OpenAIRequest,
        config: &ProviderConfig,
    ) -> Result<OpenAIResponse, ExecutionError> {
        let url = format!("{}/chat/completions", config.base_url);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| ExecutionError::ExecutionFailed(format!("API request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
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
impl AgentExecutor for MultiModelExecutor {
    async fn execute(
        &self,
        agent: &AgentStmt,
        context: &ExecutionContext,
    ) -> Result<AgentOutput, ExecutionError> {
        let provider = &context.agent_card.capabilities.provider;

        match provider.as_str() {
            "anthropic" | "" => self.anthropic.execute(agent, context).await,
            other => {
                if let Some(config) = self.providers.get(other) {
                    self.execute_openai_compatible(agent, context, config).await
                } else {
                    Err(ExecutionError::ExecutionFailed(format!(
                        "Provider '{}' not configured. Set {}_API_KEY env var.",
                        other,
                        other.to_uppercase()
                    )))
                }
            }
        }
    }

    fn name(&self) -> &str {
        "multi-model"
    }
}

// ─── OpenAI-compatible types (tool-aware) ──────────────────────────

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OpenAIRequest {
    pub model: String,
    pub messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAITool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
}

/// OpenAI tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OpenAITool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: OpenAIFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OpenAIFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// OpenAI message — supports text, assistant with tool_calls, and tool results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OpenAIMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl OpenAIMessage {
    /// Create a simple text message (system, user, or assistant)
    pub fn chat(role: &str, content: &str) -> Self {
        Self {
            role: role.to_string(),
            content: Some(content.to_string()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create a tool result message
    pub fn tool_result(tool_call_id: &str, content: &str) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.to_string()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
        }
    }
}

/// Tool call in an assistant message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OpenAIToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: OpenAIFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OpenAIFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct OpenAIResponse {
    pub choices: Vec<OpenAIChoice>,
    pub usage: Option<OpenAIUsage>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct OpenAIChoice {
    pub message: OpenAIChoiceMessage,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct OpenAIChoiceMessage {
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct OpenAIUsage {
    pub total_tokens: u32,
}

// ─── Evidence parsing ──────────────────────────────────────────────

#[derive(Deserialize)]
struct EvidenceJson {
    #[serde(default)]
    key_findings: Vec<String>,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    sources: Vec<String>,
    #[serde(default = "default_confidence")]
    confidence: f64,
    #[serde(default)]
    reasoning: String,
}

fn default_confidence() -> f64 {
    0.5
}

fn parse_evidence_json(text: &str, agent_name: &str) -> (Vec<EvidenceStmt>, f64, Option<String>) {
    // Try to extract JSON from the response (may be wrapped in markdown code blocks)
    let json_text = if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            &text[start..=end]
        } else {
            text
        }
    } else {
        text
    };

    match serde_json::from_str::<EvidenceJson>(json_text) {
        Ok(data) => {
            let evidence = vec![EvidenceStmt {
                id: format!("{}_evidence_{}", agent_name, Utc::now().timestamp()),
                source: format!("Agent: {}", agent_name),
                summary: Some(data.summary),
                url: None,
                relevance: Some(data.confidence),
                date: Some(Utc::now().format("%Y-%m-%d").to_string()),
                strength: Some(data.confidence),
                key_findings: data.key_findings,
            }];
            (evidence, data.confidence, Some(data.reasoning))
        }
        Err(_) => {
            // Fallback: treat entire text as reasoning
            let evidence = vec![EvidenceStmt {
                id: format!("{}_evidence_{}", agent_name, Utc::now().timestamp()),
                source: format!("Agent: {}", agent_name),
                summary: Some(text.chars().take(200).collect()),
                url: None,
                relevance: Some(0.5),
                date: Some(Utc::now().format("%Y-%m-%d").to_string()),
                strength: Some(0.5),
                key_findings: vec![],
            }];
            (evidence, 0.5, Some(text.to_string()))
        }
    }
}
