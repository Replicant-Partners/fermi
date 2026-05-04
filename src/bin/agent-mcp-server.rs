#!/usr/bin/env rust
//! MCP Server for Fermi Agent Bestiary
//!
//! This server exposes agent operations as MCP tools for consumption by
//! editors like Zed, enabling AI-powered forecasting research directly from
//! the editor.

use async_trait::async_trait;
use rust_mcp_sdk::{
    error::SdkResult,
    macros,
    mcp_server::{server_runtime, McpServerOptions, ServerHandler},
    schema::*,
    *,
};
use serde_json::json;
use std::sync::Arc;

use fermi::agent_backend::{
    agent_card::AgentCard, executor::ExecutionContext, llm_executor::LLMExecutor,
    registry::AgentRegistry,
};
use fermi::ast::AgentStmt;

// Tool: List all available agents
#[macros::mcp_tool(
    name = "list_agents",
    description = "List all available forecasting agents with their capabilities, performance stats, and usage information"
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, macros::JsonSchema)]
pub struct ListAgentsTool {}

// Tool: Get detailed agent information
#[macros::mcp_tool(
    name = "get_agent",
    description = "Get detailed information about a specific forecasting agent including capabilities, performance metrics, and usage statistics"
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, macros::JsonSchema)]
pub struct GetAgentTool {
    pub agent_id: String,
}

// Tool: Execute an agent with a query
#[macros::mcp_tool(
    name = "execute_agent",
    description = "Execute a forecasting agent with a research query and receive evidence-based insights with confidence scores"
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, macros::JsonSchema)]
pub struct ExecuteAgentTool {
    pub agent_id: String,
    pub query: String,
}

// Tool: Save agent statistics
#[macros::mcp_tool(
    name = "save_agent",
    description = "Save an agent's updated performance statistics to disk and commit to git for version control"
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, macros::JsonSchema)]
pub struct SaveAgentTool {
    pub agent_id: String,
}

// Tool: Search agents by tag, type, or keyword
#[macros::mcp_tool(
    name = "search_agents",
    description = "Search agents by keyword, tag, type, or tier. Returns matching agents with descriptions. Examples: 'creative', 'social-media', 'system', 'coherence'"
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, macros::JsonSchema)]
pub struct SearchAgentsTool {
    pub query: String,
}

// Tool: Get the full catalogue grouped by category
#[macros::mcp_tool(
    name = "get_catalogue",
    description = "Get the complete agent catalogue organized by category (Research, Creative, Coherence, Infrastructure, Games). Shows composition patterns and team recommendations."
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, macros::JsonSchema)]
pub struct GetCatalogueTool {}

// Tool: Ask Xaman Ek (the platform navigator)
#[macros::mcp_tool(
    name = "ask_xaman_ek",
    description = "Ask the platform navigator anything: find agents, design workspace teams, explain features, compare agents, get platform status. Xaman Ek knows every agent and composition pattern."
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, macros::JsonSchema)]
pub struct AskXamanEkTool {
    pub question: String,
}

/// Custom handler for Agent Bestiary operations
struct AgentBestiaryHandler {
    registry: Arc<AgentRegistry>,
}

impl AgentBestiaryHandler {
    pub fn new(registry: Arc<AgentRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ServerHandler for AgentBestiaryHandler {
    /// List available tools
    async fn handle_list_tools_request(
        &self,
        _request: Option<PaginatedRequestParams>,
        _runtime: std::sync::Arc<dyn McpServer>,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        Ok(ListToolsResult {
            tools: vec![
                ListAgentsTool::tool(),
                GetAgentTool::tool(),
                ExecuteAgentTool::tool(),
                SaveAgentTool::tool(),
                SearchAgentsTool::tool(),
                GetCatalogueTool::tool(),
                AskXamanEkTool::tool(),
            ],
            meta: None,
            next_cursor: None,
        })
    }

    /// Handle tool execution
    async fn handle_call_tool_request(
        &self,
        params: CallToolRequestParams,
        _runtime: std::sync::Arc<dyn McpServer>,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        match params.name.as_str() {
            "list_agents" => {
                let agents = self.registry.list_cards().unwrap_or_default();
                let result = json!({
                    "agents": agents.iter().map(|card| json!({
                        "agent_id": card.agent_id,
                        "agent_type": card.agent_type,
                        "tier": format!("{:?}", card.tier),
                        "description": card.metadata.description,
                        "skills": card.capabilities.skills,
                        "model": card.capabilities.model,
                        "performance": {
                            "accuracy_rate": format!("{:.1}%", card.performance.accuracy_rate * 100.0),
                            "avg_confidence": format!("{:.2}", card.performance.avg_confidence),
                        },
                        "usage": {
                            "total_executions": card.usage.total_executions,
                            "total_cost_usd": format!("${:.6}", card.usage.total_cost_usd),
                        }
                    })).collect::<Vec<_>>()
                });
                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&result).unwrap().into(),
                ]))
            }

