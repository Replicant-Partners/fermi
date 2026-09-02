// src/agent_backend/tools/domains/workspace.rs
//
// Phase 4 domain migration: Workspace tools.
//
// Eight tools:
//   read_workspace_file       — requires_workspace: true
//   read_workspace_output     — requires_workspace: false
//   list_workspace_outputs    — requires_workspace: false
//   list_workspace_agents     — requires_workspace: true
//   write_workspace_file      — requires_workspace: true
//   evaluate_coherence        — requires_workspace: true
//   coherence_snapshot        — requires_workspace: true
//   get_workspace_messages    — requires_workspace: true
//
// Each is a zero-size struct implementing PlatformTool. execute() calls
// a private function defined in this module.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use agent_bestiary_memory::types::CoherenceEvaluation;
use coherence_core::types::{ConversationId, Message as CoherenceMessage, ParticipantId};
use coherence_engine::SettlingEngine;
use coherence_observer::ConversationObserver;

use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;

/// All Workspace-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![
        Arc::new(ReadWorkspaceFile),
        Arc::new(ReadWorkspaceOutput),
        Arc::new(ListWorkspaceOutputs),
        Arc::new(ListWorkspaceAgents),
        Arc::new(WriteWorkspaceFile),
        Arc::new(EvaluateCoherence),
        Arc::new(CoherenceSnapshot),
        Arc::new(GetWorkspaceMessages),
    ]
}

// ─── read_workspace_file ──────────────────────────────────────────────────────

struct ReadWorkspaceFile;

#[async_trait]
impl PlatformTool for ReadWorkspaceFile {
    fn name(&self) -> &'static str {
        "read_workspace_file"
    }

    fn description(&self) -> &'static str {
        "Read a file from the current workspace's git repository."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The file path relative to workspace root"
                }
            },
            "required": ["path"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_read_workspace_file(input, ctx).await
    }
}

// ─── read_workspace_output ────────────────────────────────────────────────────

struct ReadWorkspaceOutput;

#[async_trait]
impl PlatformTool for ReadWorkspaceOutput {
    fn name(&self) -> &'static str {
        "read_workspace_output"
    }

    fn description(&self) -> &'static str {
        "Read a typed output from any workspace. Use this to consume results published by upstream workspaces (e.g., team prior → tournament path). Returns the output value, version, and last update time."
    }

    fn input_schema(&self) -> Value {
        json!({
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
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_read_workspace_output(input, ctx).await
    }
}

// ─── list_workspace_outputs ───────────────────────────────────────────────────

struct ListWorkspaceOutputs;

#[async_trait]
impl PlatformTool for ListWorkspaceOutputs {
    fn name(&self) -> &'static str {
        "list_workspace_outputs"
    }

    fn description(&self) -> &'static str {
        "List all published outputs for a workspace. Returns keys, values, versions, and update times."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "workspace_id": {
                    "type": "string",
                    "description": "UUID of the workspace to list outputs from"
                }
            },
            "required": ["workspace_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_list_workspace_outputs(input, ctx).await
    }
}

// ─── list_workspace_agents ────────────────────────────────────────────────────

struct ListWorkspaceAgents;

#[async_trait]
impl PlatformTool for ListWorkspaceAgents {
    fn name(&self) -> &'static str {
        "list_workspace_agents"
    }

    fn description(&self) -> &'static str {
        "List all agents that are members of the current workspace."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, _input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_list_workspace_agents(ctx).await
    }
}

// ─── write_workspace_file ─────────────────────────────────────────────────────

struct WriteWorkspaceFile;

#[async_trait]
impl PlatformTool for WriteWorkspaceFile {
    fn name(&self) -> &'static str {
        "write_workspace_file"
    }

    fn description(&self) -> &'static str {
        "Write a file to the current workspace's git repository. For binary files (images), provide base64-encoded content and set is_base64 to true."
    }

    fn input_schema(&self) -> Value {
        json!({
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
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_write_workspace_file(input, ctx).await
    }
}

// ─── evaluate_coherence ───────────────────────────────────────────────────────

struct EvaluateCoherence;

#[async_trait]
impl PlatformTool for EvaluateCoherence {
    fn name(&self) -> &'static str {
        "evaluate_coherence"
    }

    fn description(&self) -> &'static str {
        "Run a Thagard Explanatory Coherence (TEC) evaluation on recent workspace messages. Classifies utterances, detects coherence/incoherence relations, runs constraint-satisfaction settling, and returns global score, 7 principle scores, and health indicators. Results are stored for history."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message_limit": {
                    "type": "integer",
                    "description": "Number of recent messages to evaluate (default: 50, max: 100)",
                    "default": 50
                }
            },
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_evaluate_coherence(input, ctx).await
    }
}

