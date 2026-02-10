//! Per-agent MCP (Model Context Protocol) endpoints.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use fermi::agent_backend::executor::AgentExecutor;
use fermi::agent_backend::tool_executor::ToolAwareExecutor;
use fermi::agent_backend::tools::{ToolContext, ToolRegistry};
use fermi::agent_backend::ExecutionContext;
use fermi::ast;

use crate::{resolve_agent, resolve_agent_card, AppState};
// ─── Per-Agent MCP Endpoints ────────────────────────────────────────

pub async fn mcp_agent_manifest(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    Ok(Json(json!({
        "jsonrpc": "2.0",
        "serverInfo": {
            "name": format!("agent-bestiary:{}", agent_id),
            "version": db_agent.version,
        },
        "capabilities": {
            "tools": {}
        },
        "tools": [{
            "name": "execute",
            "description": format!("Execute the {} agent with a research query", agent_id),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The research query to execute"
                    }
                },
                "required": ["query"]
            }
        }],
        "agent": {
            "agent_id": db_agent.agent_name,
            "description": db_agent.description,
            "model": db_agent.model,
            "tags": db_agent.tags,
            "sample_queries": db_agent.sample_queries,
        }
    })))
}

#[derive(Deserialize)]
pub struct McpRpcRequest {
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

pub async fn mcp_agent_rpc(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Json(req): Json<McpRpcRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let rpc_id = req.id.clone().unwrap_or(Value::Null);

    match req.method.as_str() {
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
        "tools/list" => {
            let db_agent = resolve_agent(&state, &agent_id).await?;
            Ok(Json(json!({
                "jsonrpc": "2.0",
                "id": rpc_id,
                "result": {
                    "tools": [{
                        "name": "execute",
                        "description": format!("Execute the {} agent", db_agent.agent_name),
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": { "type": "string", "description": "The research query" }
                            },
                            "required": ["query"]
                        }
                    }]
                }
            })))
        }
        "tools/call" => {
            let params = req.params.unwrap_or(Value::Null);
            let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            if tool_name != "execute" {
                return Ok(Json(json!({
                    "jsonrpc": "2.0",
                    "id": rpc_id,
                    "error": { "code": -32602, "message": format!("Unknown tool: {}", tool_name) }
                })));
            }

            let query = params
                .get("arguments")
                .and_then(|a| a.get("query"))
                .and_then(|q| q.as_str())
                .unwrap_or("")
                .to_string();

            if query.is_empty() {
                return Ok(Json(json!({
                    "jsonrpc": "2.0",
                    "id": rpc_id,
                    "error": { "code": -32602, "message": "Missing required parameter: query" }
                })));
            }

            // Execute agent (unauthenticated for public agents)
            let db_agent = resolve_agent(&state, &agent_id).await?;
            let card = resolve_agent_card(&state, &db_agent);

            let agent_stmt = ast::AgentStmt {
                name: agent_id.clone(),
                agent_type: Some(card.agent_type.clone()),
                query: query.clone(),
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
            };

            match state.registry.execute_agent(&agent_stmt, &context).await {
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
                Err(e) => Ok(Json(json!({
                    "jsonrpc": "2.0",
                    "id": rpc_id,
                    "error": { "code": -32000, "message": format!("Execution error: {}", e) }
                }))),
            }
        }
        _ => Ok(Json(json!({
            "jsonrpc": "2.0",
            "id": rpc_id,
            "error": { "code": -32601, "message": format!("Unknown method: {}", req.method) }
        }))),
    }
}
