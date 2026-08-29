/// Built-in tool registry for agent tool-use
///
/// Provides 30 platform tools that agents can invoke via the LLM tool-calling protocol:
///   - search_knowledge: similarity search over agent's episodic memory
///   - query_ontology: get rules/entities/facts from knowledge graph
///   - execute_agent: invoke another agent (single-turn, no recursion)
///   - list_agents: discover available agents
///   - read_workspace_file: read a file from workspace git repo (workspace-only)
///   - list_workspace_agents: list agents in current workspace (workspace-only)
///   - generate_image: text-to-image via Gemini
///   - edit_image: image-to-image editing via Gemini
///   - write_workspace_file: write a file to workspace git repo (workspace-only)
///   - reduct_list_projects: list Reduct.video projects
///   - reduct_get_project: get project details with recordings and reels
///   - reduct_get_transcript: get recording transcript (JSON with timestamps)
///   - reduct_create_reel: create a new reel in a project
///   - reduct_add_block: add a clip or title block to a reel
///   - evaluate_coherence: run TEC evaluation on workspace messages (workspace-only)
///   - coherence_snapshot: get latest coherence evaluation (workspace-only)
///   - get_workspace_messages: read recent workspace conversation (workspace-only)
///   - get_shopping_profile: retrieve user's shopping preference profile (workspace-only)
///   - update_shopping_profile: recompute composite shopping embedding (workspace-only)
///   - list_marketplace: browse active marketplace listings (workspace-only)
///   - create_listing: list a shopping profile on the marketplace (workspace-only)
///   - delegate_to_agent: delegate task to workspace agent with full tools (workspace-only)
///   - h3_resolve: H3 hexagonal grid operations (gps_to_h3, neighbors, distance, grid_disk)
///   - geocode: address to GPS coordinates via OpenStreetMap Nominatim
///   - create_beacon: create an AR beacon at an H3 cell (workspace-only)
///   - query_beacons: find AR beacons near a location
///   - save_grid_map: persist a named spatial grid (workspace-only)
///   - gbif_species_search: search GBIF for insect species data
///   - mint_creature: store a minted creature specimen (workspace-only)
///   - generate_specimen_art: generate unique naturalist illustration for a creature via Gemini
///   - scan_nearby_creatures: H3 proximity scan for enemy_sensor agent threat assessment
///   - web_search: search the web via Brave Search API (requires BRAVE_SEARCH_API_KEY)
///   - run_monte_carlo: execute FPL program via the real Monte Carlo engine, returns stats + histogram
///   - run_sensitivity_analysis: Sobol global sensitivity analysis (Saltelli) on an FPL program
use crate::agent_backend::agent_card::AgentCard;
use crate::agent_backend::executor::{AgentExecutor, ExecutionContext};
use crate::agent_backend::llm_executor::ClaudeTool;
use crate::agent_backend::multi_model_executor::{OpenAIFunction, OpenAITool};
use crate::agent_backend::registry::AgentRegistry;
use crate::agent_backend::tool_executor::ToolAwareExecutor;
use agent_bestiary_memory::embeddings::EmbeddingGenerator;
use agent_bestiary_memory::store::MemoryStore;
use agent_bestiary_memory::types::CoherenceEvaluation;
use agent_bestiary_memory::WorkspaceMessage;
use agent_bestiary_ontology::WorkspaceGitManager;
use coherence_core::types::{ConversationId, Message as CoherenceMessage, ParticipantId};
use coherence_engine::SettlingEngine;
use coherence_observer::ConversationObserver;
use serde_json::json;
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

/// Context available to tools during execution
pub struct ToolContext {
    pub memory_store: Arc<MemoryStore>,
    pub embedder: Arc<dyn EmbeddingGenerator>,
    pub registry: Arc<AgentRegistry>,
    pub current_agent_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub workspace_slug: Option<String>,
    pub workspace_git: Option<Arc<WorkspaceGitManager>>,
    pub db: Option<sqlx::PgPool>,
    pub gas_fees: Option<crate::gas::GasFees>,
    pub user_id: Option<String>,
    /// Third-party / MCP tool credentials for the agent owner. NOT LLM
    /// provider keys — those live on `ExecutionContext.credentials`
    /// (SPEC_28). Renaming this to `tool_secrets` is P5.3.
    pub user_secrets: Option<std::collections::HashMap<String, String>>,
    /// LLM provider credentials for the *current* execution, carried here
    /// only so the delegation tools (`execute_agent`, `delegate_to_agent`)
    /// can propagate them when they build a child `ExecutionContext`.
    ///
    /// Executors read credentials from `ExecutionContext`, never from
    /// here. Today a delegated child runs on the parent's credentials,
    /// matching the pre-existing `user_secrets` propagation above;
    /// funding a child by *its own* owner is a SPEC_28 P5.2 follow-up.
    pub credentials: std::sync::Arc<crate::agent_backend::credentials::ResolvedCredentials>,
    /// Episode id of the execution currently running (mig-198).
    ///
    /// Set by whoever mints the episode id, BEFORE execution starts, so the
    /// delegation tools can stamp it as `parent_episode_id` on the child
    /// episodes they write. That is what makes a compound execution's true
    /// cost recoverable: the caller records only its own tokens, and the tree
    /// is reassembled from these links.
    ///
    /// `None` for paths that don't persist an episode; their delegated
    /// children are still recorded, just as roots.
    pub parent_episode_id: Option<Uuid>,
    /// Optional eval-trigger bridge. The library can't reach AppState
    /// (it lives in the bin), so handlers that have AppState build an
    /// EvalTriggerImpl and stash it here. The MCP tool
    /// `run_evaluator_registry` calls into this. Sites that pass `None`
    /// get a graceful tool error instead of a trigger.
    pub eval_trigger: Option<Arc<dyn EvalTrigger>>,
    /// Remote MCP tools this agent may call, discovered from the
    /// `mcp_servers` block on its own card.
    ///
    /// Deliberately carried on the context rather than resolved from a
    /// global registry: this is an authorization boundary. Builtin tools
    /// are global (every agent gets all of them and `execute` performs no
    /// per-agent check) — remote tools must not inherit that, or one
    /// agent's third-party credential becomes every agent's.
    ///
    /// `None` means the caller did not resolve remote tools; the agent
    /// simply has none. Never a silent anonymous fallback.
    pub remote_mcp: Option<Arc<crate::agent_backend::mcp_client::RemoteMcpCatalogue>>,
}

/// Bridge for triggering an eval run from inside a tool handler.
///
/// Implemented in `src/handlers/eval.rs` (where AppState is in scope).
/// The library-side tools.rs can't see AppState directly, so we abstract
/// the trigger behind this trait. ToolContexts that have access to
/// AppState (workspace chat, /api/agents/:id/execute) populate it.
#[async_trait::async_trait]
pub trait EvalTrigger: Send + Sync {
    /// Trigger an eval run for the given agent. Returns the new run_id.
    /// `user_id` is required to charge the wallet; `judge` toggles the
    /// LlmJudgeEvaluator inside the registry; `tags` filters test cases.
    async fn trigger_eval(
        &self,
        agent_id: Uuid,
        user_id: String,
        judge: bool,
        tags: Vec<String>,
    ) -> Result<Uuid, String>;
}

/// A built-in tool definition
pub struct BuiltinToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: serde_json::Value,
    pub requires_workspace: bool,
    /// True for tools that invoke other agents (execute_agent, delegate_to_agent)
    pub is_delegation: bool,
}

impl Default for BuiltinToolDef {
    fn default() -> Self {
        Self {
            name: "",
            description: "",
            input_schema: json!({}),
            requires_workspace: false,
            is_delegation: false,
        }
    }
}

/// All 6 built-in tools
/// Every tool the compile-time dispatcher in [`ToolRegistry::execute`] can
/// actually run.
///
/// Public because `capabilities.mcp_tools` on an agent card is validated
/// against this list. A declared name with no dispatch arm is a **phantom
/// tool**: it is advertised to the model and over `/mcp/agents/:id`, gets
/// called, and returns `Unknown tool: X`. Historically nothing checked
/// this, so cards could assert capabilities that were never wired.
pub fn platform_tools() -> Vec<BuiltinToolDef> {
    builtin_tools()
}

/// Names only — the cheap form for validation.
pub fn platform_tool_names() -> Vec<&'static str> {
    builtin_tools().into_iter().map(|t| t.name).collect()
}

/// Every builtin tool as `(name, description)`.
///
/// For the contract builder, which turns a declared tool into a candidate
/// evidence block. The description is the load-bearing half: it is the tool
/// author's own statement of what the tool returns, which is real evidence
/// about document shape in a way a port label is not.
pub fn builtin_tool_catalogue() -> Vec<(&'static str, &'static str)> {
    builtin_tools()
        .into_iter()
        .map(|t| (t.name, t.description))
        .collect()
}

/// Arms that exist in `ToolRegistry::execute` but have no `BuiltinToolDef`.
///
/// Such a tool *runs* — card declarations carrying a schema are advertised to
/// the model verbatim and the arm dispatches — but `invalid_tool_declarations`
/// rejects the card on any write, so the agent cannot be re-saved through the
/// API without silently losing the capability.
///
/// This should stay empty. It exists so the condition is nameable rather than
/// invisible: `equity_analyst` carried nine such tools (`fmp_*`, fully
/// implemented since inception) and the only symptom was a confusing 400 on
/// republish.
const ARMS_WITHOUT_DEFS: &[&str] = &[];

/// Every tool name the runtime can actually dispatch.
///
/// Distinct from [`platform_tool_names`], which is what card *validation*
/// checks. Use this to answer "will this call succeed"; use that to answer
/// "can this card be saved".
pub fn dispatchable_tool_names() -> Vec<&'static str> {
    let mut names = platform_tool_names();
    names.extend_from_slice(ARMS_WITHOUT_DEFS);
    names
}

/// Why a declared tool name can't be published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolDeclarationError {
    /// No dispatch arm and no declared remote server owns the namespace.
    NotDispatchable,
    /// Namespaced like a remote MCP tool, but the agent declares no server
    /// by that name — so nothing would resolve it.
    UnknownRemoteServer { server: String },
}

impl std::fmt::Display for ToolDeclarationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotDispatchable => write!(
                f,
                "no platform tool by this name (would be advertised to the model and then fail \
                 with 'Unknown tool')"
            ),
            Self::UnknownRemoteServer { server } => write!(
                f,
                "looks like a remote MCP tool, but this agent declares no server named '{server}'"
            ),
        }
    }
}

/// Validate the tool names an agent wants to publish.
///
/// A name is publishable if it is either a platform tool, or a remote MCP
/// tool (`server__tool`) belonging to a server the agent declares. The
/// remote case is checked by namespace rather than by live discovery on
/// purpose: a save must not fail because a third-party endpoint happens to
/// be down.
///
/// Returns the names that would be phantom tools, each with a reason.
pub fn invalid_tool_declarations(
    declared: &[String],
    declared_servers: &[crate::agent_backend::mcp_client::RemoteMcpServer],
) -> Vec<(String, ToolDeclarationError)> {
    let builtins = platform_tool_names();
    declared
        .iter()
        .filter_map(|name| {
            if builtins.contains(&name.as_str()) {
                return None;
            }
            match name.split_once(crate::agent_backend::mcp_client::NS_SEP) {
                Some((ns, _)) if !ns.is_empty() => {
                    // Compare against the sanitised namespace the client
                    // actually generates, not the raw card `name`.
                    let known = declared_servers.iter().any(|s| s.namespace() == ns);
                    if known {
                        None
                    } else {
                        Some((
                            name.clone(),
                            ToolDeclarationError::UnknownRemoteServer {
                                server: ns.to_string(),
                            },
                        ))
                    }
                }
                _ => Some((name.clone(), ToolDeclarationError::NotDispatchable)),
            }
        })
        .collect()
}

fn builtin_tools() -> Vec<BuiltinToolDef> {
    let mut tools = builtin_tools_core();
    // Weather / prediction-market stack. Kept in its own module because it is
    // a coherent domain with its own research provenance, but registered here
    // so it goes through the same phantom-tool validation as everything else.
    tools.extend(crate::agent_backend::weather_tools::tool_defs());
    tools
}

