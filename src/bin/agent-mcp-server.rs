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
    agent_card::AgentCard,
    credentials::{CredentialSource, ResolvedCredentials},
    executor::ExecutionContext,
    llm_executor::LLMExecutor,
    registry::AgentRegistry,
};

/// Credentials for this **single-tenant, operator-local** stdio server.
///
/// SPEC_28 bans env-sourced provider keys in the multi-tenant server
/// (`src/handlers/`, `src/agent_backend/`) because there "whose key is
/// this?" has a per-agent answer that env cannot express. This binary is
/// different in kind: one operator, on their own machine, running their
/// own agents on their own key. Reading env here is correct — and doing it
/// at the call site (rather than inside the credential type) keeps the
/// invariant greppable and the exception visible.
fn operator_credentials() -> std::sync::Arc<ResolvedCredentials> {
    let mut b = ResolvedCredentials::builder().funding_principal("local-operator");
    for (env_var, provider) in [
        ("ANTHROPIC_API_KEY", "anthropic"),
        ("OPENAI_API_KEY", "openai"),
        ("DEEPSEEK_API_KEY", "deepseek"),
        ("OPENROUTER_API_KEY", "openrouter"),
        ("MISTRAL_API_KEY", "mistral"),
        ("QWEN_API_KEY", "qwen"),
        ("GLM_API_KEY", "glm"),
        ("KIMI_API_KEY", "kimi"),
    ] {
        if let Ok(key) = std::env::var(env_var) {
            b = b.key(provider, key, CredentialSource::PrincipalDefault);
        }
    }
    b.build_arc()
}
use fermi::ast::AgentStmt;
use fermi::sensitivity::full_sensitivity_analysis;
use fermi::{Executor, Lexer, Parser, SemanticAnalyzer};

// BayesOps (Spec 14 §5.6) — fitting and what-if query tools.
// Same library that backs the HTTP surface at /api/bayesops/*; MCP is
// stateless so posteriors travel with each call as JSON.
use posterior::{fit_marginal as bayesops_fit_marginal, DistFamily};
use posterior_reg::{
    fit_conditional as bayesops_fit_conditional, ConditionalPosterior, RegressionConfig,
    WeightedSample,
};

// -- Schema-friendly JSON-object passthrough -------------------------------
//
// `rust-mcp-macros::JsonSchema` (rust-mcp-sdk 0.8) emits
// `{"type": "unknown"}` for any field whose Rust type it cannot introspect
// (e.g. `serde_json::Value`, multi-segment paths, tuples). Anthropic's
// strict JSON-Schema validator (used by Claude Opus 4.7 and friends)
// rejects such schemas with `tools.N.custom.input_schema: JSON schema is
// invalid` — which the host (Zed → Kilo ACP → Anthropic) surfaces as a
// silent turn failure.
//
// To keep heterogeneous JSON inputs (posteriors, feature dicts, etc.)
// while emitting a *valid* draft-2020-12 schema, we use a `#[serde(transparent)]`
// newtype `JsonObject` whose own `json_schema()` returns
// `{"type":"object","additionalProperties":true}`. The derive macro's
// "nested struct" branch (might_be_struct → call `Ty::json_schema()`)
// picks this up automatically, with no derive change needed.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct JsonObject(pub serde_json::Value);

impl JsonObject {
    pub fn into_inner(self) -> serde_json::Value {
        self.0
    }

    /// Schema function consumed by the `rust-mcp-macros::JsonSchema` derive.
    /// Returns a fully permissive schema (any JSON value satisfies it) so the
    /// fields that wrap heterogeneous JSON — e.g. `data` (array), `posterior`
    /// (object), `features` (object), `feature_ranges` (object) — all
    /// validate cleanly against draft 2020-12. Empty `{}` is the canonical
    /// "anything" schema and is accepted by Anthropic's strict validator.
    pub fn json_schema() -> serde_json::Map<String, serde_json::Value> {
        serde_json::Map::new()
    }
}

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

