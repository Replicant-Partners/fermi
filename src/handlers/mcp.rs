//! Per-agent MCP (Model Context Protocol) endpoints.
//!
//! Two routes:
//!   GET  /mcp/agents/:agent_id   — manifest (tools/list shape)
//!   POST /mcp/agents/:agent_id   — JSON-RPC 2.0 dispatcher
//!
//! The dispatcher now routes named tool calls to the same ToolRegistry
//! that agents use internally, so every tool declared in an agent card's
//! `mcp_tools` array is callable directly by MCP clients (kask, Cursor,
//! Claude Desktop, etc.) without going through a freeform LLM execution.
//!
//! # Two distinct modes on one endpoint
//!
//! These are easy to conflate, and they behave very differently:
//!
//! - **`execute`** — runs the AGENT. Prose in, prose out, via the LLM
//!   executor with the agent's system prompt, memory, and full tool loop.
//!   Costs inference. Available on every agent with no configuration.
//! - **A published tool** (anything in `capabilities.mcp_tools`) — runs
//!   THAT TOOL directly through `ToolRegistry::execute`. No LLM, no system
//!   prompt, no agent loop; the agent record only supplies the allowlist,
//!   memory scoping, and the owner's credentials. This is ABW's
//!   deterministic compute surface, not its reasoning surface.
//!
//! Publishing a tool therefore does not "connect the endpoint to the
//! agent" — it deliberately bypasses the agent.
//!
//! Auth: the MCP endpoint sits on public_routes (optional auth middleware).
//! Discovery is public: the manifest, `initialize` and `tools/list`.
//! Anything that RUNS (`tools/call`, `execute`) needs a signed-in caller or
//! an API key, passes the rate limit and the credit gate, and is charged to
//! the caller's wallet: a published tool at `GAS_GROUND_TOOL_CALL`, `execute`
//! at the execution fee with the owner's royalty. It used to run either for
//! anyone, unmetered, which made it the free door around every metered one.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use fermi::agent_backend::{
    executor::AgentExecutor,
    tool_executor::ToolAwareExecutor,
    tools::{PlatformToolRegistry, ToolContext},
    ExecutionContext,
};
use fermi::ast;
use fermi_auth::AuthPrincipal;

use crate::{resolve_agent, resolve_agent_card, AppState};

// ─── Advertised tool list ───────────────────────────────────────────────

/// The synthetic tool that runs the agent itself. Not a platform builtin —
/// it exists only on this endpoint, and `ToolRegistry` has no arm for it;
/// the dispatcher intercepts the name and routes to the LLM executor.
const EXECUTE_TOOL: &str = "execute";

/// Tools advertised for an agent, in MCP `tools/list` shape.
///
/// Shared by the GET manifest and the POST `tools/list` so the two can't
/// drift. They previously carried duplicate copies of this logic.
///
/// # `execute` is always advertised
///
/// The old logic was either/or: `execute` appeared **only** when the agent
/// published nothing. So publishing a single tool silently removed the
/// discoverable "run this agent" capability — the dispatcher still accepted
/// `execute`, but no client could find out it existed. An operator ticking
/// one box in the Published Tools panel would unknowingly hide the agent's
/// primary purpose from Claude Desktop / Cursor / Zed.
///
/// `execute` is now always first. Published tools are additive: they expose
/// deterministic compute *alongside* the agent, never instead of it.
fn advertised_tools(
    card: &fermi::agent_backend::agent_card::AgentCard,
    agent_name: &str,
) -> Vec<Value> {
    let mut tools = vec![json!({
        "name": EXECUTE_TOOL,
        "description": format!(
            "Run the {agent_name} agent: send a natural-language query and receive its \
             reasoned response. Uses the agent's system prompt, memory, and full tool loop. \
             Other tools listed here run directly without invoking the agent."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "The query to execute" }
            },
            "required": ["query"]
        }
    })];

    // Defensive dedupe: `execute` is not a platform builtin, so
    // `invalid_tool_declarations` rejects it on write — but a card authored
    // before validation existed could still contain it, and a duplicate
    // name makes some MCP clients reject the whole manifest.
    tools.extend(
        card.capabilities
            .mcp_tools
            .iter()
            .filter(|t| t.name != EXECUTE_TOOL)
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema,
                })
            }),
    );

    tools
}

// ─── Manifest (GET) ──────────────────────────────────────────────────────────

