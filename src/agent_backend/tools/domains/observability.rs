// src/agent_backend/tools/domains/observability.rs
//
// Phase 4 domain migration: Observability tools.
//
// Ten tools (all requires_workspace: false):
//   get_agent_calibration
//   query_eval_signals
//   query_eval_runs
//   query_anomalies
//   query_hitl_queue
//   query_timeline
//   query_dyad_state
//   classify_anomaly
//   route_to_hitl
//   run_evaluator_registry
//
// Each is a zero-size struct implementing PlatformTool. execute() bodies are
// inlined verbatim from tools_legacy.rs — no dispatch through ToolRegistry.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent_backend::tools::helpers::{parse_uuid_field, resolve_agent_id};
use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;

/// All Observability-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![
        Arc::new(GetAgentCalibration),
        Arc::new(QueryEvalSignals),
        Arc::new(QueryEvalRuns),
        Arc::new(QueryAnomalies),
        Arc::new(QueryHitlQueue),
        Arc::new(QueryTimeline),
        Arc::new(QueryDyadState),
        Arc::new(ClassifyAnomaly),
        Arc::new(RouteToHitl),
        Arc::new(RunEvaluatorRegistry),
    ]
}

// ─── Execute implementations ─────────────────────────────────────────────────

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

// ─── get_agent_calibration ────────────────────────────────────────────────────

struct GetAgentCalibration;

#[async_trait]
impl PlatformTool for GetAgentCalibration {
    fn name(&self) -> &'static str {
        "get_agent_calibration"
    }

    fn description(&self) -> &'static str {
        "Compute calibration statistics for an agent — Brier score, ECE, overconfidence rate, and related metrics — across its completed eval runs."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "UUID or name of the agent to compute calibration for."
                }
            },
            "required": ["agent_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Observability
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_get_agent_calibration(input, ctx).await
    }
}

// ─── query_eval_signals ───────────────────────────────────────────────────────

struct QueryEvalSignals;

#[async_trait]
impl PlatformTool for QueryEvalSignals {
    fn name(&self) -> &'static str {
        "query_eval_signals"
    }

    fn description(&self) -> &'static str {
        "List all eval signals for a given eval run."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "run_id": {
                    "type": "string",
                    "description": "UUID of the eval run to fetch signals for."
                }
            },
            "required": ["run_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Observability
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_query_eval_signals(input, ctx).await
    }
}

// ─── query_eval_runs ──────────────────────────────────────────────────────────

struct QueryEvalRuns;

#[async_trait]
impl PlatformTool for QueryEvalRuns {
    fn name(&self) -> &'static str {
        "query_eval_runs"
    }

    fn description(&self) -> &'static str {
        "List recent eval runs for an agent."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "UUID or name of the agent."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of runs to return (default: 20, max: 100).",
                    "default": 20
                }
            },
            "required": ["agent_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Observability
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_query_eval_runs(input, ctx).await
    }
}

// ─── query_anomalies ──────────────────────────────────────────────────────────

struct QueryAnomalies;

#[async_trait]
impl PlatformTool for QueryAnomalies {
    fn name(&self) -> &'static str {
        "query_anomalies"
    }

    fn description(&self) -> &'static str {
        "List anomaly events for an agent."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "UUID or name of the agent."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of anomalies to return (default: 50, max: 500).",
                    "default": 50
                }
            },
            "required": ["agent_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Observability
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_query_anomalies(input, ctx).await
    }
}

// ─── query_hitl_queue ─────────────────────────────────────────────────────────

struct QueryHitlQueue;

#[async_trait]
impl PlatformTool for QueryHitlQueue {
    fn name(&self) -> &'static str {
        "query_hitl_queue"
    }

    fn description(&self) -> &'static str {
        "List pending anomaly events that require human-in-the-loop review."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of items to return (default: 50, max: 200).",
                    "default": 50
                }
            },
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Observability
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_query_hitl_queue(input, ctx).await
    }
}

