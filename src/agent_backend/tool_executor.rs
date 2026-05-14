/// ToolAwareExecutor — wraps an inner executor with an agentic tool-calling loop
///
/// When tools are available, sends them to the LLM and loops on tool_use responses.
/// When no tools are available, delegates directly to the inner executor (single turn).
///
/// Safety:
///   - Max 5 iterations (hard cap)
///   - execute_agent calls the base executor (no tools) to prevent recursion
///   - Tokens accumulated across all iterations for billing
use crate::agent_backend::executor::{
    AgentExecutor, AgentMetadata, AgentOutput, AgentStatus, ExecutionContext, ExecutionError,
    ToolInvocation,
};
use crate::agent_backend::llm_executor::{
    extract_text_from_content, ClaudeRequest, ClaudeResponse, ClaudeThinking, ContentBlock,
    Message, MessageBlock, MessageContent,
};
use crate::agent_backend::multi_model_executor::{OpenAIMessage, OpenAIRequest, OpenAIResponse};
use crate::agent_backend::tools::{ToolContext, ToolRegistry};
use crate::ast::{AgentStmt, EvidenceStmt};
use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use std::time::Instant;

const MAX_ITERATIONS: u32 = 5;

/// Executor that wraps an inner executor with tool-calling capability
pub struct ToolAwareExecutor {
    inner: Arc<dyn AgentExecutor>,
    tool_registry: ToolRegistry,
    tool_context: Arc<ToolContext>,
    client: reqwest::Client,
}

impl ToolAwareExecutor {
    pub fn new(
        inner: Arc<dyn AgentExecutor>,
        tool_registry: ToolRegistry,
        tool_context: Arc<ToolContext>,
    ) -> Self {
        Self {
            inner,
            tool_registry,
            tool_context,
            client: reqwest::Client::builder().timeout(std::time::Duration::from_secs(90)).build().unwrap_or_default(),
        }
    }

