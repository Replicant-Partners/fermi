use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Represents a single message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
}

/// Role of a message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Configuration for LLM generation
#[derive(Debug, Clone)]
pub struct GenerationConfig {
    pub temperature: f32,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub stop_sequences: Vec<String>,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            max_tokens: Some(4096),
            top_p: None,
            stop_sequences: vec![],
        }
    }
}

/// Response from LLM generation
#[derive(Debug, Clone)]
pub struct GenerationResponse {
    pub content: String,
    pub model: String,
    pub usage: TokenUsage,
    pub finish_reason: Option<String>,
}

/// Token usage statistics
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Unified interface for LLM providers
#[async_trait]
pub trait LLMProvider: Send + Sync {
    /// Generate raw text response
    async fn generate_raw(
        &self,
        messages: Vec<Message>,
        config: &GenerationConfig,
    ) -> Result<GenerationResponse>;

    /// Get the model name used by this provider
    fn model_name(&self) -> &str;

    /// Check if this provider supports function/tool calling
    fn supports_tools(&self) -> bool;

    /// Get the provider name (e.g., "anthropic", "mistral", "qwen", "openrouter")
    fn provider_name(&self) -> &str;
}

/// Generate structured output with automatic parsing and graceful degradation
///
/// This is a helper function that wraps any LLMProvider and parses the response
/// into a typed structure. It provides automatic JSON parsing with helpful error messages.
///
/// # Example
/// ```rust,no_run
/// # use agent_bestiary_memory::*;
/// # async fn example(llm: Arc<dyn LLMProvider>) -> Result<()> {
/// #[derive(serde::Deserialize)]
/// struct Response {
///     rules: Vec<String>,
///     confidence: f64,
/// }
///
/// let response: Response = generate_structured(
///     &llm,
///     messages,
///     &config
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn generate_structured<T>(
    provider: &dyn LLMProvider,
    messages: Vec<Message>,
    config: &GenerationConfig,
) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    generate_structured_with_usage(provider, messages, config)
        .await
        .map(|(value, _usage)| value)
}

/// [`generate_structured`], but hands back what the call cost.
///
/// `generate_structured` throws `GenerationResponse.usage` away. That was
/// invisible until the dreaming pipeline needed it: consolidation drives the
/// ontologist through this function several times per cycle, and because the
/// token counts were discarded at exactly this line, the platform's most
/// frequently-invoked system agent had no measurable cost anywhere. The
/// spending was real; only the record was missing.
///
/// Kept as a separate function rather than changing the return type of
/// `generate_structured`, so callers that genuinely do not care about usage
/// are untouched.
pub async fn generate_structured_with_usage<T>(
    provider: &dyn LLMProvider,
    messages: Vec<Message>,
    config: &GenerationConfig,
) -> Result<(T, TokenUsage)>
where
    T: serde::de::DeserializeOwned,
{
    let response = provider.generate_raw(messages, config).await?;
    let usage = response.usage.clone();

    let parsed = parse_lenient::<T>(&response.content).map_err(|e| {
        crate::error::MemoryError::ExternalError(format!(
            "Failed to parse structured output: {}. Response was: {}",
            e, response.content
        ))
    })?;

    Ok((parsed, usage))
}

/// Strip a markdown code fence, if the response is wrapped in one.
///
/// Handles both the multi-line form the chat models actually emit —
///
/// ```text
/// ```json
/// [{"name": "Asilidae"}]
/// ```
/// ```
///
/// — and the degenerate single-line form ```` ```[1,2]``` ````.
fn strip_code_fence(s: &str) -> &str {
    let t = s.trim();
    if !t.starts_with("```") {
        return t;
    }
    // Drop the opening fence and its optional language tag. On the
    // single-line form there is no newline, so fall back to trimming the
    // backticks and whatever language tag is glued to them.
    let after_open = match t.find('\n') {
        Some(i) => &t[i + 1..],
        None => t.trim_matches('`'),
    };
    // Drop the closing fence. An unterminated fence (the model hit
    // max_tokens mid-array) still yields the prefix, which `parse_lenient`
    // will simply fail on — the same outcome as before, no worse.
    match after_open.rfind("```") {
        Some(i) => after_open[..i].trim(),
        None => after_open.trim(),
    }
}