fn builtin_tools_core() -> Vec<BuiltinToolDef> {
    vec![
        // ── Loop 3 Stage 0 — prospective coordination (mig-210) ──
        //
        // All six were declared on `intention_coordinator`'s card with full
        // schemas and no dispatch arm, so the agent has never functioned.
        // Descriptions and schemas below are the card's own, kept verbatim so
        // the two cannot disagree.
        BuiltinToolDef {
            name: "declare_intention",
            description: "Register an agent's planned next action in the workspace intention map. Accepts agent_id, action_type, tool (optional), description, and expected_output.",
            input_schema: json!({
                            "type": "object",
                            "properties": {
                                            "agent_id": {
                                                            "type": "string"
                                            },
                                            "action_type": {
                                                            "type": "string",
                                                            "enum": [
                                                                            "tool_call",
                                                                            "research",
                                                                            "synthesis",
                                                                            "writing",
                                                                            "review",
                                                                            "idle"
                                                            ]
                                            },
                                            "tool": {
                                                            "type": "string"
                                            },
                                            "description": {
                                                            "type": "string"
                                            },
                                            "expected_output": {
                                                            "type": "string"
                                            },
                                            "ttl_seconds": {
                                                            "type": "integer",
                                                            "default": 300
                                            }
                            },
                            "required": [
                                            "agent_id",
                                            "action_type",
                                            "description"
                            ]
            }),
            requires_workspace: true,
            ..Default::default()
        },
        // The propagation channel (mig-218). `declare_intention` lets an agent
        // register a plan; this one goes and asks for it. Placed next to its
        // sibling because the difference between them is the whole of Stage 0's
        // honesty: one records what you believe, the other records what was
        // said.
        BuiltinToolDef {
            name: "solicit_agent_plan",
            description: "Ask a workspace member what it intends to do next, and record its answer in the intention map as that agent's own plan (source=solicited). Returns the plan, the conflict signal against the rest of the map, and the agent's view of who should own what. Use this before declaring anything on a member's behalf: an intention you inferred from the transcript is your belief about that agent, and two such beliefs cannot be checked against each other.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "The member to ask, by agent name or id. Must be a member of this workspace."
                    },
                    "context": {
                        "type": "string",
                        "description": "Optional: what the workspace is trying to do right now, so the agent plans against the real goal rather than only against the transcript."
                    }
                },
                "required": ["agent_id"]
            }),
            requires_workspace: true,
            // It invokes another agent and spends against the caller's
            // credentials, so the anti-recursion tool set must be able to strip
            // it for the same reason it strips `execute_agent`.
            is_delegation: true,
        },
        BuiltinToolDef {
            name: "check_conflicts",
            description: "Check all active intentions for conflicts. Returns a list of conflict signals (CLEAR, OVERLAP_WARNING, CONFLICT_ALERT, DEPENDENCY_WAIT, BUDGET_GATE) with explanations.",
            input_schema: json!({
                            "type": "object",
                            "properties": {
                                            "agent_id": {
                                                            "type": "string",
                                                            "description": "Optional: check conflicts for a specific agent only"
                                            }
                            }
            }),
            requires_workspace: true,
            ..Default::default()
        },
        BuiltinToolDef {
            name: "get_intention_map",
            description: "Get the current intention map showing all active agent intentions, their statuses, and any flagged conflicts.",
            input_schema: json!({
                            "type": "object",
                            "properties": {}
            }),
            requires_workspace: true,
            ..Default::default()
        },
        BuiltinToolDef {
            name: "clear_intention",
            description: "Mark an agent's intention as completed or cancelled. Removes it from active conflict checking.",
            input_schema: json!({
                            "type": "object",
                            "properties": {
                                            "agent_id": {
                                                            "type": "string"
                                            },
                                            "status": {
                                                            "type": "string",
                                                            "enum": [
                                                                            "completed",
                                                                            "cancelled",
                                                                            "superseded"
                                                            ]
                                            }
                            },
                            "required": [
                                            "agent_id",
                                            "status"
                            ]
            }),
            requires_workspace: true,
            ..Default::default()
        },
        BuiltinToolDef {
            name: "suggest_differentiation",
            description: "When an overlap is detected, suggest how two agents can differentiate their work to avoid duplication. Uses context from both intentions.",
            input_schema: json!({
                            "type": "object",
                            "properties": {
                                            "agent_a": {
                                                            "type": "string"
                                            },
                                            "agent_b": {
                                                            "type": "string"
                                            }
                            },
                            "required": [
                                            "agent_a",
                                            "agent_b"
                            ]
            }),
            requires_workspace: true,
            ..Default::default()
        },
        BuiltinToolDef {
            name: "emit_coherence_signal",
            description: "Push an IntentionAligns or IntentionConflicts relation to the coherence system for incorporation into TEC evaluation.",
            input_schema: json!({
                            "type": "object",
                            "properties": {
                                            "relation_type": {
                                                            "type": "string",
                                                            "enum": [
                                                                            "IntentionAligns",
                                                                            "IntentionConflicts"
                                                            ]
                                            },
                                            "agent_a": {
                                                            "type": "string"
                                            },
                                            "agent_b": {
                                                            "type": "string"
                                            },
                                            "strength": {
                                                            "type": "number",
                                                            "minimum": 0,
                                                            "maximum": 1
                                            },
                                            "justification": {
                                                            "type": "string"
                                            }
                            },
                            "required": [
                                            "relation_type",
                                            "agent_a",
                                            "agent_b",
                                            "strength"
                            ]
            }),
            requires_workspace: true,
            ..Default::default()
        },
        // ── FPL simulation (in-process; also exposed over MCP) ──
        BuiltinToolDef {
            name: "fermi_execute_fpl",
            description: "Execute a Fermi FPL program. Runs a real Monte Carlo simulation (default 10,000 iterations, max 100,000) and returns mean, median, std_dev, p5, p25, p75, p95, min, max, base_rate and divergence figures.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "fpl_program": {"type": "string", "description": "A complete valid FPL program."},
                    "iterations": {"type": "integer", "description": "Monte Carlo iterations (default 10000, max 100000)."},
                    "seed": {"type": "integer", "description": "Optional seed for reproducibility."}
                },
                "required": ["fpl_program"]
            }),
            ..Default::default()
        },
        BuiltinToolDef {
            name: "fermi_sensitivity_analysis",
            description: "Run Sobol sensitivity analysis on an FPL program. Returns first-order and total-order indices per driver — real variance decomposition identifying which drivers actually drive output variance, with standard errors and confidence intervals.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "fpl_program": {"type": "string", "description": "The same FPL program passed to fermi_execute_fpl."},
                    "iterations": {"type": "integer", "description": "Iterations (default 5000, max 50000)."}
                },
                "required": ["fpl_program"]
            }),
            ..Default::default()
        },
        // ── Financial Modeling Prep — implemented, never declared ──
        //
        // `execute_fmp_api` has dispatched these since `equity_analyst`
        // shipped, so they have always worked when the model called them. They
        // were simply absent from `builtin_tools()`, which is what card
        // validation checks — so any write to `equity_analyst` through the API
        // would be rejected, or would strip the tools. Descriptions and schemas
        // below are the card's own, kept verbatim so the two cannot disagree.
        BuiltinToolDef {
            name: "fmp_company_profile",
            description: "Get company profile including price, market cap, sector, industry, beta, 52-week range, CEO, description. Use this first to identify the company and get current market data.",
            input_schema: json!({
                            "type": "object",
                            "properties": {
                                            "symbol": {
                                                            "type": "string",
                                                            "description": "Stock ticker symbol (e.g., AAPL, MSFT, GOOGL, TSLA)"
                                            }
                            },
                            "required": [
                                            "symbol"
                            ]
            }),
            ..Default::default()
        },
        BuiltinToolDef {
            name: "fmp_income_statement",
            description: "Get income statement data: revenue, gross profit, operating income, net income, EPS, EBITDA. Essential for growth analysis and profitability assessment.",
            input_schema: json!({
                            "type": "object",
                            "properties": {
                                            "symbol": {
                                                            "type": "string",
                                                            "description": "Stock ticker symbol"
                                            },
                                            "period": {
                                                            "type": "string",
                                                            "enum": [
                                                                            "annual",
                                                                            "quarter"
                                                            ],
                                                            "description": "Reporting period (annual or quarter)"
                                            },
                                            "limit": {
                                                            "type": "integer",
                                                            "description": "Number of periods to return (default 3)",
                                                            "default": 3
                                            }
                            },
                            "required": [
                                            "symbol",
                                            "period"
                            ]
            }),
            ..Default::default()
        },
        BuiltinToolDef {
            name: "fmp_balance_sheet",
            description: "Get balance sheet data: assets, liabilities, equity, cash, debt, inventory. Essential for financial health and leverage analysis.",
            input_schema: json!({
                            "type": "object",
                            "properties": {
                                            "symbol": {
                                                            "type": "string",
                                                            "description": "Stock ticker symbol"
                                            },
                                            "period": {
                                                            "type": "string",
                                                            "enum": [
                                                                            "annual",
                                                                            "quarter"
                                                            ],
                                                            "description": "Reporting period"
                                            },
                                            "limit": {
                                                            "type": "integer",
                                                            "description": "Number of periods to return",
                                                            "default": 3
                                            }
                            },
                            "required": [
                                            "symbol",
                                            "period"
                            ]
            }),
            ..Default::default()
        },
        BuiltinToolDef {
            name: "fmp_cash_flow",
            description: "Get cash flow statement: operating cash flow, capex, free cash flow, buybacks, dividends. Essential for cash generation quality and capital allocation analysis.",
            input_schema: json!({
                            "type": "object",
                            "properties": {
                                            "symbol": {
                                                            "type": "string",
                                                            "description": "Stock ticker symbol"
                                            },
                                            "period": {
                                                            "type": "string",
                                                            "enum": [
                                                                            "annual",
                                                                            "quarter"
                                                            ],
                                                            "description": "Reporting period"
                                            },
                                            "limit": {
                                                            "type": "integer",
                                                            "description": "Number of periods to return",
                                                            "default": 3
                                            }
                            },
                            "required": [
                                            "symbol",
                                            "period"
                            ]
            }),
            ..Default::default()
        },
        BuiltinToolDef {
            name: "fmp_ratios",
            description: "Get pre-calculated financial ratios: profitability margins, liquidity ratios, leverage ratios, valuation multiples (P/E, P/B, P/S, EV/EBITDA, PEG), efficiency ratios, dividend yield.",
            input_schema: json!({
                            "type": "object",
                            "properties": {
                                            "symbol": {
                                                            "type": "string",
                                                            "description": "Stock ticker symbol"
                                            },
                                            "period": {
                                                            "type": "string",
                                                            "enum": [
                                                                            "annual",
                                                                            "quarter"
                                                            ],
                                                            "description": "Reporting period"
                                            },
                                            "limit": {
                                                            "type": "integer",
                                                            "description": "Number of periods to return",
                                                            "default": 3
                                            }
                            },
                            "required": [
                                            "symbol",
                                            "period"
                            ]
            }),
            ..Default::default()
        },
        BuiltinToolDef {
            name: "fmp_key_metrics",
            description: "Get key financial metrics: market cap, enterprise value, EV/EBITDA, EV/Sales, ROE, ROA, ROIC, FCF yield, debt/equity, earnings yield, book value per share, Graham number.",
            input_schema: json!({
                            "type": "object",
                            "properties": {
                                            "symbol": {
                                                            "type": "string",
                                                            "description": "Stock ticker symbol"
                                            },
                                            "period": {
                                                            "type": "string",
                                                            "enum": [
                                                                            "annual",
                                                                            "quarter"
                                                            ],
                                                            "description": "Reporting period"
                                            },
                                            "limit": {
                                                            "type": "integer",
                                                            "description": "Number of periods to return",
                                                            "default": 3
                                            }
                            },
                            "required": [
                                            "symbol",
                                            "period"
                            ]
            }),
            ..Default::default()
        },
        BuiltinToolDef {
            name: "fmp_dcf",
            description: "Get discounted cash flow (DCF) intrinsic value estimate vs current stock price. Shows whether the stock is over- or under-valued based on fundamental analysis.",
            input_schema: json!({
                            "type": "object",
                            "properties": {
                                            "symbol": {
                                                            "type": "string",
                                                            "description": "Stock ticker symbol"
                                            }
                            },
                            "required": [
                                            "symbol"
                            ]
            }),
            ..Default::default()
        },
        BuiltinToolDef {
            name: "fmp_analyst_estimates",
            description: "Get Wall Street analyst consensus estimates: revenue, EBITDA, EBIT, net income, EPS (low/avg/high) with number of analysts. Forward-looking data for 1-5 years.",
            input_schema: json!({
                            "type": "object",
                            "properties": {
                                            "symbol": {
                                                            "type": "string",
                                                            "description": "Stock ticker symbol"
                                            },
                                            "period": {
                                                            "type": "string",
                                                            "enum": [
                                                                            "annual",
                                                                            "quarter"
                                                            ],
                                                            "description": "Reporting period"
                                            },
                                            "limit": {
                                                            "type": "integer",
                                                            "description": "Number of estimate periods to return",
                                                            "default": 5
                                            }
                            },
                            "required": [
                                            "symbol",
                                            "period"
                            ]
            }),
            ..Default::default()
        },
        BuiltinToolDef {
            name: "fmp_historical_price",
            description: "Get historical daily price data (OHLCV) for a date range. Useful for trend analysis, volatility assessment, and price momentum.",
            input_schema: json!({
                            "type": "object",
                            "properties": {
                                            "symbol": {
                                                            "type": "string",
                                                            "description": "Stock ticker symbol"
                                            },
                                            "from": {
                                                            "type": "string",
                                                            "description": "Start date in YYYY-MM-DD format"
                                            },
                                            "to": {
                                                            "type": "string",
                                                            "description": "End date in YYYY-MM-DD format"
                                            }
                            },
                            "required": [
                                            "symbol"
                            ]
            }),
            ..Default::default()
        },
        // ── Loop 5: the router's read path onto measured calibration ──
        //
        // Declared on three strategist cards long before this entry existed.
        // A dispatch arm alone is not enough: cards are validated against
        // `builtin_tools()`, so a tool missing from this list is a phantom
        // tool even when the arm is present.
        BuiltinToolDef {
            name: "get_agent_calibration",
            description: "Get an agent's measured calibration profile — how accurately its outputs have been validated against ground truth over time. Returns calibration_score, brier_skill_score (performance against a base-rate forecaster — gate routing decisions on this, not on calibration_score, which is inflated by outcome-skewed question sets), trend, evidence_class, n_resolved_forecasts, projection_accuracy_mean and a domain_calibration breakdown.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "The agent_id or agent_name to get calibration for."
                    }
                },
                "required": ["agent_id"]
            }),
            ..Default::default()
        },
        // ── Loop 3: the strategist writes into a member's memory ──
        BuiltinToolDef {
            name: "record_coordination_observation",
            description: "Write a coordination observation into a member agent's episodic memory, so it is consolidated into a semantic rule on that agent's next dreaming cycle. This is how coordination feedback becomes durable learning rather than one-off advice. Use for: what coherence role the agent played, where it duplicated another member, which evidence it left unengaged, what it could do differently. Only the workspace's coordination strategist may call this, and only for current members.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "The member agent (agent_id or agent_name) whose memory to write into."
                    },
                    "observation": {
                        "type": "string",
                        "description": "The observation, addressed to that agent. Structural and specific — name the pattern and cite what happened, do not prescribe a fix."
                    },
                    "session_summary": {
                        "type": "string",
                        "description": "Optional context about the session this observation came from."
                    }
                },
                "required": ["agent_id", "observation"]
            }),
            requires_workspace: true,
            ..Default::default()
        },
        // ── Loop 3b / 4: the strategist raises a composition proposal ──
        BuiltinToolDef {
            name: "propose_composition_change",
            description: "Propose a structural change to the workspace composition. Creates a pending composition_versions row for the workspace owner to accept or reject. Use ONLY when dreaming has identified a persistent structural issue — valence homophily, chronic destructive incoherence, or a role gap. Provide diff_summary and rationale. Do NOT specify which agent to add; that is the owner's decision.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "diff_summary": {
                        "type": "string",
                        "description": "Plain-language description of what should change and why."
                    },
                    "rationale": {
                        "type": "string",
                        "description": "Which episodes, principle patterns and valence distribution drove this."
                    },
                    "homophily_detected": {
                        "type": "boolean",
                        "description": "True when the valence audit found arousal or valence spread < 0.25."
                    }
                },
                "required": ["diff_summary", "rationale"]
            }),
            requires_workspace: true,
            ..Default::default()
        },
        BuiltinToolDef {
            name: "search_knowledge",
            description: "Search the agent's episodic memory for relevant past experiences using semantic similarity. Returns the most relevant episodes.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query to find relevant knowledge"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 5)",
                        "default": 5
                    }
                },
                "required": ["query"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "query_ontology",
            description: "Query the agent's knowledge graph to retrieve semantic rules, entities, and facts. Specify which types to include.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "include_rules": {
                        "type": "boolean",
                        "description": "Include semantic rules (default: true)",
                        "default": true
                    },
                    "include_entities": {
                        "type": "boolean",
                        "description": "Include entities (default: true)",
                        "default": true
                    },
                    "include_facts": {
                        "type": "boolean",
                        "description": "Include facts/relationships (default: true)",
                        "default": true
                    }
                },
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
             name: "execute_agent",
             description: "Invoke another agent with a query and get its response. When workspace_id is provided, the sub-agent runs inside that workspace's full context (cross-workspace delegation — used for Rabble creatures to consume kask-app workspaces). Without workspace_id, the sub-agent runs a single turn without tools.\n\n\
                           READ `envelope` BEFORE YOU WEIGH THE ANSWER. The \
                           result carries an `envelope` describing what actually \
                           crossed the hop, and `envelope.validation.status` is \
                           the member's document checked against the type that \
                           member itself declared:\n\
                           · `valid` — checked and conforming. Use \
                           `envelope.payload`, which is the member's own typed \
                           document and is better than re-reading its prose.\n\
                           · `invalid` — the document CONTRADICTS the type its \
                           producer declared; `violations` names the paths. Do \
                           not silently average it in. Discount it, say in your \
                           output that you did, and prefer another member or \
                           another route for this kind of task.\n\
                           · `unverified_no_schema` / `unverified_no_payload` / \
                           `unverified_unsupported_schema` — NOT a pass. Nothing \
                           was checked, because the member declares no type, \
                           returned prose, or declared a schema the validator \
                           cannot evaluate. Treat it as unverified evidence and \
                           weight it below a `valid` member.\n\
                           Also check `envelope.provenance.blocks`: a \
                           `tool_verified` value is a measurement, \
                           `model_inference` is a judgement, and combining them \
                           as if they were the same kind of number is how a \
                           coordinator launders a guess into a result. \
                           `grounding_enforced: false` means nobody has written a \
                           grounding contract for that member — an absence, not a \
                           clean bill of health.",
             input_schema: json!({
                 "type": "object",
                 "properties": {
                     "agent_id": {
                         "type": "string",
                         "description": "The agent_id of the agent to invoke (e.g. 'forage_scout', 'wild_companion')"
                     },
                     "agent_name": {
                         "type": "string",
                         "description": "Alias for agent_id (legacy parameter name)"
                     },
                     "query": {
                         "type": "string",
                         "description": "The query to send to the agent"
                     },
                     "workspace_id": {
                         "type": "string",
                         "description": "Optional: UUID of the target workspace. When provided, the agent runs inside that workspace's context with full tool access (cross-workspace delegation). Used for Rabble creatures to consume kask-app-wild or other app workspaces."
                     }
                 },
                 "required": ["query"]
             }),
             requires_workspace: false,
             is_delegation: true,
         },
        BuiltinToolDef {
            name: "delegate_to_agent",
            description: "Delegate a task to another workspace agent who will execute with full tool access (image generation, file writing, etc). The delegation appears as a visible message in workspace chat. Use this instead of execute_agent when the target agent needs tools to do its work.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_name": {
                        "type": "string",
                        "description": "The name of the workspace agent to delegate to"
                    },
                    "task": {
                        "type": "string",
                        "description": "The task description for the target agent"
                    }
                },
                "required": ["agent_name", "task"]
            }),
            requires_workspace: true,
            is_delegation: true,
        },
        // Contract validation, so an author can iterate against the SAME
        // checker the publish gate runs rather than against a description of
        // it. A guide that merely describes the rules drifts from them; this
        // calls `card_contract::validate` directly.
        // The tool that turns genome_profiler's `unavailable_no_tool_source`
        // fields into `tool_verified` ones. See src/agent_backend/ncbi_tools.rs
        // for why `ploidy` is deliberately NOT among them.
        BuiltinToolDef {
            name: "ncbi_genome_search",
            description: "Look up assembled genome statistics for a species from NCBI \
                          Assembly: genome size in Mb and assembled chromosome count, with \
                          the assembly name and accession that supplied them. Returns \
                          found=false for unsequenced species — most insects — which is a \
                          real answer, not an error.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "scientific_name": {
                        "type": "string",
                        "description": "Species binomial, e.g. 'Danaus plexippus'"
                    }
                },
                "required": ["scientific_name"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "validate_agent_card",
            description: "Check a draft agent card against the publish contract: typed \
                          output schema, ports that reference the declared type, and a \
                          grounding entry per output field saying where its value comes \
                          from. Returns every finding with the fix, or confirms it would \
                          publish. Use before proposing a card to a developer.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Id of the agent being authored (checked against the grandfathering list)"
                    },
                    "output_contract": {
                        "type": "object",
                        "description": "The draft `capabilities.output_contract`: produces_schema, schema, grounding"
                    },
                    "produces": {
                        "type": "array", "items": { "type": "string" },
                        "description": "Draft `produces` ports; each must equal the declared type name"
                    },
                    "tool_names": {
                        "type": "array", "items": { "type": "string" },
                        "description": "Tools the agent declares. A field marked `sourced` must name one of these."
                    }
                },
                "required": ["agent_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "build_output_contract",
            description: "Compile a short SKETCH into a complete, publishable typed \
                          output contract. You declare the three things that need \
                          judgement — the evidence blocks, their fields and types, and \
                          where each block's value comes from plus why — and this emits \
                          the JSON Schema, the narrowed per-block `_provenance` enums, \
                          the grounding map and the rewritten `produces`. Prefer this \
                          over hand-writing a contract: it emits schema and grounding \
                          from one pass, so they cannot disagree, and it refuses to \
                          return anything the publish gate would reject. It will NOT \
                          invent a `why`, and a block claiming to be `sourced` from a \
                          tool absent from `tool_names` is refused.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "sketch": {
                        "type": "object",
                        "description": "{domain, produces_schema (namespaced, e.g. \
                                        `myapp/risk_assessment`), title?, description?, \
                                        synthesis?, calibration?, blocks: [{name, \
                                        source: {status: sourced|inferred|narrative|unavailable, \
                                        tool?, response_field?, coverage?: complete|partial|deferred, \
                                        from?, would_need?}, why (40+ chars, never generated), \
                                        fields?: {name: type}, value?: type, required?}]}. \
                                        Type syntax: string|integer|number|boolean|object, \
                                        `enum:a|b|c`, `const:v`, or `@entity` to take the \
                                        type from the ontology; suffix `[]` for array then \
                                        `?` for nullable, in that order. `minimum`/`pattern` \
                                        are deliberately unavailable — the platform validator \
                                        cannot evaluate them, and a schema it cannot evaluate \
                                        reports `unverified`, which is not a pass."
                    },
                    "tool_names": {
                        "type": "array", "items": { "type": "string" },
                        "description": "Tools the agent declares in `capabilities.mcp_tools`. Cross-checked: a `sourced` block must name one of these."
                    },
                    "ontology": {
                        "type": "object",
                        "description": "Optional agent ontology ({entities: [{id, properties: {definition, scale|categories}}]}). Resolves `@entity` field types so vocabulary is selected rather than reinvented."
                    }
                },
                "required": ["sketch"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "list_agents",
            description: "List all available agents in the registry with their names, types, and descriptions.",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "read_workspace_file",
            description: "Read a file from the current workspace's git repository.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The file path relative to workspace root"
                    }
                },
                "required": ["path"]
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "read_workspace_output",
            description: "Read a typed output from any workspace. Use this to consume results published by upstream workspaces (e.g., team prior → tournament path). Returns the output value, version, and last update time.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "workspace_id": {
                        "type": "string",
                        "description": "UUID of the workspace to read from"
                    },
                    "key": {
                        "type": "string",
                        "description": "Output key, e.g. 'predicted_probability', 'driver_scores', 'sobol_indices'"
                    }
                },
                "required": ["workspace_id", "key"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "list_workspace_outputs",
            description: "List all published outputs for a workspace. Returns keys, values, versions, and update times.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "workspace_id": {
                        "type": "string",
                        "description": "UUID of the workspace to list outputs from"
                    }
                },
                "required": ["workspace_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "list_workspace_agents",
            description: "List all agents that are members of the current workspace.",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "generate_image",
            description: "Generate an image from a text prompt using Gemini. Returns the image as base64-encoded data with its MIME type.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "Text description of the image to generate"
                    }
                },
                "required": ["prompt"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "edit_image",
            description: "Edit/transform an image using a text prompt and a reference image URL via Gemini. Useful for style transfer, modifications, and artistic transformations. Returns the edited image as base64-encoded data.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "Text description of the desired edit/transformation"
                    },
                    "image_url": {
                        "type": "string",
                        "description": "URL of the source image to edit"
                    }
                },
                "required": ["prompt", "image_url"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "reduct_list_projects",
            description: "List all projects in the Reduct.video workspace. Returns project IDs, titles, and metadata.",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "reduct_get_project",
            description: "Get details of a Reduct.video project including its recordings and reels.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": {
                        "type": "string",
                        "description": "The Reduct project ID"
                    }
                },
                "required": ["project_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "reduct_get_transcript",
            description: "Get the transcript of a recording in a Reduct.video project. Returns segments with start/end timestamps and speaker labels.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": {
                        "type": "string",
                        "description": "The Reduct project ID"
                    },
                    "recording_id": {
                        "type": "string",
                        "description": "The recording ID within the project"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["json", "txt"],
                        "description": "Transcript format. 'json' carries per-segment start/end timestamps and is the only form clip boundaries may be taken from; 'txt' is prose with no timestamps. Default: json",
                        "default": "json"
                    }
                },
                "required": ["project_id", "recording_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "reduct_create_reel",
            description: "Create a new reel (highlight compilation) in a Reduct.video project. Returns the new reel ID.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": {
                        "type": "string",
                        "description": "The Reduct project ID"
                    },
                    "title": {
                        "type": "string",
                        "description": "Title for the new reel"
                    }
                },
                "required": ["project_id", "title"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "reduct_add_block",
            description: "Add a block to a Reduct.video reel. Use type 'doc-range' for video clips (requires recording_id, start, end times) or type 'title' for title cards (requires text).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": {
                        "type": "string",
                        "description": "The Reduct project ID"
                    },
                    "reel_id": {
                        "type": "string",
                        "description": "The reel ID to add the block to"
                    },
                    "block_type": {
                        "type": "string",
                        "description": "Block type: 'doc-range' for video clip, 'title' for title card"
                    },
                    "recording_id": {
                        "type": "string",
                        "description": "Recording ID (required for doc-range blocks)"
                    },
                    "start": {
                        "type": "number",
                        "description": "Start time in SECONDS as a number, e.g. 412.6 (required for doc-range blocks). Not a timecode string: '6:52' is rejected."
                    },
                    "end": {
                        "type": "number",
                        "description": "End time in SECONDS as a number, e.g. 448.2 (required for doc-range blocks). Must be greater than start."
                    },
                    "text": {
                        "type": "string",
                        "description": "Title text (required for title blocks)"
                    }
                },
                "required": ["project_id", "reel_id", "block_type"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "write_workspace_file",
            description: "Write a file to the current workspace's git repository. For binary files (images), provide base64-encoded content and set is_base64 to true.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to workspace root (e.g. outputs/result.png)"
                    },
                    "content": {
                        "type": "string",
                        "description": "File content as text, or base64-encoded string for binary files"
                    },
                    "is_base64": {
                        "type": "boolean",
                        "description": "If true, content is base64-encoded binary data (default: false)",
                        "default": false
                    },
                    "commit_message": {
                        "type": "string",
                        "description": "Git commit message (default: auto-generated)",
                        "default": ""
                    }
                },
                "required": ["path", "content"]
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        // ─── Voice tools ───
        BuiltinToolDef {
            name: "speak_text",
            description: "Convert text to natural speech using Cartesia Sonic. Returns audio as base64-encoded PCM data.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Text to convert to speech (max 5000 characters)"
                    },
                    "voice": {
                        "type": "string",
                        "description": "Voice style: narrator (British), conversational (friendly), or storyteller (calm)",
                        "enum": ["narrator", "conversational", "storyteller"],
                        "default": "narrator"
                    }
                },
                "required": ["text"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        // ─── Coherence tools ───
        BuiltinToolDef {
            name: "evaluate_coherence",
            description: "Run a Thagard Explanatory Coherence (TEC) evaluation on recent workspace messages. Classifies utterances, detects coherence/incoherence relations, runs constraint-satisfaction settling, and returns global score, 7 principle scores, and health indicators. Results are stored for history.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message_limit": {
                        "type": "integer",
                        "description": "Number of recent messages to evaluate (default: 50, max: 100)",
                        "default": 50
                    }
                },
                "required": []
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "coherence_snapshot",
            description: "Get the latest stored coherence evaluation for the workspace without running a new evaluation. Returns global score, quality label, principle scores, and health indicators from the most recent evaluation.",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "get_workspace_messages",
            description: "Read recent messages from the workspace conversation. Returns messages with sender name, content, type, and timestamp.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of messages to return (default: 20, max: 50)",
                        "default": 20
                    }
                },
                "required": []
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        // ─── Marketplace tools ───
        BuiltinToolDef {
            name: "get_shopping_profile",
            description: "Retrieve the current user's shopping preference profile for a given agent. Returns metadata, category tags, brand affinities, price sensitivity, and quality bias. Never exposes raw embeddings.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "profile_name": {
                        "type": "string",
                        "description": "Name of the shopping profile (e.g. 'electronics', 'fitness'). Default: 'default'",
                        "default": "default"
                    }
                },
                "required": []
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "update_shopping_profile",
            description: "Recompute the composite shopping embedding from recent episodes and update profile metadata (brand affinities, price sensitivity, quality bias, category tags). The embedding is computed server-side as a weighted centroid of episode embeddings.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "profile_name": {
                        "type": "string",
                        "description": "Name of the shopping profile to update. Default: 'default'",
                        "default": "default"
                    },
                    "category_tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Category tags for the profile (e.g. ['electronics', 'espresso', 'kitchen'])"
                    },
                    "price_sensitivity": {
                        "type": "number",
                        "description": "Price sensitivity score 0.0 (price insensitive) to 1.0 (very price sensitive)"
                    },
                    "quality_bias": {
                        "type": "number",
                        "description": "Quality bias score 0.0 (value-focused) to 1.0 (premium-focused)"
                    },
                    "brand_affinities": {
                        "type": "object",
                        "description": "Brand affinity scores, e.g. {\"nike\": 0.85, \"breville\": 0.72}"
                    }
                },
                "required": []
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "list_marketplace",
            description: "Browse active marketplace listings where consumers have listed their shopping profiles for advertiser queries. Filter by category. Returns listing metadata and pricing — never raw embeddings.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "category": {
                        "type": "string",
                        "description": "Comma-separated category filter (e.g. 'electronics,kitchen')"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum listings to return (default: 20)",
                        "default": 20
                    }
                },
                "required": []
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        // ─── AR Spatial Suite tools ───
        BuiltinToolDef {
            name: "h3_resolve",
            description: "Convert GPS coordinates to an H3 hexagonal grid cell ID, or convert an H3 cell ID back to GPS coordinates. Also computes k-ring neighbors and grid distance between cells. The foundation for all AR spatial operations.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["gps_to_h3", "h3_to_gps", "neighbors", "distance", "grid_disk"],
                        "description": "Operation: gps_to_h3 (lat/lng→cell), h3_to_gps (cell→lat/lng), neighbors (6 adjacent cells), distance (grid distance between 2 cells), grid_disk (all cells within k rings)"
                    },
                    "lat": {
                        "type": "number",
                        "description": "Latitude in decimal degrees (for gps_to_h3)"
                    },
                    "lng": {
                        "type": "number",
                        "description": "Longitude in decimal degrees (for gps_to_h3)"
                    },
                    "h3_cell": {
                        "type": "string",
                        "description": "H3 cell ID (for h3_to_gps, neighbors, distance)"
                    },
                    "h3_cell_b": {
                        "type": "string",
                        "description": "Second H3 cell ID (for distance operation)"
                    },
                    "resolution": {
                        "type": "integer",
                        "description": "H3 resolution 0-15 (default: 12, ~9m² hexes). Higher = more precise.",
                        "default": 12
                    },
                    "k": {
                        "type": "integer",
                        "description": "Ring count for grid_disk (default: 1). Total cells = 3k²+3k+1",
                        "default": 1
                    }
                },
                "required": ["operation"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "geocode",
            description: "Convert a street address or place name to GPS coordinates (lat/lng) using OpenStreetMap Nominatim. Free, no API key required. Rate limited to 1 request per second.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "address": {
                        "type": "string",
                        "description": "Street address or place name to geocode (e.g. '221B Baker Street, London' or 'Eiffel Tower')"
                    }
                },
                "required": ["address"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "create_beacon",
            description: "Create an AR beacon — place an AR asset at a physical location. Stores the beacon in the database with H3 cell, orientation, TTL, and interaction triggers. Returns the beacon record with its public asset URL.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "lat": {
                        "type": "number",
                        "description": "Latitude of placement"
                    },
                    "lng": {
                        "type": "number",
                        "description": "Longitude of placement"
                    },
                    "resolution": {
                        "type": "integer",
                        "description": "H3 resolution (default: 12)",
                        "default": 12
                    },
                    "asset_path": {
                        "type": "string",
                        "description": "Path to asset in workspace files (e.g. 'ar_assets/portal.png')"
                    },
                    "asset_type": {
                        "type": "string",
                        "description": "Asset type: image, model, video (default: image)",
                        "default": "image"
                    },
                    "azimuth_deg": {
                        "type": "number",
                        "description": "Compass bearing the asset faces, 0-360 (default: 0 = North)",
                        "default": 0
                    },
                    "elevation_deg": {
                        "type": "number",
                        "description": "Vertical tilt, -90 to 90 (default: 0 = eye level)",
                        "default": 0
                    },
                    "billboard": {
                        "type": "boolean",
                        "description": "If true, asset always faces the viewer (default: true)",
                        "default": true
                    },
                    "scale": {
                        "type": "number",
                        "description": "Scale factor (default: 1.0)",
                        "default": 1.0
                    },
                    "ttl_seconds": {
                        "type": "integer",
                        "description": "Time-to-live in seconds (default: 86400 = 24 hours)",
                        "default": 86400
                    },
                    "decay_style": {
                        "type": "string",
                        "description": "Decay style: fade, dissolve, instant, loop_decay (default: fade)",
                        "default": "fade"
                    },
                    "visibility": {
                        "type": "string",
                        "description": "Visibility: public, private, workspace (default: public)",
                        "default": "public"
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Tags for the beacon"
                    },
                    "interaction": {
                        "type": "object",
                        "description": "Interaction triggers: on_gaze, on_tap, on_proximity, on_dwell"
                    }
                },
                "required": ["lat", "lng", "asset_path"]
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "query_beacons",
            description: "Query AR beacons near a location. Returns all active (non-expired) beacons within k rings of the specified H3 cell. Used by renderers to discover nearby AR content.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "lat": {
                        "type": "number",
                        "description": "Latitude to search around"
                    },
                    "lng": {
                        "type": "number",
                        "description": "Longitude to search around"
                    },
                    "h3_cell": {
                        "type": "string",
                        "description": "H3 cell to search around (alternative to lat/lng)"
                    },
                    "radius_rings": {
                        "type": "integer",
                        "description": "Search radius in H3 rings (default: 3)",
                        "default": 3
                    },
                    "resolution": {
                        "type": "integer",
                        "description": "H3 resolution (default: 12)",
                        "default": 12
                    },
                    "include_expired": {
                        "type": "boolean",
                        "description": "Include expired beacons (default: false)",
                        "default": false
                    }
                },
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "save_grid_map",
            description: "Save or update an AR grid map — a named spatial grid with quadrants and zones. Used by ar_cartographer to persist grid definitions to the database.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Human-readable name for the grid map"
                    },
                    "description": {
                        "type": "string",
                        "description": "Description of the space"
                    },
                    "center_lat": {
                        "type": "number",
                        "description": "Center latitude"
                    },
                    "center_lng": {
                        "type": "number",
                        "description": "Center longitude"
                    },
                    "grid_resolution": {
                        "type": "integer",
                        "description": "H3 resolution for placement grid (default: 12)",
                        "default": 12
                    },
                    "radius_rings": {
                        "type": "integer",
                        "description": "Grid radius in rings (default: 5)",
                        "default": 5
                    },
                    "quadrants": {
                        "type": "array",
                        "description": "Named quadrant definitions [{h3_cell, name, description, tags, color}]"
                    },
                    "zones": {
                        "type": "array",
                        "description": "Zone groupings [{name, description, quadrants: [names], color}]"
                    }
                },
                "required": ["name", "center_lat", "center_lng"]
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        // ─── Rabble.world creature tools ───
        BuiltinToolDef {
            name: "gbif_species_search",
            description: "Call this tool to query GBIF (Global Biodiversity Information Facility) for species data. This tool is executed server-side — you do not need internet access to use it. Returns real taxonomy, common names, and media from the live GBIF API.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Species name (common or scientific) to search for"
                    },
                    "gbif_key": {
                        "type": "integer",
                        "description": "Specific GBIF species key for direct lookup"
                    },
                    "rank": {
                        "type": "string",
                        "description": "Taxonomic rank filter: SPECIES, GENUS, FAMILY (default: SPECIES)",
                        "default": "SPECIES"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max results (default: 5)",
                        "default": 5
                    },
                    "scope": {
                        "type": "string",
                        "description": "Named taxonomic scope to search within. One of: insecta (default), plantae, fungi, animalia, aves, lepidoptera, hymenoptera, magnoliopsida. Omit to keep the historical insect-only behaviour. An unrecognised name is an error, not a fallback."
                    },
                    "higher_taxon_key": {
                        "type": "integer",
                        "description": "GBIF backbone key to scope the search to, for a taxon `scope` does not name. Takes precedence over `scope`. Defaults to 216 (Insecta)."
                    }
                },
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "mint_creature",
            description: "Store a minted creature in the database. Creates the creature record with species data, asset path, variation notes, and generates a specimen name. Returns the creature ID and data card.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "scientific_name": {
                        "type": "string",
                        "description": "Scientific name of the species"
                    },
                    "common_name": {
                        "type": "string",
                        "description": "Common name (e.g. 'Red Admiral')"
                    },
                    "species_group": {
                        "type": "string",
                        "description": "Group: butterfly, dragonfly (default: butterfly)",
                        "default": "butterfly"
                    },
                    "gbif_key": {
                        "type": "integer",
                        "description": "GBIF species key for reference"
                    },
                    "taxonomy": {
                        "type": "object",
                        "description": "Full taxonomy object (kingdom through species)"
                    },
                    "asset_path": {
                        "type": "string",
                        "description": "Path to the specimen image in workspace files"
                    },
                    "flight_silhouette_path": {
                        "type": "string",
                        "description": "Path to the flight-pose image (optional)"
                    },
                    "specimen_name": {
                        "type": "string",
                        "description": "Unique name for this specimen (e.g. 'Twilight Admiral')"
                    },
                    "variation_notes": {
                        "type": "string",
                        "description": "Description of what makes this specimen unique"
                    }
                },
                "required": ["scientific_name", "asset_path"]
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "generate_specimen_art",
            description: "Generate a unique naturalist illustration for a creature using Gemini image generation. Fetches GBIF reference media for the species, then generates a stylized scientific illustration. Saves the image to static/creatures/ and updates the creature record.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "creature_id": {
                        "type": "string",
                        "description": "UUID of the creature to generate art for"
                    },
                    "scientific_name": {
                        "type": "string",
                        "description": "Scientific name (used for GBIF lookup and prompt). Required if creature_id not provided."
                    },
                    "common_name": {
                        "type": "string",
                        "description": "Common name for prompt enrichment"
                    },
                    "species_group": {
                        "type": "string",
                        "description": "butterfly or dragonfly — affects illustration style"
                    },
                    "style": {
                        "type": "string",
                        "description": "Art style hint: 'naturalist' (default), 'watercolor', 'botanical', 'field-guide', 'ukiyo-e'",
                        "default": "naturalist"
                    },
                    "gbif_key": {
                        "type": "integer",
                        "description": "GBIF species key for reference media lookup"
                    }
                },
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "segment_creature_wings",
            description: "Segment a butterfly creature's minted image into animation layers (body, left wing, right wing) using Gemini image editing. Stores layers in the database for client-side parametric wing animation. Only works for butterfly species. Costs creature_animate credits.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "creature_id": {
                        "type": "string",
                        "description": "UUID of the butterfly creature to segment into animation layers"
                    }
                },
                "required": ["creature_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "activate_formation",
            description: "Activate a premium swarm formation algorithm for a rabble. Charges credits based on the algorithm's cost. Returns the formation spec JSON for client-side execution in the SwarmEngine. Idempotent: re-activating the same algorithm in the same session returns the spec without double-charging.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "algorithm_name": {
                        "type": "string",
                        "description": "Algorithm name (e.g. 'v_formation', 'echelon', 'encircle', 'patrol', 'search')"
                    },
                    "swarm_id": {
                        "type": "string",
                        "description": "Rabble/swarm session UUID"
                    }
                },
                "required": ["algorithm_name", "swarm_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "create_listing",
            description: "List a shopping profile on the embedding marketplace so advertisers can run similarity queries against it. The consumer sets the price per query and can delist at any time. Costs a one-time listing fee.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "profile_name": {
                        "type": "string",
                        "description": "Name of the shopping profile to list. Default: 'default'",
                        "default": "default"
                    },
                    "price_credits": {
                        "type": "integer",
                        "description": "Credits to charge per advertiser query (min 1)"
                    },
                    "max_queries_per_buyer": {
                        "type": "integer",
                        "description": "Optional cap on queries per buyer (privacy control)"
                    },
                    "category_tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Category tags for marketplace discovery"
                    },
                    "description": {
                        "type": "string",
                        "description": "Public description of this listing"
                    }
                },
                "required": ["price_credits"]
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        // ─── Enemy Sensor ───
        BuiltinToolDef {
             name: "scan_nearby_creatures",
             description: "Call this tool to find creatures near a given creature using H3 proximity. This tool is executed server-side against the live Rabble database — you do not need internet access to use it. Returns the target creature's species and all nearby creatures with taxonomy data.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "creature_id": {
                        "type": "string",
                        "description": "UUID of the creature to scan around"
                    },
                    "radius_rings": {
                        "type": "integer",
                        "description": "H3 grid ring radius (default: 1, i.e. 7 cells at res 12)",
                        "default": 1
                    }
                },
                "required": ["creature_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        // ─── Genome Profiler ───
        BuiltinToolDef {
            name: "gbif_taxonomy_tree",
            description: "Call this tool to fetch the full taxonomic hierarchy for a species from GBIF. This tool is executed server-side — you do not need internet access to use it. Returns real kingdom-through-species data with GBIF keys, plus sibling taxa at each rank.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "gbif_key": {
                        "type": "integer",
                        "description": "GBIF species/taxon key"
                    },
                    "scientific_name": {
                        "type": "string",
                        "description": "Scientific name to look up (used if gbif_key not provided)"
                    }
                },
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        // ─── Wild / Foraging Tools ────────────────────────────────────
        BuiltinToolDef {
            name: "inat_observations",
            description: "Call this tool to query iNaturalist for recent species observations near a location. Server-side — no API key required. Returns community observations with species, date, photo, quality grade, and coordinates. Use for foraging scouting: what has been observed in this area recently?",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "lat": {
                        "type": "number",
                        "description": "Latitude of search centre"
                    },
                    "lng": {
                        "type": "number",
                        "description": "Longitude of search centre"
                    },
                    "radius_km": {
                        "type": "number",
                        "description": "Search radius in kilometres (default: 5, max: 50)",
                        "default": 5
                    },
                    "taxon": {
                        "type": "string",
                        "description": "Iconic taxon filter: Fungi | Plantae | Animalia etc. (default: Fungi)",
                        "default": "Fungi"
                    },
                    "days_back": {
                        "type": "integer",
                        "description": "How many days back to search (default: 30, max: 365)",
                        "default": 30
                    },
                    "quality_grade": {
                        "type": "string",
                        "description": "Minimum quality grade: research | needs_id | casual (default: needs_id)",
                        "enum": ["research", "needs_id", "casual"],
                        "default": "needs_id"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max results to return (default: 20, max: 50)",
                        "default": 20
                    }
                },
                "required": ["lat", "lng"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "mycobank_lookup",
            description: "Call this tool to look up authoritative fungal nomenclature from MycoBank. Server-side. Returns accepted name, nomenclatural status, taxonomic classification, synonyms, and MycoBank number. Use for species validation and authoritative naming.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Fungal species name to look up (scientific name)"
                    },
                    "include_synonyms": {
                        "type": "boolean",
                        "description": "Include synonyms and basionyms in the response (default: true)",
                        "default": true
                    }
                },
                "required": ["name"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "openweather_forecast",
            description: "Call this tool to get current weather conditions and 5-day forecast for a location. Server-side — requires OPENWEATHER_API_KEY. Returns temperature, humidity, precipitation, wind, and 5-day outlook. Use for microclimate foraging condition assessment.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "lat": {
                        "type": "number",
                        "description": "Latitude"
                    },
                    "lng": {
                        "type": "number",
                        "description": "Longitude"
                    },
                    "include_forecast": {
                        "type": "boolean",
                        "description": "Include 5-day forecast in addition to current conditions (default: true)",
                        "default": true
                    }
                },
                "required": ["lat", "lng"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        // ─── Web Search ───
        BuiltinToolDef {
            name: "web_search",
            description: "Search the web for current information using Brave Search. Returns recent news, articles, and web pages with titles, URLs, descriptions, and publication dates. Use this to get up-to-date evidence that goes beyond your training data cutoff.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query. Be specific: include names, dates, ticker symbols, or event terms. E.g. 'RKLB Q4 2025 earnings revenue' or 'Fed interest rate decision March 2026'."
                    },
                    "count": {
                        "type": "integer",
                        "description": "Number of results to return (default: 5, max: 10)",
                        "default": 5
                    },
                    "freshness": {
                        "type": "string",
                        "description": "Filter by recency: 'pd' = past day, 'pw' = past week, 'pm' = past month, 'py' = past year. Omit for all-time results.",
                        "enum": ["pd", "pw", "pm", "py"]
                    }
                },
                "required": ["query"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        // ─── Football (soccer) API ───
        BuiltinToolDef {
            name: "call_football_api",
            description: "Call the API-Football v3 REST API (api-football.com) to get live football/soccer data. Returns current standings, fixtures, results, team stats, player stats, injuries, lineups, head-to-head records, and match predictions. Requires FOOTBALL_API_KEY environment variable.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "endpoint": {
                        "type": "string",
                        "description": "API endpoint path (without leading slash). Examples: 'standings', 'fixtures', 'teams/statistics', 'players/topscorers', 'injuries', 'predictions', 'fixtures/headtohead', 'fixtures/statistics', 'fixtures/events', 'fixtures/lineups', 'players', 'leagues'"
                    },
                    "params": {
                        "type": "object",
                        "description": "Query parameters as key-value pairs. Common params: league (league ID), season (e.g. 2025), team (team ID), fixture (fixture ID), date (YYYY-MM-DD), from/to (date range), last (last N fixtures), next (next N fixtures), player (player ID). Example for PL standings: {\"league\": 39, \"season\": 2025}"
                    }
                },
                "required": ["endpoint"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        // ─── Monte Carlo / FPL Simulation ───
        BuiltinToolDef {
            name: "run_monte_carlo",
            description: "Execute a Monte Carlo simulation from an FPL (Fermi Probabilistic Language) program. Parses the program, samples from each driver's distribution, and returns full statistics: mean, median, percentiles (p5/p25/p75/p95), std_dev, min/max, and a histogram. Use this to produce rigorous probabilistic results rather than reasoning about distributions informally.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "fpl_program": {
                        "type": "string",
                        "description": "FPL source code defining drivers (with distributions), a model expression, and a simulate statement. Example:\n  driver x continuous { distribution: triangular(0.3, 0.6, 0.9) }\n  model: x\n  simulate 10000 iterations"
                    },
                    "iterations": {
                        "type": "integer",
                        "description": "Number of Monte Carlo iterations (default: 10000). Overrides the simulate statement in the FPL if provided.",
                        "default": 10000
                    }
                },
                "required": ["fpl_program"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "run_sensitivity_analysis",
            description: "Run Sobol global sensitivity analysis on an FPL program. Returns first-order and total-order Sobol indices for each driver, ranked by total-order impact, plus bootstrap standard errors for uncertainty quantification. Use this to identify which input variables drive the most outcome variance — a proper variance decomposition, not a heuristic tornado diagram.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "fpl_program": {
                        "type": "string",
                        "description": "FPL source code with driver definitions and model expression."
                    },
                    "iterations": {
                        "type": "integer",
                        "description": "Baseline iterations for the analysis (default: 10000). More iterations improve index precision.",
                        "default": 10000
                    }
                },
                "required": ["fpl_program"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        // ─── SimOps — Universal Resource Efficiency Engine (SOSA-aligned) ───
        BuiltinToolDef {
            name: "simops_cascade_forward",
            description: "Run a forward cascade through a multi-stage transformation process. Propagates input_quantity through all stages computing output quantities, energy, carbon delta (kg CO₂-eq), stage NER, and OPEX at each step. Returns a CascadeResult with system-level NER, total carbon, and LCC.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "process_name": {
                        "type": "string",
                        "description": "Named process config: 'ambu_bioreactor' or 'scoby_kombucha'. Omit to use ambu_bioreactor as default."
                    },
                    "process_json": {
                        "type": "object",
                        "description": "Inline process config JSON (overrides process_name). Full ProcessConfig schema."
                    },
                    "input_quantity": {
                        "type": "number",
                        "description": "Input quantity at stage 0 (in the units of the first stage's input resource)."
                    }
                },
                "required": ["input_quantity"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "simops_cascade_backward",
            description: "Run a backward cascade to determine the primary input required to produce a specified output. Given target_output at the final stage, back-calculates all intermediate quantities and the required stage-0 input.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "process_name": {
                        "type": "string",
                        "description": "Named process config: 'ambu_bioreactor' or 'scoby_kombucha'."
                    },
                    "process_json": {
                        "type": "object",
                        "description": "Inline process config JSON (overrides process_name)."
                    },
                    "target_output": {
                        "type": "number",
                        "description": "Desired output quantity at the final stage (in the final stage's output units)."
                    }
                },
                "required": ["target_output"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "simops_kpi_compute",
            description: "Compute batch KPIs for a fermentation or cultivation run: NER (Net Energy Ratio), SEC (Specific Energy Consumption kWh/kg), LCC (Levelized Cost of Calories $/million kcal), and Harvest Intensity %. Takes measured energy inputs and batch output.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "primary_energy_kwh":     { "type": "number", "description": "Primary process energy input (e.g. LED lighting) in kWh." },
                    "climate_energy_kwh":     { "type": "number", "description": "Climate control energy (heating/cooling/Peltier) in kWh." },
                    "delivery_energy_kwh":    { "type": "number", "description": "Pumping and delivery energy in kWh." },
                    "harvest_energy_kwh":     { "type": "number", "description": "Harvest and post-processing energy in kWh." },
                    "output_mass_kg":         { "type": "number", "description": "Harvested output mass in kg (dry weight for biomass)." },
                    "caloric_density_kcal_g": { "type": "number", "description": "Caloric density of the output in kcal/g." },
                    "elec_price_per_kwh":     { "type": "number", "description": "Electricity price in USD/kWh (e.g. 0.22 for German industrial)." },
                    "consumables_cost_usd":   { "type": "number", "description": "Total consumables cost for the batch in USD (nutrients, substrate, CO₂, etc.)." },
                    "capex_contribution_usd": { "type": "number", "description": "Amortized CAPEX contribution for this batch in USD (optional, default 0)." }
                },
                "required": [
                    "primary_energy_kwh", "climate_energy_kwh", "delivery_energy_kwh",
                    "harvest_energy_kwh", "output_mass_kg", "caloric_density_kcal_g",
                    "elec_price_per_kwh", "consumables_cost_usd"
                ]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "simops_predictor_train",
            description: "Fit an OLS linear regression model from historical observations. Takes an array of {features: {k: v, ...}, target: f64} records and returns model coefficients, intercept, R², and feature importance. Model JSON can be passed to simops_predictor_forecast or simops_optimize_* tools.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "observations": {
                        "type": "array",
                        "description": "Array of training observations. Each item must have 'features' (object of string→number) and 'target' (number).",
                        "items": {
                            "type": "object",
                            "properties": {
                                "features": { "type": "object", "additionalProperties": { "type": "number" } },
                                "target":   { "type": "number" }
                            },
                            "required": ["features", "target"]
                        },
                        "minItems": 4
                    }
                },
                "required": ["observations"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "simops_predictor_forecast",
            description: "Predict yield or output for a planned operational batch using a trained OLS model. Takes a model_json (from simops_predictor_train) and a feature map. Returns predicted value, R², and caloric-positive/energy-sink status.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "model_json": {
                        "type": "object",
                        "description": "Trained predictor model returned by simops_predictor_train."
                    },
                    "features": {
                        "type": "object",
                        "description": "Feature map for the planned batch (same keys as training features, e.g. {lighting_kwh: 120, nutrients_g: 6.5, temp_c: 27}).",
                        "additionalProperties": { "type": "number" }
                    }
                },
                "required": ["model_json", "features"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "simops_optimize_scale",
            description: "Proportionally scale a reference operating point to hit a target output. All inputs in the reference are scaled by the same factor. Returns scaled input values, predicted output, convergence status, and residual. Use for holistic scale-up planning.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "model_json":     { "type": "object", "description": "Trained predictor model from simops_predictor_train." },
                    "reference":      { "type": "object", "description": "Reference operating point: feature map of current/baseline input values.", "additionalProperties": { "type": "number" } },
                    "target_output":  { "type": "number", "description": "Target output value to achieve." },
                    "max_scale":      { "type": "number", "description": "Maximum scaling factor allowed (default: 5.0)." }
                },
                "required": ["model_json", "reference", "target_output"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "simops_optimize_single_input",
            description: "Solve analytically for a single free input variable to hit a target output, holding all other inputs fixed. Use for questions like 'how much more LED power do I need to produce 5 kg biomass?'. Returns the required value and convergence report.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "model_json":    { "type": "object", "description": "Trained predictor model from simops_predictor_train." },
                    "fixed_inputs":  { "type": "object", "description": "Fixed input feature values (all features except the free one).", "additionalProperties": { "type": "number" } },
                    "free_feature":  { "type": "string", "description": "Name of the single input feature to solve for." },
                    "target_output": { "type": "number", "description": "Target output value to achieve." },
                    "min_value":     { "type": "number", "description": "Minimum allowed value for the free feature (default: 0)." },
                    "max_value":     { "type": "number", "description": "Maximum allowed value for the free feature (default: 1,000,000)." }
                },
                "required": ["model_json", "fixed_inputs", "free_feature", "target_output"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        // ─── SimOps ABW-integrated tools ────────────────────────────
        // Consumed by simops_cascade, simops_predictor, simops_optimizer,
        // simops_advisor, simops_narrator. Bridge between the deterministic
        // simops crate and the ABW SOSA observation store.
        BuiltinToolDef {
            name: "simops_load_process",
            description: "Load a SimOps process configuration by name or from agent memory. Returns the full ProcessConfig JSON. Sources: built-ins (ambu_bioreactor, scoby_kombucha), inline process_json, or a config saved in agent episodic memory.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "process_name": { "type": "string", "description": "Named process: ambu_bioreactor | scoby_kombucha | any custom name saved in memory." },
                    "process_json": { "type": "object", "description": "Inline ProcessConfig JSON (takes priority over process_name)." }
                }
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "simops_write_observation",
            description: "Write a SOSA observation to the platform store. Use after each measurement cycle to build training data for simops_predictor and the session history.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id":            { "type": "string", "description": "UUID of the observation session." },
                    "observable_property":   { "type": "string", "description": "What was measured: e.g. 'biomass_dw_g', 'od600', 'titratable_acidity'." },
                    "result_value":          { "type": "number", "description": "Measured value." },
                    "result_unit":           { "type": "string", "description": "Unit of measurement: g/L, kg, pH, etc." },
                    "feature_of_interest":   { "type": "string", "description": "SOSA FeatureOfInterest URI, e.g. xid:platform/ambu-001." },
                    "phenomenon_time":       { "type": "integer", "description": "Unix milliseconds of measurement (defaults to now)." },
                    "extra":                 { "type": "object", "description": "Any additional metadata." }
                },
                "required": ["session_id", "observable_property", "result_value"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "simops_fetch_training_data",
            description: "Fetch SOSA observations for a session as structured training data. Groups by phenomenon_time into feature vectors ready for simops_predictor_train.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id":          { "type": "string", "description": "UUID of the observation session." },
                    "feature_properties":  { "type": "array", "items": { "type": "string" }, "description": "observable_property names to use as X input features." },
                    "target_property":     { "type": "string", "description": "observable_property name to use as y prediction target." },
                    "limit":              { "type": "integer", "default": 1000, "description": "Max observations per property." }
                },
                "required": ["session_id", "feature_properties", "target_property"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "get_observations",
            description: "Read recent SOSA observations for a session. Returns per-property summary statistics and the raw observation list.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id":           { "type": "string", "description": "UUID of the observation session." },
                    "observable_property":  { "type": "string", "description": "Filter to one property (optional)." },
                    "limit":               { "type": "integer", "default": 100, "description": "Max observations to return." },
                    "from_ms":             { "type": "integer", "description": "Unix ms lower bound (optional)." },
                    "to_ms":              { "type": "integer", "description": "Unix ms upper bound (optional)." }
                },
                "required": ["session_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "describe_session",
            description: "Summarise an observation session: metadata, per-property statistics, and any saved process config snapshot. Used by simops_advisor and simops_narrator for context.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "UUID of the observation session." }
                },
                "required": ["session_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "simops_check_constraints",
            description: "Validate that an optimizer or cascade result is physically feasible: non-negative quantities, stage efficiency bounds, unit compatibility, and any user-supplied min/max constraints.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "process_name":     { "type": "string" },
                    "process_json":     { "type": "object" },
                    "optimizer_result": { "type": "object", "description": "Output of simops_optimize_* or simops_cascade_*." },
                    "constraints":      { "type": "object", "description": "Optional bounds: { feature_name: { min?: f64, max?: f64 } }", "additionalProperties": { "type": "object" } }
                },
                "required": ["optimizer_result"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "simops_write_actuation_plan",
            description: "Persist an optimizer recommendation as an actuation plan in agent episodic memory. Creates a durable record of what was recommended, the rationale, and the operator decision. Feeds the agent's dreaming/consolidation cycle.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id":       { "type": "string", "description": "UUID of the observation session this plan addresses." },
                    "process_name":     { "type": "string" },
                    "optimizer_result": { "type": "object", "description": "The optimizer output being recorded." },
                    "rationale":        { "type": "string", "description": "Why this plan was chosen." },
                    "decision":         { "type": "string", "enum": ["proposed", "accept", "reject", "modify"], "default": "proposed" },
                    "modifications":    { "type": "object", "description": "Operator-applied changes, if any." },
                    "target_output":    { "type": "number", "description": "The production target this plan addresses." }
                },
                "required": ["session_id", "optimizer_result", "rationale"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        // ─── Observability composition tools ───────────────────────
        // Consumed by observability_coordinator, eval_runner,
        // anomaly_triager, dyad_observer. See docs/AGENT_MODEL.md §3.
        BuiltinToolDef {
            name: "run_evaluator_registry",
            description: "Trigger a fresh eval run for an agent — invokes the full EvaluatorRegistry (WildGuard / Faithfulness / LlmJudge / Sotopia / LifelongBench / CharacterEval / Brier) against the agent's eval test cases. Charges eval_run gas. Returns the new run_id; results stream into eval_signals and can be read with query_eval_signals.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "UUID of the agent to evaluate." },
                    "judge": { "type": "boolean", "default": true, "description": "Include LlmJudgeEvaluator (Anthropic; adds LLM cost)." },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional tag filter — only run test cases that carry any of these tags."
                    }
                },
                "required": ["agent_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "classify_anomaly",
            description: "Read context to inform anomaly severity classification: the event row, related eval_signals from the same run, the agent's persona_version, and any prior hitl_actions. Returns a JSON bundle the caller synthesises into a severity (L0-L3) and routing recommendation.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "anomaly_id": { "type": "string", "description": "UUID of the anomaly_event to classify." }
                },
                "required": ["anomaly_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "route_to_hitl",
            description: "Mark an anomaly_event as requires_review with the agent's recommended action. Stores the recommendation in payload.agent_recommendation but does NOT execute the action — that remains the reviewer's prerogative. Use after classify_anomaly to formally route an L2/L3 event.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "anomaly_id": { "type": "string", "description": "UUID of the anomaly_event." },
                    "recommended_action": {
                        "type": "string",
                        "enum": ["approve", "relabel", "intervene"],
                        "description": "Action the agent recommends a reviewer take."
                    },
                    "scope": {
                        "type": "string",
                        "enum": ["episode", "agent", "agent_wide"],
                        "default": "episode",
                        "description": "Scope of the recommended action. agent_wide triggers two-reviewer consensus on reviewer-side."
                    },
                    "justification": {
                        "type": "string",
                        "description": "Why the agent is routing this event — surfaced to reviewers."
                    }
                },
                "required": ["anomaly_id", "recommended_action", "justification"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "query_eval_signals",
            description: "Read per-evaluator, per-dimension scores from eval_signals. Required: run_id. Returns one row per (evaluator, dimension) with score, confidence, persona_version, model_used, rationale.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "run_id": { "type": "string", "description": "UUID of the eval_run to read signals for." }
                },
                "required": ["run_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "query_eval_runs",
            description: "List recent eval_runs for an agent. Returns run metadata including aggregated_signal, regression_detected, judge_enabled, pass/fail counts.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "UUID of the agent." },
                    "limit": { "type": "integer", "default": 20, "description": "Max runs to return (default 20, max 100)." }
                },
                "required": ["agent_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "query_anomalies",
            description: "Read anomaly_events rows for an agent (drift / conflict / rupture / safety). Used by anomaly_triager and observability_coordinator.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "UUID of the agent." },
                    "limit": { "type": "integer", "default": 50, "description": "Max events to return (default 50, max 500)." }
                },
                "required": ["agent_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "query_hitl_queue",
            description: "Read pending HITL events — anomaly_events where requires_review=true and resolved_at is null. Returns up to N events ordered by severity then recency.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "default": 50, "description": "Max events to return (default 50, max 200)." }
                },
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "query_timeline",
            description: "Read agent_timeline_entries — per-episode rolled-up scoring view with persona_version_at_write and aggregated scores. Used by dyad_observer for longitudinal narrative.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "UUID of the agent." },
                    "limit": { "type": "integer", "default": 100, "description": "Max entries to return (default 100, max 500)." }
                },
                "required": ["agent_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "query_dyad_state",
            description: "Read dyad_state rows — per-(agent, human) running rapport / trust / reciprocity. Used by dyad_observer.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "UUID of the agent." }
                },
                "required": ["agent_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
    ]
}