// Tool: Execute a FPL program — real Monte Carlo simulation
#[macros::mcp_tool(
    name = "fermi_execute_fpl",
    description = "Execute a Fermi FPL program string. Runs a real Monte Carlo simulation (default 10,000 iterations) and returns ExecutionResults: mean, median, std_dev, p5, p25, p75, p95, min, max, base_rate, divergence_relative, divergence_absolute."
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, macros::JsonSchema)]
pub struct FermiExecuteFplTool {
    /// A complete valid FPL program as a string
    pub fpl_program: String,
    /// Number of Monte Carlo iterations (default: 10000, max: 100000)
    #[serde(default)]
    pub iterations: Option<u32>,
    /// Optional random seed for reproducibility
    #[serde(default)]
    pub seed: Option<u64>,
}

// Tool: Run Sobol sensitivity analysis on a FPL program
#[macros::mcp_tool(
    name = "fermi_sensitivity_analysis",
    description = "Run Sobol sensitivity analysis on a FPL program. Returns first-order and total-order Sobol indices for each driver — real variance decomposition identifying which drivers actually drive output variance."
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, macros::JsonSchema)]
pub struct FermiSensitivityAnalysisTool {
    /// The same FPL program string passed to fermi_execute_fpl
    pub fpl_program: String,
    /// Number of iterations for sensitivity analysis (default: 5000)
    #[serde(default)]
    pub iterations: Option<u32>,
}

// ── BayesOps tools (Spec 14 §5.6) ───────────────────────────────────────────
//
// Domain-neutral parameter fitting. Backed by `crates/posterior` (marginal)
// and `crates/posterior-reg` (conditional). The MCP surface is stateless:
// `fit_conditional` returns the full posterior JSON which the caller passes
// back to `predict`, `input_sensitivity`, etc. on the next turn.

#[macros::mcp_tool(
    name = "fermi_fit_marginal",
    description = "BayesOps: fit a marginal distribution from a vector of scalar observations. Returns a FittedDistribution (Beta/Normal/Lognormal/Triangular) directly usable as FPL Driver parameters. Domain-neutral: works on any scalar outcome history."
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, macros::JsonSchema)]
pub struct FermiFitMarginalTool {
    /// Scalar observations to fit.
    pub observations: Vec<f64>,
    /// Optional per-observation weights (1.0 = real, 0.0–0.3 = synthetic).
    /// Must match `observations.len()` if provided.
    #[serde(default)]
    pub weights: Option<Vec<f64>>,
    /// Family: "beta" | "normal" | "lognormal" | "triangular" | "auto" (default).
    #[serde(default)]
    pub family: Option<String>,
    /// Optional human-readable provenance string for the result metadata.
    #[serde(default)]
    pub source_description: Option<String>,
}

#[macros::mcp_tool(
    name = "fermi_fit_conditional",
    description = "BayesOps: fit a conditional posterior P(outcome | features, data) via HMC. Returns the full posterior JSON which can be passed to fermi_predict / fermi_input_sensitivity / fermi_compare_scenarios / fermi_prob_exceeds / fermi_optimise_for_target."
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, macros::JsonSchema)]
pub struct FermiFitConditionalTool {
    /// Training data as JSON array of `{features: {name: f64}, outcome: f64, weight: f64}`.
    pub data: JsonObject,
    /// Regression configuration as JSON. Required field: `feature_names: [...]`.
    pub config: JsonObject,
}

#[macros::mcp_tool(
    name = "fermi_predict",
    description = "BayesOps: query a fitted ConditionalPosterior at new feature values. Returns the predictive FittedDistribution suitable for FPL injection."
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, macros::JsonSchema)]
pub struct FermiPredictTool {
    /// Posterior JSON as returned by fermi_fit_conditional.
    pub posterior: JsonObject,
    /// Query feature values as JSON object `{name: f64}`.
    pub features: JsonObject,
}