    /// Run the Anthropic tool-use loop
    async fn execute_anthropic_loop(
        &self,
        agent: &AgentStmt,
        context: &ExecutionContext,
    ) -> Result<AgentOutput, ExecutionError> {
        let start = Instant::now();

        let system_prompt = context
            .agent_card
            .system_prompt
            .clone()
            .unwrap_or_else(|| "You are a forecasting research agent.".to_string());

        let tools = self
            .tool_registry
            .to_claude_tools_with_card(&context.agent_card);

        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
            ExecutionError::ExecutionFailed("ANTHROPIC_API_KEY not set".to_string())
        })?;

        let sp = context.agent_card.capabilities.resolve_sampling_params(4096);
        let thinking_block = if sp.extended_thinking {
            sp.thinking_budget_tokens.map(|budget| ClaudeThinking {
                thinking_type: "enabled".to_string(),
                budget_tokens: budget,
            })
        } else {
            None
        };

        let mut messages: Vec<Message> = vec![Message {
            role: "user".to_string(),
            content: MessageContent::Text(agent.query.clone()),
        }];

        let mut total_input_tokens: u32 = 0;
        let mut total_output_tokens: u32 = 0;
        let mut tool_invocations: Vec<ToolInvocation> = Vec::new();
        let mut iteration: u32 = 0;
        let mut final_response: Option<ClaudeResponse> = None;

        loop {
            iteration += 1;

            let request = ClaudeRequest {
                model: context.agent_card.capabilities.model.clone(),
                max_tokens: sp.max_tokens,
                temperature: sp.temperature,
                top_p: sp.top_p,
                top_k: sp.top_k,
                thinking: thinking_block.clone(),
                system: Some(system_prompt.clone()),
                messages: messages.clone(),
                tools: Some(tools.clone()),
                tool_choice: None,
            };

            // Send request
            let response = self.send_anthropic_request(&request, &api_key).await?;

            total_input_tokens += response.usage.input_tokens;
            total_output_tokens += response.usage.output_tokens;

            let stop_reason = response.stop_reason.clone().unwrap_or_default();

            if stop_reason != "tool_use" || iteration >= MAX_ITERATIONS {
                final_response = Some(response);
                break;
            }

            // Extract tool_use blocks from response
            let tool_uses: Vec<(String, String, serde_json::Value)> = response
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::ToolUse { id, name, input } => {
                        Some((id.clone(), name.clone(), input.clone()))
                    }
                    _ => None,
                })
                .collect();

            if tool_uses.is_empty() {
                final_response = Some(response);
                break;
            }

            // Build assistant message with the full response content
            let assistant_blocks: Vec<MessageBlock> = response
                .content
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => MessageBlock::Text { text: text.clone() },
                    ContentBlock::ToolUse { id, name, input } => MessageBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    },
                })
                .collect();

            messages.push(Message {
                role: "assistant".to_string(),
                content: MessageContent::Blocks(assistant_blocks),
            });

            // Execute each tool and build tool_result blocks
            let mut result_blocks: Vec<MessageBlock> = Vec::new();

            for (tool_use_id, tool_name, tool_input) in &tool_uses {
                let tool_start = Instant::now();
                let result = self
                    .tool_registry
                    .execute(tool_name, tool_input, &self.tool_context)
                    .await;
                let tool_duration = tool_start.elapsed().as_millis() as u64;

                let output = match &result {
                    Ok(s) => s.clone(),
                    Err(e) => format!("Error: {}", e),
                };

                tool_invocations.push(ToolInvocation {
                    tool_name: tool_name.clone(),
                    input: tool_input.clone(),
                    output: output.clone(),
                    duration_ms: tool_duration,
                    iteration,
                });

                // Cap tool result to prevent context overflow
                const MAX_TOOL_RESULT_CHARS: usize = 32_000;
                let truncated_output = if output.len() > MAX_TOOL_RESULT_CHARS {
                    format!(
                        "{}... [truncated, {} chars total]",
                        &output[..MAX_TOOL_RESULT_CHARS],
                        output.len()
                    )
                } else {
                    output
                };

                result_blocks.push(MessageBlock::ToolResult {
                    tool_use_id: tool_use_id.clone(),
                    content: truncated_output,
                });
            }

            messages.push(Message {
                role: "user".to_string(),
                content: MessageContent::Blocks(result_blocks),
            });
        }

        let elapsed = start.elapsed();

        // Parse final response into evidence
        let response = final_response.ok_or_else(|| {
            ExecutionError::ExecutionFailed("No final response from LLM".to_string())
        })?;

        let text = extract_text_from_content(&response.content);
        let (evidence, confidence, reasoning) = parse_evidence_text(&text, &agent.name);

        Ok(AgentOutput {
            agent_name: agent.name.clone(),
            agent_type: agent.agent_type.clone().unwrap_or_default(),
            timestamp: Utc::now(),
            status: AgentStatus::Success,
            evidence,
            confidence,
            sources_consulted: vec!["claude-api".to_string()],
            execution_time_ms: elapsed.as_millis() as u64,
            tokens_used: Some(total_input_tokens + total_output_tokens),
            metadata: AgentMetadata {
                model_used: Some(context.agent_card.capabilities.model.clone()),
                temperature: sp.temperature,
                reasoning,
            },
            tool_invocations,
            loop_iterations: iteration,
        })
    }

    /// Run the OpenAI-compatible tool-use loop (Mistral, OpenRouter, Qwen)
    async fn execute_openai_loop(
        &self,
        agent: &AgentStmt,
        context: &ExecutionContext,
    ) -> Result<AgentOutput, ExecutionError> {
        let start = Instant::now();
        let provider = &context.agent_card.capabilities.provider;

        let (api_key, base_url) = resolve_openai_provider(provider)?;

        let system_prompt = context
            .agent_card
            .system_prompt
            .clone()
            .unwrap_or_else(|| "You are a research agent.".to_string());

        let tools = self.tool_registry.to_openai_tools();

        let sp_oai = context.agent_card.capabilities.resolve_sampling_params(2048);

        let mut messages: Vec<OpenAIMessage> = vec![
            OpenAIMessage::chat("system", &system_prompt),
            OpenAIMessage::chat("user", &agent.query),
        ];

        let mut total_tokens: u32 = 0;
        let mut tool_invocations: Vec<ToolInvocation> = Vec::new();
        let mut iteration: u32 = 0;
        let mut final_text: Option<String> = None;

        loop {
            iteration += 1;

            let request = OpenAIRequest {
                model: context.agent_card.capabilities.model.clone(),
                messages: messages.clone(),
                temperature: sp_oai.temperature,
                max_tokens: Some(sp_oai.max_tokens),
                top_p: sp_oai.top_p,
                frequency_penalty: sp_oai.frequency_penalty,
                presence_penalty: sp_oai.presence_penalty,
                repetition_penalty: sp_oai.repetition_penalty,
                seed: sp_oai.random_seed,
                tools: Some(tools.clone()),
                tool_choice: None,
            };

            let response = self
                .send_openai_request(&request, &api_key, &base_url)
                .await?;

            if let Some(ref usage) = response.usage {
                total_tokens += usage.total_tokens;
            }

            let choice = response.choices.first().ok_or_else(|| {
                ExecutionError::ExecutionFailed("No choices in response".to_string())
            })?;

            let finish_reason = choice.finish_reason.as_deref().unwrap_or("");

            if finish_reason != "tool_calls" || iteration >= MAX_ITERATIONS {
                final_text = choice.message.content.clone();
                break;
            }

            // Check for tool calls
            let tool_calls = match &choice.message.tool_calls {
                Some(calls) if !calls.is_empty() => calls.clone(),
                _ => {
                    final_text = choice.message.content.clone();
                    break;
                }
            };

            // Add assistant message with tool_calls
            messages.push(OpenAIMessage {
                role: "assistant".to_string(),
                content: choice.message.content.clone(),
                tool_calls: Some(tool_calls.clone()),
                tool_call_id: None,
            });

            // Execute each tool and add result messages
            for call in &tool_calls {
                let tool_input: serde_json::Value =
                    serde_json::from_str(&call.function.arguments).unwrap_or_default();

                let tool_start = Instant::now();
                let result = self
                    .tool_registry
                    .execute(&call.function.name, &tool_input, &self.tool_context)
                    .await;
                let tool_duration = tool_start.elapsed().as_millis() as u64;

                let output = match &result {
                    Ok(s) => s.clone(),
                    Err(e) => format!("Error: {}", e),
                };

                tool_invocations.push(ToolInvocation {
                    tool_name: call.function.name.clone(),
                    input: tool_input,
                    output: output.clone(),
                    duration_ms: tool_duration,
                    iteration,
                });

                // Cap tool result to prevent context overflow
                const MAX_TOOL_RESULT_CHARS_OAI: usize = 32_000;
                let truncated = if output.len() > MAX_TOOL_RESULT_CHARS_OAI {
                    format!(
                        "{}... [truncated, {} chars total]",
                        &output[..MAX_TOOL_RESULT_CHARS_OAI],
                        output.len()
                    )
                } else {
                    output
                };
                messages.push(OpenAIMessage::tool_result(&call.id, &truncated));
            }
        }

        let elapsed = start.elapsed();
        let text = final_text.unwrap_or_default();
        let (evidence, confidence, reasoning) = parse_evidence_text(&text, &agent.name);

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
            tokens_used: Some(total_tokens),
            metadata: AgentMetadata {
                model_used: Some(context.agent_card.capabilities.model.clone()),
                temperature: sp_oai.temperature,
                reasoning,
            },
            tool_invocations,
            loop_iterations: iteration,
        })
    }

    /// Raw Anthropic API call
    async fn send_anthropic_request(
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

    /// Raw OpenAI-compatible API call
    async fn send_openai_request(
        &self,
        request: &OpenAIRequest,
        api_key: &str,
        base_url: &str,
    ) -> Result<OpenAIResponse, ExecutionError> {
        let url = format!("{}/chat/completions", base_url);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
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
impl AgentExecutor for ToolAwareExecutor {
    async fn execute(
        &self,
        agent: &AgentStmt,
        context: &ExecutionContext,
    ) -> Result<AgentOutput, ExecutionError> {
        let provider = &context.agent_card.capabilities.provider;

        // Check if tools are available — if not, delegate to inner (single turn)
        let has_tools = !self.tool_registry.to_claude_tools().is_empty();
        if !has_tools {
            return self.inner.execute(agent, context).await;
        }

        // Agents with custom system prompts that demand specific output formats
        // (e.g., fermi's JSON decomposition) must NOT go through the tool loop.
        // The tool loop sends all platform tools (search_knowledge, etc.) which
        // confuses the LLM into calling tools and returning narrative instead of
        // following the system prompt's JSON schema. Delegate to inner executor.
        // Meta-agents (tagged "meta-agent") always bypass — they return structured
        // JSON decompositions that the tool loop would corrupt.
        let is_meta_agent = context
            .agent_card
            .metadata
            .tags
            .iter()
            .any(|t| t == "meta-agent");
        let prompt_demands_format = is_meta_agent
            || context
                .agent_card
                .system_prompt
                .as_ref()
                .map(|p| p.contains("ONLY") || p.contains("raw JSON"))
                .unwrap_or(false);
        if prompt_demands_format {
            return self.inner.execute(agent, context).await;
        }

        match provider.as_str() {
            "anthropic" | "" => self.execute_anthropic_loop(agent, context).await,
            _ => self.execute_openai_loop(agent, context).await,
        }
    }

    fn name(&self) -> &str {
        "tool-aware"
    }
}

// ─── Helpers ───────────────────────────────────────────────────────

fn resolve_openai_provider(provider: &str) -> Result<(String, String), ExecutionError> {
    let env_key = format!("{}_API_KEY", provider.to_uppercase());
    let api_key = std::env::var(&env_key)
        .map_err(|_| ExecutionError::ExecutionFailed(format!("{} not set", env_key)))?;

    let base_url = match provider {
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
        other => {
            return Err(ExecutionError::ExecutionFailed(format!(
                "Unknown provider: {}",
                other
            )))
        }
    };

    Ok((api_key, base_url))
}

/// Parse evidence from text (handles both JSON and plain text responses)
fn parse_evidence_text(text: &str, agent_name: &str) -> (Vec<EvidenceStmt>, f64, Option<String>) {
    // Try to extract JSON from the response
    let json_text = if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            &text[start..=end]
        } else {
            text
        }
    } else {
        text
    };

    #[derive(serde::Deserialize)]
    struct EvidenceJson {
        #[serde(default)]
        key_findings: Vec<String>,
        #[serde(default)]
        summary: String,
        #[serde(default)]
        confidence: f64,
        #[serde(default)]
        reasoning: String,
    }

    match serde_json::from_str::<EvidenceJson>(json_text) {
        Ok(data) => {
            let confidence = if data.confidence > 0.0 {
                data.confidence
            } else {
                0.5
            };
            let evidence = vec![EvidenceStmt {
                id: format!("{}_evidence_{}", agent_name, Utc::now().timestamp()),
                source: format!("Agent: {}", agent_name),
                summary: Some(data.summary),
                url: None,
                relevance: Some(confidence),
                date: Some(Utc::now().format("%Y-%m-%d").to_string()),
                strength: Some(confidence),
                key_findings: data.key_findings,
            }];
            (evidence, confidence, Some(data.reasoning))
        }
        Err(_) => {
            // Fallback: treat as plain text evidence.
            // IMPORTANT: preserve the FULL text as summary so downstream
            // consumers (wiki, evidence panel) can display it completely.
            // Extract key lines as findings for the evidence card display.
            let summary = text.to_string();

            // Extract meaningful lines as key findings:
            // - Bullet points (•, -, *, ▸)
            // - Numbered items (1., 2.)
            // - Lines with data signals (%, $, numbers)
            // - Lines longer than 20 chars (skip headers/blanks)
            let findings: Vec<String> = text
                .lines()
                .filter(|l| {
                    let trimmed = l.trim();
                    if trimmed.is_empty() || trimmed.len() < 15 {
                        return false;
                    }
                    // Skip markdown headers and separators
                    if trimmed.starts_with('#')
                        || trimmed.starts_with("---")
                        || trimmed.starts_with("===")
                    {
                        return false;
                    }
                    // Prefer bullet points, numbered items, and data-rich lines
                    trimmed.starts_with('-')
                        || trimmed.starts_with('•')
                        || trimmed.starts_with('*')
                        || trimmed.starts_with("▸")
                        || trimmed
                            .chars()
                            .next()
                            .map(|c| c.is_ascii_digit())
                            .unwrap_or(false)
                        || trimmed.contains('%')
                        || trimmed.contains('$')
                        || trimmed.contains("p50")
                        || trimmed.contains("Suggested")
                        || trimmed.contains("confidence")
                        || trimmed.contains("relevance")
                })
                .take(15)
                .map(|l| {
                    let trimmed = l.trim();
                    // Clean leading bullet chars for consistency
                    let cleaned = trimmed
                        .trim_start_matches('-')
                        .trim_start_matches('•')
                        .trim_start_matches('*')
                        .trim_start_matches("▸")
                        .trim();
                    cleaned.to_string()
                })
                .filter(|s| !s.is_empty())
                .collect();

            let evidence = vec![EvidenceStmt {
                id: format!("{}_evidence_{}", agent_name, Utc::now().timestamp()),
                source: format!("Agent: {}", agent_name),
                summary: Some(summary),
                url: None,
                relevance: Some(0.5),
                date: Some(Utc::now().format("%Y-%m-%d").to_string()),
                strength: Some(0.5),
                key_findings: findings,
            }];
            (evidence, 0.5, Some(text.to_string()))
        }
    }
}