/// The widest span between the first `[`/`{` and the last `]`/`}`.
///
/// Last resort for a model that prefixed its JSON with a sentence of
/// explanation despite being told to return only JSON.
fn json_span(s: &str) -> Option<&str> {
    let start = s.find(['[', '{'])?;
    let end = s.rfind([']', '}'])?;
    (end > start).then(|| &s[start..=end])
}

/// Parse model output as JSON, tolerating the two ways a chat model
/// habitually disobeys "return ONLY a JSON array".
///
/// ## Why this is not just `serde_json::from_str`
///
/// It was, and it cost the platform its entire learning loop. `gpt-4o-mini`
/// — the `ontologist`'s configured model — wraps JSON in a markdown fence
/// whenever it feels like it, which is often. `from_str` on that content
/// fails at *line 1 column 1*, because line 1 column 1 is a backtick.
///
/// Every consolidation extractor (entities, facts, knowledge rules) funnels
/// through this function, and each caller treats a parse failure as
/// non-fatal: it logs, `continue`s, and the cycle completes having learned
/// nothing. So a purely cosmetic formatting habit in the model presented as
/// "dreaming ran successfully and extracted 0 entities" — on every agent,
/// for months. The measured signature in `episodes`:
///
/// ```text
/// External API error: Failed to parse structured output: expected value
/// at line 1 column 1. Response was: ```json [ {"name": "Asilidae", ...
/// ```
///
/// The entities were right there in the error message.
///
/// ## Order matters
///
/// Strict parse first, so a well-behaved bare-JSON response is never put
/// through the salvage heuristics, and a `T` that is legitimately a string
/// or a number is unaffected. The fence and span attempts only run once the
/// strict parse has already failed.
///
/// On total failure the **original** error is returned, not the error from
/// the last salvage attempt: it describes the text the model actually sent,
/// which is what an operator reading the log needs.
fn parse_lenient<T>(raw: &str) -> serde_json::Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let trimmed = raw.trim();
    let first_err = match serde_json::from_str::<T>(trimmed) {
        Ok(v) => return Ok(v),
        Err(e) => e,
    };

    let unfenced = strip_code_fence(trimmed);
    if unfenced != trimmed {
        if let Ok(v) = serde_json::from_str::<T>(unfenced) {
            return Ok(v);
        }
    }

    if let Some(span) = json_span(unfenced) {
        if span != unfenced {
            if let Ok(v) = serde_json::from_str::<T>(span) {
                return Ok(v);
            }
        }
    }

    Err(first_err)
}

/// Factory for creating LLM providers
pub struct LLMProviderFactory;

impl LLMProviderFactory {
    /// Create a provider from configuration
    pub fn create(config: &LLMProviderConfig) -> Result<Arc<dyn LLMProvider>> {
        match config.provider_type {
            ProviderType::Anthropic => Ok(Arc::new(AnthropicProvider::new(
                config.api_key.clone(),
                config.model.clone(),
                config.base_url.clone(),
            )?)),
            ProviderType::Mistral => Ok(Arc::new(MistralProvider::new(
                config.api_key.clone(),
                config.model.clone(),
                config.base_url.clone(),
            )?)),
            ProviderType::Qwen => Ok(Arc::new(QwenProvider::new(
                config.api_key.clone(),
                config.model.clone(),
                config.base_url.clone(),
            )?)),
            ProviderType::OpenRouter => Ok(Arc::new(OpenRouterProvider::new(
                config.api_key.clone(),
                config.model.clone(),
                config.base_url.clone(),
            )?)),
            // DeepSeek and Kimi are OpenAI-compatible — reuse OpenRouterProvider
            // with their respective base URLs.
            ProviderType::DeepSeek => Ok(Arc::new(OpenRouterProvider::new(
                config.api_key.clone(),
                config.model.clone(),
                Some(
                    config
                        .base_url
                        .clone()
                        .unwrap_or_else(|| "https://api.deepseek.com/v1".to_string()),
                ),
            )?)),
            ProviderType::Kimi => Ok(Arc::new(OpenRouterProvider::new(
                config.api_key.clone(),
                config.model.clone(),
                Some(
                    config
                        .base_url
                        .clone()
                        .unwrap_or_else(|| "https://api.moonshot.cn/v1".to_string()),
                ),
            )?)),
            // OpenAI is OpenAI-compatible (obviously) — reuse OpenRouterProvider
            // pointed at OpenAI's base URL. It POSTs {base}/chat/completions with
            // `Authorization: Bearer <key>`, which is exactly OpenAI's chat API.
            ProviderType::OpenAI => Ok(Arc::new(OpenRouterProvider::new(
                config.api_key.clone(),
                config.model.clone(),
                Some(
                    config
                        .base_url
                        .clone()
                        .unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
                ),
            )?)),
        }
    }
}