/// Tool registry — collects available tools and dispatches execution
pub struct ToolRegistry {
    include_workspace: bool,
    exclude_delegation: bool,
}

impl ToolRegistry {
    /// Standard registry (4 tools, no workspace tools)
    pub fn standard() -> Self {
        Self {
            include_workspace: false,
            exclude_delegation: false,
        }
    }

    /// Registry with workspace tools
    pub fn with_workspace() -> Self {
        Self {
            include_workspace: true,
            exclude_delegation: false,
        }
    }

    /// Registry with workspace tools but NO delegation tools (for delegated agents)
    pub fn with_workspace_no_delegation() -> Self {
        Self {
            include_workspace: true,
            exclude_delegation: true,
        }
    }

    fn filter_tool(&self, t: &BuiltinToolDef) -> bool {
        if t.requires_workspace && !self.include_workspace {
            return false;
        }
        if t.is_delegation && self.exclude_delegation {
            return false;
        }
        true
    }

    /// Get available tools as Claude API format
    pub(crate) fn to_claude_tools(&self) -> Vec<ClaudeTool> {
        builtin_tools()
            .into_iter()
            .filter(|t| self.filter_tool(t))
            .map(|t| ClaudeTool {
                name: t.name.to_string(),
                description: t.description.to_string(),
                input_schema: t.input_schema,
            })
            .collect()
    }

    /// Get available tools as OpenAI API format
    pub(crate) fn to_openai_tools(&self) -> Vec<OpenAITool> {
        builtin_tools()
            .into_iter()
            .filter(|t| self.filter_tool(t))
            .map(|t| OpenAITool {
                tool_type: "function".to_string(),
                function: OpenAIFunction {
                    name: t.name.to_string(),
                    description: t.description.to_string(),
                    parameters: t.input_schema,
                },
            })
            .collect()
    }

    /// Also include any MCP tools declared on the agent card
    pub(crate) fn to_claude_tools_with_card(&self, card: &AgentCard) -> Vec<ClaudeTool> {
        self.to_claude_tools_with_card_and_remote(card, None)
    }

    /// Card tools plus any remote MCP tools discovered for this agent.
    ///
    /// Ordering is deliberate: builtins, then card-declared platform
    /// tools, then remote tools — and each later group skips names
    /// already claimed. A remote server therefore cannot shadow a
    /// platform tool by naming one of its tools `execute_agent`, and both
    /// APIs reject duplicate names with a 400 anyway.
    pub(crate) fn to_claude_tools_with_card_and_remote(
        &self,
        card: &AgentCard,
        remote: Option<&crate::agent_backend::mcp_client::RemoteMcpCatalogue>,
    ) -> Vec<ClaudeTool> {
        let mut tools = self.to_claude_tools();
        // Collect builtin names first — Anthropic API rejects duplicate tool names with 400.
        let mut claimed: std::collections::HashSet<String> =
            tools.iter().map(|t| t.name.clone()).collect();
        for mcp in &card.capabilities.mcp_tools {
            // Only include MCP tools that have schemas and aren't already registered as builtins
            if let Some(ref schema) = mcp.input_schema {
                if claimed.insert(mcp.name.clone()) {
                    tools.push(ClaudeTool {
                        name: mcp.name.clone(),
                        description: mcp.description.clone(),
                        input_schema: schema.clone(),
                    });
                }
            }
        }
        if let Some(cat) = remote {
            for rt in cat.tools() {
                if claimed.insert(rt.qualified_name.clone()) {
                    tools.push(ClaudeTool {
                        name: rt.qualified_name.clone(),
                        description: rt.description.clone(),
                        input_schema: rt.input_schema.clone(),
                    });
                }
            }
        }
        tools
    }

    /// OpenAI-format counterpart of
    /// [`Self::to_claude_tools_with_card_and_remote`].
    pub(crate) fn to_openai_tools_with_card_and_remote(
        &self,
        card: &AgentCard,
        remote: Option<&crate::agent_backend::mcp_client::RemoteMcpCatalogue>,
    ) -> Vec<OpenAITool> {
        self.to_claude_tools_with_card_and_remote(card, remote)
            .into_iter()
            .map(|t| OpenAITool {
                tool_type: "function".to_string(),
                function: OpenAIFunction {
                    name: t.name,
                    description: t.description,
                    parameters: t.input_schema,
                },
            })
            .collect()
    }

    /// Execute a tool by name
    pub async fn execute(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<String, String> {
        match tool_name {
            "search_knowledge" => execute_search_knowledge(input, ctx).await,
            "query_ontology" => execute_query_ontology(input, ctx).await,
            "execute_agent" => execute_execute_agent(input, ctx).await,
            "ncbi_genome_search" => {
                crate::agent_backend::ncbi_tools::execute_ncbi_genome_search(input).await
            }
            "validate_agent_card" => crate::card_contract::execute_validate_tool(input),
            "build_output_contract" => crate::contract_sketch::execute_build_tool(input),
            "list_agents" => execute_list_agents(ctx).await,
            "read_workspace_file" => execute_read_workspace_file(input, ctx).await,
            "read_workspace_output" => execute_read_workspace_output(input, ctx).await,
            "list_workspace_outputs" => execute_list_workspace_outputs(input, ctx).await,
            "list_workspace_agents" => execute_list_workspace_agents(ctx).await,
            "generate_image" => execute_generate_image(input).await,
            "edit_image" => execute_edit_image(input).await,
            "write_workspace_file" => execute_write_workspace_file(input, ctx).await,
            "speak_text" => execute_speak_text(input).await,
            "reduct_list_projects" => execute_reduct_list_projects(ctx).await,
            "reduct_get_project" => execute_reduct_get_project(input, ctx).await,
            "reduct_get_transcript" => execute_reduct_get_transcript(input, ctx).await,
            "reduct_create_reel" => execute_reduct_create_reel(input, ctx).await,
            "reduct_add_block" => execute_reduct_add_block(input, ctx).await,
            "delegate_to_agent" => execute_delegate_to_agent(input, ctx).await,
            "evaluate_coherence" => execute_evaluate_coherence(input, ctx).await,
            "coherence_snapshot" => execute_coherence_snapshot(ctx).await,
            "get_workspace_messages" => execute_get_workspace_messages(input, ctx).await,
            "get_shopping_profile" => execute_get_shopping_profile(input, ctx).await,
            "update_shopping_profile" => execute_update_shopping_profile(input, ctx).await,
            "list_marketplace" => execute_list_marketplace(input, ctx).await,
            "create_listing" => execute_create_listing(input, ctx).await,
            // AR Spatial Suite
            "h3_resolve" => execute_h3_resolve(input).await,
            "geocode" => execute_geocode(input).await,
            "create_beacon" => execute_create_beacon(input, ctx).await,
            "query_beacons" => execute_query_beacons(input, ctx).await,
            "save_grid_map" => execute_save_grid_map(input, ctx).await,
            "gbif_species_search" => execute_gbif_species_search(input).await,
            "mint_creature" => execute_mint_creature(input, ctx).await,
            "generate_specimen_art" => execute_generate_specimen_art(input, ctx).await,
            "segment_creature_wings" => execute_segment_creature_wings(input, ctx).await,
            "activate_formation" => execute_activate_formation(input, ctx).await,
            "scan_nearby_creatures" => execute_scan_nearby_creatures(input, ctx).await,
            "gbif_taxonomy_tree" => execute_gbif_taxonomy_tree(input).await,
            // Wild / foraging tools
            "inat_observations" => execute_inat_observations(input).await,
            "mycobank_lookup" => execute_mycobank_lookup(input).await,
            "openweather_forecast" => execute_openweather_forecast(input).await,
            // FMP (Financial Modeling Prep) tools for equity_analyst
            "fmp_company_profile" => execute_fmp_api(input, "/stable/profile", &["symbol"]).await,
            "fmp_income_statement" => {
                execute_fmp_api(
                    input,
                    "/stable/income-statement",
                    &["symbol", "period", "limit"],
                )
                .await
            }
            "fmp_balance_sheet" => {
                execute_fmp_api(
                    input,
                    "/stable/balance-sheet-statement",
                    &["symbol", "period", "limit"],
                )
                .await
            }
            "fmp_cash_flow" => {
                execute_fmp_api(
                    input,
                    "/stable/cash-flow-statement",
                    &["symbol", "period", "limit"],
                )
                .await
            }
            "fmp_ratios" => {
                execute_fmp_api(input, "/stable/ratios", &["symbol", "period", "limit"]).await
            }
            "fmp_key_metrics" => {
                execute_fmp_api(input, "/stable/key-metrics", &["symbol", "period", "limit"]).await
            }
            "fmp_dcf" => execute_fmp_api(input, "/stable/discounted-cash-flow", &["symbol"]).await,
            "fmp_analyst_estimates" => {
                execute_fmp_api(
                    input,
                    "/stable/analyst-estimates",
                    &["symbol", "period", "limit"],
                )
                .await
            }
            "fmp_historical_price" => {
                execute_fmp_api(
                    input,
                    "/stable/historical-price-eod/full",
                    &["symbol", "from", "to"],
                )
                .await
            }
            // Web Search
            "web_search" => execute_web_search(input).await,
            // Monte Carlo / FPL Simulation tools
            "run_monte_carlo" => execute_run_monte_carlo(input).await,
            "run_sensitivity_analysis" => execute_run_sensitivity_analysis(input).await,
            // Football (soccer) live data via API-Football v3
            "call_football_api" => execute_call_football_api(input).await,
            // Polymarket tools for prediction_market agent and general orchestra use
            "polymarket_search" => execute_polymarket_search(input).await,
            "polymarket_event" => execute_polymarket_event(input).await,
            // Weather / prediction-market stack (src/agent_backend/weather_tools.rs)
            name if crate::agent_backend::weather_tools::handles(name) => {
                match crate::agent_backend::weather_tools::dispatch(name, input).await {
                    Some(r) => r,
                    None => Err(format!("Unknown weather tool: {name}")),
                }
            }
            // SimOps — Universal Resource Efficiency Engine (SOSA-aligned)
            "simops_cascade_forward" => {
                crate::agent_backend::simops_tools::execute_simops_cascade_forward(input).await
            }
            "simops_cascade_backward" => {
                crate::agent_backend::simops_tools::execute_simops_cascade_backward(input).await
            }
            "simops_kpi_compute" => {
                crate::agent_backend::simops_tools::execute_simops_kpi_compute(input).await
            }
            "simops_predictor_train" => {
                crate::agent_backend::simops_tools::execute_simops_predictor_train(input).await
            }
            "simops_predictor_forecast" => {
                crate::agent_backend::simops_tools::execute_simops_predictor_forecast(input).await
            }
            "simops_optimize_scale" => {
                crate::agent_backend::simops_tools::execute_simops_optimize_scale(input).await
            }
            "simops_optimize_single_input" => {
                crate::agent_backend::simops_tools::execute_simops_optimize_single_input(input)
                    .await
            }
            // ─── SimOps ABW-integrated tools ────────────────────
            "simops_load_process" => {
                crate::agent_backend::simops_tools::execute_simops_load_process(input, ctx).await
            }
            "simops_write_observation" => {
                crate::agent_backend::simops_tools::execute_simops_write_observation(input, ctx)
                    .await
            }
            "simops_fetch_training_data" => {
                crate::agent_backend::simops_tools::execute_simops_fetch_training_data(input, ctx)
                    .await
            }
            "get_observations" => {
                crate::agent_backend::simops_tools::execute_get_observations(input, ctx).await
            }
            "describe_session" => {
                crate::agent_backend::simops_tools::execute_describe_session(input, ctx).await
            }
            "simops_check_constraints" => {
                crate::agent_backend::simops_tools::execute_simops_check_constraints(input, ctx)
                    .await
            }
            "simops_write_actuation_plan" => {
                crate::agent_backend::simops_tools::execute_simops_write_actuation_plan(input, ctx)
                    .await
            }
            // ─── Observability composition tools ───────────────
            "query_eval_signals" => execute_query_eval_signals(input, ctx).await,
            "query_eval_runs" => execute_query_eval_runs(input, ctx).await,
            "query_anomalies" => execute_query_anomalies(input, ctx).await,
            "query_hitl_queue" => execute_query_hitl_queue(input, ctx).await,
            "query_timeline" => execute_query_timeline(input, ctx).await,
            "query_dyad_state" => execute_query_dyad_state(input, ctx).await,
            "classify_anomaly" => execute_classify_anomaly(input, ctx).await,
            "route_to_hitl" => execute_route_to_hitl(input, ctx).await,
            "run_evaluator_registry" => execute_run_evaluator_registry(input, ctx).await,
            "get_agent_calibration" => execute_get_agent_calibration(input, ctx).await,
            "propose_composition_change" => execute_propose_composition_change(input, ctx).await,
            "record_coordination_observation" => {
                execute_record_coordination_observation(input, ctx).await
            }
            "fermi_execute_fpl" => execute_fermi_execute_fpl(input).await,
            "fermi_sensitivity_analysis" => execute_fermi_sensitivity_analysis(input).await,
            "declare_intention" => execute_declare_intention(input, ctx).await,
            "solicit_agent_plan" => execute_solicit_agent_plan(input, ctx).await,
            "check_conflicts" => execute_check_conflicts(input, ctx).await,
            "get_intention_map" => execute_get_intention_map(ctx).await,
            "clear_intention" => execute_clear_intention(input, ctx).await,
            "suggest_differentiation" => execute_suggest_differentiation(input, ctx).await,
            "emit_coherence_signal" => execute_emit_coherence_signal(input, ctx).await,

            // Fallthrough: a name no builtin claims may be a remote MCP
            // tool this agent's card authorised. Checked last on purpose
            // — builtins always win, so a third-party server cannot
            // shadow a platform tool by reusing its name.
            other => match ctx.remote_mcp.as_ref() {
                Some(cat) if cat.get(other).is_some() => cat.call(other, input).await,
                Some(cat) if !cat.is_empty() => Err(format!(
                    "Unknown tool: {other}. Remote MCP tools available to this agent: {}",
                    cat.tools()
                        .iter()
                        .map(|t| t.qualified_name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
                _ => Err(format!("Unknown tool: {other}")),
            },
        }
    }
}

// ─── Loop 5: routing reads measured calibration ────────────────────

/// `get_agent_calibration` — the router's read path onto Loop 5.
///
/// Declared on `moe_router_strategist`, `debate_strategist` and
/// `vote_strategist`, and dispatched by none of them until now: the only
/// implementation was the HTTP route, so Stage 0's "call
/// `get_agent_calibration` for each candidate member" returned
/// `Unknown tool: get_agent_calibration`. Worse, the card's own cold-start
/// language ("calibration data not yet available") made that read as sparse
/// data rather than a broken wire, so the loop looked young instead of
/// disconnected.
///
/// Delegates to `compute_agent_calibration`, the same function the route
/// calls, so the two cannot drift.
async fn execute_get_agent_calibration(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_id = resolve_agent_id(input, "agent_id", ctx).await?;
    let db = ctx
        .db
        .as_ref()
        .ok_or_else(|| "get_agent_calibration requires a database context".to_string())?;

    let agent = ctx
        .memory_store
        .get_agent(agent_id)
        .await
        .map_err(|e| format!("Failed to load agent: {e}"))?
        .ok_or_else(|| format!("Agent not found: {agent_id}"))?;

    let calibration = crate::calibration::compute_agent_calibration(
        db,
        &agent,
        &crate::calibration::CalibrationQuery::default(),
    )
    .await?;

    serde_json::to_string_pretty(&calibration).map_err(|e| format!("Serialization error: {e}"))
}

// ─── Loop 3b / 4: the strategist can propose a composition change ──

/// `propose_composition_change` — writes a pending `composition_versions` row.
///
/// Declared with a full `input_schema` on `cohere_and_coordinate`, and the
/// composition-dreaming prompt instructs Stage 4 to call it. It had no dispatch
/// arm, so every tension audit that concluded "the team should change" ended in
/// `Unknown tool`. That is why the Loop 4 dashboard card read "no pending
/// evolution proposals" permanently: as `handlers::composition_evolution` puts
/// it, "nothing ever generated one".
///
/// Deliberately does **not** accept `member_agent_ids`. The card is explicit —
/// "Do NOT specify which agent to add; that is the owner's decision" — and the
/// strategist's evidence is qualitative (coherence patterns, valence spread),
/// unlike the Shapley path in `composition_evolution`, which has per-agent
/// credit and may name names.
async fn execute_propose_composition_change(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or_else(|| {
        "propose_composition_change must be called inside a workspace".to_string()
    })?;

    let diff_summary = input
        .get("diff_summary")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "diff_summary is required".to_string())?;
    let rationale = input
        .get("rationale")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "rationale is required".to_string())?;
    let homophily = input
        .get("homophily_detected")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Same prefix convention the HTTP proposal route uses, so the owner sees
    // one vocabulary regardless of which path raised the proposal.
    let summary = if homophily {
        format!("[homophily detected] {diff_summary}")
    } else {
        diff_summary.to_string()
    };

    let version = agent_bestiary_memory::CompositionVersion {
        composition_version_id: Uuid::new_v4(),
        workspace_id,
        version_number: 0, // assigned by create_composition_version
        mission: None,
        coordination_strategist_id: ctx.current_agent_id,
        member_agent_ids: None,
        member_weights: None,
        diff_summary: Some(summary),
        proposed_by: Some("cohere_and_coordinate".to_string()),
        accepted_by: None,
        rejected_by: None,
        rejection_note: Some(rationale.to_string()),
        created_at: chrono::Utc::now(),
    };

    let version_id = ctx
        .memory_store
        .create_composition_version(&version)
        .await
        .map_err(|e| format!("Failed to create composition version: {e}"))?;

    serde_json::to_string_pretty(&json!({
        "version_id": version_id,
        "workspace_id": workspace_id,
        "status": "pending",
        "message": "Composition change proposed — the workspace owner must accept or reject it.",
    }))
    .map_err(|e| format!("Serialization error: {e}"))
}

// ─── Loop 3 cascade: the strategist writes into a member's memory ──

/// `record_coordination_observation` — Loop 3's actual correction mechanism.
///
/// The coordination strategist observes how a member behaved in a session and
/// writes that observation into **that member's episodic memory**. The member
/// picks it up on its next dreaming cycle, distils it into a semantic rule, and
/// carries it into every later execution via KG injection.
///
/// This is what makes Loop 3 adaptive rather than advisory. The previous design
/// wrote `_coordination/brief.md` to workspace git, which nothing read:
/// consolidation reads `episodes`, and workspace auto-injection only loads
/// files under `context/`. A brief could be written perfectly and never reach
/// a single agent.
///
/// ## Authorisation
///
/// This writes into another agent's memory, so it is the one tool where a
/// missing check is a memory-poisoning primitive. Two gates, both required:
///
/// 1. the caller must be the workspace's registered
///    `coordination_strategist_id`, and
/// 2. the target must be a current member of that same workspace.
///
/// Without (1) any agent that declared the tool could rewrite its peers'
/// beliefs; without (2) a strategist could write into agents outside the
/// workspace it coordinates.
async fn execute_record_coordination_observation(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or_else(|| {
        "record_coordination_observation must be called inside a workspace".to_string()
    })?;
    let db = ctx
        .db
        .as_ref()
        .ok_or_else(|| "record_coordination_observation requires a database context".to_string())?;
    let caller = ctx
        .current_agent_id
        .ok_or_else(|| "caller identity unavailable".to_string())?;

    // Gate 1 — only this workspace's coordination strategist.
    let strategist: Option<Uuid> =
        sqlx::query_scalar("SELECT coordination_strategist_id FROM teams WHERE id = $1")
            .bind(workspace_id)
            .fetch_optional(db)
            .await
            .map_err(|e| format!("Failed to read workspace: {e}"))?
            .flatten();
    if strategist != Some(caller) {
        return Err(
            "Only the workspace's coordination strategist may write coordination \
             observations into member memory."
                .to_string(),
        );
    }

    let target = resolve_agent_id(input, "agent_id", ctx).await?;

    // Gate 2 — target must be a member of this workspace.
    let observation = input
        .get("observation")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "observation is required".to_string())?;
    let session_summary = input
        .get("session_summary")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // The episode write itself lives in `fermi::coordination_note`, shared with
    // the platform-side delivery in `handlers::workspace::coherence`. One
    // implementation, per §3.4 — and the reason there are two callers at all is
    // that this one, the model-invoked one, produced 0 of 3,576 episodes for the
    // life of the feature. The platform now delivers the brief as a floor and
    // this remains the better path: a note targeted at one member about its own
    // behaviour.
    //
    // `since: None` — a targeted note is never a duplicate of itself. The
    // duplicate check exists so the platform's generic delivery yields to this
    // call, not the other way round.
    let delivery = crate::coordination_note::deliver(
        db,
        &ctx.memory_store,
        ctx.embedder.as_ref(),
        workspace_id,
        caller,
        target,
        observation,
        session_summary,
        None,
    )
    .await;

    match delivery {
        crate::coordination_note::Delivery::Written { episode_id } => {
            serde_json::to_string_pretty(&json!({
                "episode_id": episode_id,
                "agent_id": target,
                "workspace_id": workspace_id,
                "status": "recorded",
                "message": "Observation written to the member's episodic memory. It will be \
                            consolidated into a semantic rule on that agent's next dreaming cycle.",
            }))
            .map_err(|e| format!("Serialization error: {e}"))
        }
        crate::coordination_note::Delivery::NotAMember => Err(format!(
            "Agent {target} is not a member of this workspace; refusing to write \
             into its memory."
        )),
        crate::coordination_note::Delivery::AlreadyTargeted => Err(
            "A coordination observation for this member already exists for this \
             run. Unreachable from this path, which passes no cutoff."
                .to_string(),
        ),
        crate::coordination_note::Delivery::Failed { error } => {
            Err(format!("Failed to write observation: {error}"))
        }
    }
}

// ─── FPL execution as in-process platform tools ────────────────────

/// Parse FPL through the full pipeline: lex → parse → semantic analysis.
///
/// Mirrors `agent-mcp-server`'s private `parse_fpl`. The whole pipeline lives in
/// this crate (`fermi::lexer`, `parser`, `semantic`), which is why these are
/// in-process tools rather than a card pointing at an external MCP server: the
/// executor is already linked in, so a network hop would buy nothing.
fn parse_fpl_source(source: &str) -> Result<crate::ast::Program, String> {
    let tokens = crate::lexer::Lexer::new(source)
        .tokenize()
        .map_err(|errs| {
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        })?;
    let program = crate::parser::Parser::new(tokens)
        .parse()
        .map_err(|e| format!("Parse error: {e}"))?;
    let analysis = crate::semantic::SemanticAnalyzer::new().analyze(&program);
    if !analysis.errors.is_empty() {
        return Err(format!(
            "Semantic error: {}",
            analysis
                .errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    Ok(program)
}

/// `fermi_execute_fpl` — run a Monte Carlo simulation over an FPL program.
///
/// `monte_carlo_sim` declared this (and the sensitivity tool below) as platform
/// tools while both existed only on `agent-mcp-server`, and the card declared no
/// `mcp_servers` to resolve them through. So the model was advertised a
/// simulation capability and got `Unknown tool` — the agent's entire purpose.
async fn execute_fermi_execute_fpl(input: &serde_json::Value) -> Result<String, String> {
    let source = input
        .get("fpl_program")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "fpl_program is required".to_string())?;
    let program = parse_fpl_source(source)?;

    // Same bounds the MCP surface applies. The cap matters here more than
    // there: this runs inside a request-serving process.
    let iterations = input
        .get("iterations")
        .and_then(|v| v.as_u64())
        .unwrap_or(10_000)
        .min(100_000) as usize;

    let mut executor = match input.get("seed").and_then(|v| v.as_u64()) {
        Some(seed) => crate::executor::Executor::with_seed(iterations, seed),
        None => crate::executor::Executor::new(iterations),
    };
    let r = executor
        .execute(&program)
        .map_err(|e| format!("Execution failed: {e}"))?;

    serde_json::to_string_pretty(&json!({
        "iterations": r.iterations,
        "mean": r.mean,
        "median": r.median,
        "std_dev": r.std_dev,
        "p5": r.p5,
        "p25": r.p25,
        "p75": r.p75,
        "p95": r.p95,
        "min": r.min,
        "max": r.max,
        "base_rate": r.base_rate,
        "divergence_relative": r.divergence_relative,
        "divergence_absolute": r.divergence_absolute,
    }))
    .map_err(|e| format!("Serialization error: {e}"))
}

/// `fermi_sensitivity_analysis` — Sobol variance decomposition over an FPL program.
async fn execute_fermi_sensitivity_analysis(input: &serde_json::Value) -> Result<String, String> {
    let source = input
        .get("fpl_program")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "fpl_program is required".to_string())?;
    let program = parse_fpl_source(source)?;

    let iterations = input
        .get("iterations")
        .and_then(|v| v.as_u64())
        .unwrap_or(5_000)
        .min(50_000) as usize;

    let analysis = crate::sensitivity::full_sensitivity_analysis(&program, iterations)
        .map_err(|e| format!("Sensitivity analysis failed: {e}"))?;

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
                "ci_low": (s.total_order_index - 1.96 * s.standard_error).max(0.0),
                "ci_high": (s.total_order_index + 1.96 * s.standard_error).min(1.0),
            })
        })
        .collect();

    serde_json::to_string_pretty(&json!({
        "iterations": iterations,
        "baseline": {
            "mean": analysis.baseline.mean,
            "std_dev": analysis.baseline.std_dev,
            "p5": analysis.baseline.p5,
            "p95": analysis.baseline.p95,
        },
        "drivers": drivers,
        "top_driver": analysis.ranked_drivers.first(),
    }))
    .map_err(|e| format!("Serialization error: {e}"))
}

// ─── Loop 3 Stage 0 — prospective coordination ─────────────────────
//
// All six tools below were declared on `intention_coordinator`'s card and had
// no dispatch arm, so the agent has never functioned and Loop 3's Stage 0 has
// never run. Detection logic is in `fermi::intentions`, unit-tested; these are
// the load/store halves.
//
// State lives in `workspace_intentions` (mig-210) rather than the
// `_coordination/intention_map.json` the card described, because several
// agents declare at once and a git file has no concurrency story.

/// Load the workspace's active intention map, joined to agent names.
async fn load_intentions(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
) -> Result<Vec<crate::intentions::Intention>, String> {
    let rows = sqlx::query(
        "SELECT i.intention_id, i.agent_id, a.agent_name, i.action_type, i.tool,
                i.description, i.targets, i.depends_on, i.embedding,
                i.source, i.declared_by
           FROM workspace_intentions i
           JOIN agents a ON a.agent_id = i.agent_id
          WHERE i.workspace_id = $1 AND i.status = 'active'
          ORDER BY i.declared_at",
    )
    .bind(workspace_id)
    .fetch_all(db)
    .await
    .map_err(|e| format!("Failed to load intentions: {e}"))?;

    Ok(rows
        .iter()
        .map(|r| crate::intentions::Intention {
            intention_id: r
                .try_get::<Uuid, _>("intention_id")
                .map(|u| u.to_string())
                .unwrap_or_default(),
            agent_id: r
                .try_get::<Uuid, _>("agent_id")
                .map(|u| u.to_string())
                .unwrap_or_default(),
            agent_name: r.try_get("agent_name").unwrap_or_default(),
            action_type: r.try_get("action_type").unwrap_or_default(),
            tool: r.try_get("tool").ok(),
            description: r.try_get("description").unwrap_or_default(),
            targets: r.try_get("targets").unwrap_or_default(),
            depends_on: r.try_get("depends_on").unwrap_or_default(),
            embedding: r
                .try_get::<Option<pgvector::Vector>, _>("embedding")
                .ok()
                .flatten()
                .map(|v| v.to_vec()),
            // A read failure lands on `Unattributed` rather than on a stronger
            // claim (mig-218). If the column is missing because the migration
            // has not run, every row reads as second-hand — which suppresses
            // duplication detection until it has, and that is the right way
            // round: no overlap warnings beats warnings we cannot vouch for.
            source: crate::intentions::IntentionSource::from_db(
                r.try_get::<String, _>("source")
                    .unwrap_or_default()
                    .as_str(),
            ),
            declared_by: r
                .try_get::<Option<Uuid>, _>("declared_by")
                .ok()
                .flatten()
                .map(|u| u.to_string()),
        })
        .collect())
}

/// Output names already completed in this workspace, so a `depends_on` entry
/// can be judged satisfied.
async fn produced_outputs(db: &sqlx::PgPool, workspace_id: Uuid) -> Vec<String> {
    sqlx::query_scalar::<_, Vec<String>>(
        "SELECT targets FROM workspace_intentions
          WHERE workspace_id = $1 AND status = 'completed'",
    )
    .bind(workspace_id)
    .fetch_all(db)
    .await
    .map(|rows| rows.into_iter().flatten().collect())
    .unwrap_or_default()
}