            "get_agent" => {
                let tool: GetAgentTool = serde_json::from_value(serde_json::Value::Object(
                    params.arguments.unwrap_or_default(),
                ))
                .map_err(|e| CallToolError::new(e))?;

                let card = self
                    .registry
                    .get(&tool.agent_id)
                    .map_err(|e| CallToolError::new(e))?;

                let result = json!({
                    "agent_id": card.agent_id,
                    "agent_type": card.agent_type,
                    "version": card.version,
                    "tier": format!("{:?}", card.tier),
                    "description": card.metadata.description,
                    "author": card.metadata.author,
                    "tags": card.metadata.tags,
                    "capabilities": {
                        "executor": format!("{:?}", card.capabilities.executor),
                        "skills": card.capabilities.skills,
                        "model": card.capabilities.model,
                        "temperature": card.capabilities.temperature,
                        "mcp_tools": card.capabilities.mcp_tools,
                    },
                    "performance": {
                        "forecasts_contributed": card.performance.forecasts_contributed,
                        "accuracy_rate": format!("{:.1}%", card.performance.accuracy_rate * 100.0),
                        "avg_confidence": format!("{:.2}", card.performance.avg_confidence),
                        "avg_brier_impact": format!("{:.4}", card.performance.avg_brier_impact),
                    },
                    "usage": {
                        "total_executions": card.usage.total_executions,
                        "successful_executions": card.usage.successful_executions,
                        "failed_executions": card.usage.failed_executions,
                        "total_tokens_used": card.usage.total_tokens_used,
                        "total_cost_usd": format!("${:.6}", card.usage.total_cost_usd),
                        "avg_execution_time_ms": card.usage.avg_execution_time_ms,
                    }
                });
                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&result).unwrap().into(),
                ]))
            }

            "execute_agent" => {
                let tool: ExecuteAgentTool = serde_json::from_value(serde_json::Value::Object(
                    params.arguments.unwrap_or_default(),
                ))
                .map_err(|e| CallToolError::new(e))?;

                // Get agent card
                let card = self
                    .registry
                    .get(&tool.agent_id)
                    .map_err(|e| CallToolError::new(e))?;

                // Create a minimal agent statement for execution
                let agent = AgentStmt {
                    name: tool.agent_id.clone(),
                    agent_type: Some("research".to_string()),
                    query: tool.query.clone(),
                    executor: Some(fermi::ast::ExecutorType::LLM),
                    schedule: None,
                    driver_refs: vec![],
                    depends_on: vec![],
                    confidence_threshold: None,
                };

                // Create a minimal program with just this agent
                let program = fermi::ast::Program {
                    statements: vec![fermi::ast::Statement::Agent(agent.clone())],
                };

                let context = ExecutionContext {
                    program,
                    agent_card: card.clone(),
                    creature_id: None,
                    cognition_tier: None,
                };

                // Execute the agent
                let result = self
                    .registry
                    .execute_agent(&agent, &context)
                    .await
                    .map_err(|e| CallToolError::new(e))?;

                // Record execution and get updated card
                self.registry
                    .record_execution(&tool.agent_id, &result)
                    .map_err(|e| CallToolError::new(e))?;
                let card = self
                    .registry
                    .get(&tool.agent_id)
                    .map_err(|e| CallToolError::new(e))?;

                let output = json!({
                    "agent_name": result.agent_name,
                    "status": format!("{:?}", result.status),
                    "confidence": format!("{:.2}", result.confidence),
                    "execution_time_ms": result.execution_time_ms,
                    "tokens_used": result.tokens_used,
                    "evidence": result.evidence.iter().map(|e| json!({
                        "id": e.id,
                        "source": e.source,
                        "summary": e.summary.clone().unwrap_or_default(),
                        "key_findings": e.key_findings,
                        "relevance": e.relevance.unwrap_or(0.0),
                    })).collect::<Vec<_>>(),
                    "updated_stats": {
                        "total_executions": card.usage.total_executions,
                        "accuracy_rate": format!("{:.1}%", card.performance.accuracy_rate * 100.0),
                        "total_cost_usd": format!("${:.6}", card.usage.total_cost_usd),
                    }
                });

                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&output).unwrap().into(),
                ]))
            }

            "save_agent" => {
                let tool: SaveAgentTool = serde_json::from_value(serde_json::Value::Object(
                    params.arguments.unwrap_or_default(),
                ))
                .map_err(|e| CallToolError::new(e))?;

                let agents_dir =
                    std::env::var("AGENTS_DIR").unwrap_or_else(|_| "agents/curated".to_string());

                self.registry
                    .save_and_commit(&tool.agent_id, &agents_dir)
                    .map_err(|e| CallToolError::new(e))?;

                let result = json!({
                    "message": format!("Agent '{}' saved and committed to git", tool.agent_id),
                    "agent_id": tool.agent_id
                });

                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&result).unwrap().into(),
                ]))
            }

            "search_agents" => {
                let tool: SearchAgentsTool = serde_json::from_value(serde_json::Value::Object(
                    params.arguments.unwrap_or_default(),
                ))
                .map_err(|e| CallToolError::new(e))?;

                let agents = self.registry.list_cards().unwrap_or_default();
                let query_lower = tool.query.to_lowercase();

                let matches: Vec<_> = agents
                    .iter()
                    .filter(|card| {
                        let id_match = card.agent_id.to_lowercase().contains(&query_lower);
                        let type_match = card.agent_type.to_lowercase().contains(&query_lower);
                        let tier_match = format!("{:?}", card.tier)
                            .to_lowercase()
                            .contains(&query_lower);
                        let desc_match = card
                            .metadata
                            .description
                            .to_lowercase()
                            .contains(&query_lower);
                        let tag_match = card
                            .metadata
                            .tags
                            .iter()
                            .any(|t| t.to_lowercase().contains(&query_lower));
                        let skill_match = card
                            .capabilities
                            .skills
                            .iter()
                            .any(|s| s.to_lowercase().contains(&query_lower));
                        id_match
                            || type_match
                            || tier_match
                            || desc_match
                            || tag_match
                            || skill_match
                    })
                    .map(|card| {
                        json!({
                            "agent_id": card.agent_id,
                            "agent_type": card.agent_type,
                            "tier": format!("{:?}", card.tier),
                            "description": card.metadata.description,
                            "tags": card.metadata.tags,
                            "model": card.capabilities.model,
                        })
                    })
                    .collect();

                let result = json!({
                    "query": tool.query,
                    "matches": matches.len(),
                    "agents": matches,
                });
                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&result).unwrap().into(),
                ]))
            }

            "get_catalogue" => {
                let agents = self.registry.list_cards().unwrap_or_default();

                // Group by category based on tags and type
                let mut categories: std::collections::BTreeMap<&str, Vec<serde_json::Value>> =
                    std::collections::BTreeMap::new();
                for card in &agents {
                    let category = categorize_agent(card);
                    categories.entry(category).or_default().push(json!({
                        "agent_id": card.agent_id,
                        "description": &card.metadata.description[..card.metadata.description.len().min(100)],
                        "tier": format!("{:?}", card.tier),
                        "model": card.capabilities.model,
                    }));
                }

                let result = json!({
                    "total_agents": agents.len(),
                    "categories": categories,
                    "composition_patterns": {
                        "Artist Deck": ["style_transfer", "watermark", "delivery"],
                        "Social Media Studio": ["social_media_studio", "instagram_publisher", "bluesky_publisher"],
                        "Research Team": ["macro_forecaster", "entity_investigator", "sentiment_analyzer", "monte_carlo_sim"],
                        "Coherence Stack": ["coherence_evaluator", "coherence_consultant", "intention_coordinator"],
                        "Full Coordination": ["cohere_and_coordinate"]
                    }
                });
                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&result).unwrap().into(),
                ]))
            }

            "ask_xaman_ek" => {
                let tool: AskXamanEkTool = serde_json::from_value(serde_json::Value::Object(
                    params.arguments.unwrap_or_default(),
                ))
                .map_err(|e| CallToolError::new(e))?;

                // Get xaman_ek card
                let card = self
                    .registry
                    .get("xaman_ek")
                    .map_err(|e| CallToolError::new(e))?;

                let agent = AgentStmt {
                    name: "xaman_ek".to_string(),
                    agent_type: Some("meta".to_string()),
                    query: tool.question.clone(),
                    executor: Some(fermi::ast::ExecutorType::LLM),
                    schedule: None,
                    driver_refs: vec![],
                    depends_on: vec![],
                    confidence_threshold: None,
                };

                let program = fermi::ast::Program {
                    statements: vec![fermi::ast::Statement::Agent(agent.clone())],
                };

                let context = ExecutionContext {
                    program,
                    agent_card: card.clone(),
                    creature_id: None,
                    cognition_tier: None,
                };

                let result = self
                    .registry
                    .execute_agent(&agent, &context)
                    .await
                    .map_err(|e| CallToolError::new(e))?;

                self.registry.record_execution("xaman_ek", &result).ok(); // Don't fail on stats update

                let output = json!({
                    "navigator": "Xaman Ek",
                    "answer": result.evidence.first().map(|e| e.summary.clone().unwrap_or_default()).unwrap_or_default(),
                    "confidence": format!("{:.2}", result.confidence),
                    "evidence": result.evidence.iter().map(|e| json!({
                        "source": e.source,
                        "summary": e.summary.clone().unwrap_or_default(),
                        "key_findings": e.key_findings,
                    })).collect::<Vec<_>>(),
                });

                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&output).unwrap().into(),
                ]))
            }

            _ => Err(CallToolError::unknown_tool(params.name)),
        }
    }
}

