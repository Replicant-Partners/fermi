/// Multi-Model Executor — dispatches to different LLM providers
/// based on the agent's `capabilities.provider` field.
///
/// Supported providers:
///   - anthropic (default, Claude API)
///   - mistral (OpenAI-compatible)
///   - qwen (OpenAI-compatible)
///   - openrouter (OpenAI-compatible proxy)
///   - glm (Zhipu AI GLM, OpenAI-compatible — GLM_API_KEY / GLM_BASE_URL)
///   - deepseek (DeepSeek, OpenAI-compatible — DEEPSEEK_API_KEY)
///   - kimi (Moonshot AI Kimi, OpenAI-compatible — KIMI_API_KEY)
///   - ollama (local Ollama instance — OLLAMA_BASE_URL, no API key required)
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

/// Provider endpoint configuration.
///
/// Endpoint only — no credential. Base URLs are operator config (which
/// endpoint this deployment talks to) and legitimately come from env;
/// keys are per-agent and come from the credential store (SPEC_28).
pub(crate) struct ProviderConfig {
    /// API key for THIS execution, resolved from
    /// `ExecutionContext.credentials`. Empty for providers needing none
    /// (e.g. Ollama).
    pub(crate) api_key: String,
    pub(crate) base_url: String,
}

/// Multi-model executor that dispatches to the right provider.
///
/// Credential-stateless (SPEC_28). Previously this captured one key per
/// provider from env at boot, which meant every agent reaching it — all
/// structured-output agents, since they bypass the tool loop — ran on the
/// platform's key regardless of ownership.
pub struct MultiModelExecutor {
    /// Anthropic executor (handles Claude natively)
    anthropic: LLMExecutor,
    /// Base URLs for OpenAI-compatible providers, keyed by provider name.
    base_urls: HashMap<String, String>,
    client: reqwest::Client,
}

