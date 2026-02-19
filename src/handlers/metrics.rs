//! Metrics and observability handlers — episode detail, platform metrics, agent metrics.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use sqlx::Row;

use crate::{resolve_agent, AppState};
// ─── Observability APIs (Sprint N) ─────────────────────────────────

pub async fn get_episode_detail_handler(
    State(state): State<AppState>,
    Path(episode_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ep_uuid: uuid::Uuid = episode_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid episode ID".to_string()))?;

    let episode = state
        .memory_store
        .get_episode(ep_uuid)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Episode not found: {}", e)))?;

    let context = &episode.context;
    let tool_invocations = context
        .get("tool_invocations")
        .cloned()
        .unwrap_or(json!([]));
    let loop_iterations = context
        .get("loop_iterations")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    let reasoning = context
        .get("reasoning")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let model_used = context
        .get("model_used")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let evidence = context.get("evidence").cloned().unwrap_or(json!([]));

    // Compute timing breakdown
    let total_ms = episode.execution_time_ms;
    let tool_ms: i64 = context
        .get("tool_invocations")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.get("duration_ms").and_then(|d| d.as_i64()))
                .sum()
        })
        .unwrap_or(0);
    let llm_ms = (total_ms - tool_ms).max(0);

    Ok(Json(json!({
        "episode_id": episode.episode_id,
        "agent_id": episode.agent_id,
        "timestamp": episode.timestamp_ref,
        "query": episode.query,
        "status": episode.execution_status.to_string(),
        "error_details": episode.error_details,
        "execution_time_ms": total_ms,
        "tokens_used": episode.tokens_used,
        "cost_usd": episode.cost_usd,
        "consolidated": episode.consolidated,
        "loop_iterations": loop_iterations,
        "tool_invocations": tool_invocations,
        "evidence": evidence,
        "reasoning": reasoning,
        "model_used": model_used,
        "tags": episode.tags,
        "timing": {
            "total_ms": total_ms,
            "tool_ms": tool_ms,
            "llm_ms": llm_ms,
        }
    })))
}