/// Categorize an agent based on its tags and type
fn categorize_agent(card: &AgentCard) -> &'static str {
    let tags: Vec<String> = card
        .metadata
        .tags
        .iter()
        .map(|t| t.to_lowercase())
        .collect();
    let agent_type = card.agent_type.to_lowercase();

    if tags
        .iter()
        .any(|t| t.contains("coherence") || t.contains("coordination"))
    {
        "Coherence & Coordination"
    } else if tags
        .iter()
        .any(|t| t.contains("social-media") || t.contains("instagram") || t.contains("bluesky"))
    {
        "Social Media & Publishing"
    } else if agent_type.contains("creative")
        || tags
            .iter()
            .any(|t| t.contains("image") || t.contains("creative") || t.contains("style"))
    {
        "Creative & Visual"
    } else if tags
        .iter()
        .any(|t| t.contains("billing") || t.contains("stripe") || t.contains("payment"))
    {
        "Billing & Economics"
    } else if tags
        .iter()
        .any(|t| t.contains("meta") || t.contains("navigation") || t.contains("coaching"))
    {
        "Meta & Platform"
    } else if tags
        .iter()
        .any(|t| t.contains("game") || t.contains("puzzle") || t.contains("engagement"))
    {
        "Games & Engagement"
    } else if agent_type.contains("research")
        || agent_type.contains("analyst")
        || tags
            .iter()
            .any(|t| t.contains("forecasting") || t.contains("research") || t.contains("analysis"))
    {
        "Research & Analysis"
    } else {
        "Other"
    }
}