#[macros::mcp_tool(
    name = "fermi_input_sensitivity",
    description = "BayesOps: Sobol-style sensitivity analysis over a fitted ConditionalPosterior. Returns first-order and total-order indices per feature, identifying which features drive outcome variance."
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, macros::JsonSchema)]
pub struct FermiInputSensitivityTool {
    pub posterior: JsonObject,
    /// `{feature_name: [lo, hi]}` over which to compute sensitivity.
    pub feature_ranges: JsonObject,
    #[serde(default)]
    pub n_samples: Option<u32>,
}

#[macros::mcp_tool(
    name = "fermi_compare_scenarios",
    description = "BayesOps: compare two feature configurations under a fitted ConditionalPosterior. Returns full predictive distributions for both plus P(A>B), expected gain, and risk ratio."
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, macros::JsonSchema)]
pub struct FermiCompareScenariosTool {
    pub posterior: JsonObject,
    pub a: JsonObject,
    pub b: JsonObject,
}

#[macros::mcp_tool(
    name = "fermi_prob_exceeds",
    description = "BayesOps: P(outcome >= threshold | features) under a fitted ConditionalPosterior. The core planning-under-constraint query."
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, macros::JsonSchema)]
pub struct FermiProbExceedsTool {
    pub posterior: JsonObject,
    pub features: JsonObject,
    pub threshold: f64,
}

#[macros::mcp_tool(
    name = "fermi_optimise_for_target",
    description = "BayesOps: find the value of `free_feature` (holding `fixed_features` constant) that maximises P(outcome >= target_threshold). Returns recommended value, probability at the recommendation, predictive distribution, and the full sensitivity curve."
)]
#[derive(Debug, serde::Deserialize, serde::Serialize, macros::JsonSchema)]
pub struct FermiOptimiseForTargetTool {
    pub posterior: JsonObject,
    pub fixed_features: JsonObject,
    pub free_feature: String,
    /// 2-element `[lo, hi]` over which to search for `free_feature`.
    pub search_range: Vec<f64>,
    pub target_threshold: f64,
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
                FermiExecuteFplTool::tool(),
                FermiSensitivityAnalysisTool::tool(),
                // BayesOps (Spec 14 §5.6) — domain-neutral parameter fitting
                FermiFitMarginalTool::tool(),
                FermiFitConditionalTool::tool(),
                FermiPredictTool::tool(),
                FermiInputSensitivityTool::tool(),
                FermiCompareScenariosTool::tool(),
                FermiProbExceedsTool::tool(),
                FermiOptimiseForTargetTool::tool(),
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
                    credentials: operator_credentials(),
                    // The MCP server's tools/call surface does not accept an image today.
                    // Stated rather than defaulted: when it does, this is the line that
                    // changes, and a reader can see it was a decision.
                    attachments: Vec::new(),
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
                    credentials: operator_credentials(),
                    // The MCP server's tools/call surface does not accept an image today.
                    // Stated rather than defaulted: when it does, this is the line that
                    // changes, and a reader can see it was a decision.
                    attachments: Vec::new(),
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