pub async fn platform_metrics_handler(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let pool = &state.db;

    // 1. Totals
    let totals_row = sqlx::query(
        "SELECT
            COUNT(*) AS total,
            COUNT(*) FILTER (WHERE execution_status = 'success') AS successful,
            COUNT(*) FILTER (WHERE execution_status = 'failure') AS failed,
            COALESCE(SUM(tokens_used), 0) AS total_tokens,
            COALESCE(SUM(cost_usd), 0)::TEXT AS total_cost,
            COUNT(DISTINCT agent_id) AS active_agents
         FROM episodes",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total: i64 = totals_row.get("total");
    let successful: i64 = totals_row.get("successful");
    let failed: i64 = totals_row.get("failed");
    let total_tokens: i64 = totals_row.get("total_tokens");
    let total_cost: String = totals_row.get("total_cost");
    let active_agents: i64 = totals_row.get("active_agents");
    let success_rate = if total > 0 {
        successful as f64 / total as f64
    } else {
        0.0
    };

    // 2. Daily (30 days)
    let daily_rows = sqlx::query(
        "SELECT
            DATE(timestamp_ref) AS day,
            COUNT(*) AS executions,
            COUNT(*) FILTER (WHERE execution_status = 'failure') AS failures,
            COALESCE(SUM(tokens_used), 0) AS tokens
         FROM episodes
         WHERE timestamp_ref > NOW() - INTERVAL '30 days'
         GROUP BY DATE(timestamp_ref)
         ORDER BY day",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let daily: Vec<Value> = daily_rows
        .iter()
        .map(|r| {
            let day: chrono::NaiveDate = r.get("day");
            json!({
                "date": day.to_string(),
                "executions": r.get::<i64, _>("executions"),
                "failures": r.get::<i64, _>("failures"),
                "tokens": r.get::<i64, _>("tokens"),
            })
        })
        .collect();

    // 3. Tool usage
    let tool_rows = sqlx::query(
        "SELECT
            tool->>'tool_name' AS tool_name,
            COUNT(*) AS count
         FROM episodes,
              jsonb_array_elements(context->'tool_invocations') AS tool
         WHERE context->'tool_invocations' IS NOT NULL
           AND jsonb_array_length(context->'tool_invocations') > 0
         GROUP BY tool->>'tool_name'
         ORDER BY count DESC
         LIMIT 10",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let tool_usage: Vec<Value> = tool_rows
        .iter()
        .map(|r| {
            json!({
                "tool_name": r.get::<String, _>("tool_name"),
                "count": r.get::<i64, _>("count"),
            })
        })
        .collect();

    // 4. Top agents
    let top_rows = sqlx::query(
        "SELECT
            a.agent_name,
            COUNT(*) AS executions,
            AVG(e.execution_time_ms)::BIGINT AS avg_time_ms
         FROM episodes e
         JOIN agents a ON a.agent_id = e.agent_id
         GROUP BY a.agent_name
         ORDER BY executions DESC
         LIMIT 10",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let top_agents: Vec<Value> = top_rows
        .iter()
        .map(|r| {
            json!({
                "agent_name": r.get::<String, _>("agent_name"),
                "executions": r.get::<i64, _>("executions"),
                "avg_time_ms": r.get::<Option<i64>, _>("avg_time_ms"),
            })
        })
        .collect();

    Ok(Json(json!({
        "totals": {
            "executions": total,
            "successful": successful,
            "failed": failed,
            "success_rate": success_rate,
            "total_tokens": total_tokens,
            "total_cost_usd": total_cost,
            "active_agents": active_agents,
        },
        "daily": daily,
        "tool_usage": tool_usage,
        "top_agents": top_agents,
    })))
}

pub async fn agent_metrics_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let aid = db_agent.agent_id;
    let pool = &state.db;

    // 1. Daily (30 days)
    let daily_rows = sqlx::query(
        "SELECT
            DATE(timestamp_ref) AS day,
            COUNT(*) AS executions,
            COUNT(*) FILTER (WHERE execution_status = 'failure') AS failures,
            COALESCE(SUM(tokens_used), 0) AS tokens,
            AVG(execution_time_ms)::BIGINT AS avg_time_ms
         FROM episodes
         WHERE agent_id = $1
           AND timestamp_ref > NOW() - INTERVAL '30 days'
         GROUP BY DATE(timestamp_ref)
         ORDER BY day",
    )
    .bind(aid)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let daily: Vec<Value> = daily_rows
        .iter()
        .map(|r| {
            let day: chrono::NaiveDate = r.get("day");
            json!({
                "date": day.to_string(),
                "executions": r.get::<i64, _>("executions"),
                "failures": r.get::<i64, _>("failures"),
                "tokens": r.get::<i64, _>("tokens"),
                "avg_time_ms": r.get::<Option<i64>, _>("avg_time_ms"),
            })
        })
        .collect();

    // 2. Tool usage with avg duration
    let tool_rows = sqlx::query(
        "SELECT
            tool->>'tool_name' AS tool_name,
            COUNT(*) AS count,
            AVG((tool->>'duration_ms')::BIGINT)::BIGINT AS avg_duration_ms
         FROM episodes,
              jsonb_array_elements(context->'tool_invocations') AS tool
         WHERE agent_id = $1
           AND context->'tool_invocations' IS NOT NULL
           AND jsonb_array_length(context->'tool_invocations') > 0
         GROUP BY tool->>'tool_name'
         ORDER BY count DESC",
    )
    .bind(aid)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let tool_usage: Vec<Value> = tool_rows
        .iter()
        .map(|r| {
            json!({
                "tool_name": r.get::<String, _>("tool_name"),
                "count": r.get::<i64, _>("count"),
                "avg_duration_ms": r.get::<Option<i64>, _>("avg_duration_ms"),
            })
        })
        .collect();

    // 3. Avg loop iterations
    let iter_row = sqlx::query(
        "SELECT AVG((context->>'loop_iterations')::INT)::FLOAT AS avg_iters
         FROM episodes
         WHERE agent_id = $1
           AND context->>'loop_iterations' IS NOT NULL",
    )
    .bind(aid)
    .fetch_one(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let avg_loop_iterations: Option<f64> = iter_row.get("avg_iters");

    // 4. Recent failures
    let fail_rows = sqlx::query(
        "SELECT episode_id, timestamp_ref, query, error_details
         FROM episodes
         WHERE agent_id = $1 AND execution_status = 'failure'
         ORDER BY timestamp_ref DESC
         LIMIT 5",
    )
    .bind(aid)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let recent_failures: Vec<Value> = fail_rows
        .iter()
        .map(|r| {
            json!({
                "episode_id": r.get::<uuid::Uuid, _>("episode_id"),
                "timestamp": r.get::<chrono::DateTime<chrono::Utc>, _>("timestamp_ref"),
                "query": r.get::<String, _>("query"),
                "error": r.get::<Option<String>, _>("error_details"),
            })
        })
        .collect();

    // 5. Consolidation history
    let consol_rows = sqlx::query(
        "SELECT job_id, started_at, completed_at, duration_ms, status,
                episodes_processed, rules_extracted, entities_created, facts_created
         FROM consolidation_jobs
         WHERE agent_id = $1
         ORDER BY started_at DESC
         LIMIT 10",
    )
    .bind(aid)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let consolidation_history: Vec<Value> = consol_rows
        .iter()
        .map(|r| {
            json!({
                "job_id": r.get::<uuid::Uuid, _>("job_id"),
                "started_at": r.get::<chrono::DateTime<chrono::Utc>, _>("started_at"),
                "completed_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at"),
                "duration_ms": r.get::<Option<i64>, _>("duration_ms"),
                "status": r.get::<String, _>("status"),
                "episodes_processed": r.get::<i32, _>("episodes_processed"),
                "rules_extracted": r.get::<i32, _>("rules_extracted"),
                "entities_created": r.get::<i32, _>("entities_created"),
                "facts_created": r.get::<i32, _>("facts_created"),
            })
        })
        .collect();

    Ok(Json(json!({
        "daily": daily,
        "tool_usage": tool_usage,
        "avg_loop_iterations": avg_loop_iterations.unwrap_or(1.0),
        "recent_failures": recent_failures,
        "consolidation_history": consolidation_history,
    })))
}