// ─── query_timeline ───────────────────────────────────────────────────────────

struct QueryTimeline;

#[async_trait]
impl PlatformTool for QueryTimeline {
    fn name(&self) -> &'static str {
        "query_timeline"
    }

    fn description(&self) -> &'static str {
        "List timeline entries for an agent."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "UUID or name of the agent."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of entries to return (default: 100, max: 500).",
                    "default": 100
                }
            },
            "required": ["agent_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Observability
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_query_timeline(input, ctx).await
    }
}

// ─── query_dyad_state ─────────────────────────────────────────────────────────

struct QueryDyadState;

#[async_trait]
impl PlatformTool for QueryDyadState {
    fn name(&self) -> &'static str {
        "query_dyad_state"
    }

    fn description(&self) -> &'static str {
        "List dyad relationship states for an agent."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "UUID or name of the agent."
                }
            },
            "required": ["agent_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Observability
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_query_dyad_state(input, ctx).await
    }
}

// ─── classify_anomaly ─────────────────────────────────────────────────────────

struct ClassifyAnomaly;

#[async_trait]
impl PlatformTool for ClassifyAnomaly {
    fn name(&self) -> &'static str {
        "classify_anomaly"
    }

    fn description(&self) -> &'static str {
        "Retrieve a full anomaly event with related eval signals, agent persona version, and prior HITL actions. Use this to gather context before calling route_to_hitl."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "anomaly_id": {
                    "type": "string",
                    "description": "UUID of the anomaly event."
                }
            },
            "required": ["anomaly_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Observability
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_classify_anomaly(input, ctx).await
    }
}

// ─── route_to_hitl ────────────────────────────────────────────────────────────

struct RouteToHitl;

#[async_trait]
impl PlatformTool for RouteToHitl {
    fn name(&self) -> &'static str {
        "route_to_hitl"
    }

    fn description(&self) -> &'static str {
        "Flag an anomaly event for human review with a recommended action and justification. Sets requires_review=true on the anomaly and stores your recommendation. The event must not already be resolved."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "anomaly_id": {
                    "type": "string",
                    "description": "UUID of the anomaly event to route."
                },
                "recommended_action": {
                    "type": "string",
                    "description": "Your recommended resolution: approve | relabel | intervene",
                    "enum": ["approve", "relabel", "intervene"]
                },
                "scope": {
                    "type": "string",
                    "description": "Scope of the recommendation: episode | agent | agent_wide",
                    "enum": ["episode", "agent", "agent_wide"],
                    "default": "episode"
                },
                "justification": {
                    "type": "string",
                    "description": "Why you are routing this to a human reviewer."
                }
            },
            "required": ["anomaly_id", "recommended_action", "justification"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Observability
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_route_to_hitl(input, ctx).await
    }
}

// ─── run_evaluator_registry ───────────────────────────────────────────────────

struct RunEvaluatorRegistry;

#[async_trait]
impl PlatformTool for RunEvaluatorRegistry {
    fn name(&self) -> &'static str {
        "run_evaluator_registry"
    }

    fn description(&self) -> &'static str {
        "Trigger an eval run for an agent via the evaluator registry. Requires eval_trigger to be plumbed into the ToolContext (available in the agent detail page's Run + Judge flow)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "UUID or name of the agent to evaluate."
                },
                "judge": {
                    "type": "boolean",
                    "description": "Whether to run the judge step (default: true).",
                    "default": true
                },
                "tags": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Optional tags to attach to the eval run."
                }
            },
            "required": ["agent_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Observability
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_run_evaluator_registry(input, ctx).await
    }
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
    fn all_categories_are_observability() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Observability,
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
    fn tool_count_is_ten() {
        assert_eq!(tools().len(), 10);
    }

    #[test]
    fn none_require_workspace() {
        for tool in tools() {
            assert!(
                !tool.requires_workspace(),
                "tool `{}` should NOT require workspace",
                tool.name()
            );
        }
    }
}
