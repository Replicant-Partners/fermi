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
    // Call generate_raw and parse JSON
    let response = provider.generate_raw(messages, config).await?;

    serde_json::from_str::<T>(&response.content).map_err(|e| {
        crate::error::MemoryError::ExternalError(format!(
            "Failed to parse structured output: {}. Response was: {}",
            e, response.content
        ))
    })
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
                Some(config.base_url.clone().unwrap_or_else(|| {
                    "https://api.deepseek.com/v1".to_string()
                })),
            )?)),
            ProviderType::Kimi => Ok(Arc::new(OpenRouterProvider::new(
                config.api_key.clone(),
                config.model.clone(),
                Some(config.base_url.clone().unwrap_or_else(|| {
                    "https://api.moonshot.cn/v1".to_string()
                })),
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
            client: reqwest::Client::new(),
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
            client: reqwest::Client::new(),
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
            client: reqwest::Client::new(),
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
            client: reqwest::Client::new(),
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
