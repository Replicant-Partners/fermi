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
    extract_summary_from_json_contract, extract_text_from_content, is_json_contract_text,
    ClaudeRequest, ClaudeResponse, ClaudeThinking, ContentBlock, Message, MessageBlock,
    MessageContent,
};
use crate::agent_backend::multi_model_executor::{OpenAIMessage, OpenAIRequest, OpenAIResponse};
use crate::agent_backend::tools::{ToolContext, ToolRegistry};
use crate::ast::{AgentStmt, EvidenceStmt};
use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use std::time::Instant;

const MAX_ITERATIONS: u32 = 5;

/// Detect whether a system prompt declares a structured-output contract
/// (typically "return raw JSON, no prose"). Agents that demand structured
/// output must bypass the tool loop — the platform's injected tools
/// (search_knowledge, web_search, …) encourage the LLM to keep tool-using
/// past MAX_ITERATIONS and return tool_use blocks with no final assistant
/// text. The result is empty content despite tens of thousands of tokens
/// consumed (issue #3 / docs/specs/10_RESEARCH_AGENTS_EMPTY_LLM_OUTPUT.md).
///
/// Conservative on purpose: matches verbatim phrases used in real curated
/// agent cards. Adding a new JSON-contract agent requires either reusing
/// one of these phrases or wiring the agent through `LLMExecutor` directly.
pub(crate) fn prompt_demands_structured_output(prompt: &str) -> bool {
    prompt.contains("ONLY")
        || prompt.contains("raw JSON")
        || prompt.contains("Return a valid JSON")
        || prompt.contains("return a valid JSON")
        || prompt.contains("no prose outside")
        || prompt.contains("JSON object — no prose")
        // Rabble creature agents (enemy_sensor, genome_profiler, prey_locator)
        // use "output valid JSON only" (lowercase "only") or "Return JSON:"
        || prompt.contains("output valid JSON only")
        || prompt.contains("Return JSON:")
}

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
        // Set when the loop exits because it hit MAX_ITERATIONS while still in
        // a tool_use state — the response is incomplete and we'll do a flush
        // turn below to coax the LLM into producing its final answer.
        let mut hit_iteration_cap_in_tool_use = false;

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

            // Honest cap-out: if we hit MAX_ITERATIONS while the LLM was still
            // tool-using, the response has no final assistant text — issue #3.
            // Mark it so the flush turn below runs.
            if stop_reason == "tool_use" && iteration >= MAX_ITERATIONS {
                hit_iteration_cap_in_tool_use = true;
                final_response = Some(response);
                break;
            }

            if stop_reason != "tool_use" {
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

        let mut response = final_response.ok_or_else(|| {
            ExecutionError::ExecutionFailed("No final response from LLM".to_string())
        })?;
        let mut stop_reason = response.stop_reason.clone();

        // ── Flush turn ──
        //
        // If we hit the iteration cap while the LLM was still tool-using
        // (or it stopped with no usable text for some other reason), do one
        // final no-tools call telling the LLM the loop is over and to produce
        // its final answer now. This converts what used to be empty content
        // into a structured response — see issue #3 / Doc 10.
        let initial_text = extract_text_from_content(&response.content);
        let need_flush = hit_iteration_cap_in_tool_use || initial_text.trim().is_empty();
        if need_flush {
            // Append the partial assistant response and a user nudge.
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
            // Synthesize empty tool_result blocks so the conversation is
            // well-formed: every tool_use the assistant emitted needs a
            // corresponding tool_result before we can send another user turn.
            let stub_tool_results: Vec<MessageBlock> = response
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::ToolUse { id, .. } => Some(MessageBlock::ToolResult {
                        tool_use_id: id.clone(),
                        content: "(tool not executed: iteration limit reached)".to_string(),
                    }),
                    _ => None,
                })
                .collect();
            if !assistant_blocks.is_empty() {
                messages.push(Message {
                    role: "assistant".to_string(),
                    content: MessageContent::Blocks(assistant_blocks),
                });
            }
            if !stub_tool_results.is_empty() {
                messages.push(Message {
                    role: "user".to_string(),
                    content: MessageContent::Blocks(stub_tool_results),
                });
            }
            messages.push(Message {
                role: "user".to_string(),
                content: MessageContent::Text(
                    "You have reached the tool-use iteration limit. \
                     Produce your final answer now using the output format \
                     specified in your system prompt. Do not request more tools."
                        .to_string(),
                ),
            });

            let flush_request = ClaudeRequest {
                model: context.agent_card.capabilities.model.clone(),
                max_tokens: sp.max_tokens,
                temperature: sp.temperature,
                top_p: sp.top_p,
                top_k: sp.top_k,
                thinking: thinking_block.clone(),
                system: Some(system_prompt.clone()),
                messages: messages.clone(),
                // Explicitly no tools — force a text response.
                tools: None,
                tool_choice: None,
            };

            if let Ok(flush_response) = self.send_anthropic_request(&flush_request, &api_key).await
            {
                total_input_tokens += flush_response.usage.input_tokens;
                total_output_tokens += flush_response.usage.output_tokens;
                stop_reason = flush_response.stop_reason.clone();
                response = flush_response;
            }
            // If the flush call itself failed, keep the partial response we had.
        }

        let elapsed = start.elapsed();
        let text = extract_text_from_content(&response.content);
        let (evidence, confidence, reasoning) = parse_evidence_text(&text, &agent.name);

        let trimmed_text_empty = text.trim().is_empty();
        let (status, failure_reason) = if trimmed_text_empty {
            (
                AgentStatus::Failed,
                Some(format!(
                    "tool loop produced empty content (stop_reason={}, iterations={}{})",
                    stop_reason.as_deref().unwrap_or("?"),
                    iteration,
                    if hit_iteration_cap_in_tool_use {
                        ", hit_iteration_cap"
                    } else {
                        ""
                    }
                )),
            )
        } else if stop_reason.as_deref() == Some("max_tokens") {
            (
                AgentStatus::Failed,
                Some("llm hit max_tokens; response is truncated".to_string()),
            )
        } else if hit_iteration_cap_in_tool_use {
            // Flush turn produced text, but we still want callers to know the
            // tool loop didn't run to completion.
            (
                AgentStatus::Success,
                Some(format!(
                    "tool loop hit iteration cap ({}); answer produced from flush turn",
                    MAX_ITERATIONS
                )),
            )
        } else {
            (AgentStatus::Success, None)
        };

        Ok(AgentOutput {
            agent_name: agent.name.clone(),
            agent_type: agent.agent_type.clone().unwrap_or_default(),
            timestamp: Utc::now(),
            status,
            evidence,
            confidence,
            sources_consulted: vec!["claude-api".to_string()],
            execution_time_ms: elapsed.as_millis() as u64,
            tokens_used: Some(total_input_tokens + total_output_tokens),
            metadata: AgentMetadata {
                model_used: Some(context.agent_card.capabilities.model.clone()),
                temperature: sp.temperature,
                reasoning,
                provider: Some("anthropic".to_string()),
                stop_reason,
                failure_reason,
                ..Default::default()
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
        let mut last_finish_reason: Option<String> = None;
        let mut hit_iteration_cap_in_tool_calls = false;

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
            last_finish_reason = Some(finish_reason.to_string());

            // Cap-out while still tool-calling — flag for flush turn (issue #3).
            if finish_reason == "tool_calls" && iteration >= MAX_ITERATIONS {
                hit_iteration_cap_in_tool_calls = true;
                final_text = choice.message.content.clone();
                break;
            }

            if finish_reason != "tool_calls" {
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

        let mut text = final_text.unwrap_or_default();

        // ── Flush turn (OpenAI-compatible) ──
        //
        // If we hit the iteration cap while still tool-calling, or the final
        // assistant message has no content, do one more call with no tools
        // telling the model to produce its final answer (issue #3 / Doc 10).
        if hit_iteration_cap_in_tool_calls || text.trim().is_empty() {
            messages.push(OpenAIMessage::chat(
                "user",
                "You have reached the tool-use iteration limit. \
                 Produce your final answer now using the output format \
                 specified in your system prompt. Do not request more tools.",
            ));

            let flush_request = OpenAIRequest {
                model: context.agent_card.capabilities.model.clone(),
                messages: messages.clone(),
                temperature: sp_oai.temperature,
                max_tokens: Some(sp_oai.max_tokens),
                top_p: sp_oai.top_p,
                frequency_penalty: sp_oai.frequency_penalty,
                presence_penalty: sp_oai.presence_penalty,
                repetition_penalty: sp_oai.repetition_penalty,
                seed: sp_oai.random_seed,
                // Explicitly no tools — force a text response.
                tools: None,
                tool_choice: None,
            };
            if let Ok(flush_response) = self
                .send_openai_request(&flush_request, &api_key, &base_url)
                .await
            {
                if let Some(ref usage) = flush_response.usage {
                    total_tokens += usage.total_tokens;
                }
                if let Some(choice) = flush_response.choices.first() {
                    last_finish_reason = choice
                        .finish_reason
                        .clone()
                        .or(last_finish_reason.clone());
                    if let Some(ref content) = choice.message.content {
                        if !content.trim().is_empty() {
                            text = content.clone();
                        }
                    }
                }
            }
        }

        let elapsed = start.elapsed();
        let (evidence, confidence, reasoning) = parse_evidence_text(&text, &agent.name);

        let (status, failure_reason) = if text.trim().is_empty() {
            (
                AgentStatus::Failed,
                Some(format!(
                    "tool loop produced empty content (finish_reason={}, iterations={}{})",
                    last_finish_reason.as_deref().unwrap_or("?"),
                    iteration,
                    if hit_iteration_cap_in_tool_calls {
                        ", hit_iteration_cap"
                    } else {
                        ""
                    }
                )),
            )
        } else if last_finish_reason.as_deref() == Some("length") {
            (
                AgentStatus::Failed,
                Some("llm hit length cap; response is truncated".to_string()),
            )
        } else if hit_iteration_cap_in_tool_calls {
            (
                AgentStatus::Success,
                Some(format!(
                    "tool loop hit iteration cap ({}); answer produced from flush turn",
                    MAX_ITERATIONS
                )),
            )
        } else {
            (AgentStatus::Success, None)
        };

        Ok(AgentOutput {
            agent_name: agent.name.clone(),
            agent_type: agent
                .agent_type
                .clone()
                .unwrap_or_else(|| "research".to_string()),
            timestamp: Utc::now(),
            status,
            evidence,
            confidence,
            sources_consulted: vec![],
            execution_time_ms: elapsed.as_millis() as u64,
            tokens_used: Some(total_tokens),
            metadata: AgentMetadata {
                model_used: Some(context.agent_card.capabilities.model.clone()),
                temperature: sp_oai.temperature,
                reasoning,
                provider: Some(provider.clone()),
                stop_reason: last_finish_reason,
                failure_reason,
                ..Default::default()
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
        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");
        // Skip Authorization header for providers that need no key (e.g. Ollama)
        if !api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }
        let response = req
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
        // (e.g., fermi's JSON decomposition, supply_chain_oracle's BoM contract)
        // must NOT go through the tool loop. The tool loop sends all platform
        // tools (search_knowledge, web_search, etc.) which encourages the LLM to
        // keep calling tools past MAX_ITERATIONS and return tool_use blocks with
        // no final assistant text. The result is empty content despite tens of
        // thousands of tokens consumed (issue #3 / Doc 10).
        //
        // Meta-agents (tagged "meta-agent") always bypass.
        //
        // Heuristic for JSON-contract agents — match common phrases used in
        // curated system prompts. Conservative on purpose: matches the verbatim
        // phrases used in real agent cards (supply_chain_oracle, comparator,
        // sidestream_miner, …) so that adding new ones can't accidentally
        // re-enable the tool loop unless the prompt is rewritten.
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
                .as_deref()
                .map(prompt_demands_structured_output)
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
    // Ollama needs no API key — just a base URL.
    if provider == "ollama" {
        let base_url = std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| {
            // Fall back to localhost default if OLLAMA_ENABLE is set
            "http://localhost:11434/v1".to_string()
        });
        return Ok((String::new(), base_url));
    }

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

/// Parse evidence from text (handles both JSON and plain text responses).
///
/// Decision tree:
/// 1. Valid JSON that looks like a domain-specific contract (enemy_sensor,
///    genome_profiler, prey_locator, supply_chain_oracle, …): preserve the
///    full text as `reasoning` (the channel the handler reads), salvage a
///    summary and key_findings for the evidence card.
/// 2. Valid JSON that looks like the EvidenceJson shape (has `key_findings`
///    AND/OR both `summary`+`confidence`/`reasoning`): parse into EvidenceStmt
///    with `reasoning = data.reasoning`.
/// 3. Plain text: put into summary + extract bullet findings.
///
/// The critical invariant: `reasoning` (the third return value) must contain
/// the full agent response text whenever the response is non-empty, so that
/// `dispatch_rabble_action` can return it to the creature-agent handlers for
/// JSON parsing.
fn parse_evidence_text(text: &str, agent_name: &str) -> (Vec<EvidenceStmt>, f64, Option<String>) {

    fn make_stub(agent_name: &str, summary: Option<String>, findings: Vec<String>, confidence: f64) -> Vec<EvidenceStmt> {
        vec![EvidenceStmt {
            id: format!("{}_evidence_{}", agent_name, Utc::now().timestamp()),
            source: format!("Agent: {}", agent_name),
            summary,
            url: None,
            relevance: Some(confidence),
            date: Some(Utc::now().format("%Y-%m-%d").to_string()),
            strength: Some(confidence),
            key_findings: findings,
        }]
    }

    // ── 1. JSON contract detection ────────────────────────────────────────
    // is_json_contract_text validates the whole text is parseable JSON.
    // Creature agents always fall here because their responses are pure JSON.
    if is_json_contract_text(text) {
        let (summary, findings) = extract_summary_from_json_contract(text);
        return (make_stub(agent_name, summary, findings, 0.5), 0.5, Some(text.to_string()));
    }

    // ── 2. Embedded JSON — try EvidenceJson shape ─────────────────────────
    // Only match if the text actually contains `key_findings` OR contains
    // both `summary` and at least one of `confidence`/`reasoning`.
    // This prevents greedy matching of domain JSON like enemy_sensor's
    // {"threat_level","threats","summary"} which has a `summary` key but
    // is NOT an EvidenceJson — it would deserialise with empty defaults
    // and set reasoning="" causing the handler to receive an empty response.
    let json_text = if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') { &text[start..=end] } else { text }
    } else { text };

    let looks_like_evidence_json = json_text.contains("\"key_findings\"")
        || (json_text.contains("\"summary\"")
            && (json_text.contains("\"confidence\"") || json_text.contains("\"reasoning\"")));

    if looks_like_evidence_json {
        #[derive(serde::Deserialize)]
        struct EvidenceJson {
            #[serde(default)] key_findings: Vec<String>,
            #[serde(default)] summary: String,
            #[serde(default)] confidence: f64,
            #[serde(default)] reasoning: String,
        }
        if let Ok(data) = serde_json::from_str::<EvidenceJson>(json_text) {
            let confidence = if data.confidence > 0.0 { data.confidence } else { 0.5 };
            return (make_stub(agent_name, Some(data.summary), data.key_findings, confidence),
                    confidence, Some(data.reasoning));
        }
    }

    // ── 3. Prose + fenced or embedded JSON ───────────────────────────────
    // Agent returned prose followed by ```json{...}``` or prose with {…}
    // embedded. Extract the JSON block and treat as a domain contract.
    // This handles "Perfect! I have GBIF data...\n```json\n{...}\n```"
    {
        // Try fenced block first
        let extracted = if let Some(fs) = text.find("```json").or_else(|| text.find("```JSON")) {
            let after = text[fs..].trim_start_matches('`').trim_start_matches("json").trim_start_matches("JSON").trim_start();
            after.find("```").map(|fe| after[..fe].trim().to_string())
        } else {
            None
        };
        // Fallback: first { to last }
        let extracted = extracted.or_else(|| {
            text.find('{').and_then(|s| text.rfind('}').map(|e| text[s..=e].to_string()))
        });

        if let Some(candidate) = extracted {
            if serde_json::from_str::<serde_json::Value>(&candidate).map(|v| v.is_object()).unwrap_or(false) {
                let (summary, findings) = extract_summary_from_json_contract(&candidate);
                return (make_stub(agent_name, summary, findings, 0.5), 0.5, Some(candidate));
            }
        }
    }

    // ── 4. Plain text ─────────────────────────────────────────────────────
    let findings: Vec<String> = text
        .lines()
        .filter(|l| {
            let t = l.trim();
            if t.is_empty() || t.len() < 15 { return false; }
            if t.starts_with('#') || t.starts_with("---") || t.starts_with("===") { return false; }
            t.starts_with('-') || t.starts_with('•') || t.starts_with('*') || t.starts_with("▸")
                || t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
                || t.contains('%') || t.contains('$') || t.contains("p50")
                || t.contains("Suggested") || t.contains("confidence") || t.contains("relevance")
        })
        .take(15)
        .map(|l| l.trim().trim_start_matches(['-','•','*']).trim_start_matches("▸").trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    (make_stub(agent_name, Some(text.to_string()), findings, 0.5), 0.5, Some(text.to_string()))
}


// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::prompt_demands_structured_output;

    /// Regression for issue #3 — supply_chain_oracle's actual system prompt.
    /// Pre-fix, the bypass heuristic only matched "ONLY" or "raw JSON"; this
    /// agent's prompt uses neither, so it went through the tool loop and
    /// returned empty content. The heuristic must catch it now.
    #[test]
    fn detects_supply_chain_oracle_contract() {
        let prompt = "Return a valid JSON object — no prose outside it:\n\n```json\n{\n  \"items\": [...]\n}\n```";
        assert!(
            prompt_demands_structured_output(prompt),
            "supply_chain_oracle-style prompt must bypass the tool loop"
        );
    }

    #[test]
    fn detects_comparator_contract() {
        let prompt = "You write a narrative.\n\nreturn a valid JSON object";
        assert!(prompt_demands_structured_output(prompt));
    }

    #[test]
    fn detects_simops_advisor_contract() {
        let prompt = "Output: raw JSON only.";
        assert!(prompt_demands_structured_output(prompt));
    }

    #[test]
    fn does_not_match_generic_research_prompt() {
        // Generic prompts that don't declare a JSON contract should keep
        // running through the tool loop.
        let prompt = "You are a research agent. Use web_search to find sources \
                      and write a thorough analysis with citations.";
        assert!(!prompt_demands_structured_output(prompt));
    }

    #[test]
    fn does_not_match_prompts_that_merely_mention_json() {
        let prompt = "You may receive JSON input from the user. Reply in markdown.";
        assert!(!prompt_demands_structured_output(prompt));
    }

    /// Catches a small typo / casing slip in the heuristic that would silently
    /// disable bypass for one of the affected agents.
    #[test]
    fn matches_uppercase_and_lowercase_return_variants() {
        assert!(prompt_demands_structured_output("Return a valid JSON object"));
        assert!(prompt_demands_structured_output("return a valid JSON object"));
    }

    // ─── Issue #4 — parse_evidence_text addendum suppression ──────────

    /// Regression for issue #4: when the tool loop's final text is a JSON
    /// object that doesn't match EvidenceJson, the fallback branch must NOT
    /// stuff the whole raw JSON blob into `summary`. Otherwise the downstream
    /// formatter emits the same JSON twice in `content`.
    ///
    /// Updated for ABW fix: the extractor now salvages `summary` / findings
    /// from well-known keys. For a supply_chain_oracle-style payload that has
    /// no `summary` key but does have `items`, the raw JSON should NOT appear
    /// in `summary`, and items with a `name` field are harvested into
    /// `key_findings`. The raw text is still preserved via `reasoning`.
    #[test]
    fn parse_evidence_text_does_not_stuff_json_contract_into_summary() {
        let json_text = r#"{"items":[{"name":"Tea"}],"total_bom_cost":42}"#;
        let (evidence, _confidence, reasoning) =
            super::parse_evidence_text(json_text, "test_agent");
        assert_eq!(evidence.len(), 1);
        // No `summary` key in this payload — must still be None (raw JSON
        // must not be stuffed into the summary field).
        assert!(
            evidence[0].summary.is_none(),
            "summary must be None when JSON has no summary key; got {:?}",
            evidence[0].summary
        );
        // Raw text is preserved in the reasoning channel so the formatter
        // and metadata.raw_response still surface it as the primary answer.
        assert_eq!(reasoning.as_deref(), Some(json_text));
    }

    #[test]
    fn parse_evidence_text_recognises_array_contract() {
        let (evidence, _conf, reasoning) =
            super::parse_evidence_text(r#"[1, 2, 3]"#, "test_agent");
        assert_eq!(evidence.len(), 1);
        assert!(evidence[0].summary.is_none());
        assert_eq!(reasoning.as_deref(), Some("[1, 2, 3]"));
    }

    #[test]
    fn parse_evidence_text_preserves_summary_for_free_form_text() {
        let text = "Analysis paragraph.\n- bullet one\n- bullet two";
        let (evidence, _conf, _reasoning) = super::parse_evidence_text(text, "test_agent");
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].summary.as_deref(), Some(text));
    }

    // ─── ABW creature-agent fix — "Success + empty payload" ───────────

    /// enemy_sensor response shape: must extract `summary` and threat descriptions.
    #[test]
    fn parse_evidence_text_enemy_sensor_response() {
        let json = r#"{
            "threat_level": "medium",
            "threats": [
                {"creature_id": "abc", "species": "Anax junius", "relationship": "aerial predator", "risk": "medium"}
            ],
            "summary": "A dragonfly in the immediate vicinity poses moderate predation risk."
        }"#;
        let (evidence, _conf, reasoning) = super::parse_evidence_text(json, "enemy_sensor");
        assert_eq!(evidence.len(), 1);
        assert_eq!(
            evidence[0].summary.as_deref(),
            Some("A dragonfly in the immediate vicinity poses moderate predation risk."),
            "enemy_sensor summary must be extracted from the `summary` field"
        );
        assert!(!evidence[0].key_findings.is_empty(), "threat descriptions should populate key_findings");
        assert!(reasoning.is_some(), "raw JSON must still be in reasoning");
    }

    /// enemy_sensor with threat_level but no summary field — fallback synthesises one.
    #[test]
    fn parse_evidence_text_enemy_sensor_no_summary_field() {
        let json = r#"{"threat_level": "none", "threats": []}"#;
        let (evidence, _conf, _reasoning) = super::parse_evidence_text(json, "enemy_sensor");
        assert_eq!(evidence.len(), 1);
        assert_eq!(
            evidence[0].summary.as_deref(),
            Some("Threat level: none"),
            "threat_level fallback must produce a minimal summary"
        );
    }

    /// prey_locator SCAN response shape: must extract `hunting_summary`.
    #[test]
    fn parse_evidence_text_prey_locator_scan_response() {
        let json = r#"{
            "prey_targets": [
                {"creature_id": "xyz", "species": "Aedes aegypti", "order": "Diptera", "vulnerability": "high", "distance_cells": 1, "reasoning": "within range"}
            ],
            "hunting_summary": "One viable prey target identified within immediate range.",
            "predator_advantage": "speed and aerial agility"
        }"#;
        let (evidence, _conf, _reasoning) = super::parse_evidence_text(json, "prey_locator");
        assert_eq!(evidence.len(), 1);
        assert_eq!(
            evidence[0].summary.as_deref(),
            Some("One viable prey target identified within immediate range."),
            "prey_locator summary must be extracted from `hunting_summary`"
        );
        assert!(!evidence[0].key_findings.is_empty(), "prey_targets should populate key_findings");
    }

    /// genome_profiler response: must extract `summary` from the nested `conservation` object.
    #[test]
    fn parse_evidence_text_genome_profiler_response() {
        let json = r#"{
            "taxonomy": {"kingdom": "Animalia", "order": "Lepidoptera", "species": "Danaus plexippus"},
            "genome": {"estimated_size_mb": "480", "ploidy": "diploid"},
            "phylogeny": {"superorder": "Holometabola", "sister_taxa": ["Papilionidae"], "divergence_mya": "90"},
            "conservation": {"iucn_status": "Not Evaluated"},
            "summary": "Danaus plexippus occupies Holometabola with a ~480 Mb genome typical for Lepidoptera."
        }"#;
        let (evidence, _conf, _reasoning) = super::parse_evidence_text(json, "genome_profiler");
        assert_eq!(evidence.len(), 1);
        assert_eq!(
            evidence[0].summary.as_deref(),
            Some("Danaus plexippus occupies Holometabola with a ~480 Mb genome typical for Lepidoptera."),
            "genome_profiler summary must be extracted from top-level `summary` field"
        );
    }

    /// Creature agent prompts use "STEP 2 — RESPOND with a JSON object" — this
    /// must NOT trigger the bypass (they need the tool loop to call GBIF/scan).
    /// The bypass is reserved for agents that demand pure JSON with no tool phase.
    #[test]
    fn creature_agent_prompts_do_not_bypass_tool_loop() {
        // New genome_profiler / enemy_sensor wording
        assert!(
            !super::prompt_demands_structured_output(
                "STEP 1 — GATHER DATA: Use gbif_species_search...\nSTEP 2 — RESPOND with a JSON object in this exact shape:"
            ),
            "creature agent two-phase prompts must NOT bypass the tool loop"
        );
        // New prey_locator wording
        assert!(
            !super::prompt_demands_structured_output(
                "STEP 1 — GATHER DATA: Use scan_nearby_creatures...\nSTEP 2 — RESPOND with a JSON object in this exact shape:"
            ),
            "prey_locator two-phase prompt must NOT bypass the tool loop"
        );
    }

    /// Regression: when Claude returns prose + ```json fence, the JSON must be
    /// extracted and returned as reasoning, not swallowed into plain-text summary.
    /// This is the "Perfect! I have GBIF data...\n```json\n{...}\n```" pattern.
    #[test]
    fn parse_evidence_text_extracts_json_from_prose_plus_fence() {
        let text = "Perfect! I have GBIF data for *Protosticta myristicaensis*.\n\n```json\n{\
            \"taxonomy\": {\"order\": \"Odonata\", \"species\": \"Protosticta myristicaensis\"},\
            \"summary\": \"A rare damselfly from the Platystictidae family.\"\
        }\n```";
        let (evidence, _conf, reasoning) = super::parse_evidence_text(text, "genome_profiler");
        assert_eq!(evidence.len(), 1);
        // Summary must be extracted from the JSON, not contain prose
        assert_eq!(
            evidence[0].summary.as_deref(),
            Some("A rare damselfly from the Platystictidae family."),
            "summary must come from JSON field, not prose"
        );
        // Reasoning must be the extracted JSON, not the full prose+fence text
        let r = reasoning.unwrap_or_default();
        assert!(r.contains("Odonata"), "reasoning must contain the JSON content");
        assert!(!r.contains("Perfect!"), "reasoning must not contain the prose preamble");
    }

    /// Prose + bare JSON (no fence) must also be handled.
    #[test]
    fn parse_evidence_text_extracts_json_from_prose_plus_bare_json() {
        let text = "Here is the threat assessment: {\"threat_level\": \"low\", \"threats\": [], \"summary\": \"No immediate threats.\"}";
        let (_evidence, _conf, reasoning) = super::parse_evidence_text(text, "enemy_sensor");
        let r = reasoning.unwrap_or_default();
        assert!(r.contains("threat_level"), "reasoning must contain the JSON");
        assert!(!r.contains("Here is the threat"), "reasoning must not contain the prose");
    }
}