/// Configuration for LLM provider
#[derive(Debug, Clone)]
pub struct LLMProviderConfig {
    pub provider_type: ProviderType,
    pub api_key: String,
    pub model: String,
    pub base_url: Option<String>,
}

/// Supported LLM provider types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderType {
    Anthropic,
    Mistral,
    Qwen,
    OpenRouter,
    DeepSeek,
    Kimi,
    OpenAI,
}

impl std::str::FromStr for ProviderType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "anthropic" | "claude" => Ok(ProviderType::Anthropic),
            "mistral" => Ok(ProviderType::Mistral),
            "qwen" => Ok(ProviderType::Qwen),
            "openrouter" => Ok(ProviderType::OpenRouter),
            "deepseek" => Ok(ProviderType::DeepSeek),
            "kimi" | "moonshot" => Ok(ProviderType::Kimi),
            "openai" | "gpt" => Ok(ProviderType::OpenAI),
            _ => Err(format!("Unknown provider type: {}", s)),
        }
    }
}

// ===== Anthropic Provider =====

pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: String, base_url: Option<String>) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(90))
                .build()
                .unwrap_or_default(),
            api_key,
            model,
            base_url: base_url.unwrap_or_else(|| "https://api.anthropic.com".to_string()),
        })
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    async fn generate_raw(
        &self,
        messages: Vec<Message>,
        config: &GenerationConfig,
    ) -> Result<GenerationResponse> {
        #[derive(Serialize)]
        struct AnthropicRequest {
            model: String,
            messages: Vec<AnthropicMessage>,
            max_tokens: u32,
            temperature: f32,
            #[serde(skip_serializing_if = "Option::is_none")]
            top_p: Option<f32>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            stop_sequences: Vec<String>,
        }

        #[derive(Serialize)]
        struct AnthropicMessage {
            role: String,
            content: String,
        }

        #[derive(Deserialize)]
        struct AnthropicResponse {
            content: Vec<ContentBlock>,
            model: String,
            usage: AnthropicUsage,
            stop_reason: Option<String>,
        }

        #[derive(Deserialize)]
        struct ContentBlock {
            text: String,
        }

        #[derive(Deserialize)]
        struct AnthropicUsage {
            input_tokens: u32,
            output_tokens: u32,
        }

        // Separate system messages from conversation
        let system_msg = messages
            .iter()
            .find(|m| matches!(m.role, MessageRole::System))
            .map(|m| m.content.clone());

        let conversation: Vec<AnthropicMessage> = messages
            .into_iter()
            .filter(|m| !matches!(m.role, MessageRole::System))
            .map(|m| AnthropicMessage {
                role: match m.role {
                    MessageRole::User => "user".to_string(),
                    MessageRole::Assistant => "assistant".to_string(),
                    MessageRole::System => "user".to_string(), // Should not happen
                },
                content: m.content,
            })
            .collect();

        let request = AnthropicRequest {
            model: self.model.clone(),
            messages: conversation,
            max_tokens: config.max_tokens.unwrap_or(4096),
            temperature: config.temperature,
            top_p: config.top_p,
            stop_sequences: config.stop_sequences.clone(),
        };

        let mut req_builder = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");

        // Add system message if present
        if let Some(system) = system_msg {
            req_builder = req_builder.header("anthropic-system", system);
        }

        let response = req_builder
            .json(&request)
            .send()
            .await
            .map_err(|e| crate::error::MemoryError::ExternalError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(crate::error::MemoryError::ExternalError(format!(
                "Anthropic API error {}: {}",
                status, error_text
            )));
        }

        let anthropic_resp: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| crate::error::MemoryError::ExternalError(e.to_string()))?;

        Ok(GenerationResponse {
            content: anthropic_resp
                .content
                .first()
                .map(|c| c.text.clone())
                .unwrap_or_default(),
            model: anthropic_resp.model,
            usage: TokenUsage {
                prompt_tokens: anthropic_resp.usage.input_tokens,
                completion_tokens: anthropic_resp.usage.output_tokens,
                total_tokens: anthropic_resp.usage.input_tokens
                    + anthropic_resp.usage.output_tokens,
            },
            finish_reason: anthropic_resp.stop_reason,
        })
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn provider_name(&self) -> &str {
        "anthropic"
    }
}