#[tokio::main]
async fn main() -> SdkResult<()> {
    // Initialize tracing for debugging (writes to stderr)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    // Load agents directory from environment or use default
    let agents_dir = std::env::var("AGENTS_DIR").unwrap_or_else(|_| "agents/curated".to_string());

    // Create agent registry with LLM executor if API key is available
    let registry = if let Ok(llm_executor) = LLMExecutor::from_env() {
        eprintln!("✓ Using LLM Executor (Claude API)");
        Arc::new(AgentRegistry::with_executor(Arc::new(llm_executor)))
    } else {
        eprintln!("⚠ No ANTHROPIC_API_KEY found, using Mock Executor");
        Arc::new(AgentRegistry::new())
    };

    // Load agents from filesystem
    match registry.load_from_directory(&agents_dir) {
        Ok(count) if count > 0 => {
            eprintln!("✓ Loaded {} agent(s) from {}", count, agents_dir);
        }
        Ok(_) => {
            eprintln!("⚠ No agents found in {}", agents_dir);
        }
        Err(e) => {
            eprintln!("⚠ Failed to load agents: {}", e);
        }
    }

    // Server details
    let server_details = InitializeResult {
        server_info: Implementation {
            name: "fermi-agent-bestiary".into(),
            version: "0.1.0".into(),
            title: Some("Fermi Agent Bestiary MCP Server".into()),
            description: Some("AI-powered forecasting research agents accessible via MCP".into()),
            icons: vec![],
            website_url: Some("https://github.com/fermi-project/fermi".into()),
        },
        capabilities: ServerCapabilities {
            tools: Some(ServerCapabilitiesTools {
                list_changed: None,
            }),
            ..Default::default()
        },
        protocol_version: ProtocolVersion::V2025_11_25.into(),
        instructions: Some("Use these tools to interact with the Fermi Agent Bestiary. Start with get_catalogue for an overview, search_agents to find specific agents, ask_xaman_ek for platform guidance, or execute_agent to run queries. 27 agents across Research, Creative, Coherence, Billing, and Social Media categories.".into()),
        meta: None,
    };

    // Create transport and handler
    let transport = StdioTransport::new(TransportOptions::default())?;
    let handler = AgentBestiaryHandler::new(registry).to_mcp_server_handler();

    // Create and start server
    let server = server_runtime::create_server(McpServerOptions {
        transport,
        handler,
        server_details,
        task_store: None,
        client_task_store: None,
    });

    eprintln!("🚀 Fermi Agent Bestiary MCP Server started");
    eprintln!("   Tools: list_agents, get_agent, execute_agent, save_agent, search_agents, get_catalogue, ask_xaman_ek");

    server.start().await
}