// ─── coherence_snapshot ───────────────────────────────────────────────────────

struct CoherenceSnapshot;

#[async_trait]
impl PlatformTool for CoherenceSnapshot {
    fn name(&self) -> &'static str {
        "coherence_snapshot"
    }

    fn description(&self) -> &'static str {
        "Get the latest stored coherence evaluation for the workspace without running a new evaluation. Returns global score, quality label, principle scores, and health indicators from the most recent evaluation."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, _input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_coherence_snapshot(ctx).await
    }
}

// ─── get_workspace_messages ───────────────────────────────────────────────────

struct GetWorkspaceMessages;

#[async_trait]
impl PlatformTool for GetWorkspaceMessages {
    fn name(&self) -> &'static str {
        "get_workspace_messages"
    }

    fn description(&self) -> &'static str {
        "Read recent messages from the workspace conversation. Returns messages with sender name, content, type, and timestamp."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of messages to return (default: 20, max: 50)",
                    "default": 20
                }
            },
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_get_workspace_messages(input, ctx).await
    }
}

// ─── Private execute functions ────────────────────────────────────────────────

async fn execute_read_workspace_file(input: &Value, ctx: &ToolContext) -> Result<String, String> {
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
async fn execute_read_workspace_output(input: &Value, ctx: &ToolContext) -> Result<String, String> {
    let workspace_id = input
        .get("workspace_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: workspace_id")?;
    let key = input
        .get("key")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: key")?;

    let ws_uuid: Uuid = workspace_id
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
    input: &Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = input
        .get("workspace_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: workspace_id")?;

    let ws_uuid: Uuid = workspace_id
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
    // Returns typed capability info so strategist agents (moe_router, pipeline,
    // cohere_and_coordinate) can route on schema IDs rather than description
    // heuristics. `accepts` and `produces` are the schema ID arrays; the
    // `*_schema_id` fields are the canonical type names from the compiled
    // output_contract and input_contract, ready for schema-ID matching.
    let rows = sqlx::query(
        "SELECT a.agent_name,
                a.agent_type,
                a.description,
                a.accepts,
                a.produces,
                a.output_contract->>'produces_schema' AS output_schema_id,
                a.output_contract->'input_contract'->>'accepts_schema' AS input_schema_id
         FROM workspace_agents wa
         JOIN agents a ON wa.agent_id = a.id
         WHERE wa.workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Query failed: {}", e))?;

    let agents: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name":             row.get::<String, _>("agent_name"),
                "type":             row.get::<String, _>("agent_type"),
                "description":      row.get::<Option<String>, _>("description"),
                // Typed interface — use these for routing decisions, not description text.
                "accepts":          row.get::<Vec<String>, _>("accepts"),
                "produces":         row.get::<Vec<String>, _>("produces"),
                "input_schema_id":  row.get::<Option<String>, _>("input_schema_id"),
                "output_schema_id": row.get::<Option<String>, _>("output_schema_id"),
            })
        })
        .collect();

    serde_json::to_string_pretty(&agents).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_write_workspace_file(input: &Value, ctx: &ToolContext) -> Result<String, String> {
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

async fn execute_evaluate_coherence(input: &Value, ctx: &ToolContext) -> Result<String, String> {
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
    input: &Value,
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

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_names_are_dispatchable() {
        for tool in tools() {
            assert!(!tool.name().is_empty(), "tool has empty name");
        }
    }

    #[test]
    fn all_categories_are_workspace() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Workspace,
                "tool `{}` has wrong category",
                tool.name()
            );
        }
    }

    #[test]
    fn input_schemas_are_objects() {
        for tool in tools() {
            let schema = tool.input_schema();
            assert_eq!(
                schema["type"],
                "object",
                "tool `{}` input_schema missing \"type\": \"object\"",
                tool.name()
            );
        }
    }

    #[test]
    fn tool_count_is_eight() {
        assert_eq!(tools().len(), 8);
    }

    #[test]
    fn workspace_flags_are_correct() {
        let tools = tools();
        let requires: Vec<(&str, bool)> = tools
            .iter()
            .map(|t| (t.name(), t.requires_workspace()))
            .collect();

        // false: read_workspace_output, list_workspace_outputs
        for (name, flag) in &requires {
            match *name {
                "read_workspace_output" | "list_workspace_outputs" => {
                    assert!(!flag, "tool `{}` should NOT require workspace", name);
                }
                _ => {
                    assert!(flag, "tool `{}` should require workspace", name);
                }
            }
        }
    }
}