fn intention_ctx(ctx: &ToolContext) -> Result<(Uuid, &sqlx::PgPool), String> {
    let ws = ctx
        .workspace_id
        .ok_or_else(|| "intention tools must be called inside a workspace".to_string())?;
    let db = ctx
        .db
        .as_ref()
        .ok_or_else(|| "intention tools require a database context".to_string())?;
    Ok((ws, db))
}

/// The one place an intention row is written.
///
/// Shared by `declare_intention` (a model choosing to register a plan) and
/// `solicit_agent_plan` (the platform recording an answer it asked for), so the
/// supersede-then-insert-then-check sequence has a single implementation and
/// the two paths cannot drift on provenance.
///
/// `source` is decided by the caller and never by the input, which is the whole
/// point of mig-218: a tool argument saying "this is the agent's own plan"
/// would be a claim the platform cannot check, made by the party with the most
/// reason to overstate it.
///
/// Takes explicit dependencies rather than a `&ToolContext` because
/// [`crate::plan_solicitation`] calls it too, and that module must not depend
/// on the tool layer's shape — the floor runs from an HTTP handler that builds
/// no `ToolContext` at all.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_intention(
    db: &sqlx::PgPool,
    embedder: &dyn EmbeddingGenerator,
    workspace_id: Uuid,
    agent_id: Uuid,
    declared_by: Option<Uuid>,
    action_type: &str,
    tool: Option<&str>,
    description: &str,
    targets: &[String],
    depends_on: &[String],
    source: crate::intentions::IntentionSource,
) -> Result<serde_json::Value, String> {
    if !matches!(
        action_type,
        "tool_call" | "research" | "synthesis" | "writing" | "review" | "idle"
    ) {
        return Err(format!("unknown action_type: {action_type}"));
    }

    // Embed the description so duplication detection is semantic. Populated
    // here, on the write path — not deferred to a worker that will not do it.
    let embedding = embedder
        .generate(description)
        .await
        .ok()
        .map(pgvector::Vector::from);
    if embedding.is_none() {
        tracing::warn!(
            %agent_id,
            "could not embed intention; duplication detection degrades to \
             resource and dependency signals for this declaration"
        );
    }

    // One live intention per agent: supersede the previous rather than
    // accumulating stale rows that generate phantom conflicts forever.
    sqlx::query(
        "UPDATE workspace_intentions
            SET status = 'superseded', resolved_at = NOW()
          WHERE workspace_id = $1 AND agent_id = $2 AND status = 'active'",
    )
    .bind(workspace_id)
    .bind(agent_id)
    .execute(db)
    .await
    .map_err(|e| format!("Failed to supersede prior intention: {e}"))?;

    let intention_id: Uuid = sqlx::query_scalar(
        "INSERT INTO workspace_intentions
           (workspace_id, agent_id, action_type, tool, description,
            targets, depends_on, embedding, source, declared_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         RETURNING intention_id",
    )
    .bind(workspace_id)
    .bind(agent_id)
    .bind(action_type)
    .bind(tool)
    .bind(description)
    .bind(targets)
    .bind(depends_on)
    .bind(embedding)
    .bind(source.as_str())
    .bind(declared_by)
    .fetch_one(db)
    .await
    .map_err(|e| format!("Failed to declare intention: {e}"))?;

    // Check immediately: an intention declared and not checked is the same as
    // no intention at all.
    let intentions = load_intentions(db, workspace_id).await?;
    let produced = produced_outputs(db, workspace_id).await;
    let conflicts = crate::intentions::detect_conflicts(
        &intentions,
        &produced,
        Some(
            &intentions
                .iter()
                .find(|i| i.intention_id == intention_id.to_string())
                .map(|i| i.agent_name.clone())
                .unwrap_or_default(),
        ),
    );
    let grounding = crate::intentions::Grounding::of(&intentions);

    Ok(json!({
        "intention_id": intention_id,
        "source": source.as_str(),
        "signal": crate::intentions::overall_signal(&conflicts),
        "conflicts": conflicts,
        "active_intentions": intentions.len(),
        // Reported on every write, not only on request. A CLEAR signal over a
        // map the team never confirmed is the reading most likely to be
        // mistaken for coordination, so the caveat travels with the signal.
        "grounding": grounding,
        "grounding_reading": grounding.reading(),
    }))
}

async fn execute_declare_intention(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let (workspace_id, db) = intention_ctx(ctx)?;
    let agent_id = resolve_agent_id(input, "agent_id", ctx).await?;

    let action_type = input
        .get("action_type")
        .and_then(|v| v.as_str())
        .unwrap_or("research");
    let description = input
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "description is required".to_string())?;
    let tool = input.get("tool").and_then(|v| v.as_str());
    let str_list = |key: &str| -> Vec<String> {
        input
            .get(key)
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let targets = str_list("targets");
    let depends_on = str_list("depends_on");

    // Provenance, derived and not asked for (mig-218).
    //
    // An agent registering its own next action is stating an intention. An
    // agent registering somebody else's is stating a belief about one, and
    // until now the two produced identical rows — which mattered because the
    // second case is the only one that has ever happened in production: the
    // strategist's Stage 0 declares on every member's behalf from a transcript.
    let source = match ctx.current_agent_id {
        Some(caller) if caller == agent_id => crate::intentions::IntentionSource::SelfDeclared,
        Some(_) => crate::intentions::IntentionSource::Inferred,
        // No caller identity: we cannot claim first-hand, so we do not.
        None => crate::intentions::IntentionSource::Unattributed,
    };

    let mut out = write_intention(
        db,
        ctx.embedder.as_ref(),
        workspace_id,
        agent_id,
        ctx.current_agent_id,
        action_type,
        tool,
        description,
        &targets,
        &depends_on,
        source,
    )
    .await?;

    if !source.is_first_hand() {
        out["note"] = json!(
            "Recorded as second-hand: you declared this for another agent, so it \
             is your reading of that agent's plan rather than its own statement. \
             Overlap detection between two second-hand rows is suppressed. Use \
             solicit_agent_plan to ask the agent directly and record what it \
             actually says."
        );
    }

    serde_json::to_string_pretty(&out).map_err(|e| format!("Serialization error: {e}"))
}

/// `solicit_agent_plan` — ask a member what it is about to do, and record what
/// it says.
///
/// # The gap this fills
///
/// Stage 0 has always been described as intention coordination, and what it
/// actually did was: the strategist read twenty messages of transcript and
/// called `declare_intention` once per member, describing what it *supposed*
/// each was about to do. No member was ever asked. Every row in every intention
/// map on the platform is one agent's guesswork about several others, and the
/// conflict checker then compared those guesses to each other.
///
/// ReMALIS (arXiv:2407.12532 §3.1) separates the two objects the platform had
/// collapsed. Agent *i* holds a private intention
/// `I_i = (γ_i, Σ_i, π_i, δ_i)` — goal, sub-goals, next-sub-goal distribution,
/// desired teammate assignment. What agent *j* can hold is a belief
/// `b_j(I_i | m_ji) = f_Λ(m_ji)`, formed from a message *i* actually sent.
/// §4.4 Table 3 measures what the difference is worth: sub-task alignment of
/// 31%/23%/17% (easy/medium/hard) with no communication against 91%/71%/62%
/// with full intention sharing.
///
/// This tool is `f_Λ`: the round trip that turns a belief into a report. It is
/// the platform's own call, so the `solicited` provenance is something the
/// platform can vouch for rather than a claim in a tool argument.
///
/// # Σ and δ are asked for, not inferred
///
/// The elicitation asks for sub-goals (`Σ_i`, as `depends_on`/`targets`) and
/// for who else the agent thinks should take what (`δ_i`). Those are the two
/// components a transcript reading cannot recover: a member's own view of its
/// dependencies, and its own view of the division of labour. `δ_i` is returned
/// to the caller rather than written, because an agent's opinion about a
/// teammate's work is not that teammate's intention — which is the exact error
/// this tool exists to stop.
///
/// # Authorisation
///
/// The target must be a member of this workspace. The caller is not otherwise
/// gated, and deliberately: `declare_intention` already lets any workspace
/// agent write a row about any other, and what this produces is a strictly
/// better-grounded version of that same row. Gating it more tightly than the
/// weaker tool would push callers toward the weaker tool.
async fn execute_solicit_agent_plan(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let (workspace_id, db) = intention_ctx(ctx)?;
    let target = resolve_agent_id(input, "agent_id", ctx).await?;

    let asker = crate::plan_solicitation::Asker {
        db: db.clone(),
        memory_store: ctx.memory_store.clone(),
        embedder: ctx.embedder.clone(),
        registry: ctx.registry.clone(),
        credentials: ctx.credentials.clone(),
    };

    // `freshness: None` — no staleness window on this path.
    //
    // The floor yields to a plan the member stated recently, because it is
    // spending an LLM call speculatively. A strategist that has read the map
    // and chosen to ask anyway has a reason the platform cannot see, and
    // overruling it here would make the tool weaker than the automatic
    // behaviour it is supposed to improve on.
    let outcome = crate::plan_solicitation::solicit(
        &asker,
        workspace_id,
        ctx.current_agent_id,
        target,
        input.get("context").and_then(|v| v.as_str()),
        None,
        ctx.parent_episode_id,
    )
    .await;

    use crate::plan_solicitation::Solicited;
    match outcome {
        Solicited::Recorded {
            intention_id,
            description,
            signal,
        } => {
            // Re-read for the caller-facing extras. `solicit` returns the
            // decision; the map view is this layer’s job.
            let intentions = load_intentions(db, workspace_id).await?;
            let grounding = crate::intentions::Grounding::of(&intentions);
            serde_json::to_string_pretty(&json!({
                "intention_id": intention_id,
                "agent_id": target,
                "description": description,
                "source": "solicited",
                "signal": signal,
                "active_intentions": intentions.len(),
                "grounding": grounding,
                "grounding_reading": grounding.reading(),
                "note": "Recorded as this agent’s own plan. Its view of who should \
                         own what is not written as anyone’s intention — solicit those \
                         agents directly if you want their answer.",
            }))
            .map_err(|e| format!("Serialization error: {e}"))
        }
        Solicited::AlreadyFresh { source } => serde_json::to_string_pretty(&json!({
            "agent_id": target,
            "status": "already_fresh",
            "source": source.as_str(),
            "note": "This member already has a current first-hand plan; nothing was asked.",
        }))
        .map_err(|e| format!("Serialization error: {e}")),
        Solicited::NotAMember => Err(format!(
            "Agent {target} is not a member of this workspace; refusing to record \
             its plan in this workspace’s intention map."
        )),
        Solicited::Unreachable { error } => Err(error),
        Solicited::Unparseable { reply_excerpt } => Err(format!(
            "That agent did not return a parseable plan, so nothing was recorded. \
             Its reply was: {reply_excerpt}"
        )),
    }
}

async fn execute_check_conflicts(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let (workspace_id, db) = intention_ctx(ctx)?;
    let intentions = load_intentions(db, workspace_id).await?;
    let produced = produced_outputs(db, workspace_id).await;

    // Optional filter, accepted as an agent name or id.
    let only = match input.get("agent_id").and_then(|v| v.as_str()) {
        Some(_) => resolve_agent_id(input, "agent_id", ctx)
            .await
            .ok()
            .and_then(|id| {
                intentions
                    .iter()
                    .find(|i| i.agent_id == id.to_string())
                    .map(|i| i.agent_name.clone())
            }),
        None => None,
    };

    let conflicts = crate::intentions::detect_conflicts(&intentions, &produced, only.as_deref());
    let grounding = crate::intentions::Grounding::of(&intentions);
    serde_json::to_string_pretty(&json!({
        "signal": crate::intentions::overall_signal(&conflicts),
        "conflicts": conflicts,
        "checked": intentions.len(),
        // A CLEAR signal means two different things depending on this, and
        // until mig-218 the caller could not tell them apart: "the team's
        // stated plans do not collide" versus "nobody has stated a plan and
        // the map is your own reading of a transcript".
        "grounding": grounding,
        "grounding_reading": grounding.reading(),
        "note": if intentions.iter().any(|i| i.embedding.is_none()) {
            Some("Some intentions carry no embedding; duplication detection is \
                  incomplete for those. Resource and dependency signals are unaffected.")
        } else { None },
    }))
    .map_err(|e| format!("Serialization error: {e}"))
}

async fn execute_get_intention_map(ctx: &ToolContext) -> Result<String, String> {
    let (workspace_id, db) = intention_ctx(ctx)?;
    let intentions = load_intentions(db, workspace_id).await?;
    let entries: Vec<serde_json::Value> = intentions
        .iter()
        .map(|i| {
            json!({
                "agent": i.agent_name,
                "action_type": i.action_type,
                "tool": i.tool,
                "description": i.description,
                "targets": i.targets,
                "depends_on": i.depends_on,
                "has_embedding": i.embedding.is_some(),
                // Whose plan this is, and who said so (mig-218). Without these
                // a map the coordinator wrote entirely by itself is
                // indistinguishable from one the team filled in.
                "source": i.source.as_str(),
                "first_hand": i.source.is_first_hand(),
                "declared_by": i.declared_by,
            })
        })
        .collect();
    let grounding = crate::intentions::Grounding::of(&intentions);
    serde_json::to_string_pretty(&json!({
        "workspace_id": workspace_id,
        "active": entries.len(),
        "intentions": entries,
        "grounding": grounding,
        "grounding_reading": grounding.reading(),
    }))
    .map_err(|e| format!("Serialization error: {e}"))
}

async fn execute_clear_intention(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let (workspace_id, db) = intention_ctx(ctx)?;
    let agent_id = resolve_agent_id(input, "agent_id", ctx).await?;
    let status = input
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("completed");
    if !matches!(status, "completed" | "cancelled" | "superseded") {
        return Err(format!("unknown status: {status}"));
    }

    let n = sqlx::query(
        "UPDATE workspace_intentions
            SET status = $3, resolved_at = NOW()
          WHERE workspace_id = $1 AND agent_id = $2 AND status = 'active'",
    )
    .bind(workspace_id)
    .bind(agent_id)
    .bind(status)
    .execute(db)
    .await
    .map_err(|e| format!("Failed to clear intention: {e}"))?
    .rows_affected();

    serde_json::to_string_pretty(&json!({
        "cleared": n,
        "status": status,
        // `completed` intentions' targets become satisfied dependencies for
        // everyone else, so clearing is what unblocks a DEPENDENCY_WAIT.
        "note": if status == "completed" {
            "Targets of this intention now count as produced outputs."
        } else {
            "Removed from conflict checks without marking its targets produced."
        },
    }))
    .map_err(|e| format!("Serialization error: {e}"))
}

async fn execute_suggest_differentiation(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let (workspace_id, db) = intention_ctx(ctx)?;
    let a_name = input
        .get("agent_a")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "agent_a is required".to_string())?;
    let b_name = input
        .get("agent_b")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "agent_b is required".to_string())?;

    let intentions = load_intentions(db, workspace_id).await?;
    let find = |n: &str| intentions.iter().find(|i| i.agent_name == n);
    let (Some(a), Some(b)) = (find(a_name), find(b_name)) else {
        return Err(format!(
            "both agents must have an active intention; have: {}",
            intentions
                .iter()
                .map(|i| i.agent_name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };

    // Report the overlap; do not prescribe the split.
    //
    // The card's own constraint is "structural, not prescriptive: name the
    // pattern, do not prescribe the fix", and it is right for a reason beyond
    // style — this tool has the two descriptions and nothing about the
    // workspace's goal, so any concrete division of labour it invented would be
    // a guess dressed as advice. The agents have the context; give them the
    // facts.
    let shared_targets: Vec<&String> = a.targets.iter().filter(|t| b.targets.contains(t)).collect();
    let similarity = match (&a.embedding, &b.embedding) {
        (Some(_), Some(_)) => {
            crate::intentions::detect_conflicts(&[a.clone(), b.clone()], &[], None)
                .into_iter()
                .find_map(|c| match c {
                    crate::intentions::Conflict::Duplication { similarity, .. } => Some(similarity),
                    _ => None,
                })
        }
        _ => None,
    };

    serde_json::to_string_pretty(&json!({
        "agent_a": {
            "name": a.agent_name, "intent": a.description, "targets": a.targets,
            "source": a.source.as_str(), "first_hand": a.source.is_first_hand(),
        },
        "agent_b": {
            "name": b.agent_name, "intent": b.description, "targets": b.targets,
            "source": b.source.as_str(), "first_hand": b.source.is_first_hand(),
        },
        "shared_targets": shared_targets,
        "description_similarity": similarity,
        // The caveat has to travel with the suggestion. Telling two agents to
        // divide work on the strength of two sentences the coordinator wrote
        // about them is the failure mode this whole column exists to name.
        "grounding_caveat": match (a.source.is_first_hand(), b.source.is_first_hand()) {
            (true, true) => None,
            (false, false) => Some(
                "NEITHER intention is first-hand. Both descriptions are your own \
                 reading, so their similarity measures your paraphrasing and not \
                 these agents' plans. Solicit both plans before asking anyone to \
                 differentiate."
            ),
            _ => Some(
                "One of these intentions is your inference rather than the \
                 agent's own statement. Say which when you raise the overlap."
            ),
        },
        "guidance": "These two intentions overlap on the axes above. Decide the                      split yourselves — you have the workspace goal and this tool                      does not. State the division explicitly in the conversation                      so the other agent can rely on it.",
    }))
    .map_err(|e| format!("Serialization error: {e}"))
}

async fn execute_emit_coherence_signal(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let (workspace_id, db) = intention_ctx(ctx)?;
    let relation_type = input
        .get("relation_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !matches!(relation_type, "IntentionAligns" | "IntentionConflicts") {
        return Err("relation_type must be IntentionAligns or IntentionConflicts".to_string());
    }
    let strength = input
        .get("strength")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);
    let rationale = input.get("rationale").and_then(|v| v.as_str());

    let resolve = |key: &'static str| async move {
        let v = input.get(key).and_then(|x| x.as_str()).unwrap_or("");
        sqlx::query_scalar::<_, Uuid>("SELECT agent_id FROM agents WHERE agent_name = $1")
            .bind(v)
            .fetch_optional(db)
            .await
            .ok()
            .flatten()
            .ok_or_else(|| format!("{key} does not name a known agent: {v}"))
    };
    let agent_a = resolve("agent_a").await?;
    let agent_b = resolve("agent_b").await?;

    sqlx::query(
        "INSERT INTO workspace_intention_signals
           (workspace_id, relation_type, agent_a, agent_b, strength, rationale)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(workspace_id)
    .bind(relation_type)
    .bind(agent_a)
    .bind(agent_b)
    .bind(strength)
    .bind(rationale)
    .execute(db)
    .await
    .map_err(|e| format!("Failed to record signal: {e}"))?;

    // Post it into the conversation as well, because that is what actually
    // reaches coherence: `ConversationObserver::observe` builds the TEC graph
    // from workspace messages. A row in a table nothing reads would be the
    // deferred-work pattern again.
    let a_name = input.get("agent_a").and_then(|v| v.as_str()).unwrap_or("?");
    let b_name = input.get("agent_b").and_then(|v| v.as_str()).unwrap_or("?");
    let body = match rationale {
        Some(r) => {
            format!("**{relation_type}** — {a_name} ↔ {b_name} (strength {strength:.2}): {r}")
        }
        None => format!("**{relation_type}** — {a_name} ↔ {b_name} (strength {strength:.2})"),
    };
    let posted = sqlx::query(
        "INSERT INTO workspace_messages
           (message_id, workspace_id, sender_type, sender_id, sender_name, content, message_type)
         VALUES (gen_random_uuid(), $1, 'system', 'intention_coordinator',
                 'Intention Coordinator', $2, 'intention_signal')",
    )
    .bind(workspace_id)
    .bind(&body)
    .execute(db)
    .await;
    if let Err(e) = &posted {
        tracing::warn!(error = %e, "intention signal recorded but not posted to the conversation");
    }

    serde_json::to_string_pretty(&json!({
        "relation_type": relation_type,
        "strength": strength,
        "recorded": true,
        "posted_to_conversation": posted.is_ok(),
    }))
    .map_err(|e| format!("Serialization error: {e}"))
}

// ─── Tool implementations ──────────────────────────────────────────

/// Search Polymarket for events matching a query.
/// Used by orchestra agents (especially prediction_market) during research.
async fn execute_polymarket_search(input: &serde_json::Value) -> Result<String, String> {
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: query")?;
    let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

    let gamma = crate::polymarket::GammaClient::new();
    let events = gamma
        .search_events(query, limit)
        .await
        .map_err(|e| format!("Polymarket search failed: {}", e))?;

    if events.is_empty() {
        return Ok("No matching Polymarket markets found for this query.".to_string());
    }

    let mut output = String::new();
    for event in &events {
        output.push_str(&format!("## {}\n", event.title));
        output.push_str(&format!(
            "Event ID: {} | Volume 24h: ${:.0} | Liquidity: ${:.0}\n",
            event.id, event.volume_24hr, event.liquidity
        ));
        if let Some(ref end) = event.end_date {
            output.push_str(&format!("End date: {}\n", end));
        }
        for market in &event.markets {
            let processed = crate::polymarket::process_market_public(event, market);
            output.push_str(&format!(
                "  → {} | YES: {:.1}% | bid/ask: {:.3}/{:.3} | vol24h: ${:.0} | confidence: {}\n",
                processed.question,
                processed.market_price * 100.0,
                processed.bid_price,
                processed.ask_price,
                processed.volume_24h,
                processed.confidence_signal.label(),
            ));
            if let Some(ref change) = processed.price_change_1w {
                output.push_str(&format!("    1-week change: {:+.1}pp\n", change * 100.0));
            }
        }
        output.push('\n');
    }

    // Truncate if very large
    if output.len() > 24_000 {
        output.truncate(24_000);
        output.push_str("\n... [truncated]");
    }

    Ok(output)
}

/// Get details for a specific Polymarket event by ID.
async fn execute_polymarket_event(input: &serde_json::Value) -> Result<String, String> {
    let event_id = input
        .get("event_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: event_id")?;

    let gamma = crate::polymarket::GammaClient::new();
    let event = gamma
        .get_event(event_id)
        .await
        .map_err(|e| format!("Polymarket event fetch failed: {}", e))?;

    let mut output = String::new();
    output.push_str(&format!("# {}\n\n", event.title));
    output.push_str(&format!(
        "Description: {}\n\n",
        &event.description[..event.description.len().min(500)]
    ));
    output.push_str(&format!(
        "Total volume: ${:.0} | 24h volume: ${:.0} | Liquidity: ${:.0}\n",
        event.volume, event.volume_24hr, event.liquidity
    ));
    if let Some(ref end) = event.end_date {
        output.push_str(&format!("End date: {}\n", end));
    }
    output.push_str(&format!(
        "Active: {} | Closed: {}\n\n",
        event.active, event.closed
    ));

    output.push_str("## Markets\n\n");
    for market in &event.markets {
        let processed = crate::polymarket::process_market_public(&event, market);
        output.push_str(&format!("### {}\n", processed.question));
        output.push_str(&format!("  Market ID: {}\n", processed.pm_market_id));
        output.push_str(&format!(
            "  YES price: {:.1}% (midpoint: {:.1}%)\n",
            processed.market_price * 100.0,
            processed.midpoint_price * 100.0
        ));
        output.push_str(&format!(
            "  Bid/Ask: {:.3} / {:.3} (spread: {:.3})\n",
            processed.bid_price, processed.ask_price, processed.spread
        ));
        output.push_str(&format!(
            "  Volume 24h: ${:.0} | Total: ${:.0}\n",
            processed.volume_24h, processed.volume_total
        ));
        output.push_str(&format!("  Liquidity: ${:.0}\n", processed.liquidity));
        output.push_str(&format!(
            "  Confidence: {} ({:.0}% quality)\n",
            processed.confidence_signal.label(),
            processed.confidence_signal.quality_score() * 100.0
        ));
        if let Some(change) = processed.price_change_1w {
            output.push_str(&format!(
                "  1-week price change: {:+.1}pp\n",
                change * 100.0
            ));
        }
        if let Some(change) = processed.price_change_1m {
            output.push_str(&format!(
                "  1-month price change: {:+.1}pp\n",
                change * 100.0
            ));
        }
        output.push_str(&format!(
            "  Status: {}\n",
            if processed.resolved {
                "RESOLVED"
            } else if processed.closed {
                "CLOSED"
            } else if processed.active {
                "ACTIVE"
            } else {
                "INACTIVE"
            }
        ));
        if let Some(ref group) = processed.group_item_title {
            output.push_str(&format!("  Group: {}\n", group));
        }
        output.push('\n');
    }

    output.push_str(&format!(
        "Tags: {}\n",
        event
            .tags
            .iter()
            .map(|t| t.label.clone())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    output.push_str(&format!(
        "URL: https://polymarket.com/event/{}\n",
        event.slug
    ));

    Ok(output)
}

/// Generic FMP API executor — builds a GET request from the input parameters
/// and the endpoint path. Appends the FMP API key from env or hardcoded fallback.
async fn execute_fmp_api(
    input: &serde_json::Value,
    endpoint: &str,
    param_names: &[&str],
) -> Result<String, String> {
    let api_key = std::env::var("FMP_API_KEY")
        .unwrap_or_else(|_| "xadhcaZJ9suK6jthYq2axsDINSE31Nxj".to_string());

    let base_url = "https://financialmodelingprep.com";
    let mut url = format!("{}{}", base_url, endpoint);

    // Build query string from known parameter names
    let mut params: Vec<(String, String)> = Vec::new();
    for &name in param_names {
        if let Some(val) = input.get(name) {
            let s = match val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                other => other.to_string().trim_matches('"').to_string(),
            };
            if !s.is_empty() {
                params.push((name.to_string(), s));
            }
        }
    }
    params.push(("apikey".to_string(), api_key));

    let query_string: String = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&");

    url = format!("{}?{}", url, query_string);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "FermiConsole/1.0")
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("FMP API request failed: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read FMP response: {}", e))?;

    if !status.is_success() {
        return Err(format!(
            "FMP API error (HTTP {}): {}",
            status.as_u16(),
            body
        ));
    }

    // If response is empty array, return a clear message
    if body.trim() == "[]" {
        return Ok("No data found for the given parameters.".to_string());
    }

    // Compact the JSON if it's very large (>8k chars) — keep structure but trim
    if body.len() > 8000 {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&body) {
            // For arrays, limit to first 3 entries to save token budget
            if let Some(arr) = parsed.as_array() {
                let limited: Vec<&serde_json::Value> = arr.iter().take(3).collect();
                let note = if arr.len() > 3 {
                    format!("\n[Showing 3 of {} results]", arr.len())
                } else {
                    String::new()
                };
                return Ok(format!(
                    "{}{}",
                    serde_json::to_string_pretty(&limited).unwrap_or(body),
                    note
                ));
            }
        }
    }

    Ok(body)
}

async fn execute_search_knowledge(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: query")?;
    let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

    let agent_id = ctx
        .current_agent_id
        .ok_or("No agent context for search_knowledge")?;

    // Generate embedding for the query
    let embedding = ctx
        .embedder
        .generate(query)
        .await
        .map_err(|e| format!("Embedding generation failed: {}", e))?;

    // Search similar episodes
    let results = ctx
        .memory_store
        .search_similar_episodes(agent_id, &embedding, limit)
        .await
        .map_err(|e| format!("Search failed: {}", e))?;

    // Format results
    let formatted: Vec<serde_json::Value> = results
        .iter()
        .map(|(episode, distance)| {
            json!({
                "query": episode.query,
                "context": episode.context,
                "timestamp": episode.timestamp_ref.to_rfc3339(),
                "similarity": 1.0 - distance,
            })
        })
        .collect();

    serde_json::to_string_pretty(&formatted).map_err(|e| format!("Serialization error: {}", e))
}

// ─── GBIF Taxonomy Tree ────────────────────────────────────────────