            "fermi_execute_fpl" => {
                let tool: FermiExecuteFplTool = serde_json::from_value(serde_json::Value::Object(
                    params.arguments.unwrap_or_default(),
                ))
                .map_err(|e| CallToolError::new(e))?;

                // Parse the FPL program through the full pipeline
                let program = parse_fpl(&tool.fpl_program).map_err(CallToolError::from_message)?;

                // Execute Monte Carlo simulation
                let iterations = (tool.iterations.unwrap_or(10_000) as usize).min(100_000);
                let mut executor = match tool.seed {
                    Some(seed) => Executor::with_seed(iterations, seed),
                    None => Executor::new(iterations),
                };
                let results = executor
                    .execute(&program)
                    .map_err(|e| CallToolError::from_message(e.to_string()))?;

                let output = json!({
                    "iterations": results.iterations,
                    "mean":   results.mean,
                    "median": results.median,
                    "std_dev": results.std_dev,
                    "p5":  results.p5,
                    "p25": results.p25,
                    "p75": results.p75,
                    "p95": results.p95,
                    "min": results.min,
                    "max": results.max,
                    "base_rate": results.base_rate,
                    "divergence_relative": results.divergence_relative,
                    "divergence_absolute": results.divergence_absolute,
                });
                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&output).unwrap().into(),
                ]))
            }

            "fermi_sensitivity_analysis" => {
                let tool: FermiSensitivityAnalysisTool = serde_json::from_value(
                    serde_json::Value::Object(params.arguments.unwrap_or_default()),
                )
                .map_err(|e| CallToolError::new(e))?;

                let program = parse_fpl(&tool.fpl_program).map_err(CallToolError::from_message)?;

                let iterations = (tool.iterations.unwrap_or(5_000) as usize).min(50_000);
                let analysis = full_sensitivity_analysis(&program, iterations)
                    .map_err(|e| CallToolError::from_message(e.to_string()))?;

                // Build per-driver sensitivity objects, ranked by total-order index
                let drivers: Vec<serde_json::Value> = analysis
                    .ranked_drivers
                    .iter()
                    .filter_map(|name| analysis.driver_sensitivities.get(name))
                    .map(|s| {
                        json!({
                            "driver": s.driver_name,
                            "first_order_index": s.first_order_index,
                            "total_order_index": s.total_order_index,
                            "variance_contribution": s.variance_contribution,
                            "standard_error": s.standard_error,
                            "ci_low":  (s.total_order_index - 1.96 * s.standard_error).max(0.0),
                            "ci_high": (s.total_order_index + 1.96 * s.standard_error).min(1.0),
                        })
                    })
                    .collect();

                let output = json!({
                    "iterations": iterations,
                    "baseline": {
                        "mean":    analysis.baseline.mean,
                        "std_dev": analysis.baseline.std_dev,
                        "p5":      analysis.baseline.p5,
                        "p95":     analysis.baseline.p95,
                    },
                    "drivers": drivers,
                    "top_driver": analysis.ranked_drivers.first(),
                });
                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&output).unwrap().into(),
                ]))
            }

            // ── BayesOps tools ───────────────────────────────────────────────
            "fermi_fit_marginal" => {
                let tool: FermiFitMarginalTool = serde_json::from_value(serde_json::Value::Object(
                    params.arguments.unwrap_or_default(),
                ))
                .map_err(|e| CallToolError::new(e))?;

                let family = parse_dist_family(tool.family.as_deref())
                    .map_err(CallToolError::from_message)?;
                let weights = tool.weights.as_deref();
                let (fitted, mut meta) = bayesops_fit_marginal(&tool.observations, weights, family)
                    .map_err(|e| CallToolError::from_message(e.to_string()))?;
                if let Some(desc) = tool.source_description {
                    meta.source_description = desc;
                }
                let output = json!({
                    "fitted": fitted,
                    "metadata": meta,
                    "fpl_params": fitted.to_fpl_params(),
                });
                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&output).unwrap().into(),
                ]))
            }

            "fermi_fit_conditional" => {
                let tool: FermiFitConditionalTool = serde_json::from_value(
                    serde_json::Value::Object(params.arguments.unwrap_or_default()),
                )
                .map_err(|e| CallToolError::new(e))?;

                let data: Vec<WeightedSample> = serde_json::from_value(tool.data.into_inner())
                    .map_err(|e| CallToolError::from_message(format!("invalid data: {}", e)))?;
                let config: RegressionConfig = serde_json::from_value(tool.config.into_inner())
                    .map_err(|e| CallToolError::from_message(format!("invalid config: {}", e)))?;

                let posterior = bayesops_fit_conditional(&data, &config)
                    .await
                    .map_err(|e| CallToolError::from_message(e.to_string()))?;

                // Return the entire posterior JSON so the caller can pass it back
                // to predict / sensitivity / etc. The MCP world is stateless —
                // no server-side cache.
                let output = serde_json::to_value(&posterior).map_err(|e| {
                    CallToolError::from_message(format!("serialise posterior: {}", e))
                })?;
                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&output).unwrap().into(),
                ]))
            }

            "fermi_predict" => {
                let tool: FermiPredictTool = serde_json::from_value(serde_json::Value::Object(
                    params.arguments.unwrap_or_default(),
                ))
                .map_err(|e| CallToolError::new(e))?;

                let posterior: ConditionalPosterior =
                    serde_json::from_value(tool.posterior.into_inner()).map_err(|e| {
                        CallToolError::from_message(format!("invalid posterior: {}", e))
                    })?;
                let features: std::collections::HashMap<String, f64> =
                    serde_json::from_value(tool.features.into_inner()).map_err(|e| {
                        CallToolError::from_message(format!("invalid features: {}", e))
                    })?;

                let fitted = posterior
                    .predict(&features)
                    .map_err(|e| CallToolError::from_message(e.to_string()))?;

                let output = json!({
                    "fitted": fitted,
                    "fpl_params": fitted.to_fpl_params(),
                });
                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&output).unwrap().into(),
                ]))
            }

            "fermi_input_sensitivity" => {
                let tool: FermiInputSensitivityTool = serde_json::from_value(
                    serde_json::Value::Object(params.arguments.unwrap_or_default()),
                )
                .map_err(|e| CallToolError::new(e))?;

                let posterior: ConditionalPosterior =
                    serde_json::from_value(tool.posterior.into_inner()).map_err(|e| {
                        CallToolError::from_message(format!("invalid posterior: {}", e))
                    })?;
                let feature_ranges: std::collections::HashMap<String, (f64, f64)> =
                    serde_json::from_value(tool.feature_ranges.into_inner()).map_err(|e| {
                        CallToolError::from_message(format!("invalid feature_ranges: {}", e))
                    })?;
                let n = tool.n_samples.unwrap_or(256u32);

                let result = posterior
                    .input_sensitivity(&feature_ranges, n as usize)
                    .map_err(|e| CallToolError::from_message(e.to_string()))?;

                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&json!({ "sensitivity": result }))
                        .unwrap()
                        .into(),
                ]))
            }

            "fermi_compare_scenarios" => {
                let tool: FermiCompareScenariosTool = serde_json::from_value(
                    serde_json::Value::Object(params.arguments.unwrap_or_default()),
                )
                .map_err(|e| CallToolError::new(e))?;

                let posterior: ConditionalPosterior =
                    serde_json::from_value(tool.posterior.into_inner()).map_err(|e| {
                        CallToolError::from_message(format!("invalid posterior: {}", e))
                    })?;
                let a: std::collections::HashMap<String, f64> =
                    serde_json::from_value(tool.a.into_inner()).map_err(|e| {
                        CallToolError::from_message(format!("invalid scenario a: {}", e))
                    })?;
                let b: std::collections::HashMap<String, f64> =
                    serde_json::from_value(tool.b.into_inner()).map_err(|e| {
                        CallToolError::from_message(format!("invalid scenario b: {}", e))
                    })?;

                let comp = posterior
                    .compare_scenarios(&a, &b)
                    .map_err(|e| CallToolError::from_message(e.to_string()))?;

                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&comp).unwrap().into(),
                ]))
            }

            "fermi_prob_exceeds" => {
                let tool: FermiProbExceedsTool = serde_json::from_value(serde_json::Value::Object(
                    params.arguments.unwrap_or_default(),
                ))
                .map_err(|e| CallToolError::new(e))?;

                let posterior: ConditionalPosterior =
                    serde_json::from_value(tool.posterior.into_inner()).map_err(|e| {
                        CallToolError::from_message(format!("invalid posterior: {}", e))
                    })?;
                let features: std::collections::HashMap<String, f64> =
                    serde_json::from_value(tool.features.into_inner()).map_err(|e| {
                        CallToolError::from_message(format!("invalid features: {}", e))
                    })?;

                let probability = posterior
                    .prob_exceeds(&features, tool.threshold)
                    .map_err(|e| CallToolError::from_message(e.to_string()))?;

                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&json!({ "probability": probability }))
                        .unwrap()
                        .into(),
                ]))
            }

            "fermi_optimise_for_target" => {
                let tool: FermiOptimiseForTargetTool = serde_json::from_value(
                    serde_json::Value::Object(params.arguments.unwrap_or_default()),
                )
                .map_err(|e| CallToolError::new(e))?;

                let posterior: ConditionalPosterior =
                    serde_json::from_value(tool.posterior.into_inner()).map_err(|e| {
                        CallToolError::from_message(format!("invalid posterior: {}", e))
                    })?;
                let fixed: std::collections::HashMap<String, f64> =
                    serde_json::from_value(tool.fixed_features.into_inner()).map_err(|e| {
                        CallToolError::from_message(format!("invalid fixed_features: {}", e))
                    })?;

                // search_range comes in as a 2-element Vec<f64> for schema friendliness;
                // convert back to the (lo, hi) tuple the backend expects.
                if tool.search_range.len() != 2 {
                    return Err(CallToolError::from_message(format!(
                        "search_range must have exactly 2 elements [lo, hi], got {}",
                        tool.search_range.len()
                    )));
                }
                let search_range = (tool.search_range[0], tool.search_range[1]);

                let result = posterior
                    .optimise_for_target(
                        &fixed,
                        &tool.free_feature,
                        search_range,
                        tool.target_threshold,
                    )
                    .map_err(|e| CallToolError::from_message(e.to_string()))?;

                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&result).unwrap().into(),
                ]))
            }

            _ => Err(CallToolError::unknown_tool(params.name)),
        }
    }
}