/// The caller and wallet a running call is charged to, after the auth, rate
/// limit and credit gates. `Err` is the JSON-RPC error to return.
async fn paying_caller(
    state: &AppState,
    principal: &Option<AuthPrincipal>,
    min_balance: i32,
    rpc_id: &Value,
) -> Result<(String, fermi_auth::Wallet), Value> {
    use fermi::gate_trust::{decided, Decision, Gate};
    let Some(p) = principal else {
        return Err(mcp_error(
            rpc_id.clone(),
            -32001,
            "Running a tool or the agent requires an ABW account: send \
             Authorization: Bearer <api-key>. Discovery (tools/list) is public.",
        ));
    };
    let user_id = p.user_id();
    if let Err(retry) = state.rate_limits.llm.check(&format!("user:{user_id}")) {
        decided(Gate::RateLimit, Decision::Refused, Some("mcp"));
        return Err(mcp_error(
            rpc_id.clone(),
            -32029,
            &format!("Rate limit exceeded. Retry after {retry} seconds."),
        ));
    }
    decided(Gate::RateLimit, Decision::Approved, None);
    let wallet = fermi_auth::get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| mcp_error(rpc_id.clone(), -32000, &format!("Wallet error: {e}")))?;
    if wallet.balance < min_balance.max(1) {
        decided(
            Gate::Credit,
            Decision::Refused,
            Some("mcp: insufficient balance"),
        );
        return Err(mcp_error(
            rpc_id.clone(),
            -32002,
            "Insufficient credits. Top up your balance to continue.",
        ));
    }
    decided(Gate::Credit, Decision::Approved, None);
    Ok((user_id, wallet))
}

pub async fn mcp_agent_manifest(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let card = resolve_agent_card(&state, &db_agent);
    let tools = advertised_tools(&card, &db_agent.agent_name);

    Ok(Json(json!({
        "jsonrpc": "2.0",
        "serverInfo": {
            "name": format!("agent-bestiary:{}", agent_id),
            "version": db_agent.version,
        },
        "capabilities": { "tools": {} },
        "tools": tools,
        "agent": {
            "agent_id": db_agent.agent_name,
            "description": db_agent.description,
            "model": db_agent.model,
            "tags": db_agent.tags,
            "sample_queries": db_agent.sample_queries,
        }
    })))
}

// ─── RPC request shape ───────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct McpRpcRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

// ─── RPC dispatcher (POST) ───────────────────────────────────────────────────