/// Base URL for an OpenAI-compatible provider. Operator config, so env is
/// the right source; `None` for an unrecognised provider name.
pub(crate) fn provider_base_url(provider: &str) -> Option<String> {
    let url = match provider {
        "mistral" => "https://api.mistral.ai/v1".to_string(),
        "qwen" => std::env::var("QWEN_BASE_URL")
            .unwrap_or_else(|_| "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string()),
        "openrouter" => "https://openrouter.ai/api/v1".to_string(),
        "glm" => std::env::var("GLM_BASE_URL")
            .unwrap_or_else(|_| "https://open.bigmodel.cn/api/paas/v4".to_string()),
        "deepseek" => std::env::var("DEEPSEEK_BASE_URL")
            .unwrap_or_else(|_| "https://api.deepseek.com/v1".to_string()),
        "kimi" => std::env::var("KIMI_BASE_URL")
            .unwrap_or_else(|_| "https://api.moonshot.cn/v1".to_string()),
        "ollama" => std::env::var("OLLAMA_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:11434/v1".to_string()),
        _ => return None,
    };
    Some(url)
}

/// Every OpenAI-compatible provider the executor can dispatch to.
pub(crate) const OPENAI_COMPATIBLE_PROVIDERS: &[&str] = &[
    "mistral",
    "qwen",
    "openrouter",
    "glm",
    "deepseek",
    "kimi",
    "ollama",
];

impl MultiModelExecutor {
    /// Build the executor. Reads no credentials — only endpoint config.
    ///
    /// Provider *availability* is no longer a boot-time property (it used
    /// to mean "the server has an env key for it"). It is now a per-agent
    /// property: an agent can use any provider its owner has funded.
    pub fn from_env() -> Result<Self, ExecutionError> {
        let anthropic = LLMExecutor::from_env()?;

        let mut base_urls = HashMap::new();
        for p in OPENAI_COMPATIBLE_PROVIDERS {
            if let Some(url) = provider_base_url(p) {
                base_urls.insert((*p).to_string(), url);
            }
        }

        println!(
            "  Multi-model: {} OpenAI-compatible provider endpoint(s) known; \
             keys resolved per-agent from the credential store",
            base_urls.len()
        );

        Ok(Self {
            anthropic,
            base_urls,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(90))
                .build()
                .unwrap_or_default(),
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

        let sp = context
            .agent_card
            .capabilities
            .resolve_sampling_params(2048);

        let request = OpenAIRequest {
            model: context.agent_card.capabilities.model.clone(),
            messages: vec![
                OpenAIMessage::chat("system", &system_prompt),
                OpenAIMessage::chat("user", &user_prompt),
            ],
            temperature: sp.temperature,
            max_tokens: Some(sp.max_tokens),
            top_p: sp.top_p,
            frequency_penalty: sp.frequency_penalty,
            presence_penalty: sp.presence_penalty,
            repetition_penalty: sp.repetition_penalty,
            seed: sp.random_seed,
            tools: None,
            tool_choice: None,
        };

        let provider_name = context.agent_card.capabilities.provider.clone();
        let funding = context.funding_provenance(&provider_name);
        let oai_response = self.send_openai_request(&request, config).await?;

        let text = oai_response
            .choices
            .first()
            .map(|c| c.message.content.clone().unwrap_or_default())
            .unwrap_or_default();

        let tokens_used = oai_response.usage.as_ref().map(|u| u.total_tokens);
        // Only trust the split when the provider actually reported it;
        // otherwise leave it absent so pricing assumes a split instead of
        // reading a missing breakdown as a free run.
        let split_in = oai_response.usage.as_ref().and_then(|u| u.prompt_tokens);
        let split_out = oai_response
            .usage
            .as_ref()
            .and_then(|u| u.completion_tokens);
        let elapsed = start.elapsed();

        // Try to parse JSON evidence
        let (evidence, confidence, reasoning) = parse_evidence_json(&text, &agent.name);

        Ok(AgentOutput {
            raw_response: Some(text.clone()),
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
            input_tokens: split_in,
            output_tokens: split_out,
            metadata: AgentMetadata {
                model_used: Some(context.agent_card.capabilities.model.clone()),
                temperature: sp.temperature,
                reasoning,
                provider: Some(provider_name),
                funding_principal: funding.0,
                credential_source: funding.1,
                card_prompt_hash: context.card_prompt_hash(),
                effective_prompt_hash: context.effective_prompt_hash(),
                ..Default::default()
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
        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");
        // Skip Authorization header for providers that don't require a key (e.g. Ollama)
        if !config.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", config.api_key));
        }
        let response =
            req.json(request).send().await.map_err(|e| {
                ExecutionError::ExecutionFailed(format!("API request failed: {}", e))
            })?;

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
        // ADR-011 Phase 2: resolve model from ladder when creature tier is present
        let resolved_ctx;
        let effective_context: &ExecutionContext = if let Some(tier) = &context.cognition_tier {
            let mut patched = context.clone();
            patched.agent_card.capabilities.apply_tier_resolution(tier);
            resolved_ctx = patched;
            &resolved_ctx
        } else {
            context
        };

        let provider = &effective_context.agent_card.capabilities.provider;

        match provider.as_str() {
            "anthropic" | "" => self.anthropic.execute(agent, effective_context).await,
            other => {
                // Endpoint from operator config; key from THIS execution's
                // credentials. An unfunded agent gets
                // `ExecutionError::Unfunded` naming its owner's remedy,
                // rather than the old message telling the owner to set a
                // server env var they cannot reach.
                let base_url = self.base_urls.get(other).cloned().ok_or_else(|| {
                    ExecutionError::ExecutionFailed(format!(
                        "Unknown provider '{}'. Supported: anthropic, {}.",
                        other,
                        OPENAI_COMPATIBLE_PROVIDERS.join(", ")
                    ))
                })?;
                let config = ProviderConfig {
                    api_key: effective_context.key_for(other)?.to_string(),
                    base_url,
                };
                self.execute_openai_compatible(agent, effective_context, &config)
                    .await
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
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    /// Used by Mistral and some other providers (not standard OpenAI)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repetition_penalty: Option<f64>,
    /// Reproducibility seed — OpenAI calls this "seed", Mistral also supports it
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
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
    /// Input tokens. `Option` because OpenAI-compatible providers are
    /// inconsistent about returning the breakdown — several return only
    /// `total_tokens`. Absent reads as "split unknown", which pricing
    /// handles by assuming a split rather than by charging zero.
    #[serde(default)]
    pub prompt_tokens: Option<u32>,
    /// Output tokens. See `prompt_tokens`.
    #[serde(default)]
    pub completion_tokens: Option<u32>,
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