/// Parse a family string into the typed `DistFamily` enum.
///
/// Note: this file glob-imports `rust_mcp_sdk::*` which brings its own `Result`
/// type into scope. We use a fully-qualified `std::result::Result` so we get
/// the standard one.
fn parse_dist_family(name: Option<&str>) -> std::result::Result<DistFamily, String> {
    Ok(match name.unwrap_or("auto").to_lowercase().as_str() {
        "beta" => DistFamily::Beta,
        "normal" => DistFamily::Normal,
        "lognormal" => DistFamily::Lognormal,
        "triangular" => DistFamily::Triangular,
        "auto" | "" => DistFamily::Auto,
        other => return std::result::Result::Err(format!("unknown family '{}'", other)),
    })
}

/// Parse an FPL program string through the full lexer → parser → semantic pipeline.
/// Returns the validated Program or a human-readable error string.
fn parse_fpl(source: &str) -> std::result::Result<fermi::ast::Program, String> {
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().map_err(|errs| {
        errs.iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let mut parser = Parser::new(tokens);
    let program = parser.parse().map_err(|e| format!("Parse error: {e}"))?;
    let analyzer = SemanticAnalyzer::new();
    let analysis = analyzer.analyze(&program);
    if !analysis.errors.is_empty() {
        let msgs = analysis
            .errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("Semantic error: {msgs}"));
    }
    Ok(program)
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

    // Create agent registry with LLM executor if a key is available.
    //
    // `LLMExecutor::from_env()` is now infallible (it holds no key — keys
    // travel on the ExecutionContext), so the presence check has to be
    // explicit here rather than inferred from its Result.
    let registry = if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        eprintln!("✓ Using LLM Executor (Claude API)");
        Arc::new(AgentRegistry::with_executor(Arc::new(LLMExecutor::new())))
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

    eprintln!("Fermi Agent Bestiary MCP Server started");
    eprintln!("   Tools:    list_agents, get_agent, execute_agent, save_agent, search_agents, get_catalogue, ask_xaman_ek");
    eprintln!("   FPL:      fermi_execute_fpl, fermi_sensitivity_analysis");
    eprintln!("   BayesOps: fermi_fit_marginal, fermi_fit_conditional, fermi_predict,");
    eprintln!("             fermi_input_sensitivity, fermi_compare_scenarios,");
    eprintln!("             fermi_prob_exceeds, fermi_optimise_for_target");

    server.start().await
}