async fn execute_gbif_taxonomy_tree(input: &serde_json::Value) -> Result<String, String> {
    let client = reqwest::Client::new();
    let ua = "AgentBestiaryWorld/1.0 (rabble.world)";

    // Resolve GBIF key — either directly provided or via name search
    let gbif_key: i64 = if let Some(key) = input.get("gbif_key").and_then(|v| v.as_i64()) {
        key
    } else if let Some(name) = input.get("scientific_name").and_then(|v| v.as_str()) {
        let resp = client
            .get("https://api.gbif.org/v1/species/match")
            .query(&[("name", name), ("kingdom", "Animalia")])
            .header("User-Agent", ua)
            .send()
            .await
            .map_err(|e| format!("GBIF match failed: {}", e))?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Parse error: {}", e))?;
        body.get("usageKey")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| format!("No GBIF match for '{}'", name))?
    } else {
        return Err("Either 'gbif_key' or 'scientific_name' is required".to_string());
    };

    // Fetch the species record (includes full taxonomy)
    let species_url = format!("https://api.gbif.org/v1/species/{}", gbif_key);
    let species_resp = client
        .get(&species_url)
        .header("User-Agent", ua)
        .send()
        .await
        .map_err(|e| format!("GBIF species fetch failed: {}", e))?;
    let species: serde_json::Value = species_resp
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    // Fetch parent chain (full classification)
    let parents_url = format!("https://api.gbif.org/v1/species/{}/parents", gbif_key);
    let parents_resp = client
        .get(&parents_url)
        .header("User-Agent", ua)
        .send()
        .await
        .map_err(|e| format!("GBIF parents fetch failed: {}", e))?;
    let parents: serde_json::Value = parents_resp
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    // Fetch siblings at family level (for phylogenetic context)
    let family_key = species.get("familyKey").and_then(|v| v.as_i64());
    let siblings = if let Some(fk) = family_key {
        let sibs_url = format!("https://api.gbif.org/v1/species/{}/children?limit=10", fk);
        let sibs_resp = client
            .get(&sibs_url)
            .header("User-Agent", ua)
            .send()
            .await
            .ok();
        if let Some(r) = sibs_resp {
            r.json::<serde_json::Value>().await.ok()
        } else {
            None
        }
    } else {
        None
    };

    // Fetch siblings at order level (other families in same order)
    let order_key = species.get("orderKey").and_then(|v| v.as_i64());
    let order_children = if let Some(ok) = order_key {
        let url = format!("https://api.gbif.org/v1/species/{}/children?limit=20", ok);
        let resp = client.get(&url).header("User-Agent", ua).send().await.ok();
        if let Some(r) = resp {
            r.json::<serde_json::Value>().await.ok()
        } else {
            None
        }
    } else {
        None
    };

    let result = json!({
        "species": species,
        "parents": parents,
        "family_siblings": siblings.unwrap_or(json!({"results": []})),
        "order_families": order_children.unwrap_or(json!({"results": []})),
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

// ─── activate_formation tool ───────────────────────────────────────

async fn execute_activate_formation(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let algorithm_name = input
        .get("algorithm_name")
        .and_then(|v| v.as_str())
        .ok_or("algorithm_name is required")?;
    let swarm_id_str = input
        .get("swarm_id")
        .and_then(|v| v.as_str())
        .ok_or("swarm_id is required")?;
    let swarm_id: uuid::Uuid = swarm_id_str
        .parse()
        .map_err(|_| "Invalid swarm_id UUID".to_string())?;

    let db = ctx.db.as_ref().ok_or("Database not available")?;
    let user_id = ctx
        .user_id
        .as_ref()
        .ok_or("User context required for formation activation")?;

    // Look up algorithm
    let algorithm = sqlx::query(
        "SELECT algorithm_id, name, display_name, formation_spec, tier, cost_credits \
         FROM swarm_algorithms WHERE name = $1",
    )
    .bind(algorithm_name)
    .fetch_optional(db)
    .await
    .map_err(|e| format!("DB error: {}", e))?
    .ok_or_else(|| format!("Algorithm '{}' not found", algorithm_name))?;

    let algorithm_id: uuid::Uuid = algorithm.get("algorithm_id");
    let display_name: String = algorithm.get("display_name");
    let formation_spec: serde_json::Value = algorithm.get("formation_spec");
    let tier: String = algorithm.get("tier");
    let cost: i32 = algorithm.get("cost_credits");

    // Free algorithms return spec directly
    if tier == "free" {
        let result = json!({
            "algorithm_id": algorithm_id,
            "name": algorithm_name,
            "display_name": display_name,
            "formation_spec": formation_spec,
            "activated": true,
            "charged": false,
        });
        return serde_json::to_string_pretty(&result)
            .map_err(|e| format!("Serialization error: {}", e));
    }

    // Check idempotency
    let existing = sqlx::query(
        "SELECT activation_id FROM swarm_activations \
         WHERE user_id = $1 AND swarm_id = $2 AND algorithm_id = $3",
    )
    .bind(user_id)
    .bind(swarm_id)
    .bind(algorithm_id)
    .fetch_optional(db)
    .await
    .map_err(|e| format!("DB error: {}", e))?;

    if existing.is_some() {
        let result = json!({
            "algorithm_id": algorithm_id,
            "name": algorithm_name,
            "display_name": display_name,
            "formation_spec": formation_spec,
            "activated": true,
            "charged": false,
            "message": "Already activated for this session",
        });
        return serde_json::to_string_pretty(&result)
            .map_err(|e| format!("Serialization error: {}", e));
    }

    // Charge credits
    let wallet = fermi_auth::get_or_create_wallet(db, "user", user_id)
        .await
        .map_err(|e| format!("Wallet error: {}", e))?;

    fermi_auth::credit_charge(
        db,
        wallet.wallet_id,
        cost,
        "formation_activate",
        &format!("Activate {} formation", display_name),
        Some(&algorithm_id.to_string()),
    )
    .await
    .map_err(|e| format!("Payment failed: {}", e))?;

    // Insert activation
    let activation_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO swarm_activations (activation_id, algorithm_id, user_id, swarm_id) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(activation_id)
    .bind(algorithm_id)
    .bind(user_id)
    .bind(swarm_id)
    .execute(db)
    .await
    .map_err(|e| format!("DB error: {}", e))?;

    let result = json!({
        "algorithm_id": algorithm_id,
        "activation_id": activation_id,
        "name": algorithm_name,
        "display_name": display_name,
        "formation_spec": formation_spec,
        "activated": true,
        "charged": true,
        "cost_credits": cost,
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Enemy Sensor tool implementation ──────────────────────────────

async fn execute_scan_nearby_creatures(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    use h3o::CellIndex;

    let creature_id_str = input
        .get("creature_id")
        .and_then(|v| v.as_str())
        .ok_or("creature_id is required")?;
    let creature_id: Uuid = creature_id_str
        .parse()
        .map_err(|_| "Invalid creature_id UUID".to_string())?;
    let radius = input
        .get("radius_rings")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;

    let pool = ctx.memory_store.pool();

    // 1. Look up target creature's current state + species info
    //    LEFT JOIN creature_state — creature may not have a state row yet (pre-flight)
    //    Fallback: use latest creature_flights for location
    let target = sqlx::query(
        "SELECT c.creature_id, c.scientific_name, c.common_name, c.species_group,
                c.taxonomy,
                COALESCE(NULLIF(cs.h3_cell, ''), NULLIF(cf.h3_cell, '')) AS h3_cell,
                COALESCE(cs.location_lat, cf.center_lat) AS location_lat,
                COALESCE(cs.location_lng, cf.center_lng) AS location_lng,
                cs.rabble_id, cs.state
         FROM creatures c
         LEFT JOIN creature_state cs ON cs.creature_id = c.creature_id
         LEFT JOIN LATERAL (
             SELECT h3_cell, center_lat, center_lng FROM creature_flights
             WHERE creature_id = c.creature_id ORDER BY started_at DESC LIMIT 1
         ) cf ON true
         WHERE c.creature_id = $1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error: {}", e))?
    .ok_or("Creature not found")?;

    let h3_cell: Option<String> = target
        .try_get("h3_cell")
        .ok()
        .flatten()
        .filter(|s: &String| !s.is_empty());

    // Fallback: compute h3_cell from lat/lng if missing
    let h3_cell = match h3_cell {
        Some(c) => c,
        None => {
            let lat: Option<f64> = target.try_get("location_lat").ok().flatten();
            let lng: Option<f64> = target.try_get("location_lng").ok().flatten();
            match (lat, lng) {
                (Some(lat), Some(lng)) if lat != 0.0 || lng != 0.0 => {
                    use h3o::{LatLng, Resolution};
                    LatLng::new(lat, lng)
                        .map(|ll| ll.to_cell(Resolution::Twelve).to_string())
                        .map_err(|_| "Creature has no valid location".to_string())?
                }
                _ => return Err("Creature has no location — perch or fly first".to_string()),
            }
        }
    };

    let taxonomy: Option<serde_json::Value> = target.try_get("taxonomy").ok().flatten();
    let order = taxonomy
        .as_ref()
        .and_then(|t| t.get("order"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");
    let family = taxonomy
        .as_ref()
        .and_then(|t| t.get("family"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");

    let target_info = json!({
        "creature_id": creature_id,
        "scientific_name": target.try_get::<Option<String>, _>("scientific_name").unwrap_or(None),
        "common_name": target.try_get::<Option<String>, _>("common_name").unwrap_or(None),
        "species_group": target.try_get::<Option<String>, _>("species_group").unwrap_or(None),
        "order": order,
        "family": family,
        "h3_cell": &h3_cell,
        "lat": target.try_get::<Option<f64>, _>("location_lat").unwrap_or(None),
        "lng": target.try_get::<Option<f64>, _>("location_lng").unwrap_or(None),
        "rabble_id": target.try_get::<Option<Uuid>, _>("rabble_id").ok().flatten(),
    });

    // 2. Compute H3 grid disk
    let center_cell: CellIndex = h3_cell
        .parse()
        .map_err(|e| format!("Invalid H3 cell '{}': {}", h3_cell, e))?;
    let disk: Vec<CellIndex> = center_cell.grid_disk::<Vec<_>>(radius);
    let cell_strings: Vec<String> = disk.iter().map(|c| c.to_string()).collect();

    // 3. Query nearby creatures (excluding target, excluding private)
    //    Use LATERAL fallback to creature_flights for creatures without creature_state
    let placeholders: Vec<String> = (1..=cell_strings.len())
        .map(|i| format!("${}", i))
        .collect();
    let in_clause = placeholders.join(", ");

    let sql = format!(
        "SELECT c.creature_id, c.scientific_name, c.common_name, c.species_group,
                c.taxonomy,
                COALESCE(NULLIF(cs.h3_cell, ''), NULLIF(cf.h3_cell, '')) AS h3_cell,
                cs.rabble_id,
                COALESCE(cc.visibility, 'public') AS visibility
         FROM creatures c
         LEFT JOIN creature_state cs ON cs.creature_id = c.creature_id
         LEFT JOIN LATERAL (
             SELECT h3_cell FROM creature_flights
             WHERE creature_id = c.creature_id ORDER BY started_at DESC LIMIT 1
         ) cf ON cs.h3_cell IS NULL
         LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
         WHERE COALESCE(NULLIF(cs.h3_cell, ''), NULLIF(cf.h3_cell, '')) IN ({})
           AND c.creature_id != ${}
           AND COALESCE(cc.visibility, 'public') != 'private'
         LIMIT 50",
        in_clause,
        cell_strings.len() + 1
    );

    let mut query = sqlx::query(&sql);
    for cs in &cell_strings {
        query = query.bind(cs);
    }
    query = query.bind(creature_id);

    let rows = query
        .fetch_all(pool)
        .await
        .map_err(|e| format!("DB error: {}", e))?;

    let target_rabble: Option<Uuid> = target
        .try_get::<Option<Uuid>, _>("rabble_id")
        .ok()
        .flatten();

    let nearby: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let tax: Option<serde_json::Value> = r.try_get("taxonomy").ok().flatten();
            let nearby_rabble: Option<Uuid> =
                r.try_get::<Option<Uuid>, _>("rabble_id").ok().flatten();
            let in_same_rabble = match (&target_rabble, &nearby_rabble) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            };
            json!({
                "creature_id": r.get::<Uuid, _>("creature_id"),
                "scientific_name": r.try_get::<Option<String>, _>("scientific_name").unwrap_or(None),
                "common_name": r.try_get::<Option<String>, _>("common_name").unwrap_or(None),
                "species_group": r.try_get::<Option<String>, _>("species_group").unwrap_or(None),
                "order": tax.as_ref().and_then(|t| t.get("order")).and_then(|v| v.as_str()).unwrap_or("Unknown"),
                "family": tax.as_ref().and_then(|t| t.get("family")).and_then(|v| v.as_str()).unwrap_or("Unknown"),
                "h3_cell": r.try_get::<Option<String>, _>("h3_cell").unwrap_or(None),
                "in_same_rabble": in_same_rabble,
            })
        })
        .collect();

    let result = json!({
        "target": target_info,
        "nearby_count": nearby.len(),
        "nearby": nearby,
        "radius_rings": radius,
        "cells_searched": cell_strings.len(),
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

// ─── AR Spatial Suite tool implementations ─────────────────────────

async fn execute_h3_resolve(input: &serde_json::Value) -> Result<String, String> {
    use h3o::{CellIndex, LatLng, Resolution};

    let operation = input
        .get("operation")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: operation")?;

    let parse_resolution = |input: &serde_json::Value| -> Result<Resolution, String> {
        let res = input
            .get("resolution")
            .and_then(|v| v.as_u64())
            .unwrap_or(12) as u8;
        Resolution::try_from(res).map_err(|_| format!("Invalid resolution: {}. Must be 0-15.", res))
    };

    let parse_cell = |s: &str| -> Result<CellIndex, String> {
        s.parse::<CellIndex>()
            .map_err(|e| format!("Invalid H3 cell '{}': {}", s, e))
    };

    match operation {
        "gps_to_h3" => {
            let lat = input
                .get("lat")
                .and_then(|v| v.as_f64())
                .ok_or("gps_to_h3 requires 'lat'")?;
            let lng = input
                .get("lng")
                .and_then(|v| v.as_f64())
                .ok_or("gps_to_h3 requires 'lng'")?;
            let resolution = parse_resolution(input)?;

            let ll = LatLng::new(lat, lng).map_err(|e| format!("Invalid coordinates: {}", e))?;
            let cell = ll.to_cell(resolution);
            let center = LatLng::from(cell);

            let result = json!({
                "h3_cell": cell.to_string(),
                "resolution": u8::from(resolution),
                "center_lat": f64::from(center.lat()),
                "center_lng": f64::from(center.lng()),
                "input_lat": lat,
                "input_lng": lng,
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        "h3_to_gps" => {
            let cell_str = input
                .get("h3_cell")
                .and_then(|v| v.as_str())
                .ok_or("h3_to_gps requires 'h3_cell'")?;
            let cell = parse_cell(cell_str)?;
            let center = LatLng::from(cell);

            let result = json!({
                "h3_cell": cell.to_string(),
                "resolution": u8::from(cell.resolution()),
                "lat": f64::from(center.lat()),
                "lng": f64::from(center.lng()),
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        "neighbors" => {
            let cell_str = input
                .get("h3_cell")
                .and_then(|v| v.as_str())
                .ok_or("neighbors requires 'h3_cell'")?;
            let cell = parse_cell(cell_str)?;

            // grid_disk(1) returns center + 6 neighbors
            let disk: Vec<CellIndex> = cell.grid_disk::<Vec<_>>(1);
            let neighbors: Vec<serde_json::Value> = disk
                .iter()
                .filter(|c| **c != cell)
                .map(|c| {
                    let ll = LatLng::from(*c);
                    json!({
                        "h3_cell": c.to_string(),
                        "lat": f64::from(ll.lat()),
                        "lng": f64::from(ll.lng()),
                    })
                })
                .collect();

            let result = json!({
                "center": cell.to_string(),
                "neighbors": neighbors,
                "count": neighbors.len(),
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        "distance" => {
            let cell_a_str = input
                .get("h3_cell")
                .and_then(|v| v.as_str())
                .ok_or("distance requires 'h3_cell'")?;
            let cell_b_str = input
                .get("h3_cell_b")
                .and_then(|v| v.as_str())
                .ok_or("distance requires 'h3_cell_b'")?;
            let cell_a = parse_cell(cell_a_str)?;
            let cell_b = parse_cell(cell_b_str)?;

            let distance = cell_a
                .grid_distance(cell_b)
                .map_err(|_| "Cannot compute distance between cells at different resolutions or too far apart")?;

            let result = json!({
                "cell_a": cell_a.to_string(),
                "cell_b": cell_b.to_string(),
                "grid_distance": distance,
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        "grid_disk" => {
            let lat = input.get("lat").and_then(|v| v.as_f64());
            let lng = input.get("lng").and_then(|v| v.as_f64());
            let cell_str = input.get("h3_cell").and_then(|v| v.as_str());
            let k = input.get("k").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let resolution = parse_resolution(input)?;

            let center_cell = if let Some(cs) = cell_str {
                parse_cell(cs)?
            } else if let (Some(lat), Some(lng)) = (lat, lng) {
                let ll = LatLng::new(lat, lng)
                    .map_err(|e| format!("Invalid coordinates: {}", e))?;
                ll.to_cell(resolution)
            } else {
                return Err("grid_disk requires either 'h3_cell' or 'lat'+'lng'".to_string());
            };

            let disk: Vec<CellIndex> = center_cell.grid_disk::<Vec<_>>(k);
            let cells: Vec<serde_json::Value> = disk
                .iter()
                .map(|c| {
                    let ll = LatLng::from(*c);
                    json!({
                        "h3_cell": c.to_string(),
                        "lat": f64::from(ll.lat()),
                        "lng": f64::from(ll.lng()),
                    })
                })
                .collect();

            let total = 3 * k * k + 3 * k + 1;
            let result = json!({
                "center": center_cell.to_string(),
                "k": k,
                "resolution": u8::from(resolution),
                "total_cells": total,
                "cells": cells,
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        other => Err(format!(
            "Unknown h3_resolve operation: '{}'. Use: gps_to_h3, h3_to_gps, neighbors, distance, grid_disk",
            other
        )),
    }
}

async fn execute_geocode(input: &serde_json::Value) -> Result<String, String> {
    let address = input
        .get("address")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: address")?;

    let client = reqwest::Client::new();
    let response = client
        .get("https://nominatim.openstreetmap.org/search")
        .query(&[
            ("q", address),
            ("format", "json"),
            ("limit", "3"),
            ("addressdetails", "1"),
        ])
        .header("User-Agent", "AgentBestiary/1.0 (AR Spatial Suite)")
        .send()
        .await
        .map_err(|e| format!("Geocoding request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Nominatim error: {}", response.status()));
    }

    let results: Vec<serde_json::Value> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse geocoding response: {}", e))?;

    if results.is_empty() {
        return Ok(json!({
            "status": "not_found",
            "message": format!("No results for '{}'. Try a more specific address or use GPS coordinates directly.", address)
        }).to_string());
    }

    let formatted: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            let lat: f64 = r
                .get("lat")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            let lng: f64 = r
                .get("lon")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);

            json!({
                "lat": lat,
                "lng": lng,
                "display_name": r.get("display_name").and_then(|v| v.as_str()).unwrap_or(""),
                "type": r.get("type").and_then(|v| v.as_str()).unwrap_or(""),
                "importance": r.get("importance").and_then(|v| v.as_f64()).unwrap_or(0.0),
            })
        })
        .collect();

    let result = json!({
        "query": address,
        "results": formatted,
        "best_match": formatted.first(),
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_create_beacon(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    use h3o::{LatLng, Resolution};

    let workspace_id = ctx
        .workspace_id
        .ok_or("create_beacon requires a workspace context")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("create_beacon requires a user context")?;

    let lat = input
        .get("lat")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: lat")?;
    let lng = input
        .get("lng")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: lng")?;
    let asset_path = input
        .get("asset_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: asset_path")?;

    let res_num = input
        .get("resolution")
        .and_then(|v| v.as_u64())
        .unwrap_or(12) as u8;
    let resolution =
        Resolution::try_from(res_num).map_err(|_| format!("Invalid resolution: {}", res_num))?;

    let ll = LatLng::new(lat, lng).map_err(|e| format!("Invalid coordinates: {}", e))?;
    let cell = ll.to_cell(resolution);
    let center = LatLng::from(cell);

    let asset_type = input
        .get("asset_type")
        .and_then(|v| v.as_str())
        .unwrap_or("image");
    let azimuth = input
        .get("azimuth_deg")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let elevation = input
        .get("elevation_deg")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let billboard = input
        .get("billboard")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let scale = input.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let ttl_seconds = input
        .get("ttl_seconds")
        .and_then(|v| v.as_i64())
        .unwrap_or(86400) as i32;
    let decay_style = input
        .get("decay_style")
        .and_then(|v| v.as_str())
        .unwrap_or("fade");
    let visibility = input
        .get("visibility")
        .and_then(|v| v.as_str())
        .unwrap_or("public");
    let tags = input.get("tags").cloned().unwrap_or(json!([]));
    let interaction = input.get("interaction").cloned().unwrap_or(json!({}));

    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::seconds(ttl_seconds as i64);
    let beacon_id = Uuid::new_v4();

    let pool = ctx.memory_store.pool();
    sqlx::query(
        "INSERT INTO ar_beacons (beacon_id, workspace_id, creator_id, agent_name,
         h3_cell, h3_resolution, center_lat, center_lng,
         asset_path, asset_type,
         azimuth_deg, elevation_deg, billboard, scale,
         ttl_seconds, decay_style, expires_at,
         visibility, tags, interaction, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $21)"
    )
    .bind(beacon_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind("ar_beacon")
    .bind(cell.to_string())
    .bind(res_num as i32)
    .bind(f64::from(center.lat()))
    .bind(f64::from(center.lng()))
    .bind(asset_path)
    .bind(asset_type)
    .bind(azimuth)
    .bind(elevation)
    .bind(billboard)
    .bind(scale)
    .bind(ttl_seconds)
    .bind(decay_style)
    .bind(expires_at)
    .bind(visibility)
    .bind(&tags)
    .bind(&interaction)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create beacon: {}", e))?;

    let result = json!({
        "beacon_id": beacon_id,
        "h3_cell": cell.to_string(),
        "h3_resolution": res_num,
        "center_lat": f64::from(center.lat()),
        "center_lng": f64::from(center.lng()),
        "asset_path": asset_path,
        "asset_url": format!("/api/beacons/{}/asset", beacon_id),
        "expires_at": expires_at.to_rfc3339(),
        "ttl_seconds": ttl_seconds,
        "decay_style": decay_style,
        "visibility": visibility,
        "orientation": {
            "azimuth_deg": azimuth,
            "elevation_deg": elevation,
            "billboard": billboard,
        },
        "scale": scale,
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_query_beacons(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    use h3o::{CellIndex, LatLng, Resolution};

    let radius = input
        .get("radius_rings")
        .and_then(|v| v.as_u64())
        .unwrap_or(3) as u32;
    let res_num = input
        .get("resolution")
        .and_then(|v| v.as_u64())
        .unwrap_or(12) as u8;
    let include_expired = input
        .get("include_expired")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let resolution =
        Resolution::try_from(res_num).map_err(|_| format!("Invalid resolution: {}", res_num))?;

    // Resolve center cell from h3_cell or lat/lng
    let center_cell = if let Some(cs) = input.get("h3_cell").and_then(|v| v.as_str()) {
        cs.parse::<CellIndex>()
            .map_err(|e| format!("Invalid H3 cell: {}", e))?
    } else {
        let lat = input
            .get("lat")
            .and_then(|v| v.as_f64())
            .ok_or("query_beacons requires 'h3_cell' or 'lat'+'lng'")?;
        let lng = input
            .get("lng")
            .and_then(|v| v.as_f64())
            .ok_or("query_beacons requires 'lng'")?;
        let ll = LatLng::new(lat, lng).map_err(|e| format!("Invalid coordinates: {}", e))?;
        ll.to_cell(resolution)
    };

    // Compute all cells in the search radius
    let disk: Vec<CellIndex> = center_cell.grid_disk::<Vec<_>>(radius);
    let cell_strings: Vec<String> = disk.iter().map(|c| c.to_string()).collect();

    let pool = ctx.memory_store.pool();

    // Build query with IN clause for H3 cells
    let placeholders: Vec<String> = (1..=cell_strings.len())
        .map(|i| format!("${}", i))
        .collect();
    let in_clause = placeholders.join(", ");

    let time_filter = if include_expired {
        "".to_string()
    } else {
        format!(" AND expires_at > ${}", cell_strings.len() + 1)
    };

    let sql = format!(
        "SELECT beacon_id, workspace_id, h3_cell, h3_resolution, center_lat, center_lng,
                asset_path, asset_type, azimuth_deg, elevation_deg, billboard, scale,
                ttl_seconds, decay_style, expires_at, visibility, tags, interaction,
                created_at
         FROM ar_beacons WHERE h3_cell IN ({}){}
         ORDER BY created_at DESC LIMIT 100",
        in_clause, time_filter
    );

    let mut query = sqlx::query(&sql);
    for cs in &cell_strings {
        query = query.bind(cs);
    }
    if !include_expired {
        query = query.bind(chrono::Utc::now());
    }

    let rows = query
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Beacon query failed: {}", e))?;

    let beacons: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            json!({
                "beacon_id": row.get::<Uuid, _>("beacon_id"),
                "workspace_id": row.get::<Uuid, _>("workspace_id"),
                "h3_cell": row.get::<String, _>("h3_cell"),
                "center_lat": row.get::<f64, _>("center_lat"),
                "center_lng": row.get::<f64, _>("center_lng"),
                "asset_path": row.get::<String, _>("asset_path"),
                "asset_type": row.get::<String, _>("asset_type"),
                "asset_url": format!("/api/beacons/{}/asset", row.get::<Uuid, _>("beacon_id")),
                "orientation": {
                    "azimuth_deg": row.get::<f64, _>("azimuth_deg"),
                    "elevation_deg": row.get::<f64, _>("elevation_deg"),
                    "billboard": row.get::<bool, _>("billboard"),
                },
                "scale": row.get::<f64, _>("scale"),
                "expires_at": row.get::<chrono::DateTime<chrono::Utc>, _>("expires_at").to_rfc3339(),
                "visibility": row.get::<String, _>("visibility"),
                "tags": row.get::<serde_json::Value, _>("tags"),
                "interaction": row.get::<serde_json::Value, _>("interaction"),
            })
        })
        .collect();

    let result = json!({
        "center": center_cell.to_string(),
        "radius_rings": radius,
        "total_beacons": beacons.len(),
        "beacons": beacons,
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_save_grid_map(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    use h3o::{LatLng, Resolution};

    let workspace_id = ctx
        .workspace_id
        .ok_or("save_grid_map requires a workspace context")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("save_grid_map requires a user context")?;

    let name = input
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: name")?;
    let center_lat = input
        .get("center_lat")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: center_lat")?;
    let center_lng = input
        .get("center_lng")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: center_lng")?;

    let description = input.get("description").and_then(|v| v.as_str());
    let grid_res = input
        .get("grid_resolution")
        .and_then(|v| v.as_u64())
        .unwrap_or(12) as u8;
    let radius_rings = input
        .get("radius_rings")
        .and_then(|v| v.as_i64())
        .unwrap_or(5) as i32;
    let quadrants = input.get("quadrants").cloned().unwrap_or(json!([]));
    let zones = input.get("zones").cloned().unwrap_or(json!([]));

    let resolution =
        Resolution::try_from(grid_res).map_err(|_| format!("Invalid resolution: {}", grid_res))?;
    // Center resolution is 3 levels above grid resolution (or 0 if grid_res < 3)
    let center_res_num = if grid_res >= 3 { grid_res - 3 } else { 0 };
    let center_resolution = Resolution::try_from(center_res_num)
        .map_err(|_| format!("Invalid center resolution: {}", center_res_num))?;

    let ll =
        LatLng::new(center_lat, center_lng).map_err(|e| format!("Invalid coordinates: {}", e))?;
    let center_cell = ll.to_cell(center_resolution);

    let k = radius_rings as u32;
    let total_cells = (3 * k * k + 3 * k + 1) as i32;
    let map_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let pool = ctx.memory_store.pool();
    sqlx::query(
        "INSERT INTO ar_grid_maps (map_id, workspace_id, creator_id, name, description,
         center_lat, center_lng, center_h3, center_resolution,
         grid_resolution, radius_rings, total_cells,
         quadrants, zones, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $15)",
    )
    .bind(map_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(name)
    .bind(description)
    .bind(center_lat)
    .bind(center_lng)
    .bind(center_cell.to_string())
    .bind(center_res_num as i32)
    .bind(grid_res as i32)
    .bind(radius_rings)
    .bind(total_cells)
    .bind(&quadrants)
    .bind(&zones)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to save grid map: {}", e))?;

    let result = json!({
        "map_id": map_id,
        "name": name,
        "center_h3": center_cell.to_string(),
        "center_resolution": center_res_num,
        "grid_resolution": grid_res,
        "radius_rings": radius_rings,
        "total_cells": total_cells,
        "quadrants_count": quadrants.as_array().map(|a| a.len()).unwrap_or(0),
        "zones_count": zones.as_array().map(|a| a.len()).unwrap_or(0),
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Rabble.world creature tools ───────────────────────────────────

/// GBIF backbone keys for the scopes `gbif_species_search` can be pointed at.
///
/// **Every key below was verified against the live GBIF API on 2026-08-17** via
/// `GET /v1/species/match?name=<name>`, which returned `matchType: EXACT` and
/// the stated rank for each. `Animalia` matched `HIGHERRANK`, so it was
/// confirmed a second way with `GET /v1/species/1`. They are recorded here
/// rather than resolved at runtime because a name lookup per search would add a
/// round trip to answer a question whose answer does not change.
///
/// The verification is noted because a plausible-looking key table written from
/// memory is indistinguishable from a correct one until a search silently
/// returns nothing — the failure would present as "GBIF has no record of this",
/// which is a claim about the world rather than about a wrong constant.
///
/// `(name, key, rank)` — rank is carried only so a reader can see that `insecta`
/// is a CLASS while `plantae` is a KINGDOM, which is why this is not called
/// `kingdom`.
pub const GBIF_SCOPES: &[(&str, i64, &str)] = &[
    ("insecta", 216, "CLASS"),
    ("plantae", 6, "KINGDOM"),
    ("fungi", 5, "KINGDOM"),
    ("animalia", 1, "KINGDOM"),
    ("aves", 212, "CLASS"),
    ("lepidoptera", 797, "ORDER"),
    ("hymenoptera", 1457, "ORDER"),
    ("magnoliopsida", 220, "CLASS"),
];

/// The default scope: Insecta.
///
/// This tool was written for the Rabble insect ecosystem and hard-coded
/// `highertaxonKey=216` into the name search. Six agents depend on that
/// behaviour (`naturalist`, `species_resolver`, `swarm_host`, `enemy_sensor`,
/// `genome_profiler`, `prey_locator`), so the default stays exactly what it was
/// and the scope is opt-in. Changing the default would silently widen every
/// existing caller's search, which is how an insect agent starts confidently
/// describing a plant that happens to share a genus name.
pub const GBIF_DEFAULT_SCOPE_KEY: i64 = 216;

/// Resolve the `highertaxonKey` filter for a GBIF name search.
///
/// Precedence: explicit `higher_taxon_key`, then named `scope`, then the
/// historical Insecta default.
///
/// An unrecognised `scope` is an **error, not a fallback**. Silently defaulting
/// a typo like `"plantea"` to Insecta would return zero results for a real
/// plant, and the caller would read that as "GBIF has nothing for this" — a
/// `tool_no_match` verdict manufactured by a spelling mistake. That confusion
/// between "asked and empty" and "asked the wrong question" is precisely what
/// the provenance vocabulary exists to keep apart, so it must not be created
/// here.
pub fn gbif_higher_taxon_key(input: &serde_json::Value) -> Result<i64, String> {
    if let Some(k) = input.get("higher_taxon_key").and_then(|v| v.as_i64()) {
        return Ok(k);
    }
    match input.get("scope").and_then(|v| v.as_str()) {
        None => Ok(GBIF_DEFAULT_SCOPE_KEY),
        Some(name) => {
            let wanted = name.trim().to_ascii_lowercase();
            GBIF_SCOPES
                .iter()
                .find(|(n, _, _)| *n == wanted)
                .map(|(_, k, _)| *k)
                .ok_or_else(|| {
                    let known: Vec<&str> = GBIF_SCOPES.iter().map(|(n, _, _)| *n).collect();
                    format!(
                        "unknown scope `{name}`. Known scopes: {}. Or pass \
                         `higher_taxon_key` with a GBIF backbone key directly. \
                         Refusing to fall back to the default: a mis-spelled \
                         scope would return zero results for a real species, \
                         and the caller would read that as GBIF having no \
                         record rather than as a bad argument.",
                        known.join(", ")
                    )
                })
        }
    }
}

/// Pick the vernacular name to show, from a GBIF search result's
/// `vernacularNames` array, ranked by how many independent sources list it.
///
/// ## Why this function exists
///
/// The extraction here previously read `vernacularName` (singular). **That key
/// does not exist in the `/species/search` response** — the field is
/// `vernacularNames`, an array of `{vernacularName, language}`. So the tool
/// emitted `vernacularName: null` on every call, while its own description
/// promised "common names", and `species_resolver`'s prompt asks for a
/// `common_name` it was therefore never given. Filled from the model instead,
/// unlabelled. Often correct — *Vanessa atalanta* really is the Red Admiral —
/// but unverifiable, which is the `genome_profiler` shape rather than a wrong
/// answer.
///
/// ## Why frequency rather than the first entry
///
/// Measured against the live API on 2026-08-17, first-in-array is a poor
/// choice. *Danaus plexippus* has 40 English names and the first is
/// `"Milkweed"`; `"Monarch"` appears 13 times across independent checklists
/// while `"Milkweed"` appears 4. Counting sources picked the expected name for
/// all five species checked (monarch, southern live oak, chanterelle, death
/// cap, buff-tailed bumblebee).
///
/// GBIF's `preferred` flag would be the principled answer and is unpopulated on
/// every record inspected, so it is not used.
///
/// Deterministic: counts are taken on a trimmed, lowercased key, ties keep the
/// earliest-seen variant, and the returned string is the first original casing
/// of the winning form. Same input always yields the same name, which is what
/// lets the caller treat it as sourced rather than chosen.
pub fn gbif_preferred_vernacular(species: &serde_json::Value, language: &str) -> Option<String> {
    let list = species.get("vernacularNames")?.as_array()?;
    // (lowercased key, count, first original casing seen)
    let mut tally: Vec<(String, usize, String)> = Vec::new();
    for entry in list {
        if entry.get("language").and_then(|v| v.as_str()) != Some(language) {
            continue;
        }
        let Some(raw) = entry.get("vernacularName").and_then(|v| v.as_str()) else {
            continue;
        };
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        let key = name.to_lowercase();
        match tally.iter_mut().find(|(k, _, _)| *k == key) {
            Some(slot) => slot.1 += 1,
            None => tally.push((key, 1, name.to_string())),
        }
    }
    // `>` not `>=`, so the earliest variant wins a tie.
    let mut best: Option<&(String, usize, String)> = None;
    for row in &tally {
        if best.is_none_or(|b| row.1 > b.1) {
            best = Some(row);
        }
    }
    best.map(|(_, _, original)| original.clone())
}

/// Public so a handler that already has a name can ground it without standing
/// up a full [`ToolContext`].
///
/// This function takes no `ctx` — it is a keyless HTTP wrapper — so requiring a
/// memory store, an embedder and an agent registry to call it would push callers
/// toward re-implementing the GBIF request themselves, and a second copy of a
/// lookup is a second answer to the same question.
pub async fn execute_gbif_species_search(input: &serde_json::Value) -> Result<String, String> {
    // Direct key lookup
    if let Some(key) = input.get("gbif_key").and_then(|v| v.as_i64()) {
        let url = format!("https://api.gbif.org/v1/species/{}", key);
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .header("User-Agent", "AgentBestiaryWorld/1.0 (rabble.world)")
            .send()
            .await
            .map_err(|e| format!("GBIF request failed: {}", e))?;

        let species: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse GBIF response: {}", e))?;

        // Also fetch media
        let media_url = format!("https://api.gbif.org/v1/species/{}/media", key);
        let media_resp = client
            .get(&media_url)
            .header("User-Agent", "AgentBestiaryWorld/1.0 (rabble.world)")
            .send()
            .await
            .ok();

        let media: Option<serde_json::Value> = if let Some(r) = media_resp {
            r.json().await.ok()
        } else {
            None
        };

        let result = json!({
            "species": species,
            "media": media.unwrap_or(json!({"results": []})),
        });
        return serde_json::to_string_pretty(&result)
            .map_err(|e| format!("Serialization error: {}", e));
    }

    // Search by name
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("Either 'query' or 'gbif_key' is required")?;
    let rank = input
        .get("rank")
        .and_then(|v| v.as_str())
        .unwrap_or("SPECIES");
    let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(5);

    let limit_str = limit.to_string();
    // Insecta unless the caller asks otherwise. See `gbif_higher_taxon_key`.
    let higher_taxon = gbif_higher_taxon_key(input)?.to_string();
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.gbif.org/v1/species/search")
        .query(&[
            ("q", query),
            ("rank", rank),
            ("limit", limit_str.as_str()),
            ("highertaxonKey", higher_taxon.as_str()),
        ])
        .header("User-Agent", "AgentBestiaryWorld/1.0 (rabble.world)")
        .send()
        .await
        .map_err(|e| format!("GBIF request failed: {}", e))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse GBIF response: {}", e))?;

    // Extract just the useful fields from results
    let results = body
        .get("results")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let species: Vec<serde_json::Value> = results
        .into_iter()
        .map(|s| {
            json!({
                "key": s.get("key"),
                "scientificName": s.get("scientificName"),
                "canonicalName": s.get("canonicalName"),
                // Same key as before, and now actually populated. It read
                // `s.get("vernacularName")` — a field the search response does
                // not have — so it was null on every call since the tool was
                // written. See `gbif_preferred_vernacular`.
                "vernacularName": gbif_preferred_vernacular(&s, "eng"),
                "vernacularNameLanguage": "eng",
                "vernacularNamesEnglish": s
                    .get("vernacularNames")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        let mut seen: Vec<String> = Vec::new();
                        for e in a {
                            if e.get("language").and_then(|v| v.as_str()) != Some("eng") {
                                continue;
                            }
                            if let Some(n) = e.get("vernacularName").and_then(|v| v.as_str()) {
                                let n = n.trim().to_string();
                                if !n.is_empty() && !seen.iter().any(|x| x.eq_ignore_ascii_case(&n))
                                {
                                    seen.push(n);
                                }
                            }
                        }
                        seen.truncate(8);
                        seen
                    }),
                "kingdom": s.get("kingdom"),
                "phylum": s.get("phylum"),
                "class": s.get("class"),
                "order": s.get("order"),
                "family": s.get("family"),
                "genus": s.get("genus"),
                "species": s.get("species"),
                "rank": s.get("rank"),
                "taxonomicStatus": s.get("taxonomicStatus"),
            })
        })
        .collect();

    let result = json!({
        "count": species.len(),
        "species": species,
        "note": "Use gbif_key with a species key for full details + media"
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_mint_creature(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = ctx
        .workspace_id
        .ok_or("mint_creature requires a workspace context")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("mint_creature requires a user context")?;

    let scientific_name = input
        .get("scientific_name")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: scientific_name")?;
    let asset_path = input
        .get("asset_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: asset_path")?;

    let common_name = input.get("common_name").and_then(|v| v.as_str());
    let species_group = input
        .get("species_group")
        .and_then(|v| v.as_str())
        .unwrap_or("butterfly");
    let gbif_key = input.get("gbif_key").and_then(|v| v.as_i64());
    let taxonomy = input.get("taxonomy").cloned().unwrap_or(json!({}));
    let flight_silhouette_path = input.get("flight_silhouette_path").and_then(|v| v.as_str());
    let specimen_name = input.get("specimen_name").and_then(|v| v.as_str());
    let variation_notes = input.get("variation_notes").and_then(|v| v.as_str());

    let creature_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    // Generate a specimen name if not provided
    let final_specimen_name = specimen_name.map(|s| s.to_string()).unwrap_or_else(|| {
        let base = common_name.unwrap_or(scientific_name);
        format!("{} #{}", base, &creature_id.to_string()[..6])
    });

    let pool = ctx.memory_store.pool();
    sqlx::query(
        "INSERT INTO creatures (creature_id, owner_id, workspace_id,
         scientific_name, common_name, species_group, gbif_key,
         taxonomy, specimen_name, variation_notes,
         asset_path, flight_silhouette_path,
         created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)",
    )
    .bind(creature_id)
    .bind(user_id)
    .bind(workspace_id)
    .bind(scientific_name)
    .bind(common_name)
    .bind(species_group)
    .bind(gbif_key)
    .bind(&taxonomy)
    .bind(&final_specimen_name)
    .bind(variation_notes)
    .bind(asset_path)
    .bind(flight_silhouette_path)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to mint creature: {}", e))?;

    let result = json!({
        "creature_id": creature_id,
        "specimen_name": final_specimen_name,
        "scientific_name": scientific_name,
        "common_name": common_name,
        "species_group": species_group,
        "gbif_key": gbif_key,
        "asset_path": asset_path,
        "variation_notes": variation_notes,
        "data_card": {
            "minted_at": now.to_rfc3339(),
            "minted_by": user_id,
            "workspace_id": workspace_id,
            "taxonomy": taxonomy,
        }
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

/// Generate a unique naturalist illustration for a creature.
///
/// Pipeline: resolve species → fetch GBIF media → build art prompt → Gemini generate → save PNG → update DB
async fn execute_generate_specimen_art(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .map_err(|_| "GEMINI_API_KEY not set — image generation unavailable")?;

    let pool = ctx.memory_store.pool();

    // ── Step 1: Resolve creature data ──
    // Either from creature_id (DB lookup) or from input params directly
    let (creature_id, scientific_name, common_name, species_group, gbif_key) =
        if let Some(id_str) = input.get("creature_id").and_then(|v| v.as_str()) {
            let cid =
                Uuid::parse_str(id_str).map_err(|_| format!("Invalid creature_id: {}", id_str))?;
            let row = sqlx::query(
                "SELECT creature_id, scientific_name, common_name, species_group, gbif_key
                 FROM creatures WHERE creature_id = $1",
            )
            .bind(cid)
            .fetch_optional(pool)
            .await
            .map_err(|e| format!("DB lookup failed: {}", e))?
            .ok_or_else(|| format!("Creature {} not found", cid))?;

            (
                Some(cid),
                row.get::<String, _>("scientific_name"),
                row.get::<Option<String>, _>("common_name"),
                row.get::<String, _>("species_group"),
                row.get::<Option<i64>, _>("gbif_key"),
            )
        } else {
            let sci = input
                .get("scientific_name")
                .and_then(|v| v.as_str())
                .ok_or("Either creature_id or scientific_name is required")?;
            (
                None,
                sci.to_string(),
                input
                    .get("common_name")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                input
                    .get("species_group")
                    .and_then(|v| v.as_str())
                    .unwrap_or("butterfly")
                    .to_string(),
                input.get("gbif_key").and_then(|v| v.as_i64()),
            )
        };

    let style = input
        .get("style")
        .and_then(|v| v.as_str())
        .unwrap_or("naturalist");

    // ── Step 2: Fetch GBIF reference media description ──
    let mut reference_desc = String::new();
    if let Some(key) = gbif_key {
        let client = reqwest::Client::new();
        let media_url = format!("https://api.gbif.org/v1/species/{}/media", key);
        if let Ok(resp) = client
            .get(&media_url)
            .header("User-Agent", "AgentBestiaryWorld/1.0 (rabble.world)")
            .send()
            .await
        {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(results) = body.get("results").and_then(|v| v.as_array()) {
                    // Collect descriptions from first few media items for reference
                    let descs: Vec<&str> = results
                        .iter()
                        .take(3)
                        .filter_map(|m| {
                            m.get("description")
                                .or(m.get("title"))
                                .and_then(|v| v.as_str())
                        })
                        .collect();
                    if !descs.is_empty() {
                        reference_desc = format!(" Reference descriptions: {}", descs.join("; "));
                    }
                }
            }
        }
    }

    // ── Step 3: Build art generation prompt ──
    let display_name = common_name
        .as_deref()
        .map(|c| format!("{} ({})", c, scientific_name))
        .unwrap_or_else(|| scientific_name.clone());

    let style_instruction = match style {
        "watercolor" => "Soft watercolor painting style with visible brush strokes and subtle color bleeding at edges. Muted earth tones with occasional vivid accents.",
        "botanical" => "Precise botanical illustration style on cream parchment background. Fine ink linework with delicate hand-tinted color washes. Labeled anatomical features.",
        "field-guide" => "Clean field guide illustration style. Crisp outlines, accurate proportions, neutral white background, specimen positioned at 3/4 view with wings spread.",
        "ukiyo-e" => "Japanese woodblock print (ukiyo-e) style in the tradition of Edo-period naturalist prints. Bold black outlines with flat color planes. Subtle gradation (bokashi) on wings. Warm washi paper background texture. Include a small red hanko seal stamp in one corner. Muted indigo, ochre, and grey tones with selective bold color accents. Multiple views of the same specimen at different scales, as in traditional insect study prints.",
        _ => "Detailed naturalist scientific illustration in the style of Maria Sibylla Merian. Rich, accurate colors on aged vellum background. Fine detail on wing patterns and body segments.",
    };

    let group_detail = match species_group.as_str() {
        "dragonfly" => "Show detailed wing venation patterns, elongated abdomen segments, and compound eye structure. Wings should be translucent with visible cells.",
        "beetle" => "Show detailed elytra (wing covers) with surface texture, compound eyes, segmented antennae, and jointed legs. Ventral view option showing wing deployment.",
        "bee" => "Show fuzzy body texture, compound eyes, pollen baskets on legs, translucent wing venation, and banded abdomen coloring.",
        "locust" => "Show powerful hind legs, segmented antennae, compound eyes, and folded wing structure. Textured exoskeleton detail.",
        "fly" => "Show compound eyes, halteres, translucent wing venation, and segmented body. Metallic sheen where appropriate.",
        "bug" => "Show piercing-sucking mouthparts, shield-shaped body, wing membrane detail, and segmented antennae.",
        _ => "Show detailed wing scale patterns, proboscis, antennae, and leg segments. Upper and lower wing surfaces visible.",
    };

    let prompt = format!(
        "Create a beautiful scientific illustration of a {} ({}).\n\n\
         Style: {}\n\n\
         Species details: {}\n\n\
         Requirements:\n\
         - Single specimen, centered composition\n\
         - Anatomically accurate proportions and markings\n\
         - {}\n\
         - No text, labels, or watermarks\n\
         - Square format, high detail\n\
         - Dark background (#1A2E20) to make the specimen pop{}",
        display_name,
        species_group,
        style_instruction,
        group_detail,
        if species_group == "dragonfly" {
            "Include subtle iridescence on wings and thorax"
        } else {
            "Include subtle iridescence on wing scales where appropriate"
        },
        reference_desc
    );

    // ── Step 4: Generate image via Gemini ──
    let body = json!({
        "contents": [{
            "parts": [{ "text": prompt }]
        }],
        "generationConfig": {
            "responseModalities": ["IMAGE"]
        }
    });

    let client = reqwest::Client::new();
    let response = client
        .post(GEMINI_IMAGE_URL)
        .header("x-goog-api-key", &api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gemini API request failed: {}", e))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Gemini API error: {}", error_text));
    }

    let gemini_resp: GeminiToolResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;

    // Extract base64 image data
    let (mime_type, image_data) = gemini_resp
        .candidates
        .iter()
        .flat_map(|c| c.content.parts.iter())
        .find_map(|p| {
            p.inline_data
                .as_ref()
                .map(|d| (d.mime_type.clone(), d.data.clone()))
        })
        .ok_or("Gemini returned no image data")?;

    // ── Step 5: Save image to static/creatures/ ──
    let extension = if mime_type.contains("png") {
        "png"
    } else if mime_type.contains("webp") {
        "webp"
    } else {
        "jpg"
    };

    let file_id = creature_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let filename = format!("{}.{}", file_id, extension);
    let relative_path = format!("/static/creatures/{}", filename);
    let fs_path = format!("static/creatures/{}", filename);

    // Decode base64 and write
    use base64::Engine;
    let decoder = base64::engine::general_purpose::STANDARD;
    let bytes = decoder
        .decode(&image_data)
        .map_err(|e| format!("Failed to decode image data: {}", e))?;

    // Ensure directory exists
    std::fs::create_dir_all("static/creatures")
        .map_err(|e| format!("Failed to create creatures directory: {}", e))?;
    std::fs::write(&fs_path, &bytes).map_err(|e| format!("Failed to write image: {}", e))?;

    // ── Step 6: Update creature record if creature_id provided ──
    let generation_params = json!({
        "style": style,
        "prompt": prompt,
        "mime_type": mime_type,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "gbif_key": gbif_key,
        "file_size_bytes": bytes.len(),
    });

    if let Some(cid) = creature_id {
        sqlx::query(
            "UPDATE creatures SET asset_path = $1, generation_params = $2, updated_at = NOW()
             WHERE creature_id = $3",
        )
        .bind(&relative_path)
        .bind(&generation_params)
        .bind(cid)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to update creature record: {}", e))?;
    }

    let result = json!({
        "status": "generated",
        "creature_id": creature_id,
        "asset_path": relative_path,
        "mime_type": mime_type,
        "file_size_bytes": bytes.len(),
        "style": style,
        "scientific_name": scientific_name,
        "common_name": common_name,
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Wing segmentation tool ────────────────────────────────────────

async fn execute_segment_creature_wings(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .map_err(|_| "GEMINI_API_KEY not set — wing segmentation unavailable")?;

    let pool = ctx.memory_store.pool();

    // Parse creature_id
    let creature_id_str = input
        .get("creature_id")
        .and_then(|v| v.as_str())
        .ok_or("creature_id is required")?;
    let creature_id = Uuid::parse_str(creature_id_str)
        .map_err(|_| format!("Invalid creature_id: {}", creature_id_str))?;

    // Look up creature
    let row =
        sqlx::query("SELECT species_group, animation_status FROM creatures WHERE creature_id = $1")
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| format!("DB lookup failed: {}", e))?
            .ok_or_else(|| format!("Creature {} not found", creature_id))?;

    let species_group: String = row.get("species_group");
    if species_group != "butterfly" {
        return Err(
            "Wing segmentation only works for butterflies. Other species coming soon!".to_string(),
        );
    }

    let status: Option<String> = row.try_get("animation_status").unwrap_or(None);
    if status.as_deref() == Some("ready") {
        return Ok(json!({
            "status": "already_ready",
            "creature_id": creature_id,
            "layers": {
                "body": format!("/api/creatures/{}/animation/body", creature_id),
                "left_wing": format!("/api/creatures/{}/animation/left_wing", creature_id),
                "right_wing": format!("/api/creatures/{}/animation/right_wing", creature_id),
            }
        })
        .to_string());
    }

    // Charge credits if user_id and gas_fees available
    if let (Some(ref gas_fees), Some(ref user_id)) = (&ctx.gas_fees, &ctx.user_id) {
        let wallet = fermi_auth::get_or_create_wallet(pool, "user", user_id)
            .await
            .map_err(|e| format!("Wallet error: {}", e))?;
        crate::gas::charge_gas(
            pool,
            wallet.wallet_id,
            gas_fees.creature_animate,
            "creature_animate",
            &format!("Wing segmentation for creature {}", creature_id),
            Some(&creature_id.to_string()),
        )
        .await
        .map_err(|e| format!("Credit charge failed: {}", e.1))?;
    }

    // Set status to processing
    let _ = sqlx::query(
        "UPDATE creatures SET animation_status = 'processing', updated_at = NOW() WHERE creature_id = $1",
    )
    .bind(creature_id)
    .execute(pool)
    .await;

    // Fetch source image from creature_images
    let img_row =
        sqlx::query("SELECT image_bytes, mime_type FROM creature_images WHERE creature_id = $1")
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| format!("DB error fetching image: {}", e))?
            .ok_or_else(|| "No image found for creature. Generate art first.".to_string())?;

    let image_bytes: Vec<u8> = img_row.get("image_bytes");
    let source_mime: String = img_row.get("mime_type");

    use base64::Engine;
    let encoder = base64::engine::general_purpose::STANDARD;
    let img_base64 = encoder.encode(&image_bytes);

    // Segmentation prompts
    let layers = [
        ("left_wing", "Isolate ONLY the left wing (viewer's left) of this butterfly specimen. Remove the body, right wing, antennae, and all other parts completely. Output ONLY the left wing on a fully transparent background (PNG with alpha). Preserve the exact wing shape, coloration, scale patterns, and venation. The wing should be positioned exactly where it appears in the original image."),
        ("right_wing", "Isolate ONLY the right wing (viewer's right) of this butterfly specimen. Remove the body, left wing, antennae, and all other parts completely. Output ONLY the right wing on a fully transparent background (PNG with alpha). Preserve the exact wing shape, coloration, scale patterns, and venation. The wing should be positioned exactly where it appears in the original image."),
        ("body", "Isolate ONLY the body (thorax, abdomen, head, antennae, legs) of this butterfly specimen. Remove both wings completely, leaving only the central body structure. Output on a fully transparent background (PNG with alpha). Preserve exact body position, coloration, and detail from the original image."),
    ];

    let client = reqwest::Client::new();
    let mut results = Vec::new();

    for (layer_name, prompt) in &layers {
        let body = json!({
            "contents": [{
                "parts": [
                    { "text": prompt },
                    {
                        "inlineData": {
                            "mimeType": source_mime,
                            "data": img_base64
                        }
                    }
                ]
            }],
            "generationConfig": {
                "responseModalities": ["TEXT", "IMAGE"]
            }
        });

        let response = client
            .post(GEMINI_IMAGE_URL)
            .header("x-goog-api-key", &api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Gemini request failed for {}: {}", layer_name, e))?;

        if !response.status().is_success() {
            let err = response.text().await.unwrap_or_default();
            let _ = sqlx::query(
                "UPDATE creatures SET animation_status = 'failed', updated_at = NOW() WHERE creature_id = $1",
            )
            .bind(creature_id)
            .execute(pool)
            .await;
            return Err(format!("Gemini error for {}: {}", layer_name, err));
        }

        let gemini_resp: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Parse error for {}: {}", layer_name, e))?;

        let inline_data = gemini_resp
            .pointer("/candidates/0/content/parts")
            .and_then(|parts| parts.as_array())
            .and_then(|parts| parts.iter().find_map(|p| p.get("inlineData")))
            .ok_or_else(|| format!("No image in Gemini response for {}", layer_name))?;

        let mime_type = inline_data
            .get("mimeType")
            .and_then(|v| v.as_str())
            .unwrap_or("image/png");
        let b64_data = inline_data
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("No image data for {}", layer_name))?;

        let decoded = encoder
            .decode(b64_data)
            .map_err(|e| format!("Decode error for {}: {}", layer_name, e))?;

        if decoded.len() < 100 {
            let _ = sqlx::query(
                "UPDATE creatures SET animation_status = 'failed', updated_at = NOW() WHERE creature_id = $1",
            )
            .bind(creature_id)
            .execute(pool)
            .await;
            return Err(format!(
                "Layer {} too small ({} bytes), segmentation likely failed",
                layer_name,
                decoded.len()
            ));
        }

        // Persist to DB (inline upsert — handlers module not accessible from lib crate)
        let _ = sqlx::query(
            "INSERT INTO creature_animation_layers (creature_id, layer_name, image_bytes, mime_type, file_size)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (creature_id, layer_name) DO UPDATE
             SET image_bytes = $3, mime_type = $4, file_size = $5, updated_at = NOW()",
        )
        .bind(creature_id)
        .bind(*layer_name)
        .bind(&decoded)
        .bind(mime_type)
        .bind(decoded.len() as i32)
        .execute(pool)
        .await;

        results.push(json!({
            "layer": layer_name,
            "mime_type": mime_type,
            "file_size_bytes": decoded.len(),
            "url": format!("/api/creatures/{}/animation/{}", creature_id, layer_name),
        }));

        // Rate limit between calls
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    // Mark as ready
    let _ = sqlx::query(
        "UPDATE creatures SET animation_status = 'ready', updated_at = NOW() WHERE creature_id = $1",
    )
    .bind(creature_id)
    .execute(pool)
    .await;

    Ok(json!({
        "status": "ready",
        "creature_id": creature_id,
        "message": "Wing segmentation complete. Your butterfly is now flight-ready.",
        "layers": results,
    })
    .to_string())
}

// ─── Marketplace tool implementations ──────────────────────────────

async fn execute_get_shopping_profile(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_id = ctx
        .current_agent_id
        .ok_or("No agent context for get_shopping_profile")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("No user context for get_shopping_profile")?;
    let profile_name = input
        .get("profile_name")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    let profile = ctx
        .memory_store
        .get_shopping_profile(user_id, agent_id, profile_name)
        .await
        .map_err(|e| format!("Profile lookup failed: {}", e))?;

    match profile {
        Some(p) => {
            let result = json!({
                "profile_id": p.profile_id,
                "profile_name": p.profile_name,
                "embedding_version": p.embedding_version,
                "episode_count": p.episode_count,
                "category_tags": p.category_tags,
                "price_sensitivity": p.price_sensitivity,
                "quality_bias": p.quality_bias,
                "brand_affinities": p.brand_affinities,
                "is_listed": p.is_listed,
                "updated_at": p.updated_at.to_rfc3339(),
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        None => Ok(json!({
            "status": "not_found",
            "message": format!("No shopping profile '{}' found. Use update_shopping_profile to create one.", profile_name)
        })
        .to_string()),
    }
}

async fn execute_update_shopping_profile(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_id = ctx
        .current_agent_id
        .ok_or("No agent context for update_shopping_profile")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("No user context for update_shopping_profile")?;
    let profile_name = input
        .get("profile_name")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    // Extract metadata from input
    let category_tags: Vec<String> = input
        .get("category_tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let price_sensitivity = input.get("price_sensitivity").and_then(|v| v.as_f64());
    let quality_bias = input.get("quality_bias").and_then(|v| v.as_f64());
    let brand_affinities = input.get("brand_affinities").cloned().unwrap_or(json!({}));

    // Compute composite embedding from episodes (weighted centroid)
    let episodes = ctx
        .memory_store
        .get_all_episodes_with_embeddings(agent_id)
        .await
        .map_err(|e| format!("Episode fetch failed: {}", e))?;

    let now = chrono::Utc::now();
    let mut weighted_sum: Option<Vec<f64>> = None;
    let mut total_weight = 0.0f64;
    let mut episode_count = 0i32;

    for episode in &episodes {
        if let Some(ref emb) = episode.embedding {
            let age_days = (now - episode.timestamp_ref).num_hours() as f64 / 24.0;
            let recency_weight = (-0.1 * age_days).exp();
            let success_weight = match episode.execution_status {
                agent_bestiary_memory::ExecutionStatus::Success => 1.0,
                _ => 0.3,
            };
            let w = recency_weight * success_weight;

            match &mut weighted_sum {
                Some(sum) => {
                    for (i, &val) in emb.iter().enumerate() {
                        if i < sum.len() {
                            sum[i] += w * val as f64;
                        }
                    }
                }
                None => {
                    weighted_sum = Some(emb.iter().map(|&v| w * v as f64).collect());
                }
            }
            total_weight += w;
            episode_count += 1;
        }
    }

    // L2 normalize the composite embedding
    let composite: Option<Vec<f32>> = weighted_sum.map(|sum| {
        let norm: f64 = sum.iter().map(|v| v * v).sum::<f64>().sqrt();
        if norm > 1e-10 {
            sum.iter().map(|&v| (v / norm) as f32).collect()
        } else {
            sum.iter().map(|&v| v as f32).collect()
        }
    });

    let episode_ids_for_centroid: Vec<uuid::Uuid> = episodes
        .iter()
        .filter(|e| e.embedding.is_some())
        .map(|e| e.episode_id)
        .collect();

    let profile_id = if let Some(ref composite_vec) = composite {
        // Centroid was computed — record full Spec 22 provenance. The centroid
        // inherits the model identity of the constituent episode embeddings,
        // which all come from `ctx.embedder` (single shared embedder per
        // server).
        let source_ref = json!({
            "kind": "shopping_profile_centroid",
            "member_episode_ids": episode_ids_for_centroid,
            "episode_count": episode_count,
            "total_weight": total_weight,
        });
        ctx.memory_store
            .upsert_shopping_profile_with_provenance(
                user_id,
                agent_id,
                profile_name,
                composite_vec,
                episode_count,
                &category_tags,
                price_sensitivity,
                quality_bias,
                &brand_affinities,
                ctx.embedder.model_id(),
                ctx.embedder.model_version(),
                ctx.embedder.dimension() as i32,
                source_ref,
            )
            .await
            .map_err(|e| format!("Profile upsert failed: {}", e))?
    } else {
        // No episodes had embeddings → no centroid to compute. Fall back to
        // the legacy upsert path; the row is created without an embedding.
        #[allow(deprecated)]
        ctx.memory_store
            .upsert_shopping_profile(
                user_id,
                agent_id,
                profile_name,
                None,
                episode_count,
                &category_tags,
                price_sensitivity,
                quality_bias,
                &brand_affinities,
            )
            .await
            .map_err(|e| format!("Profile upsert failed: {}", e))?
    };

    let result = json!({
        "profile_id": profile_id,
        "profile_name": profile_name,
        "episode_count": episode_count,
        "embedding_computed": composite.is_some(),
        "category_tags": category_tags,
        "price_sensitivity": price_sensitivity,
        "quality_bias": quality_bias,
        "brand_affinities": brand_affinities,
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_list_marketplace(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let cat_str = input.get("category").and_then(|v| v.as_str());
    let cat_filter: Option<Vec<String>> =
        cat_str.map(|s| s.split(',').map(|t| t.trim().to_string()).collect());
    let limit = input.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);

    let listings = ctx
        .memory_store
        .get_active_listings(cat_filter.as_deref(), limit)
        .await
        .map_err(|e| format!("Marketplace query failed: {}", e))?;

    let items: Vec<serde_json::Value> = listings
        .iter()
        .map(|l| {
            json!({
                "listing_id": l.listing_id,
                "seller_id": l.seller_id,
                "price_credits": l.price_credits,
                "total_queries": l.total_queries,
                "category_tags": l.category_tags,
                "description": l.description,
            })
        })
        .collect();

    let result = json!({ "listings": items, "count": items.len() });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_create_listing(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_id = ctx
        .current_agent_id
        .ok_or("No agent context for create_listing")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("No user context for create_listing")?;
    let profile_name = input
        .get("profile_name")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let price_credits = input
        .get("price_credits")
        .and_then(|v| v.as_i64())
        .unwrap_or(1)
        .max(1) as i32;
    let max_queries = input
        .get("max_queries_per_buyer")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32);
    let category_tags: Vec<String> = input
        .get("category_tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let description = input.get("description").and_then(|v| v.as_str());

    // Look up the profile
    let profile = ctx
        .memory_store
        .get_shopping_profile(user_id, agent_id, profile_name)
        .await
        .map_err(|e| format!("Profile lookup failed: {}", e))?
        .ok_or_else(|| {
            format!(
                "No shopping profile '{}' found. Create one with update_shopping_profile first.",
                profile_name
            )
        })?;

    // Charge listing fee if pool is available
    if let (Some(db), Some(gas)) = (&ctx.db, &ctx.gas_fees) {
        let wallet = fermi_auth::get_or_create_wallet(db, "user", user_id)
            .await
            .map_err(|e| format!("Wallet error: {}", e))?;
        fermi_auth::credit_charge(
            db,
            wallet.wallet_id,
            gas.marketplace_listing_fee,
            "marketplace_listing_fee",
            "Marketplace listing creation",
            Some(&profile.profile_id.to_string()),
        )
        .await
        .map_err(|e| format!("Insufficient credits for listing fee: {}", e))?;
    }

    let listing_id = ctx
        .memory_store
        .create_marketplace_listing(
            profile.profile_id,
            user_id,
            price_credits,
            max_queries,
            &category_tags,
            description,
        )
        .await
        .map_err(|e| format!("Listing creation failed: {}", e))?;

    let result = json!({
        "listing_id": listing_id,
        "profile_id": profile.profile_id,
        "status": "active",
        "price_credits": price_credits,
        "message": format!("Profile '{}' is now listed on the marketplace at {} credits per query.", profile_name, price_credits),
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_query_ontology(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let include_rules = input
        .get("include_rules")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let include_entities = input
        .get("include_entities")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let include_facts = input
        .get("include_facts")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let agent_id = ctx
        .current_agent_id
        .ok_or("No agent context for query_ontology")?;

    let mut result = json!({});

    if include_rules {
        let rules = ctx
            .memory_store
            .get_agent_semantic_rules(agent_id)
            .await
            .map_err(|e| format!("Failed to get rules: {}", e))?;
        let rules_json: Vec<serde_json::Value> = rules
            .iter()
            .map(|r| {
                json!({
                    "content": r.rule_content,
                    "description": r.rule_description,
                    "confidence": r.confidence_score,
                    "status": r.verification_status,
                })
            })
            .collect();
        result["rules"] = json!(rules_json);
    }

    if include_entities {
        let entities = ctx
            .memory_store
            .get_agent_entities(agent_id)
            .await
            .map_err(|e| format!("Failed to get entities: {}", e))?;
        let entities_json: Vec<serde_json::Value> = entities
            .iter()
            .map(|e| {
                json!({
                    "name": e.entity_name,
                    "type": e.entity_type,
                    "summary": e.summary,
                })
            })
            .collect();
        result["entities"] = json!(entities_json);
    }

    if include_facts {
        let facts = ctx
            .memory_store
            .get_agent_facts(agent_id)
            .await
            .map_err(|e| format!("Failed to get facts: {}", e))?;
        let facts_json: Vec<serde_json::Value> = facts
            .iter()
            .map(|f| {
                json!({
                    "relation_type": f.relation_type,
                    "confidence": f.confidence,
                    "reasoning": f.reasoning,
                })
            })
            .collect();
        result["facts"] = json!(facts_json);
    }

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

/// Persist an episode for a delegated child execution (mig-198).
///
/// Before this existed, both delegation tools ran a child agent, read its
/// `reasoning` and `evidence`, and dropped the rest of the `AgentOutput` on the
/// floor. The child's tokens, cost, provider and model were never recorded, so
/// a compound agent under-reported its true cost by its entire fan-out and a
/// delegate-only agent had no economic record at all.
///
/// Writes the child's OWN episode rather than folding its tokens into the
/// caller's, so each agent stays separately costable and creditable — the
/// premise the marketplace rests on. Priced through the same
/// `agent_output_to_episode` / `AgentOutput::cost()` path as every other
/// episode, so a delegated run cannot drift onto a different cost basis.
///
/// Best-effort by design: a bookkeeping failure must never fail the delegation
/// the caller is waiting on. Logged at `warn` because a silent gap here
/// under-reports real spend, and returns the new episode id so nested
/// delegation can carry the chain further.
/// `episode_id` is minted by the caller BEFORE the child runs, so it can be
/// placed on the child's own `ToolContext.parent_episode_id` and a grandchild
/// can link to it. Same reason the request handler mints ahead of execution
/// (mig-197): a row that is written later cannot be pointed at by a task that
/// starts earlier.
async fn record_delegated_episode(
    ctx: &ToolContext,
    target_agent_id: Uuid,
    episode_id: Uuid,
    task: &str,
    output: &crate::agent_backend::executor::AgentOutput,
) -> Option<Uuid> {
    let mut episode = crate::episodes::agent_output_to_episode(target_agent_id, task, output);
    episode.episode_id = episode_id;
    episode.parent_episode_id = ctx.parent_episode_id;
    // Findable as delegated work without having to join on the parent.
    episode.tags.push("delegated".to_string());
    if let Some(caller) = ctx.current_agent_id {
        episode.tags.push(format!("delegated_by:{caller}"));
    }

    match ctx.memory_store.store_episode(episode).await {
        Ok(id) => Some(id),
        Err(e) => {
            tracing::warn!(
                target_agent = %target_agent_id,
                parent_episode = ?ctx.parent_episode_id,
                error = %e,
                "[delegation] failed to record child episode — this run's cost \
                 will be missing from per-forecast and per-agent totals",
            );
            None
        }
    }
}

async fn execute_execute_agent(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    // Support both "agent_id" (MCP convention) and "agent_name" (legacy)
    let agent_name = input
        .get("agent_id")
        .or_else(|| input.get("agent_name"))
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: agent_id")?;
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: query")?;

    // mig-198: minted before the child runs, so it can be placed on the
    // child's own ToolContext and used as the id of the episode recorded for
    // this delegated execution. An id generated after the fact could not be
    // handed to a task that has already started.
    let child_episode_id = Uuid::new_v4();

    // Optional cross-workspace delegation: when workspace_id is provided,
    // the target agent runs inside that workspace's full context (tools,
    // workspace git, KG). This is the seam between Rabble creatures and
    // kask-app workspaces (e.g. kask-app-wild).
    let target_workspace_id: Option<uuid::Uuid> = input
        .get("workspace_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok());

    // Get the target agent card
    let card = ctx
        .registry
        .get(agent_name)
        .map_err(|e| format!("Agent not found: {}", e))?;

    // Enrich card with KG context from past dream cycles
    let card = if let Some(ref db) = ctx.db {
        let (enriched, _) = crate::agent_backend::kg_context::enrich_with_kg_context_by_name(
            &ctx.memory_store,
            &ctx.embedder,
            db,
            agent_name,
            query,
            card,
        )
        .await;
        enriched
    } else {
        card
    };

    // Build a minimal AgentStmt for execution
    let stmt = crate::ast::AgentStmt {
        name: agent_name.to_string(),
        agent_type: Some(card.agent_type.clone()),
        query: query.to_string(),
        executor: None,
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    };

    // Captured before `card` moves into the execution context below: the
    // envelope needs only the declared contract, and holding a whole card
    // alive for it would be wrong.
    let declared_output_contract = card.capabilities.output_contract.clone();

    let context = crate::agent_backend::executor::ExecutionContext {
        program: crate::ast::Program { statements: vec![] },
        agent_card: card,
        creature_id: None,
        cognition_tier: None,
        // Delegated child inherits the parent execution's funding.
        credentials: ctx.credentials.clone(),
        // ...but NOT the parent's attachments, and that is not a dropped frame.
        //
        // Attachments belong to a request, not to a session. Delegation builds a
        // new request whose content is the text the parent chose to send, so the
        // child was never promised an image. Propagating one silently would hand
        // a frame to an agent that may not declare `accepts: image`, and the
        // parent would have no way to know whether it arrived.
        //
        // If a compound agent needs to pass a photograph to a specialist, that
        // wants to be an explicit argument on `execute_agent` — visible in the
        // call, and checkable against the child's declared inputs.
        attachments: Vec::new(),
    };

    let output = if let Some(ws_id) = target_workspace_id {
        // ── Cross-workspace delegation ────────────────────────────────
        // Build a full ToolContext for the target workspace so the
        // delegated agent has access to its workspace git, tools, and KG.
        // Anti-recursion: use with_workspace_no_delegation to strip
        // further cross-workspace calls from the sub-agent's tool list.
        if let Some(ref db) = ctx.db {
            // Look up workspace slug for git context
            let slug: String = sqlx::query_scalar("SELECT slug FROM teams WHERE id = $1")
                .bind(ws_id)
                .fetch_optional(db)
                .await
                .ok()
                .flatten()
                .unwrap_or_default();

            // Look up the calling agent's DB UUID for current_agent_id
            let calling_agent_id: Option<uuid::Uuid> =
                sqlx::query_scalar("SELECT agent_id FROM agents WHERE agent_name = $1 LIMIT 1")
                    .bind(agent_name)
                    .fetch_optional(db)
                    .await
                    .ok()
                    .flatten();

            // Resolved here rather than after the run, because the reservation
            // below needs it and a lookup that only happens on the success path
            // is how the row came to be missing in the first place.
            let target_db_id_for_reservation: Option<uuid::Uuid> =
                sqlx::query_scalar("SELECT agent_id FROM agents WHERE agent_name = $1 LIMIT 1")
                    .bind(agent_name)
                    .fetch_optional(db)
                    .await
                    .ok()
                    .flatten();

            // Reserve the child's row BEFORE its id is handed to grandchildren.
            //
            // Everything the child delegates during its run points at
            // `child_episode_id`, and until now the row behind it was only
            // written after the run finished - so a child that failed to record
            // orphaned every grandchild permanently. 6 of the platform's 12
            // delegation edges are in that state.
            if let Some(tid) = target_db_id_for_reservation {
                if let Err(e) = ctx
                    .memory_store
                    .reserve_episode(child_episode_id, tid, query)
                    .await
                {
                    tracing::warn!(
                        agent = %agent_name,
                        error = %e,
                        "[delegation] could not reserve the child episode; any \
                         grandchild will point at a row that does not exist",
                    );
                }
            }

            let target_tool_context = std::sync::Arc::new(ToolContext {
                // The child's own episode, so anything IT delegates to links
                // to the child rather than skipping a level (mig-198).
                parent_episode_id: Some(child_episode_id),
                credentials: ctx.credentials.clone(),
                memory_store: ctx.memory_store.clone(),
                embedder: ctx.embedder.clone(),
                registry: ctx.registry.clone(),
                current_agent_id: calling_agent_id,
                workspace_id: Some(ws_id),
                workspace_slug: Some(slug.clone()),
                workspace_git: ctx.workspace_git.clone(),
                db: ctx.db.clone(),
                gas_fees: ctx.gas_fees.clone(),
                user_id: ctx.user_id.clone(),
                user_secrets: None,
                eval_trigger: ctx.eval_trigger.clone(),
                remote_mcp: None,
            });

            let tool_executor = crate::agent_backend::tool_executor::ToolAwareExecutor::new(
                ctx.registry.executor_arc(),
                ToolRegistry::with_workspace_no_delegation(),
                target_tool_context,
            );

            tool_executor
                .execute(&stmt, &context)
                .await
                .map_err(|e| format!("Cross-workspace agent execution failed: {}", e))?
        } else {
            // No DB — fall back to base executor
            ctx.registry
                .execute_agent(&stmt, &context)
                .await
                .map_err(|e| format!("Agent execution failed: {}", e))?
        }
    } else {
        // ── Standard (same-workspace or global) execution ────────────
        // Execute via the base executor (no tools — prevents recursion
        // in the common case where workspace_id is not specified).
        ctx.registry
            .execute_agent(&stmt, &context)
            .await
            .map_err(|e| format!("Agent execution failed: {}", e))?
    };

    // mig-198: record the child's own cost. Needs the target's DB uuid, which
    // this tool never resolved because it only ever needed the card by name.
    // When there is no DB handle we cannot write an episode at all, so the
    // spend stays unrecorded — logged rather than passed over in silence,
    // because that is a hole in the cost ledger and should be visible as one.
    if let Some(ref db) = ctx.db {
        match sqlx::query_scalar::<_, Uuid>(
            "SELECT agent_id FROM agents WHERE agent_name = $1 LIMIT 1",
        )
        .bind(agent_name)
        .fetch_optional(db)
        .await
        {
            Ok(Some(target_db_id)) => {
                record_delegated_episode(ctx, target_db_id, child_episode_id, query, &output).await;
            }
            _ => tracing::warn!(
                agent = %agent_name,
                "[delegation] target agent not found in DB; child episode not \
                 recorded and its cost will be missing from totals",
            ),
        }
    } else {
        tracing::debug!(
            agent = %agent_name,
            "[delegation] no DB handle; child episode not recorded",
        );
    }

    // Format the output — include metadata.reasoning so callers can
    // parse domain-specific JSON (e.g. forage_scout's structured response)
    // ── the delegation envelope (additive) ────────────────────────────
    //
    // Every key below this is unchanged. The envelope is added alongside so
    // existing coordinator prompts, which read `response` and `evidence`,
    // keep working byte-for-byte.
    //
    // What it adds is the thing delegation never had: the child's OWN
    // document, enforced against its grounding contract, with per-block
    // provenance. Before this, `response` was `metadata.reasoning` — a
    // per-agent parser's reading of the output — and a fabricated field
    // stripped at the creature-module boundary passed freely between agents.
    let envelope = crate::agent_backend::envelope::build(
        agent_name,
        declared_output_contract.as_ref(),
        &output,
        child_episode_id,
    );

    // Third consumer of the same verdict: the trend.
    //
    // The coordinator reads `envelope.validation` on this hop and
    // `gate_trust` counts it in aggregate, but neither accrues per agent —
    // the counters are process-local and reset on deploy. Without this row,
    // "is this member's output getting better or worse" has no answer, which
    // is the input loop 4 needs to change a roster on measured contribution.
    //
    // Writes nothing when nothing was checked. See `schema_conformance`.
    if let Some(ref db) = ctx.db {
        if let Some(status) = envelope
            .pointer("/validation/status")
            .and_then(|s| s.as_str())
        {
            if crate::schema_conformance::score_for(status).is_some() {
                if let Ok(Some(target_db_id)) = sqlx::query_scalar::<_, Uuid>(
                    "SELECT agent_id FROM agents WHERE agent_name = $1 LIMIT 1",
                )
                .bind(agent_name)
                .fetch_optional(db)
                .await
                {
                    crate::schema_conformance::record(
                        db,
                        target_db_id,
                        child_episode_id,
                        status,
                        envelope.get("type").and_then(|t| t.as_str()),
                    )
                    .await;
                }
            }
        }
    }

    let result = json!({
        "agent": output.agent_name,
        "confidence": output.confidence,
        "status": format!("{:?}", output.status),
        "response": output.metadata.reasoning,
        "envelope": envelope,
        "evidence": output.evidence.iter().map(|e| {
            json!({
                "summary": e.summary,
                "key_findings": e.key_findings,
                "strength": e.strength,
            })
        }).collect::<Vec<_>>(),
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_delegate_to_agent(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_name = input
        .get("agent_name")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: agent_name")?;
    let task = input
        .get("task")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: task")?;

    let ws_id = ctx
        .workspace_id
        .ok_or("delegate_to_agent requires a workspace context")?;
    let ws_slug = ctx.workspace_slug.as_deref().unwrap_or("");

    let pool = ctx.memory_store.pool();

    // Verify agent is in workspace
    let agent_row = sqlx::query(
        "SELECT a.agent_id, a.agent_name, a.display_alias FROM workspace_agents wa
         JOIN agents a ON a.agent_id = wa.agent_id
         WHERE wa.workspace_id = $1 AND a.agent_name = $2",
    )
    .bind(ws_id)
    .bind(agent_name)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error: {}", e))?
    .ok_or_else(|| format!("Agent '{}' is not in this workspace", agent_name))?;

    let target_agent_id: Uuid = agent_row.get("agent_id");
    let display: String = agent_row
        .try_get::<Option<String>, _>("display_alias")
        .unwrap_or(None)
        .unwrap_or_else(|| agent_name.to_string());

    // Post delegation message to workspace chat
    let delegation_msg = WorkspaceMessage {
        message_id: Uuid::new_v4(),
        workspace_id: ws_id,
        sender_type: "agent".to_string(),
        sender_id: ctx
            .current_agent_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        sender_name: Some(format!(
            "{} → {}",
            ctx.current_agent_id.map(|_| "compound").unwrap_or("system"),
            display
        )),
        content: format!("Delegating to {}: {}", display, task),
        message_type: "system_event".to_string(),
        metadata: json!({"delegation": true, "target": agent_name}),
        created_at: chrono::Utc::now(),
        episode_id: None,
    };
    let _ = ctx
        .memory_store
        .store_workspace_message(&delegation_msg)
        .await;

    // Resolve agent card
    let card = ctx
        .registry
        .get(agent_name)
        .map_err(|e| format!("Agent card not found: {}", e))?;

    // Enrich card with KG context from past dream cycles
    let (card, _) = crate::agent_backend::kg_context::enrich_with_kg_context(
        &ctx.memory_store,
        &ctx.embedder,
        target_agent_id,
        task,
        card,
    )
    .await;

    // Build execution context
    let stmt = crate::ast::AgentStmt {
        name: agent_name.to_string(),
        agent_type: Some(card.agent_type.clone()),
        query: task.to_string(),
        executor: None,
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    };

    let context = ExecutionContext {
        program: crate::ast::Program { statements: vec![] },
        agent_card: card,
        creature_id: None,
        cognition_tier: None,
        // Delegated child inherits the parent execution's funding.
        credentials: ctx.credentials.clone(),
        // ...but NOT the parent's attachments, and that is not a dropped frame.
        //
        // Attachments belong to a request, not to a session. Delegation builds a
        // new request whose content is the text the parent chose to send, so the
        // child was never promised an image. Propagating one silently would hand
        // a frame to an agent that may not declare `accepts: image`, and the
        // parent would have no way to know whether it arrived.
        //
        // If a compound agent needs to pass a photograph to a specialist, that
        // wants to be an explicit argument on `execute_agent` — visible in the
        // call, and checkable against the child's declared inputs.
        attachments: Vec::new(),
    };

    // mig-198: minted before the child runs so it can be handed to the child's
    // own ToolContext below, letting a grandchild link to it.
    let child_episode_id = Uuid::new_v4();

    // Build a ToolAwareExecutor with workspace tools but NO delegation
    let tool_context = Arc::new(ToolContext {
        // The child's own episode, so nested delegation links to the child
        // rather than skipping a level (mig-198).
        parent_episode_id: Some(child_episode_id),
        credentials: ctx.credentials.clone(),
        memory_store: ctx.memory_store.clone(),
        embedder: ctx.embedder.clone(),
        registry: ctx.registry.clone(),
        current_agent_id: Some(target_agent_id),
        workspace_id: Some(ws_id),
        workspace_slug: Some(ws_slug.to_string()),
        workspace_git: ctx.workspace_git.clone(),
        db: ctx.db.clone(),
        gas_fees: ctx.gas_fees.clone(),
        user_id: ctx.user_id.clone(),
        user_secrets: ctx.user_secrets.clone(),
        // Delegated child agents inherit the parent's trigger capability.
        eval_trigger: ctx.eval_trigger.clone(),
        remote_mcp: None,
    });

    let tool_executor = ToolAwareExecutor::new(
        ctx.registry.executor_arc(),
        ToolRegistry::with_workspace_no_delegation(),
        tool_context,
    );

    let output = tool_executor
        .execute(&stmt, &context)
        .await
        .map_err(|e| format!("Delegation failed: {}", e))?;

    // mig-198: record the child's own cost before its output is reduced to
    // prose. Everything below this line throws the token accounting away.
    record_delegated_episode(ctx, target_agent_id, child_episode_id, task, &output).await;

    let raw_response = output.metadata.reasoning.clone().unwrap_or_default();
    // Post the result as a workspace message from the delegated agent.
    //
    // Pass the raw LLM response through verbatim (see issue #2 / docs/specs/
    // 09_RESEARCH_AGENT_OUTPUT_STRIPPED.md). Falling back to evidence summaries
    // alone destroys structured JSON outputs from research-tier agents.
    let raw_response = output.metadata.reasoning.clone().unwrap_or_default();
    let evidence_text = output
        .evidence
        .iter()
        .filter_map(|e| {
            let s = e.summary.as_deref().unwrap_or("").trim();
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let result_text = if !raw_response.trim().is_empty() {
        raw_response.clone()
    } else {
        evidence_text.clone()
    };

    let result_msg = WorkspaceMessage {
        message_id: Uuid::new_v4(),
        workspace_id: ws_id,
        sender_type: "agent".to_string(),
        sender_id: target_agent_id.to_string(),
        sender_name: Some(display.clone()),
        content: if result_text.is_empty() {
            "(no output)".to_string()
        } else {
            result_text.clone()
        },
        message_type: "execution_result".to_string(),
        metadata: json!({
            "delegated_by": ctx.current_agent_id,
            "tokens_used": output.tokens_used,
            "tool_invocations": output.tool_invocations.len(),
            "loop_iterations": output.loop_iterations,
            "raw_response": raw_response,
        }),
        created_at: chrono::Utc::now(),
        episode_id: None,
    };
    let _ = ctx.memory_store.store_workspace_message(&result_msg).await;

    // Return result to calling agent
    Ok(if result_text.is_empty() {
        format!("{} completed the delegation but produced no text output. Check workspace files for artifacts.", display)
    } else {
        result_text
    })
}

async fn execute_list_agents(ctx: &ToolContext) -> Result<String, String> {
    let cards = ctx
        .registry
        .list_cards()
        .map_err(|e| format!("Failed to list agents: {}", e))?;

    let agents: Vec<serde_json::Value> = cards
        .iter()
        .map(|c| {
            json!({
                "id": c.agent_id,
                "type": c.agent_type,
                "description": c.metadata.description,
                "skills": c.capabilities.skills,
            })
        })
        .collect();

    serde_json::to_string_pretty(&agents).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_read_workspace_file(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let path = input
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: path")?;

    let slug = ctx
        .workspace_slug
        .as_deref()
        .ok_or("Not in a workspace context")?;
    let git = ctx
        .workspace_git
        .as_ref()
        .ok_or("Workspace git not available")?;

    // read_file is sync (git2), so run on blocking thread
    let git = Arc::clone(git);
    let slug = slug.to_string();
    let path = path.to_string();
    tokio::task::spawn_blocking(move || git.read_file(&slug, &path))
        .await
        .map_err(|e| format!("Join error: {}", e))?
        .map_err(|e| format!("Failed to read file: {}", e))
}

/// Read a single typed output from any workspace (cross-workspace read).
async fn execute_read_workspace_output(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = input
        .get("workspace_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: workspace_id")?;
    let key = input
        .get("key")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: key")?;

    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| "Invalid workspace_id — must be a UUID".to_string())?;

    let pool = ctx.memory_store.pool();
    let row = sqlx::query(
        "SELECT value, version, updated_at, updated_by
         FROM workspace_outputs
         WHERE workspace_id = $1 AND key = $2",
    )
    .bind(ws_uuid)
    .bind(key)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Database error: {}", e))?
    .ok_or_else(|| format!("Output '{}' not found in workspace {}", key, workspace_id))?;

    let value: serde_json::Value = row.get("value");
    let version: i32 = row.get("version");
    let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

    Ok(serde_json::json!({
        "workspace_id": workspace_id,
        "key": key,
        "value": value,
        "version": version,
        "updated_at": updated_at.to_rfc3339(),
    })
    .to_string())
}

/// List all published outputs for a workspace (cross-workspace read).
async fn execute_list_workspace_outputs(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = input
        .get("workspace_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: workspace_id")?;

    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| "Invalid workspace_id — must be a UUID".to_string())?;

    let pool = ctx.memory_store.pool();
    let rows = sqlx::query(
        "SELECT key, value, version, updated_at
         FROM workspace_outputs
         WHERE workspace_id = $1
         ORDER BY key",
    )
    .bind(ws_uuid)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Database error: {}", e))?;

    let outputs: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "key": r.get::<String, _>("key"),
                "value": r.get::<serde_json::Value, _>("value"),
                "version": r.get::<i32, _>("version"),
                "updated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(serde_json::json!({
        "workspace_id": workspace_id,
        "outputs": outputs,
        "count": outputs.len(),
    })
    .to_string())
}

async fn execute_list_workspace_agents(ctx: &ToolContext) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or("Not in a workspace context")?;

    let pool = ctx.memory_store.pool();
    let rows = sqlx::query(
        "SELECT a.agent_name, a.agent_type, a.description
         FROM workspace_agents wa
         JOIN agents a ON wa.agent_id = a.id
         WHERE wa.workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Query failed: {}", e))?;

    use sqlx::Row;
    let agents: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name": row.get::<String, _>("agent_name"),
                "type": row.get::<String, _>("agent_type"),
                "description": row.get::<Option<String>, _>("description"),
            })
        })
        .collect();

    serde_json::to_string_pretty(&agents).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Gemini image generation tools ─────────────────────────────────

/// Gemini API response types (shared with avatar generation)
#[derive(serde::Deserialize)]
struct GeminiToolResponse {
    candidates: Vec<GeminiToolCandidate>,
}

#[derive(serde::Deserialize)]
struct GeminiToolCandidate {
    content: GeminiToolContent,
}

#[derive(serde::Deserialize)]
struct GeminiToolContent {
    parts: Vec<GeminiToolPart>,
}

#[derive(serde::Deserialize)]
struct GeminiToolPart {
    text: Option<String>,
    #[serde(rename = "inlineData")]
    inline_data: Option<GeminiToolInlineData>,
}

#[derive(serde::Deserialize)]
struct GeminiToolInlineData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

const GEMINI_IMAGE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-image:generateContent";

async fn execute_generate_image(input: &serde_json::Value) -> Result<String, String> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .map_err(|_| "GEMINI_API_KEY not set — image generation unavailable")?;

    let prompt = input
        .get("prompt")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: prompt")?;

    let body = json!({
        "contents": [{
            "parts": [{ "text": prompt }]
        }],
        "generationConfig": {
            "responseModalities": ["IMAGE"]
        }
    });

    let client = reqwest::Client::new();
    let response = client
        .post(GEMINI_IMAGE_URL)
        .header("x-goog-api-key", &api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gemini API request failed: {}", e))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Gemini API error: {}", error_text));
    }

    let gemini_resp: GeminiToolResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;

    // Extract image data from response
    for candidate in &gemini_resp.candidates {
        for part in &candidate.content.parts {
            if let Some(ref inline_data) = part.inline_data {
                let result = json!({
                    "image": {
                        "mime_type": inline_data.mime_type,
                        "data": inline_data.data,
                    },
                    "description": candidate.content.parts.iter()
                        .filter_map(|p| p.text.as_deref())
                        .collect::<Vec<_>>()
                        .join(" "),
                });
                return serde_json::to_string_pretty(&result)
                    .map_err(|e| format!("Serialization error: {}", e));
            }
        }
    }

    Err("Gemini returned no image data".to_string())
}

async fn execute_edit_image(input: &serde_json::Value) -> Result<String, String> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .map_err(|_| "GEMINI_API_KEY not set — image editing unavailable")?;

    let prompt = input
        .get("prompt")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: prompt")?;

    let image_url = input
        .get("image_url")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: image_url")?;

    // Fetch the source image and convert to base64
    let client = reqwest::Client::new();
    let img_response = client
        .get(image_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch source image: {}", e))?;

    if !img_response.status().is_success() {
        return Err(format!(
            "Failed to fetch image ({}): {}",
            img_response.status(),
            image_url
        ));
    }

    let content_type = img_response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/png")
        .to_string();
    let img_bytes = img_response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read image bytes: {}", e))?;

    use base64::Engine;
    let img_b64 = base64::engine::general_purpose::STANDARD.encode(&img_bytes);

    let body = json!({
        "contents": [{
            "parts": [
                { "text": prompt },
                {
                    "inline_data": {
                        "mime_type": content_type,
                        "data": img_b64
                    }
                }
            ]
        }],
        "generationConfig": {
            "responseModalities": ["TEXT", "IMAGE"]
        }
    });

    let response = client
        .post(GEMINI_IMAGE_URL)
        .header("x-goog-api-key", &api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gemini API request failed: {}", e))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Gemini API error: {}", error_text));
    }

    let gemini_resp: GeminiToolResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;

    // Extract image + text from response
    for candidate in &gemini_resp.candidates {
        for part in &candidate.content.parts {
            if let Some(ref inline_data) = part.inline_data {
                let result = json!({
                    "image": {
                        "mime_type": inline_data.mime_type,
                        "data": inline_data.data,
                    },
                    "description": candidate.content.parts.iter()
                        .filter_map(|p| p.text.as_deref())
                        .collect::<Vec<_>>()
                        .join(" "),
                });
                return serde_json::to_string_pretty(&result)
                    .map_err(|e| format!("Serialization error: {}", e));
            }
        }
    }

    Err("Gemini returned no image data".to_string())
}

// ─── Workspace file write tool ─────────────────────────────────────

async fn execute_write_workspace_file(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let path = input
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: path")?;

    let content = input
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: content")?;

    let is_base64 = input
        .get("is_base64")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let commit_message = input
        .get("commit_message")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let slug = ctx
        .workspace_slug
        .as_deref()
        .ok_or("Not in a workspace context")?;
    let git = ctx
        .workspace_git
        .as_ref()
        .ok_or("Workspace git not available")?;

    let message = if commit_message.is_empty() {
        format!("agent: write {}", path)
    } else {
        commit_message.to_string()
    };

    if is_base64 {
        // Decode base64 and write as binary
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(content)
            .map_err(|e| format!("Invalid base64 content: {}", e))?;
        let size = bytes.len();

        let git = Arc::clone(git);
        let slug = slug.to_string();
        let path = path.to_string();
        let commit = tokio::task::spawn_blocking(move || {
            git.commit_file_bytes(&slug, &path, &bytes, &message)
        })
        .await
        .map_err(|e| format!("Join error: {}", e))?
        .map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(json!({
            "path": input.get("path").and_then(|v| v.as_str()).unwrap_or(""),
            "sha": commit.sha,
            "message": commit.message,
            "size_bytes": size,
        })
        .to_string())
    } else {
        let git = Arc::clone(git);
        let slug = slug.to_string();
        let path = path.to_string();
        let content = content.to_string();
        let commit =
            tokio::task::spawn_blocking(move || git.commit_file(&slug, &path, &content, &message))
                .await
                .map_err(|e| format!("Join error: {}", e))?
                .map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(json!({
            "path": input.get("path").and_then(|v| v.as_str()).unwrap_or(""),
            "sha": commit.sha,
            "message": commit.message,
        })
        .to_string())
    }
}

// ─── Voice synthesis tool ───────────────────────────────────────────

async fn execute_speak_text(input: &serde_json::Value) -> Result<String, String> {
    use crate::voice::{cartesia::VoiceStyle, CartesiaClient};

    let text = input
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: text")?;

    if text.len() > 5000 {
        return Err("Text exceeds maximum length of 5000 characters".to_string());
    }

    let voice_str = input
        .get("voice")
        .and_then(|v| v.as_str())
        .unwrap_or("narrator");

    let voice_style = match voice_str {
        "conversational" => VoiceStyle::Conversational,
        "storyteller" => VoiceStyle::Storyteller,
        _ => VoiceStyle::Narrator,
    };

    let api_key = std::env::var("CARTESIA_API_KEY")
        .map_err(|_| "CARTESIA_API_KEY not set — voice synthesis unavailable".to_string())?;

    let client = CartesiaClient::new(api_key);

    let audio_bytes = client
        .synthesize(text, voice_style)
        .await
        .map_err(|e| format!("Cartesia API error: {}", e))?;

    let duration_ms = client.estimate_duration_ms(text);

    // Encode as base64 for transport
    use base64::Engine;
    let audio_base64 = base64::engine::general_purpose::STANDARD.encode(&audio_bytes);

    Ok(json!({
        "audio": audio_base64,
        "format": "pcm_f32le",
        "sample_rate": 44100,
        "duration_ms": duration_ms,
        "character_count": text.len(),
    })
    .to_string())
}

// ─── Reduct.video API tools ────────────────────────────────────────
//
// Reduct's REST API is version 3 and lives under `/api/v3`. The interactive
// documentation is at `/backstage/api/`, which is a logged-in single-page app
// and NOT the request path — pointing a client at it yields a redirect to
// `/login`, which is worth recording because the two are one character apart in
// a card description and only one of them is callable.

const REDUCT_BASE_URL: &str = "https://app.reduct.video/api/v3";

/// Name of the credential, in both the scoped secret store and the env.
const REDUCT_KEY_NAME: &str = "REDUCT_API_KEY";

/// The workspace API key for this execution.
///
/// Scoped secret store first, process env second — the ordering
/// `RemoteMcpAuth` already documents (`secret_key`, then `env` "for
/// platform-owned integrations"). It matters here for a specific case rather
/// than for symmetry: `video_analyst` is `curated`, so
/// `resolve_agent_owner_secrets` returns `None` for it by design and the env
/// key is the correct source. A **fork** of it is owner-owned, carries its
/// owner's `REDUCT_API_KEY` in `user_secrets`, and — while these functions
/// took no `ToolContext` at all — could not reach it. That fork would then
/// have read someone else's workspace on the platform's key, which is the
/// cross-tenant leak SPEC_28 closed for LLM providers and had left open for
/// tool credentials.
///
/// `ctx` is `Option` so the two keyless call shapes in this file stay
/// possible; `None` means env only, which is what a context-free caller
/// honestly has.
fn reduct_api_key(ctx: Option<&ToolContext>) -> Result<String, String> {
    if let Some(k) = ctx
        .and_then(|c| c.user_secrets.as_ref())
        .and_then(|s| s.get(REDUCT_KEY_NAME))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return Ok(k.to_string());
    }
    match std::env::var(REDUCT_KEY_NAME) {
        Ok(k) if !k.trim().is_empty() => Ok(k.trim().to_string()),
        // Owner-facing, and deliberately does not tell the reader to set an
        // env var: the person who can fix this for an owned agent is its
        // owner, on their profile page. Same rule as
        // `ExecutionError::Unfunded`.
        _ => Err(format!(
            "No {REDUCT_KEY_NAME} available, so the Reduct.video tools cannot \
             run. An agent's owner sets it under Profile → Agent Secrets at \
             {}/profile; for a platform-operated agent it is deployment \
             configuration. Generate the key from Reduct at \
             https://app.reduct.video/backstage/api/ (Professional or \
             Enterprise plan). Report this rather than describing clips you \
             could not read.",
            crate::agent_backend::credentials::abw_base_url(),
        )),
    }
}

async fn reduct_get(path: &str, ctx: Option<&ToolContext>) -> Result<serde_json::Value, String> {
    let api_key = reduct_api_key(ctx)?;
    let url = format!("{}{}", REDUCT_BASE_URL, path);
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("X-Auth-Key", &api_key)
        .send()
        .await
        .map_err(|e| format!("Reduct API request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Reduct API error {}: {}", status, error_text));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Reduct response: {}", e))
}

async fn reduct_post(
    path: &str,
    body: &serde_json::Value,
    ctx: Option<&ToolContext>,
) -> Result<serde_json::Value, String> {
    let api_key = reduct_api_key(ctx)?;
    let url = format!("{}{}", REDUCT_BASE_URL, path);
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("X-Auth-Key", &api_key)
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .await
        .map_err(|e| format!("Reduct API request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Reduct API error {}: {}", status, error_text));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Reduct response: {}", e))
}

async fn execute_reduct_list_projects(ctx: &ToolContext) -> Result<String, String> {
    let data = reduct_get("/project", Some(ctx)).await?;
    serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_reduct_get_project(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let project_id = input
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: project_id")?;

    let data = reduct_get(&format!("/project/{}", project_id), Some(ctx)).await?;
    serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_reduct_get_transcript(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let project_id = input
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: project_id")?;

    let recording_id = input
        .get("recording_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: recording_id")?;

    // Anything other than an explicit `txt` is `json`, and that default is
    // load-bearing rather than tidy: only the JSON form carries segment
    // timestamps, and a transcript without timestamps is one a model can only
    // guess clip boundaries from. A typo in this argument must not silently
    // downgrade the caller to the representation that invites fabrication.
    let format = input
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("json");

    let ext = if format == "txt" { "txt" } else { "json" };
    let path = format!(
        "/project/{}/recording/{}/transcript.{}",
        project_id, recording_id, ext
    );

    if ext == "txt" {
        // Plain text transcript — fetch as text, not JSON
        let api_key = reduct_api_key(Some(ctx))?;
        let url = format!("{}{}", REDUCT_BASE_URL, path);
        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header("X-Auth-Key", &api_key)
            .send()
            .await
            .map_err(|e| format!("Reduct API request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Reduct API error {}: {}", status, error_text));
        }

        response
            .text()
            .await
            .map_err(|e| format!("Failed to read transcript: {}", e))
    } else {
        let data = reduct_get(&path, Some(ctx)).await?;
        serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))
    }
}

async fn execute_reduct_create_reel(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let project_id = input
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: project_id")?;

    let title = input
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: title")?;

    let data = reduct_post(
        &format!("/project/{}/reel", project_id),
        &json!({ "title": title }),
        Some(ctx),
    )
    .await?;

    serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_reduct_add_block(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let project_id = input
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: project_id")?;

    let reel_id = input
        .get("reel_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: reel_id")?;

    let block_type = input
        .get("block_type")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: block_type")?;

    let body = match block_type {
        "doc-range" => {
            let recording_id = input
                .get("recording_id")
                .and_then(|v| v.as_str())
                .ok_or("doc-range block requires recording_id")?;
            // `as_f64` rejects `"412.6"` and `"6:52"` alike, and the error
            // below says which was wanted. A formatted timecode is the
            // characteristic mistake here — see `abw/video_highlight_reel`'s
            // `clips.start_seconds` — and it must fail at the call rather than
            // be coerced into a number that plays the wrong moment.
            let start = input
                .get("start")
                .and_then(|v| v.as_f64())
                .ok_or("doc-range block requires `start` as a NUMBER of seconds (e.g. 412.6), not a timecode string")?;
            let end = input
                .get("end")
                .and_then(|v| v.as_f64())
                .ok_or("doc-range block requires `end` as a NUMBER of seconds (e.g. 448.2), not a timecode string")?;
            if end <= start {
                return Err(format!(
                    "doc-range block has end ({end}) at or before start ({start}). \
                     Reduct would store a zero- or negative-length clip, which \
                     plays as nothing and reads in the reel as a clip that \
                     exists."
                ));
            }

            json!({
                "type": "doc-range",
                "recording": recording_id,
                "start": start,
                "end": end
            })
        }
        "title" => {
            let text = input
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or("title block requires text")?;

            json!({
                "type": "title",
                "text": text
            })
        }
        other => {
            return Err(format!(
                "Unknown block type: {}. Use 'doc-range' or 'title'.",
                other
            ))
        }
    };

    let data = reduct_post(
        &format!("/project/{}/reel/{}/block", project_id, reel_id),
        &body,
        Some(ctx),
    )
    .await?;

    serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Coherence tools ───────────────────────────────────────────────

async fn execute_evaluate_coherence(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or("Not in a workspace context")?;

    let message_limit = input
        .get("message_limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(50)
        .min(100) as i64;

    // Fetch recent messages
    let messages = ctx
        .memory_store
        .get_workspace_messages(workspace_id, message_limit, None)
        .await
        .map_err(|e| format!("Failed to get messages: {}", e))?;

    if messages.is_empty() {
        return Ok(json!({
            "error": "No messages in workspace to evaluate"
        })
        .to_string());
    }

    // Convert to coherence-core Messages (reverse: DB returns DESC, observer expects chronological)
    let conv_id = ConversationId(workspace_id);
    let coherence_msgs: Vec<CoherenceMessage> = messages
        .iter()
        .rev()
        .map(|m| {
            let pid = ParticipantId(
                uuid::Uuid::parse_str(&m.sender_id).unwrap_or_else(|_| Uuid::new_v4()),
            );
            CoherenceMessage::new(pid, &m.content)
        })
        .collect();

    // Run observation pipeline: classify utterances + detect relations
    let observer = ConversationObserver::new(conv_id);
    let mut system = observer.observe(&coherence_msgs);

    // Run settling engine
    let engine = SettlingEngine::with_defaults();
    let _result = engine.settle(&mut system);

    // Extract snapshot
    let snapshot = system.snapshot();

    let principle_scores = serde_json::to_value(&snapshot.principle_scores).unwrap_or(json!({}));

    let health_indicators = json!({
        "feedback_action": serde_json::to_value(&snapshot.feedback_action).unwrap_or(json!("unknown")),
        "converged": snapshot.global_coherence.converged,
        "accepted_count": snapshot.global_coherence.accepted_count,
        "rejected_count": snapshot.global_coherence.rejected_count,
        "settling_cycles": snapshot.global_coherence.settling_cycles,
        "utterance_stats": {
            "total": snapshot.utterance_stats.total,
            "evidence_density": snapshot.utterance_stats.evidence_density(),
            "explanation_density": snapshot.utterance_stats.explanation_density(),
        },
    });

    // Store evaluation
    let eval = CoherenceEvaluation {
        eval_id: Uuid::new_v4(),
        workspace_id,
        global_score: snapshot.global_coherence.score,
        quality_label: snapshot.global_coherence.quality_label().to_string(),
        principle_scores: principle_scores.clone(),
        health_indicators: health_indicators.clone(),
        utterance_count: snapshot.utterance_stats.total as i32,
        message_window: Some(json!({
            "message_count": messages.len(),
            "from": messages.last().map(|m| m.created_at),
            "to": messages.first().map(|m| m.created_at),
        })),
        created_at: chrono::Utc::now(),
    };

    let eval_id = ctx
        .memory_store
        .store_coherence_evaluation(&eval)
        .await
        .map_err(|e| format!("Failed to store evaluation: {}", e))?;

    let result = json!({
        "eval_id": eval_id,
        "global_score": eval.global_score,
        "quality_label": eval.quality_label,
        "principle_scores": principle_scores,
        "health_indicators": health_indicators,
        "utterance_count": eval.utterance_count,
        "messages_evaluated": messages.len(),
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_coherence_snapshot(ctx: &ToolContext) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or("Not in a workspace context")?;

    let eval = ctx
        .memory_store
        .get_latest_coherence(workspace_id)
        .await
        .map_err(|e| format!("Failed to get coherence: {}", e))?;

    match eval {
        Some(e) => {
            let result = json!({
                "eval_id": e.eval_id,
                "global_score": e.global_score,
                "quality_label": e.quality_label,
                "principle_scores": e.principle_scores,
                "health_indicators": e.health_indicators,
                "utterance_count": e.utterance_count,
                "message_window": e.message_window,
                "evaluated_at": e.created_at.to_rfc3339(),
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        None => Ok(json!({
            "message": "No coherence evaluations yet for this workspace. Use evaluate_coherence to run the first evaluation."
        })
        .to_string()),
    }
}

async fn execute_get_workspace_messages(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or("Not in a workspace context")?;

    let limit = input
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(20)
        .min(50) as i64;

    let messages = ctx
        .memory_store
        .get_workspace_messages(workspace_id, limit, None)
        .await
        .map_err(|e| format!("Failed to get messages: {}", e))?;

    let formatted: Vec<serde_json::Value> = messages
        .iter()
        .rev() // chronological order
        .map(|m| {
            json!({
                "sender": m.sender_name.as_deref().unwrap_or(&m.sender_id),
                "sender_type": m.sender_type,
                "content": m.content,
                "type": m.message_type,
                "timestamp": m.created_at.to_rfc3339(),
            })
        })
        .collect();

    serde_json::to_string_pretty(&formatted).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Football API ─────────────────────────────────────────────────

/// Call API-Football v3 (https://www.api-football.com/documentation-v3).
/// Requires FOOTBALL_API_KEY environment variable.
async fn execute_call_football_api(input: &serde_json::Value) -> Result<String, String> {
    let endpoint = input
        .get("endpoint")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: endpoint")?
        .trim_start_matches('/');

    let api_key = std::env::var("FOOTBALL_API_KEY")
        .map_err(|_| "FOOTBALL_API_KEY environment variable not set.".to_string())?;

    let client = reqwest::Client::new();
    let url = format!("https://v3.football.api-sports.io/{}", endpoint);

    let mut req = client
        .get(&url)
        .header("x-apisports-key", &api_key)
        .header("Accept", "application/json");

    // Apply query params from the `params` object
    if let Some(params) = input.get("params").and_then(|v| v.as_object()) {
        let query: Vec<(String, String)> = params
            .iter()
            .map(|(k, v)| {
                let val = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                (k.clone(), val)
            })
            .collect();
        req = req.query(&query);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("API-Football request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("API-Football error {}: {}", status, body));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse API-Football response: {}", e))?;

    // Check API-level errors
    if let Some(errors) = data.get("errors") {
        if !errors.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            return Err(format!("API-Football errors: {}", errors));
        }
    }

    // Return the response, truncated if very large
    let result =
        serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))?;

    if result.len() > 16000 {
        Ok(format!(
            "{}... [truncated, {} total chars]",
            &result[..16000],
            result.len()
        ))
    } else {
        Ok(result)
    }
}

// ─── Web Search ───────────────────────────────────────────────────

/// Search the web using the Brave Search API.
/// Requires BRAVE_SEARCH_API_KEY environment variable.
async fn execute_web_search(input: &serde_json::Value) -> Result<String, String> {
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: query")?;
    let count = input
        .get("count")
        .and_then(|v| v.as_u64())
        .unwrap_or(5)
        .min(10) as usize;
    let freshness = input.get("freshness").and_then(|v| v.as_str());

    let api_key = std::env::var("BRAVE_SEARCH_API_KEY")
        .map_err(|_| "BRAVE_SEARCH_API_KEY environment variable not set. Get a free API key at https://brave.com/search/api/".to_string())?;

    let client = reqwest::Client::new();
    let mut req = client
        .get("https://api.search.brave.com/res/v1/web/search")
        .header("Accept", "application/json")
        .header("X-Subscription-Token", &api_key)
        .query(&[
            ("q", query),
            ("count", &count.to_string()),
            ("search_lang", "en"),
        ]);

    if let Some(f) = freshness {
        req = req.query(&[("freshness", f)]);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Brave Search request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Brave Search API error {}: {}", status, body));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Brave Search response: {}", e))?;

    let results = data
        .get("web")
        .and_then(|w| w.get("results"))
        .and_then(|r| r.as_array());

    let Some(results) = results else {
        return Ok("No web results found for this query.".to_string());
    };

    if results.is_empty() {
        return Ok("No web results found for this query.".to_string());
    }

    let mut output = format!("## Web Search Results for: {}\n\n", query);
    for (i, result) in results.iter().enumerate() {
        let title = result
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("(no title)");
        let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let description = result
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("(no description)");
        let age = result.get("age").and_then(|v| v.as_str()).unwrap_or("");
        let published = result
            .get("page_age")
            .and_then(|v| v.as_str())
            .or(if age.is_empty() { None } else { Some(age) })
            .unwrap_or("(date unknown)");

        output.push_str(&format!(
            "**{}. {}**\n{}\n{}\n{}\n\n",
            i + 1,
            title,
            url,
            published,
            description
        ));
    }

    // Truncate to avoid context overflow
    if output.len() > 12_000 {
        output.truncate(12_000);
        output.push_str("\n... [truncated]");
    }

    Ok(output)
}

// ─── Monte Carlo / FPL Simulation tools ───────────────────────────

/// Parse an FPL program string into an AST Program, returning a human-readable error on failure.
fn parse_fpl(source: &str) -> Result<crate::ast::Program, String> {
    let tokens = crate::lexer::Lexer::new(source)
        .tokenize()
        .map_err(|errs| {
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        })?;
    crate::parser::Parser::new(tokens)
        .parse()
        .map_err(|e| e.to_string())
}

/// Run a Monte Carlo simulation from an FPL program.
async fn execute_run_monte_carlo(input: &serde_json::Value) -> Result<String, String> {
    let source = input
        .get("fpl_program")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: fpl_program")?;

    let program = parse_fpl(source)?;

    let iterations = input
        .get("iterations")
        .and_then(|v| v.as_u64())
        .unwrap_or(10_000) as usize;

    let mut executor = crate::executor::Executor::new(iterations);
    let results = executor
        .execute(&program)
        .map_err(|e| format!("Simulation error: {}", e))?;

    // Build a compact ASCII histogram (10 bins)
    let histogram = results.histogram(10);
    let max_count = histogram.iter().map(|(_, c)| *c).max().unwrap_or(1);
    let bar_width = 30usize;
    let mut hist_str = String::new();
    for (bin_start, count) in &histogram {
        let bar_len = (count * bar_width) / max_count;
        hist_str.push_str(&format!(
            "  {:>6.3} | {:<30} {}\n",
            bin_start,
            "#".repeat(bar_len),
            count
        ));
    }

    let result = json!({
        "iterations": results.iterations,
        "mean": results.mean,
        "median": results.median,
        "std_dev": results.std_dev,
        "min": results.min,
        "max": results.max,
        "percentiles": {
            "p5": results.p5,
            "p25": results.p25,
            "p75": results.p75,
            "p95": results.p95,
        },
        "base_rate": results.base_rate,
        "divergence_relative": results.divergence_relative,
        "divergence_absolute": results.divergence_absolute,
        "histogram_ascii": hist_str,
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

/// Run Sobol global sensitivity analysis on an FPL program.
async fn execute_run_sensitivity_analysis(input: &serde_json::Value) -> Result<String, String> {
    let source = input
        .get("fpl_program")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: fpl_program")?;

    let program = parse_fpl(source)?;

    let iterations = input
        .get("iterations")
        .and_then(|v| v.as_u64())
        .unwrap_or(10_000) as usize;

    let analysis = crate::sensitivity::full_sensitivity_analysis(&program, iterations)
        .map_err(|e| format!("Sensitivity analysis error: {}", e))?;

    // Build ranked driver list with indices
    let drivers: Vec<serde_json::Value> = analysis
        .ranked_drivers
        .iter()
        .filter_map(|name| analysis.driver_sensitivities.get(name))
        .map(|ds| {
            let ci_low = (ds.total_order_index - 1.96 * ds.standard_error).max(0.0);
            let ci_high = (ds.total_order_index + 1.96 * ds.standard_error).min(1.0);
            json!({
                "driver": ds.driver_name,
                "first_order_index": ds.first_order_index,
                "total_order_index": ds.total_order_index,
                "variance_contribution": ds.variance_contribution,
                "standard_error": ds.standard_error,
                "confidence_interval_95": [ci_low, ci_high],
            })
        })
        .collect();

    // ASCII tornado diagram
    let mut tornado = String::new();
    for ds in &drivers {
        let s_t = ds["total_order_index"].as_f64().unwrap_or(0.0);
        let bar_len = (s_t * 40.0) as usize;
        tornado.push_str(&format!(
            "  {:<30} | {:<40} {:.3}\n",
            ds["driver"].as_str().unwrap_or(""),
            "#".repeat(bar_len),
            s_t
        ));
    }

    let result = json!({
        "baseline": {
            "mean": analysis.baseline.mean,
            "std_dev": analysis.baseline.std_dev,
            "p5": analysis.baseline.p5,
            "p95": analysis.baseline.p95,
        },
        "drivers_ranked_by_total_order": drivers,
        "tornado_diagram_ascii": tornado,
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Observability composition tools ───────────────────────────────
//
// Read-side wrappers around MemoryStore methods for the observability
// composition (observability_coordinator + eval_runner + anomaly_triager
// + dyad_observer). See docs/AGENT_MODEL.md §3 and §4.2.2.
//
// All six are pure reads — no gas charged, no writes. Action tools
// (run_evaluator_registry, route_to_hitl, classify_anomaly) will be
// added in a follow-up commit since they have larger blast radius.

fn parse_uuid_field(input: &serde_json::Value, field: &str) -> Result<Uuid, String> {
    let s = input
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Missing required parameter: {}", field))?;
    Uuid::parse_str(s).map_err(|e| format!("Invalid UUID for {}: {}", field, e))
}

/// Resolve an `agent_id` field that may be either a UUID string or an
/// agent-name slug (e.g. "equity_analyst").  UUID is tried first; on
/// failure we hit the DB via `get_agent_by_name` to obtain the real UUID.
async fn resolve_agent_id(
    input: &serde_json::Value,
    field: &str,
    ctx: &ToolContext,
) -> Result<Uuid, String> {
    let s = input
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Missing required parameter: {}", field))?;

    if let Ok(uuid) = Uuid::parse_str(s) {
        return Ok(uuid);
    }

    // Treat as a name slug and look up the UUID in the agents table.
    ctx.memory_store
        .get_agent_by_name(s)
        .await
        .map(|a| a.agent_id)
        .map_err(|e| format!("Agent '{}' not found (tried as name slug): {}", s, e))
}

async fn execute_query_eval_signals(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let run_id = parse_uuid_field(input, "run_id")?;
    let signals = ctx
        .memory_store
        .list_eval_signals_for_run(run_id)
        .await
        .map_err(|e| format!("Failed to list eval_signals: {}", e))?;

    serde_json::to_string_pretty(&json!({
        "run_id": run_id,
        "count": signals.len(),
        "signals": signals,
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_query_eval_runs(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_id = resolve_agent_id(input, "agent_id", ctx).await?;
    let limit = input
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(20)
        .clamp(1, 100);

    let runs = ctx
        .memory_store
        .list_eval_runs(agent_id, limit)
        .await
        .map_err(|e| format!("Failed to list eval_runs: {}", e))?;

    serde_json::to_string_pretty(&json!({
        "agent_id": agent_id,
        "count": runs.len(),
        "runs": runs,
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_query_anomalies(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_id = resolve_agent_id(input, "agent_id", ctx).await?;
    let limit = input
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(50)
        .clamp(1, 500);

    let events = ctx
        .memory_store
        .list_anomaly_events_for_agent(agent_id, limit)
        .await
        .map_err(|e| format!("Failed to list anomalies: {}", e))?;

    serde_json::to_string_pretty(&json!({
        "agent_id": agent_id,
        "count": events.len(),
        "anomalies": events,
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_query_hitl_queue(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let limit = input
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(50)
        .clamp(1, 200);

    let events = ctx
        .memory_store
        .list_pending_anomaly_events(limit)
        .await
        .map_err(|e| format!("Failed to list HITL queue: {}", e))?;

    serde_json::to_string_pretty(&json!({
        "count": events.len(),
        "pending": events,
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_query_timeline(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_id = resolve_agent_id(input, "agent_id", ctx).await?;
    let limit = input
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(100)
        .clamp(1, 500);

    let entries = ctx
        .memory_store
        .list_timeline_entries(agent_id, limit)
        .await
        .map_err(|e| format!("Failed to list timeline: {}", e))?;

    serde_json::to_string_pretty(&json!({
        "agent_id": agent_id,
        "count": entries.len(),
        "timeline": entries,
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_query_dyad_state(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_id = resolve_agent_id(input, "agent_id", ctx).await?;

    let dyads = ctx
        .memory_store
        .list_dyads_for_agent(agent_id)
        .await
        .map_err(|e| format!("Failed to list dyads: {}", e))?;

    serde_json::to_string_pretty(&json!({
        "agent_id": agent_id,
        "count": dyads.len(),
        "dyads": dyads,
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_classify_anomaly(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let anomaly_id = parse_uuid_field(input, "anomaly_id")?;

    let event = ctx
        .memory_store
        .get_anomaly_event(anomaly_id)
        .await
        .map_err(|e| format!("Failed to get anomaly: {}", e))?
        .ok_or_else(|| format!("Anomaly {} not found", anomaly_id))?;

    // Related signals from the same run, if any.
    let related_signals = match event.run_id {
        Some(rid) => ctx
            .memory_store
            .list_eval_signals_for_run(rid)
            .await
            .unwrap_or_default(),
        None => Vec::new(),
    };

    // Agent persona version + prior HITL actions on this event.
    let agent = ctx
        .memory_store
        .get_agent(event.agent_id)
        .await
        .map_err(|e| format!("Failed to get agent: {}", e))?;

    let prior_actions = ctx
        .memory_store
        .list_hitl_actions_for_anomaly(anomaly_id)
        .await
        .unwrap_or_default();

    serde_json::to_string_pretty(&json!({
        "event": event,
        "related_signals": related_signals,
        "related_signal_count": related_signals.len(),
        "agent_persona_version": agent.as_ref().map(|a| a.persona_version),
        "agent_name": agent.as_ref().map(|a| &a.agent_name),
        "prior_hitl_actions": prior_actions,
        "prior_action_count": prior_actions.len(),
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_route_to_hitl(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let anomaly_id = parse_uuid_field(input, "anomaly_id")?;
    let recommended_action = input
        .get("recommended_action")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: recommended_action")?;
    if !matches!(recommended_action, "approve" | "relabel" | "intervene") {
        return Err(format!(
            "Invalid recommended_action '{}' — must be approve|relabel|intervene",
            recommended_action
        ));
    }
    let scope = input
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("episode");
    if !matches!(scope, "episode" | "agent" | "agent_wide") {
        return Err(format!(
            "Invalid scope '{}' — must be episode|agent|agent_wide",
            scope
        ));
    }
    let justification = input
        .get("justification")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: justification")?;

    let db = ctx
        .db
        .as_ref()
        .ok_or("route_to_hitl requires database context")?;

    // Refuse to route an already-resolved event.
    let event = ctx
        .memory_store
        .get_anomaly_event(anomaly_id)
        .await
        .map_err(|e| format!("Failed to get anomaly: {}", e))?
        .ok_or_else(|| format!("Anomaly {} not found", anomaly_id))?;
    if event.resolved_at.is_some() {
        return Err(format!(
            "Anomaly {} is already resolved — cannot route",
            anomaly_id
        ));
    }

    let by_agent = ctx
        .current_agent_id
        .map(|u| u.to_string())
        .unwrap_or_else(|| "unknown".into());
    let recommendation = json!({
        "action": recommended_action,
        "scope": scope,
        "justification": justification,
        "by_agent": by_agent,
        "at": chrono::Utc::now().to_rfc3339(),
    });

    // Merge agent_recommendation into payload jsonb, set requires_review=true.
    // jsonb || jsonb is the merge operator; existing keys in payload are
    // preserved unless the right-hand side overrides them.
    sqlx::query(
        r#"UPDATE anomaly_events
           SET payload = COALESCE(payload, '{}'::jsonb)
                        || jsonb_build_object('agent_recommendation', $2::jsonb),
               requires_review = TRUE
           WHERE event_id = $1"#,
    )
    .bind(anomaly_id)
    .bind(&recommendation)
    .execute(db)
    .await
    .map_err(|e| format!("Failed to update anomaly: {}", e))?;

    serde_json::to_string_pretty(&json!({
        "routed": true,
        "anomaly_id": anomaly_id,
        "recommendation": recommendation,
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_run_evaluator_registry(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let trigger = ctx
        .eval_trigger
        .as_ref()
        .ok_or("run_evaluator_registry is not available in this execution context (no eval_trigger plumbed). Use the agent detail page's Run + Judge button or POST /api/agents/:id/eval/run.")?;

    let agent_id = resolve_agent_id(input, "agent_id", ctx).await?;
    let user_id = ctx
        .user_id
        .clone()
        .ok_or("run_evaluator_registry requires user_id in ToolContext")?;
    let judge = input.get("judge").and_then(|v| v.as_bool()).unwrap_or(true);
    let tags: Vec<String> = input
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let run_id = trigger
        .trigger_eval(agent_id, user_id, judge, tags)
        .await
        .map_err(|e| format!("Failed to trigger eval: {}", e))?;

    serde_json::to_string_pretty(&json!({
        "run_id": run_id,
        "agent_id": agent_id,
        "status": "running",
        "note": "Run started in background. Poll with query_eval_runs or query_eval_signals once it completes.",
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

// ─── Wild / Foraging tool implementations ──────────────────────────

/// iNaturalist observations near a lat/lng.
/// Uses the iNaturalist API v2 — no authentication required for reads.
async fn execute_inat_observations(input: &serde_json::Value) -> Result<String, String> {
    let lat = input
        .get("lat")
        .and_then(|v| v.as_f64())
        .ok_or("lat is required")?;
    let lng = input
        .get("lng")
        .and_then(|v| v.as_f64())
        .ok_or("lng is required")?;
    let radius_km = input
        .get("radius_km")
        .and_then(|v| v.as_f64())
        .unwrap_or(5.0)
        .min(50.0);
    let taxon = input
        .get("taxon")
        .and_then(|v| v.as_str())
        .unwrap_or("Fungi");
    let days_back = input
        .get("days_back")
        .and_then(|v| v.as_u64())
        .unwrap_or(30)
        .min(365);
    let quality_grade = input
        .get("quality_grade")
        .and_then(|v| v.as_str())
        .unwrap_or("needs_id");
    let limit = input
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20)
        .min(50);

    // Calculate date range
    let d1 = (chrono::Utc::now() - chrono::Duration::days(days_back as i64))
        .format("%Y-%m-%d")
        .to_string();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let url = "https://api.inaturalist.org/v2/observations";
    let resp = client
        .get(url)
        .header("User-Agent", "AgentBestiaryWorld/1.0 (kask.bio/projects/wild)")
        .query(&[
            ("lat", lat.to_string()),
            ("lng", lng.to_string()),
            ("radius", radius_km.to_string()),
            ("iconic_taxa[]", taxon.to_string()),
            ("quality_grade", quality_grade.to_string()),
            ("d1", d1),
            ("order_by", "observed_on".to_string()),
            ("order", "desc".to_string()),
            ("per_page", limit.to_string()),
            ("fields", "taxon.name,taxon.preferred_common_name,observed_on,quality_grade,location,photos.url".to_string()),
        ])
        .send()
        .await
        .map_err(|e| format!("iNaturalist API request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("iNaturalist API error: {}", resp.status()));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse iNaturalist response: {}", e))?;

    let results = data
        .get("results")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();
    let total = data
        .get("total_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // Summarise into a compact form for the agent
    let observations: Vec<serde_json::Value> = results
        .iter()
        .map(|obs| {
            let taxon_name = obs
                .pointer("/taxon/name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let common_name = obs
                .pointer("/taxon/preferred_common_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let date = obs
                .get("observed_on")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let grade = obs
                .get("quality_grade")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let location = obs.get("location").and_then(|v| v.as_str()).unwrap_or("");
            let has_photo = obs.pointer("/photos/0/url").is_some();
            json!({
                "species": taxon_name,
                "common_name": common_name,
                "observed_on": date,
                "quality_grade": grade,
                "location": location,
                "has_photo": has_photo,
            })
        })
        .collect();

    // Count unique species
    let mut species_counts: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    for obs in &results {
        let name = obs
            .pointer("/taxon/name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");
        *species_counts.entry(name).or_insert(0) += 1;
    }
    let mut species_summary: Vec<(&str, u32)> =
        species_counts.iter().map(|(k, v)| (*k, *v)).collect();
    species_summary.sort_by(|a, b| b.1.cmp(&a.1));

    serde_json::to_string_pretty(&json!({
        "search_params": {
            "lat": lat, "lng": lng,
            "radius_km": radius_km,
            "taxon": taxon,
            "days_back": days_back,
            "quality_grade": quality_grade,
        },
        "total_observations": total,
        "returned": observations.len(),
        "species_summary": species_summary.iter().take(10).map(|(s, c)| json!({"species": s, "count": c})).collect::<Vec<_>>(),
        "observations": observations,
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

/// MycoBank fungal nomenclature lookup.
/// Uses the bio-aware MycoBank web services API.
/// Requires MYCOBANK_API_KEY environment variable (Bearer token).
/// Falls back to a descriptive error if key is not set.
/// Public for the same reason as [`execute_gbif_species_search`].
///
/// Note this one degrades honestly: with no `MYCOBANK_API_KEY` it falls back to
/// GBIF scoped to Fungi and says so in its own `source` field, so a caller can
/// tell which database answered.
pub async fn execute_mycobank_lookup(input: &serde_json::Value) -> Result<String, String> {
    let name = input
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("name is required")?;
    let include_synonyms = input
        .get("include_synonyms")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let api_key = std::env::var("MYCOBANK_API_KEY").unwrap_or_default();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    // If no API key, fall back to GBIF for fungal taxonomy
    if api_key.is_empty() {
        // Graceful degradation: use GBIF which covers fungi
        let gbif_url = "https://api.gbif.org/v1/species/match";
        let resp = client
            .get(gbif_url)
            .header(
                "User-Agent",
                "AgentBestiaryWorld/1.0 (kask.bio/projects/wild)",
            )
            .query(&[("name", name), ("kingdom", "Fungi"), ("verbose", "true")])
            .send()
            .await
            .map_err(|e| format!("GBIF fallback request failed: {}", e))?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse GBIF response: {}", e))?;

        return serde_json::to_string_pretty(&json!({
            "source": "GBIF (MycoBank API key not configured)",
            "query": name,
            "accepted_name": data.get("species").or_else(|| data.get("canonicalName")),
            "status": data.get("status"),
            "rank": data.get("rank"),
            "kingdom": data.get("kingdom"),
            "phylum": data.get("phylum"),
            "class": data.get("class"),
            "order": data.get("order"),
            "family": data.get("family"),
            "genus": data.get("genus"),
            "gbif_key": data.get("speciesKey").or_else(|| data.get("usageKey")),
            "confidence": data.get("confidence"),
            "note": "Configure MYCOBANK_API_KEY for authoritative MycoBank nomenclature"
        }))
        .map_err(|e| format!("Serialization error: {}", e));
    }

    // MycoBank API
    let base_url = "https://webservices.bio-aware.com/cbsdatabase_new/mycobank/taxonnames";
    let resp = client
        .get(base_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header(
            "User-Agent",
            "AgentBestiaryWorld/1.0 (kask.bio/projects/wild)",
        )
        .query(&[("filter", format!("name startWith '{}'", name))])
        .send()
        .await
        .map_err(|e| format!("MycoBank API request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("MycoBank API error: {}", resp.status()));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse MycoBank response: {}", e))?;

    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if items.is_empty() {
        return Ok(serde_json::to_string_pretty(&json!({
            "source": "MycoBank",
            "query": name,
            "found": false,
            "message": "No records found in MycoBank for this name"
        }))
        .unwrap_or_default());
    }

    // Find the best match — prefer exact name match with valid status
    let best = items
        .iter()
        .find(|item| {
            item.get("name")
                .and_then(|v| v.as_str())
                .map(|n| n.to_lowercase() == name.to_lowercase())
                .unwrap_or(false)
                && item
                    .get("nameStatus")
                    .and_then(|v| v.as_str())
                    .map(|s| s != "Illegitimate" && s != "Invalid")
                    .unwrap_or(true)
        })
        .or_else(|| items.first());

    let result = best.cloned().unwrap_or(json!({}));

    serde_json::to_string_pretty(&json!({
        "source": "MycoBank",
        "query": name,
        "found": true,
        "mycobank_number": result.get("mycobankNr"),
        "accepted_name": result.pointer("/synonymy/currentName").or_else(|| result.get("name")),
        "name_status": result.get("nameStatus"),
        "author": result.get("authors"),
        "year": result.get("year"),
        "rank": result.get("rank"),
        "phylum": result.pointer("/classification/phylum"),
        "class": result.pointer("/classification/class"),
        "order": result.pointer("/classification/order"),
        "family": result.pointer("/classification/family"),
        "genus": result.pointer("/classification/genus"),
        "synonyms_count": if include_synonyms { items.len() } else { 0 },
        "url": result.get("mycobankNr").and_then(|n| n.as_str())
            .map(|n| format!("https://www.mycobank.org/page/Name%20details%20page/field/Mycobank%20%23/{}", n)),
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

/// OpenWeather current conditions + 5-day forecast.
/// Requires OPENWEATHER_API_KEY environment variable.
async fn execute_openweather_forecast(input: &serde_json::Value) -> Result<String, String> {
    let lat = input
        .get("lat")
        .and_then(|v| v.as_f64())
        .ok_or("lat is required")?;
    let lng = input
        .get("lng")
        .and_then(|v| v.as_f64())
        .ok_or("lng is required")?;
    let include_forecast = input
        .get("include_forecast")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let api_key = std::env::var("OPENWEATHER_API_KEY").map_err(|_| {
        "OPENWEATHER_API_KEY not set. Get a free key at https://openweathermap.org/api".to_string()
    })?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();

    // Current conditions
    let current_resp = client
        .get("https://api.openweathermap.org/data/2.5/weather")
        .query(&[
            ("lat", lat.to_string()),
            ("lon", lng.to_string()),
            ("appid", api_key.clone()),
            ("units", "metric".to_string()),
        ])
        .send()
        .await
        .map_err(|e| format!("OpenWeather current request failed: {}", e))?;

    if !current_resp.status().is_success() {
        return Err(format!("OpenWeather API error: {}", current_resp.status()));
    }

    let current: serde_json::Value = current_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse current weather: {}", e))?;

    let current_summary = json!({
        "temp_c": current.pointer("/main/temp"),
        "feels_like_c": current.pointer("/main/feels_like"),
        "humidity_pct": current.pointer("/main/humidity"),
        "pressure_hpa": current.pointer("/main/pressure"),
        "description": current.pointer("/weather/0/description"),
        "wind_speed_ms": current.pointer("/wind/speed"),
        "wind_direction_deg": current.pointer("/wind/deg"),
        "rain_1h_mm": current.pointer("/rain/1h").unwrap_or(&serde_json::Value::Null),
        "clouds_pct": current.pointer("/clouds/all"),
        "visibility_m": current.get("visibility"),
        "sunrise": current.pointer("/sys/sunrise"),
        "sunset": current.pointer("/sys/sunset"),
    });

    if !include_forecast {
        return serde_json::to_string_pretty(&json!({
            "location": { "lat": lat, "lng": lng },
            "current": current_summary,
        }))
        .map_err(|e| format!("Serialization error: {}", e));
    }

    // 5-day / 3-hour forecast
    let forecast_resp = client
        .get("https://api.openweathermap.org/data/2.5/forecast")
        .query(&[
            ("lat", lat.to_string()),
            ("lon", lng.to_string()),
            ("appid", api_key),
            ("units", "metric".to_string()),
            ("cnt", "40".to_string()), // 5 days × 8 readings/day
        ])
        .send()
        .await
        .map_err(|e| format!("OpenWeather forecast request failed: {}", e))?;

    let forecast: serde_json::Value = forecast_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse forecast: {}", e))?;

    // Summarise by day
    let mut daily: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    if let Some(list) = forecast.get("list").and_then(|v| v.as_array()) {
        for entry in list {
            let dt_txt = entry.get("dt_txt").and_then(|v| v.as_str()).unwrap_or("");
            let day = dt_txt.split(' ').next().unwrap_or(dt_txt).to_string();
            let temp = entry
                .pointer("/main/temp")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let rain = entry
                .pointer("/rain/3h")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let humidity = entry
                .pointer("/main/humidity")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            let d = daily.entry(day).or_insert(json!({
                "temps": [], "rain_total_mm": 0.0, "humidity_avg": 0.0, "count": 0
            }));
            if let Some(obj) = d.as_object_mut() {
                if let Some(arr) = obj.get_mut("temps").and_then(|v| v.as_array_mut()) {
                    arr.push(json!(temp));
                }
                let rain_total = obj
                    .get("rain_total_mm")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let hum_acc = obj
                    .get("humidity_avg")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let count = obj.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                obj.insert("rain_total_mm".to_string(), json!(rain_total + rain));
                obj.insert(
                    "humidity_avg".to_string(),
                    json!((hum_acc * count as f64 + humidity) / (count + 1) as f64),
                );
                obj.insert("count".to_string(), json!(count + 1));
            }
        }
    }

    let forecast_summary: Vec<serde_json::Value> = daily
        .iter()
        .map(|(day, d)| {
            let temps = d
                .get("temps")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let temps_f: Vec<f64> = temps.iter().filter_map(|v| v.as_f64()).collect();
            let min_t = temps_f.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_t = temps_f.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            json!({
                "date": day,
                "temp_min_c": if min_t.is_finite() { min_t } else { 0.0 },
                "temp_max_c": if max_t.is_finite() { max_t } else { 0.0 },
                "rain_total_mm": d.get("rain_total_mm"),
                "humidity_avg_pct": d.get("humidity_avg"),
            })
        })
        .collect();

    // Foraging condition assessment
    let current_temp = current
        .pointer("/main/temp")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let current_humidity = current
        .pointer("/main/humidity")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let recent_rain: f64 = forecast_summary
        .iter()
        .take(2)
        .filter_map(|d| d.get("rain_total_mm").and_then(|v| v.as_f64()))
        .sum();

    let foraging_signal = if current_temp > 5.0
        && current_temp < 25.0
        && current_humidity > 70.0
        && recent_rain > 5.0
    {
        "good"
    } else if current_temp > 0.0 && current_temp < 30.0 && current_humidity > 50.0 {
        "fair"
    } else {
        "poor"
    };

    serde_json::to_string_pretty(&json!({
        "location": { "lat": lat, "lng": lng },
        "current": current_summary,
        "forecast_5day": forecast_summary,
        "foraging_conditions": {
            "signal": foraging_signal,
            "temp_in_range": current_temp > 5.0 && current_temp < 25.0,
            "humidity_sufficient": current_humidity > 70.0,
            "recent_rainfall_mm": recent_rain,
            "note": match foraging_signal {
                "good" => "Conditions are favourable for fungal fruiting. Scout within 1-4 days.",
                "fair" => "Conditions are marginal. Check specific species requirements.",
                _ => "Conditions are unfavourable. Wait for rain and temperature moderation.",
            }
        }
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

#[cfg(test)]
mod gbif_scope_tests {
    use super::*;
    use serde_json::json;

    /// The whole point of the change: every existing caller keeps the exact
    /// behaviour it had, because none of them pass a scope.
    ///
    /// Six agents depend on the insect filter — naturalist, species_resolver,
    /// swarm_host, enemy_sensor, genome_profiler, prey_locator. If this test
    /// ever fails, an insect agent has quietly been given the whole tree of
    /// life to confuse itself with.
    #[test]
    fn omitting_the_scope_keeps_the_historical_insect_filter() {
        for input in [
            json!({ "query": "Danaus plexippus" }),
            json!({ "query": "Bombus", "rank": "GENUS" }),
            json!({ "query": "x", "limit": 10 }),
        ] {
            assert_eq!(
                gbif_higher_taxon_key(&input),
                Ok(216),
                "default scope changed for {input}"
            );
        }
        assert_eq!(GBIF_DEFAULT_SCOPE_KEY, 216);
    }

    #[test]
    fn a_named_scope_widens_the_search() {
        let cases = [
            ("plantae", 6),
            ("fungi", 5),
            ("aves", 212),
            ("lepidoptera", 797),
            ("insecta", 216),
        ];
        for (name, key) in cases {
            assert_eq!(
                gbif_higher_taxon_key(&json!({ "query": "q", "scope": name })),
                Ok(key),
                "scope `{name}` resolved wrongly"
            );
        }
    }

    #[test]
    fn scope_names_are_case_and_whitespace_insensitive() {
        for spelling in ["Plantae", "PLANTAE", "  plantae  "] {
            assert_eq!(
                gbif_higher_taxon_key(&json!({ "scope": spelling })),
                Ok(6),
                "`{spelling}` did not resolve"
            );
        }
    }

    /// An explicit key wins, so a taxon the named table does not cover is
    /// still reachable without editing this file.
    #[test]
    fn an_explicit_key_takes_precedence() {
        assert_eq!(
            gbif_higher_taxon_key(&json!({ "higher_taxon_key": 220, "scope": "plantae" })),
            Ok(220)
        );
        assert_eq!(
            gbif_higher_taxon_key(&json!({ "higher_taxon_key": 1 })),
            Ok(1)
        );
    }

    /// **A typo must not silently become the default.**
    ///
    /// If `"plantea"` fell back to Insecta, a search for a real plant would
    /// return zero results and the caller would record `tool_no_match` — "GBIF
    /// has no record of this" — when the truth is "you asked the wrong
    /// question". That is a false claim about the world manufactured by a
    /// spelling mistake, and the provenance vocabulary exists to keep those two
    /// cases apart.
    #[test]
    fn an_unknown_scope_is_an_error_not_a_silent_fallback() {
        let err = gbif_higher_taxon_key(&json!({ "query": "Quercus", "scope": "plantea" }))
            .expect_err("a mis-spelled scope was accepted");
        assert!(
            err.contains("plantea"),
            "error does not quote the input: {err}"
        );
        assert!(
            err.contains("plantae"),
            "error does not list valid scopes: {err}"
        );
    }

    /// The recorded keys must match what was verified against GBIF, and the
    /// table must stay unique — two names for one key is harmless, but one name
    /// mapping twice is a silent shadowing bug.
    #[test]
    fn the_scope_table_matches_what_was_verified_against_gbif() {
        // Verified 2026-08-17 via GET /v1/species/match?name=<name>, matchType
        // EXACT for all but Animalia, which was confirmed via /v1/species/1.
        let verified = [
            ("insecta", 216, "CLASS"),
            ("plantae", 6, "KINGDOM"),
            ("fungi", 5, "KINGDOM"),
            ("animalia", 1, "KINGDOM"),
            ("aves", 212, "CLASS"),
            ("lepidoptera", 797, "ORDER"),
            ("hymenoptera", 1457, "ORDER"),
            ("magnoliopsida", 220, "CLASS"),
        ];
        assert_eq!(
            GBIF_SCOPES.len(),
            verified.len(),
            "a scope was added or removed without recording its verification"
        );
        for (name, key, rank) in verified {
            let found = GBIF_SCOPES
                .iter()
                .find(|(n, _, _)| *n == name)
                .unwrap_or_else(|| panic!("scope `{name}` is missing"));
            assert_eq!(found.1, key, "`{name}` key drifted");
            assert_eq!(found.2, rank, "`{name}` rank drifted");
        }
        let mut names: Vec<&str> = GBIF_SCOPES.iter().map(|(n, _, _)| *n).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate scope name shadows another");
    }

    /// Scope names must be lowercase, since lookup lowercases the input.
    #[test]
    fn scope_names_are_stored_lowercase() {
        for (name, _, _) in GBIF_SCOPES {
            assert_eq!(
                *name,
                name.to_ascii_lowercase(),
                "`{name}` can never be matched"
            );
        }
    }
}

#[cfg(test)]
mod gbif_vernacular_tests {
    use super::*;
    use serde_json::json;

    /// Shape taken from the live `/species/search` response, trimmed.
    fn monarch() -> serde_json::Value {
        // Real proportions from GBIF on 2026-08-17: "Milkweed" is first in the
        // array, "Monarch" is listed by more independent sources.
        json!({ "vernacularNames": [
            { "vernacularName": "Milkweed",           "language": "eng" },
            { "vernacularName": "Milkweed Butterfly", "language": "eng" },
            { "vernacularName": "Milkweed",           "language": "eng" },
            { "vernacularName": "Monarch",            "language": "eng" },
            { "vernacularName": "Monarch",            "language": "eng" },
            { "vernacularName": "monarch",            "language": "eng" },
            { "vernacularName": "Monarque",           "language": "fra" },
            { "vernacularName": "Monarchfalter",      "language": "deu" }
        ]})
    }

    /// The reason this is frequency-ranked and not `[0]`.
    #[test]
    fn the_most_widely_listed_name_wins_not_the_first_one() {
        assert_eq!(
            gbif_preferred_vernacular(&monarch(), "eng").as_deref(),
            Some("Monarch"),
            "picked the first array entry instead of the best-attested name"
        );
    }

    /// Counting is case-insensitive, but the returned string keeps the casing
    /// GBIF used — a HUD line reading "monarch" looks like a typo.
    #[test]
    fn casing_is_normalised_for_counting_and_preserved_for_display() {
        let v = gbif_preferred_vernacular(&monarch(), "eng").unwrap();
        assert_eq!(v, "Monarch");
        assert!(v.starts_with('M'), "display casing was lost");
    }

    #[test]
    fn the_language_filter_is_respected() {
        assert_eq!(
            gbif_preferred_vernacular(&monarch(), "fra").as_deref(),
            Some("Monarque")
        );
        assert_eq!(
            gbif_preferred_vernacular(&monarch(), "deu").as_deref(),
            Some("Monarchfalter")
        );
        assert_eq!(gbif_preferred_vernacular(&monarch(), "swe"), None);
    }

    /// Obscure taxa genuinely have none — measured: `Clastoptera querci` and
    /// `Glyptotus cribratus` both return an empty array. `None` must mean "GBIF
    /// listed none", never an empty string that renders as a blank line.
    #[test]
    fn no_vernacular_name_is_none_rather_than_empty() {
        assert_eq!(
            gbif_preferred_vernacular(&json!({ "vernacularNames": [] }), "eng"),
            None
        );
        assert_eq!(gbif_preferred_vernacular(&json!({}), "eng"), None);
        assert_eq!(
            gbif_preferred_vernacular(&json!({ "vernacularNames": "not an array" }), "eng"),
            None
        );
        // Whitespace-only entries are an absence, not a name.
        assert_eq!(
            gbif_preferred_vernacular(
                &json!({ "vernacularNames": [{ "vernacularName": "   ", "language": "eng" }] }),
                "eng"
            ),
            None
        );
    }

    /// Same input, same answer — the property that lets the caller label this
    /// sourced rather than chosen.
    #[test]
    fn selection_is_deterministic_including_on_ties() {
        let tied = json!({ "vernacularNames": [
            { "vernacularName": "Beta",  "language": "eng" },
            { "vernacularName": "Alpha", "language": "eng" }
        ]});
        // One each: the earliest-seen variant wins, every time.
        for _ in 0..25 {
            assert_eq!(
                gbif_preferred_vernacular(&tied, "eng").as_deref(),
                Some("Beta")
            );
        }
        for _ in 0..25 {
            assert_eq!(
                gbif_preferred_vernacular(&monarch(), "eng").as_deref(),
                Some("Monarch")
            );
        }
    }

    /// A malformed entry must not take the lookup down or shift the winner.
    #[test]
    fn malformed_entries_are_skipped() {
        let messy = json!({ "vernacularNames": [
            { "language": "eng" },
            { "vernacularName": 42, "language": "eng" },
            "a bare string",
            { "vernacularName": "Chanterelle", "language": "eng" },
            { "vernacularName": "Chanterelle", "language": "eng" }
        ]});
        assert_eq!(
            gbif_preferred_vernacular(&messy, "eng").as_deref(),
            Some("Chanterelle")
        );
    }
}