pub async fn mcp_agent_rpc(
    State(state): State<AppState>,
    principal: Option<AuthPrincipal>,
    Path(agent_id): Path<String>,
    Json(req): Json<McpRpcRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let rpc_id = req.id.clone().unwrap_or(Value::Null);

    match req.method.as_str() {
        // ── Handshake ────────────────────────────────────────────────────────
        "initialize" => {
            let db_agent = resolve_agent(&state, &agent_id).await?;
            Ok(Json(json!({
                "jsonrpc": "2.0",
                "id": rpc_id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "serverInfo": {
                        "name": format!("agent-bestiary:{}", agent_id),
                        "version": db_agent.version,
                    },
                    "capabilities": { "tools": {} }
                }
            })))
        }

        // ── Tool catalogue ───────────────────────────────────────────────────
        "tools/list" => {
            let db_agent = resolve_agent(&state, &agent_id).await?;
            let card = resolve_agent_card(&state, &db_agent);

            let tools = advertised_tools(&card, &db_agent.agent_name);

            Ok(Json(json!({
                "jsonrpc": "2.0",
                "id": rpc_id,
                "result": { "tools": tools }
            })))
        }

        // ── Named tool call ──────────────────────────────────────────────────
        "tools/call" => {
            let params = req.params.unwrap_or(Value::Null);
            let tool_name = params
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or(Value::Object(serde_json::Map::new()));

            if tool_name.is_empty() {
                return Ok(Json(mcp_error(rpc_id, -32602, "Missing tool name")));
            }

            // If the tool is "execute" (or the card declares no tools),
            // fall through to the LLM execution path below.
            let db_agent = resolve_agent(&state, &agent_id).await?;
            let card = resolve_agent_card(&state, &db_agent);
            let declares_tool = card
                .capabilities
                .mcp_tools
                .iter()
                .any(|t| t.name == tool_name);

            if tool_name == "execute" || (!declares_tool && tool_name != "execute") {
                if tool_name != "execute" {
                    return Ok(Json(mcp_error(
                        rpc_id,
                        -32602,
                        &format!("Tool '{}' not declared by agent '{}'", tool_name, agent_id),
                    )));
                }
                // Fall through to LLM execution
                let query = arguments
                    .get("query")
                    .and_then(|q| q.as_str())
                    .unwrap_or("")
                    .to_string();
                if query.is_empty() {
                    return Ok(Json(mcp_error(
                        rpc_id,
                        -32602,
                        "Missing required parameter: query",
                    )));
                }
                return run_llm_execute(&state, &principal, &agent_id, &query, rpc_id).await;
            }

            let price = state.gas_fees.ground_tool_call;
            let (caller_id, wallet) = match paying_caller(&state, &principal, price, &rpc_id).await
            {
                Ok(c) => c,
                Err(e) => return Ok(Json(e)),
            };

            // ── Dispatch to ToolRegistry directly ────────────────────────────
            //
            // Build a ToolContext from whatever we have. Workspace-scoped tools
            // (list_workspace_agents, read_workspace_file, etc.) look for
            // workspace_id in both the tool arguments and the context; we pass
            // through what we can and let the tool surface a clear error when
            // something required is missing.
            let workspace_id = arguments
                .get("workspace_id")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<uuid::Uuid>().ok());

            let workspace_slug = arguments
                .get("workspace_slug")
                .and_then(|v| v.as_str())
                .map(String::from);

            // Resolve this agent's remote MCP tools so a *published*
            // remote tool is actually callable here. Without it, an agent
            // could declare `someserver__sometool` in its published list and
            // an external client would get `Unknown tool` — a phantom tool
            // introduced by the very feature meant to remove them.
            //
            // Uses the agent owner's secret scope (not the caller's), so an
            // external MCP client cannot borrow someone else's credential by
            // invoking their agent.
            let owner_secrets = crate::resolve_agent_owner_secrets(&state, &db_agent).await;
            let remote_mcp = if card.capabilities.mcp_servers.is_empty() {
                None
            } else {
                Some(Arc::new(
                    fermi::agent_backend::mcp_client::RemoteMcpCatalogue::discover(
                        &card.capabilities.mcp_servers,
                        owner_secrets.as_ref(),
                    )
                    .await,
                ))
            };

            let tool_ctx = Arc::new(ToolContext {
                // This path persists no episode of its own (mig-198), so there
                // is nothing for a child to point at: anything delegated from
                // here is recorded as a root. Its cost is still captured, just
                // not linked into a delegation tree.
                parent_episode_id: None,
                memory_store: state.memory_store.clone(),
                embedder: state.embedder.clone(),
                registry: state.registry.clone(),
                current_agent_id: Some(db_agent.agent_id),
                workspace_id,
                workspace_slug,
                workspace_git: Some(state.workspace_git.clone()),
                db: Some(state.db.clone()),
                gas_fees: Some(state.gas_fees.clone()),
                user_id: principal.as_ref().map(|p| p.user_id()),
                // Third-party TOOL credentials for the agent's owner — the
                // same resolution `/execute` performs, and it belongs here for
                // the same reason. This was `None`, which read as "this path
                // has no owner secrets" when what it meant was "nobody looked":
                // a published tool with a credential (`reduct_*`, `fmp_*`) then
                // fell through to the process env, so an owner-owned agent ran
                // a third-party call on the PLATFORM's key. That is the
                // cross-tenant leak SPEC_28 closed for LLM providers, on the
                // deterministic surface instead of the reasoning one — and
                // note the line below already resolves per-agent LLM
                // credentials correctly, so the two halves of one execution
                // disagreed about whose money it was.
                //
                // Returns `None` for `system` and `curated` tiers by design
                // (`is_platform_funded`), which is exactly when env IS the
                // right source.
                user_secrets: crate::resolve_agent_owner_secrets(&state, &db_agent).await,
                // SPEC_28 — funds any agent execution a dispatched tool
                // triggers (e.g. delegate_to_agent) from THIS agent's
                // owning principal, not the calling MCP client's.
                credentials: crate::build_execution_credentials(&state, &db_agent, &card).await,
                eval_trigger: Some(Arc::new(crate::handlers::eval::EvalTriggerImpl {
                    state: state.clone(),
                })),
                remote_mcp,
            });

            // with_workspace: a card that declares a workspace tool should
            // be able to serve it. `standard()` silently filtered those out.
            let registry = PlatformToolRegistry::all();
            match registry.execute(&tool_name, &arguments, &tool_ctx).await {
                Ok(result_str) => {
                    // Charged only when the tool answered, as on the
                    // grounding service: a failed call produced nothing.
                    if let Err((_, e)) = fermi::gas::charge_gas(
                        &state.db,
                        wallet.wallet_id,
                        price,
                        "gas_fee",
                        &format!("mcp tool call: {tool_name} on {agent_id}"),
                        None,
                    )
                    .await
                    {
                        tracing::error!(caller = %caller_id, tool = %tool_name, error = %e,
                            "mcp tool call ran and was not charged");
                    }
                    // Try to parse as JSON for a cleaner response; fall back to text
                    let content = serde_json::from_str::<Value>(&result_str)
                        .map(|v| json!({ "type": "json", "json": v }))
                        .unwrap_or_else(|_| json!({ "type": "text", "text": result_str }));

                    Ok(Json(json!({
                        "jsonrpc": "2.0",
                        "id": rpc_id,
                        "result": { "content": [content] }
                    })))
                }
                Err(e) => Ok(Json(mcp_error(
                    rpc_id,
                    -32000,
                    &format!("Tool error: {}", e),
                ))),
            }
        }

        // ── Legacy direct-execute shorthand (some MCP clients use this) ──────
        "execute" => {
            let params = req.params.unwrap_or(Value::Null);
            let query = params
                .get("query")
                .and_then(|q| q.as_str())
                .unwrap_or("")
                .to_string();
            if query.is_empty() {
                return Ok(Json(mcp_error(
                    rpc_id,
                    -32602,
                    "Missing required parameter: query",
                )));
            }
            run_llm_execute(&state, &principal, &agent_id, &query, rpc_id).await
        }

        _ => Ok(Json(mcp_error(
            rpc_id,
            -32601,
            &format!("Unknown method: {}", req.method),
        ))),
    }
}