// ===== Mistral Provider =====

pub struct MistralProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl MistralProvider {
    pub fn new(api_key: String, model: String, base_url: Option<String>) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(90))
                .build()
                .unwrap_or_default(),
            api_key,
            model,
            base_url: base_url.unwrap_or_else(|| "https://api.mistral.ai".to_string()),
        })
    }
}

#[async_trait]
impl LLMProvider for MistralProvider {
    async fn generate_raw(
        &self,
        messages: Vec<Message>,
        config: &GenerationConfig,
    ) -> Result<GenerationResponse> {
        #[derive(Serialize)]
        struct MistralRequest {
            model: String,
            messages: Vec<MistralMessage>,
            temperature: f32,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_tokens: Option<u32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            top_p: Option<f32>,
        }

        #[derive(Serialize)]
        struct MistralMessage {
            role: String,
            content: String,
        }

        #[derive(Deserialize)]
        struct MistralResponse {
            choices: Vec<Choice>,
            model: String,
            usage: MistralUsage,
        }

        #[derive(Deserialize)]
        struct Choice {
            message: ResponseMessage,
            finish_reason: Option<String>,
        }

        #[derive(Deserialize)]
        struct ResponseMessage {
            content: String,
        }

        #[derive(Deserialize)]
        struct MistralUsage {
            prompt_tokens: u32,
            completion_tokens: u32,
            total_tokens: u32,
        }

        let mistral_messages: Vec<MistralMessage> = messages
            .into_iter()
            .map(|m| MistralMessage {
                role: match m.role {
                    MessageRole::User => "user".to_string(),
                    MessageRole::Assistant => "assistant".to_string(),
                    MessageRole::System => "system".to_string(),
                },
                content: m.content,
            })
            .collect();

        let request = MistralRequest {
            model: self.model.clone(),
            messages: mistral_messages,
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            top_p: config.top_p,
        };

        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| crate::error::MemoryError::ExternalError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(crate::error::MemoryError::ExternalError(format!(
                "Mistral API error {}: {}",
                status, error_text
            )));
        }

        let mistral_resp: MistralResponse = response
            .json()
            .await
            .map_err(|e| crate::error::MemoryError::ExternalError(e.to_string()))?;

        let choice = mistral_resp.choices.first().ok_or_else(|| {
            crate::error::MemoryError::ExternalError("No choices in Mistral response".to_string())
        })?;

        Ok(GenerationResponse {
            content: choice.message.content.clone(),
            model: mistral_resp.model,
            usage: TokenUsage {
                prompt_tokens: mistral_resp.usage.prompt_tokens,
                completion_tokens: mistral_resp.usage.completion_tokens,
                total_tokens: mistral_resp.usage.total_tokens,
            },
            finish_reason: choice.finish_reason.clone(),
        })
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn provider_name(&self) -> &str {
        "mistral"
    }
}

// ===== Qwen Provider =====

pub struct QwenProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl QwenProvider {
    pub fn new(api_key: String, model: String, base_url: Option<String>) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(90))
                .build()
                .unwrap_or_default(),
            api_key,
            model,
            base_url: base_url
                .unwrap_or_else(|| "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string()),
        })
    }
}

#[async_trait]
impl LLMProvider for QwenProvider {
    async fn generate_raw(
        &self,
        messages: Vec<Message>,
        config: &GenerationConfig,
    ) -> Result<GenerationResponse> {
        // Qwen uses OpenAI-compatible API format
        #[derive(Serialize)]
        struct QwenRequest {
            model: String,
            messages: Vec<QwenMessage>,
            temperature: f32,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_tokens: Option<u32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            top_p: Option<f32>,
        }

        #[derive(Serialize)]
        struct QwenMessage {
            role: String,
            content: String,
        }

        #[derive(Deserialize)]
        struct QwenResponse {
            choices: Vec<Choice>,
            model: String,
            usage: QwenUsage,
        }

        #[derive(Deserialize)]
        struct Choice {
            message: ResponseMessage,
            finish_reason: Option<String>,
        }

        #[derive(Deserialize)]
        struct ResponseMessage {
            content: String,
        }

        #[derive(Deserialize)]
        struct QwenUsage {
            prompt_tokens: u32,
            completion_tokens: u32,
            total_tokens: u32,
        }

        let qwen_messages: Vec<QwenMessage> = messages
            .into_iter()
            .map(|m| QwenMessage {
                role: match m.role {
                    MessageRole::User => "user".to_string(),
                    MessageRole::Assistant => "assistant".to_string(),
                    MessageRole::System => "system".to_string(),
                },
                content: m.content,
            })
            .collect();

        let request = QwenRequest {
            model: self.model.clone(),
            messages: qwen_messages,
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            top_p: config.top_p,
        };

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| crate::error::MemoryError::ExternalError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(crate::error::MemoryError::ExternalError(format!(
                "Qwen API error {}: {}",
                status, error_text
            )));
        }

