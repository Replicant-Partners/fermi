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
//! Auth: the MCP endpoint sits on public_routes (optional auth middleware).
//! Workspace-scoped tools (read_workspace_file, list_workspace_agents, etc.)
//! require a workspace_id in params and a valid Bearer token / API key.
//! Public tools (list_agents, execute_agent on public agents) work unauthenticated.

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
    tools::{ToolContext, ToolRegistry},
    ExecutionContext,
};
use fermi::ast;
use fermi_auth::AuthPrincipal;

use crate::{resolve_agent, resolve_agent_card, AppState};

// ─── Manifest (GET) ──────────────────────────────────────────────────────────

pub async fn mcp_agent_manifest(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let card = resolve_agent_card(&state, &db_agent);

    // Build the tools list from the card's declared mcp_tools.
    // Fall back to the generic `execute` tool for agents that declare none.
    let tools: Vec<Value> = if card.capabilities.mcp_tools.is_empty() {
        vec![json!({
            "name": "execute",
            "description": format!("Execute the {} agent with a query", agent_id),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "The query to execute" }
                },
                "required": ["query"]
            }
        })]
    } else {
        // Each mcp_tool already has name + description + input_schema from the card
        card.capabilities
            .mcp_tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema,
                })
            })
            .collect()
    };

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

            let tools: Vec<Value> = if card.capabilities.mcp_tools.is_empty() {
                vec![json!({
                    "name": "execute",
                    "description": format!("Execute the {} agent", db_agent.agent_name),
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string", "description": "The research query" }
                        },
                        "required": ["query"]
                    }
                })]
            } else {
                card.capabilities
                    .mcp_tools
                    .iter()
                    .map(|t| {
                        json!({
                            "name": t.name,
                            "description": t.description,
                            "inputSchema": t.input_schema,
                        })
                    })
                    .collect()
            };

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
                user_secrets: None,
                eval_trigger: Some(Arc::new(crate::handlers::eval::EvalTriggerImpl {
                    state: state.clone(),
                })),
                remote_mcp,
            });

            // with_workspace: a card that declares a workspace tool should
            // be able to serve it. `standard()` silently filtered those out.
            let registry = ToolRegistry::with_workspace();
            match registry.execute(&tool_name, &arguments, &tool_ctx).await {
                Ok(result_str) => {
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

    let context = ExecutionContext {
        program,
        agent_card: card.clone(),
        creature_id: None,
        cognition_tier: None,
    };

    let tool_ctx = Arc::new(ToolContext {
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
        ToolRegistry::standard(),
        tool_ctx,
    );

    match tool_executor.execute(&agent_stmt, &context).await {
        Ok(output) => {
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