// ─── LLM execution path (tool_name == "execute") ─────────────────────────────

async fn run_llm_execute(
    state: &AppState,
    principal: &Option<AuthPrincipal>,
    agent_id: &str,
    query: &str,
    rpc_id: Value,
) -> Result<Json<Value>, (StatusCode, String)> {
    let (caller_id, wallet) =
        match paying_caller(state, principal, state.gas_fees.execution_min, &rpc_id).await {
            Ok(c) => c,
            Err(e) => return Ok(Json(e)),
        };
    let db_agent = resolve_agent(state, agent_id).await?;
    let card = resolve_agent_card(state, &db_agent);

    let agent_stmt = ast::AgentStmt {
        name: agent_id.to_string(),
        agent_type: Some(card.agent_type.clone()),
        query: query.to_string(),
        executor: Some(ast::ExecutorType::LLM),
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    };

    let program = ast::Program {
        statements: vec![ast::Statement::Agent(agent_stmt.clone())],
    };

    // SPEC_28 — an external MCP client invoking this agent must not be
    // able to borrow the platform's key. Funding follows the agent's owner.
    let credentials = crate::build_execution_credentials(&state, &db_agent, &card).await;

    let context = ExecutionContext {
        program,
        agent_card: card.clone(),
        creature_id: None,
        cognition_tier: None,
        credentials: credentials.clone(),
        registry: None,
        // Text-only path: this caller carries no image. Stated rather than
        // defaulted, so a path that should carry one cannot acquire the field
        // silently.
        attachments: Vec::new(),
    };

    let tool_ctx = Arc::new(ToolContext {
        // As above: no episode is written on this path, so delegated children
        // are recorded as roots rather than linked to a (nonexistent) parent.
        parent_episode_id: None,
        credentials,
        memory_store: state.memory_store.clone(),
        embedder: state.embedder.clone(),
        registry: state.registry.clone(),
        current_agent_id: Some(db_agent.agent_id),
        workspace_id: None,
        workspace_slug: None,
        workspace_git: Some(state.workspace_git.clone()),
        db: Some(state.db.clone()),
        gas_fees: Some(state.gas_fees.clone()),
        user_id: principal.as_ref().map(|p| p.user_id()),
        user_secrets: None,
        eval_trigger: Some(Arc::new(crate::handlers::eval::EvalTriggerImpl {
            state: state.clone(),
        })),
        remote_mcp: None,
    });

    let tool_executor = ToolAwareExecutor::new(
        state.registry.executor_arc(),
        PlatformToolRegistry::standard(),
        tool_ctx,
    );

    match tool_executor.execute(&agent_stmt, &context).await {
        Ok(output) => {
            // Charged like `/execute`: the execution fee, with the owner's
            // royalty, plus the gas surcharge.
            let tokens = output.tokens_used.unwrap_or(0) as i32;
            let (execution_fee, gas_fee) = state.gas_fees.execution_fee(tokens);
            let _ = fermi::gas::charge_execution_with_royalty(
                &state.db,
                wallet.wallet_id,
                &caller_id,
                execution_fee,
                db_agent.owner_id.as_deref(),
                &db_agent.tier,
                &db_agent.agent_name,
                tokens,
                None,
                state.gas_fees.execution_owner_royalty_pct,
            )
            .await;
            let _ = fermi::gas::charge_gas(
                &state.db,
                wallet.wallet_id,
                gas_fee,
                "gas_fee",
                &format!("mcp execute gas fee for {}", db_agent.agent_name),
                None,
            )
            .await;
            let result_json = json!({
                "status": format!("{:?}", output.status),
                "confidence": output.confidence,
                "execution_time_ms": output.execution_time_ms,
                "tokens_used": output.tokens_used,
                "evidence": output.evidence.iter().map(|e| json!({
                    "source": e.source,
                    "summary": e.summary,
                    "key_findings": e.key_findings,
                    "relevance": e.relevance,
                })).collect::<Vec<_>>(),
                "reasoning": output.metadata.reasoning,
            });
            Ok(Json(json!({
                "jsonrpc": "2.0",
                "id": rpc_id,
                "result": {
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result_json).unwrap_or_default()
                    }]
                }
            })))
        }
        Err(e) => Ok(Json(mcp_error(
            rpc_id,
            -32000,
            &format!("Execution error: {}", e),
        ))),
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn mcp_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