        let qwen_resp: QwenResponse = response
            .json()
            .await
            .map_err(|e| crate::error::MemoryError::ExternalError(e.to_string()))?;

        let choice = qwen_resp.choices.first().ok_or_else(|| {
            crate::error::MemoryError::ExternalError("No choices in Qwen response".to_string())
        })?;

        Ok(GenerationResponse {
            content: choice.message.content.clone(),
            model: qwen_resp.model,
            usage: TokenUsage {
                prompt_tokens: qwen_resp.usage.prompt_tokens,
                completion_tokens: qwen_resp.usage.completion_tokens,
                total_tokens: qwen_resp.usage.total_tokens,
            },
            finish_reason: choice.finish_reason.clone(),
        })
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn supports_tools(&self) -> bool {
        false // Qwen tool calling may vary by model
    }

    fn provider_name(&self) -> &str {
        "qwen"
    }
}

// ===== OpenRouter Provider =====

pub struct OpenRouterProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenRouterProvider {
    pub fn new(api_key: String, model: String, base_url: Option<String>) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(90))
                .build()
                .unwrap_or_default(),
            api_key,
            model,
            base_url: base_url.unwrap_or_else(|| "https://openrouter.ai/api/v1".to_string()),
        })
    }
}

#[async_trait]
impl LLMProvider for OpenRouterProvider {
    async fn generate_raw(
        &self,
        messages: Vec<Message>,
        config: &GenerationConfig,
    ) -> Result<GenerationResponse> {
        // OpenRouter uses OpenAI-compatible API format
        #[derive(Serialize)]
        struct OpenRouterRequest {
            model: String,
            messages: Vec<OpenRouterMessage>,
            temperature: f32,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_tokens: Option<u32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            top_p: Option<f32>,
        }

        #[derive(Serialize)]
        struct OpenRouterMessage {
            role: String,
            content: String,
        }

        #[derive(Deserialize)]
        struct OpenRouterResponse {
            choices: Vec<Choice>,
            model: String,
            usage: OpenRouterUsage,
        }

        #[derive(Deserialize)]
        struct Choice {
            message: ResponseMessage,
            finish_reason: Option<String>,
        }

        #[derive(Deserialize)]
        struct ResponseMessage {
            content: String,
        }

        #[derive(Deserialize)]
        struct OpenRouterUsage {
            prompt_tokens: u32,
            completion_tokens: u32,
            total_tokens: u32,
        }

        let openrouter_messages: Vec<OpenRouterMessage> = messages
            .into_iter()
            .map(|m| OpenRouterMessage {
                role: match m.role {
                    MessageRole::User => "user".to_string(),
                    MessageRole::Assistant => "assistant".to_string(),
                    MessageRole::System => "system".to_string(),
                },
                content: m.content,
            })
            .collect();

        let request = OpenRouterRequest {
            model: self.model.clone(),
            messages: openrouter_messages,
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            top_p: config.top_p,
        };

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| crate::error::MemoryError::ExternalError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(crate::error::MemoryError::ExternalError(format!(
                "OpenRouter API error {}: {}",
                status, error_text
            )));
        }

        let openrouter_resp: OpenRouterResponse = response
            .json()
            .await
            .map_err(|e| crate::error::MemoryError::ExternalError(e.to_string()))?;

        let choice = openrouter_resp.choices.first().ok_or_else(|| {
            crate::error::MemoryError::ExternalError(
                "No choices in OpenRouter response".to_string(),
            )
        })?;

        Ok(GenerationResponse {
            content: choice.message.content.clone(),
            model: openrouter_resp.model,
            usage: TokenUsage {
                prompt_tokens: openrouter_resp.usage.prompt_tokens,
                completion_tokens: openrouter_resp.usage.completion_tokens,
                total_tokens: openrouter_resp.usage.total_tokens,
            },
            finish_reason: choice.finish_reason.clone(),
        })
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn supports_tools(&self) -> bool {
        true // OpenRouter supports tools for compatible models
    }

    fn provider_name(&self) -> &str {
        "openrouter"
    }
}

#[cfg(test)]
mod structured_output_tests {
    use super::parse_lenient;

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Entity {
        name: String,
        #[serde(rename = "type")]
        entity_type: String,
    }

    /// Bare JSON must still parse, and must not be routed through the
    /// salvage path. This is the regression guard on the fast path.
    #[test]
    fn bare_json_parses() {
        let v: Vec<Entity> = parse_lenient(r#"[{"name":"Asilidae","type":"Concept"}]"#).unwrap();
        assert_eq!(v[0].name, "Asilidae");
    }

    /// The exact shape that broke the learning loop. Transcribed from a real
    /// `episodes.error_details` row on the `ontologist` for `prey_locator`:
    ///
    /// ```text
    /// Failed to parse structured output: expected value at line 1 column 1.
    /// Response was: ```json [     {"name": "Asilidae", "type": "Concept", ...
    /// ```
    #[test]
    fn fenced_json_from_the_real_failure_parses() {
        let raw = "```json\n[\n    {\"name\": \"Asilidae\", \"type\": \"Concept\"},\n    \
                   {\"name\": \"Prionyx popovi\", \"type\": \"Concept\"}\n]\n```";
        let v: Vec<Entity> = parse_lenient(raw).expect("fenced JSON must parse");
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].name, "Prionyx popovi");
    }

    /// A fence with no language tag is just as common.
    #[test]
    fn untagged_fence_parses() {
        let v: Vec<Entity> =
            parse_lenient("```\n[{\"name\":\"London\",\"type\":\"Location\"}]\n```").unwrap();
        assert_eq!(v[0].entity_type, "Location");
    }

    #[test]
    fn single_line_fence_parses() {
        let v: Vec<Entity> =
            parse_lenient("```[{\"name\":\"X1\",\"type\":\"Concept\"}]```").unwrap();
        assert_eq!(v[0].name, "X1");
    }

    /// Prose on either side, despite "Return ONLY a JSON array".
    #[test]
    fn prose_wrapped_json_parses() {
        let raw = "Here are the entities I found:\n\
                   [{\"name\":\"Arsenal\",\"type\":\"Organization\"}]\n\
                   Let me know if you need more.";
        let v: Vec<Entity> = parse_lenient(raw).unwrap();
        assert_eq!(v[0].name, "Arsenal");
    }

    /// An empty array is a legitimate answer ("no named entities") and must
    /// not be confused with a parse failure.
    #[test]
    fn empty_array_is_not_a_failure() {
        let v: Vec<Entity> = parse_lenient("```json\n[]\n```").unwrap();
        assert!(v.is_empty());
    }

    /// Salvage must not rescue genuinely malformed output into something
    /// wrong. A truncated response (model hit `max_tokens`) has to fail.
    #[test]
    fn truncated_json_still_fails() {
        let raw = "```json\n[{\"name\": \"Asilidae\", \"type\": \"Conc";
        assert!(parse_lenient::<Vec<Entity>>(raw).is_err());
    }

    /// The reported error must describe the text the model actually sent,
    /// not the last salvage attempt — that message is the only thing an
    /// operator sees in `episodes.error_details`.
    ///
    /// A fenced-but-invalid payload distinguishes the two: the original
    /// error points at the backtick (line 1, column 1), whereas the salvage
    /// error would point somewhere inside the unfenced object.
    #[test]
    fn error_describes_the_original_response() {
        let err = parse_lenient::<Vec<Entity>>("```json\n{\"oops\"\n```").unwrap_err();
        assert_eq!(
            (err.line(), err.column()),
            (1, 1),
            "expected the error about the raw fenced response, got: {err}"
        );
    }

    #[test]
    fn plain_garbage_still_fails() {
        assert!(parse_lenient::<Vec<Entity>>("not json at all").is_err());
    }

    /// Scalars and strings go through the strict path untouched, so widening
    /// the parser cannot change behaviour for non-collection `T`.
    #[test]
    fn scalars_are_unaffected() {
        assert_eq!(parse_lenient::<String>(r#""hello""#).unwrap(), "hello");
        assert_eq!(parse_lenient::<i64>("42").unwrap(), 42);
    }
}