// ─── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use fermi::agent_backend::agent_card::{AgentCard, McpTool};

    fn card_with(tool_names: &[&str]) -> AgentCard {
        let mut card = AgentCard::new("t".into(), "research".into());
        card.capabilities.mcp_tools = tool_names
            .iter()
            .map(|n| McpTool {
                name: (*n).to_string(),
                description: format!("desc for {n}"),
                input_schema: Some(json!({ "type": "object" })),
            })
            .collect();
        card
    }

    fn names(tools: &[Value]) -> Vec<String> {
        tools
            .iter()
            .filter_map(|t| t["name"].as_str().map(String::from))
            .collect()
    }

    /// An agent publishing nothing is still usable: `execute` is advertised.
    #[test]
    fn execute_is_advertised_when_nothing_is_published() {
        let tools = advertised_tools(&card_with(&[]), "my_agent");
        assert_eq!(names(&tools), vec![EXECUTE_TOOL]);
        assert!(tools[0]["inputSchema"]["properties"]["query"].is_object());
    }

    /// The regression this test exists for: the old either/or logic dropped
    /// `execute` from the manifest as soon as anything was published, so
    /// ticking one box in the Published Tools panel silently hid the agent's
    /// primary capability from every MCP client.
    #[test]
    fn execute_survives_publishing_and_stays_first() {
        let tools = advertised_tools(&card_with(&["run_monte_carlo", "h3_resolve"]), "my_agent");
        assert_eq!(
            names(&tools),
            vec![EXECUTE_TOOL, "run_monte_carlo", "h3_resolve"],
            "published tools must be additive to `execute`, not a replacement"
        );
    }

    /// A duplicate name can make an MCP client reject the whole manifest.
    /// `execute` is not a platform builtin so validation rejects it on write,
    /// but cards authored before validation existed could still carry it.
    #[test]
    fn execute_is_not_duplicated_if_a_card_declares_it() {
        let tools = advertised_tools(&card_with(&["execute", "h3_resolve"]), "my_agent");
        assert_eq!(names(&tools), vec![EXECUTE_TOOL, "h3_resolve"]);
    }

    /// Published entries pass through their own description and schema — the
    /// manifest must not substitute anything of its own.
    #[test]
    fn published_tools_keep_their_schema_and_description() {
        let tools = advertised_tools(&card_with(&["h3_resolve"]), "my_agent");
        let published = &tools[1];
        assert_eq!(published["name"], "h3_resolve");
        assert_eq!(published["description"], "desc for h3_resolve");
        assert_eq!(published["inputSchema"]["type"], "object");
    }
}
