//! Ontology and projection handlers.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use fermi_auth::AuthPrincipal;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;

use agent_bestiary_projector::ProjectionMethod;

use crate::{resolve_agent, AppState};
pub async fn get_ontology(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Primary: latest database snapshot
    if let Ok(db_agent) = resolve_agent(&state, &agent_id).await {
        let row = sqlx::query(
            r#"
            SELECT snapshot_id, version, git_commit_sha, github_url,
                   entity_count, fact_count, community_count, rule_count,
                   mermaid_content, dream_synopsis, created_at
            FROM ontology_snapshots
            WHERE agent_id = $1
            ORDER BY version DESC LIMIT 1
            "#,
        )
        .bind(db_agent.agent_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

        if let Some(row) = row {
            return Ok(Json(json!({
                "ontology_id": format!("{}_ontology", agent_id),
                "agent_id": agent_id,
                "version": row.get::<i32, _>("version"),
                "mermaid_content": row.get::<String, _>("mermaid_content"),
                "git_commit_sha": row.get::<String, _>("git_commit_sha"),
                "github_url": row.get::<Option<String>, _>("github_url"),
                "dream_synopsis": row.get::<Option<String>, _>("dream_synopsis"),
                "entities": [],
                "relationships": [],
                "evolution_commits": row.get::<i32, _>("version"),
                "stats": {
                    "entity_count": row.get::<i32, _>("entity_count"),
                    "fact_count": row.get::<i32, _>("fact_count"),
                    "community_count": row.get::<i32, _>("community_count"),
                    "rule_count": row.get::<i32, _>("rule_count"),
                },
                "source": "database",
            })));
        }
    }

    // Fallback: sample files
    let sample_path = format!("ontologies/samples/{}_ontology.json", agent_id);
    if let Ok(content) = std::fs::read_to_string(&sample_path) {
        if let Ok(ontology) = serde_json::from_str::<Value>(&content) {
            return Ok(Json(ontology));
        }
    }

    Ok(Json(json!({
        "ontology_id": format!("{}_ontology", agent_id),
        "agent_id": agent_id,
        "version": "1.0.0",
        "entities": [],
        "relationships": [],
        "evolution_commits": 0,
        "metadata": {
            "status": "empty",
            "message": "No ontology data available for this agent"
        }
    })))
}

// ─── Projector API routes ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ProjectionParams {
    method: Option<String>,
    dimensions: Option<u8>,
}

pub async fn get_agent_projections(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(params): Query<ProjectionParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let dims = params.dimensions.unwrap_or(3);
    let method = parse_projection_method(params.method.as_deref());

    // Check cache
    let cache_key = agent_bestiary_projector::CacheKey {
        agent_id: Some(db_agent.agent_id),
        method: method.name().to_string(),
        dimensions: dims,
    };
    if let Some(cached) = state.projection_cache.get(&cache_key) {
        return Ok(Json(serde_json::to_value(cached).map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?));
    }

    let result = state
        .projection_engine
        .project_agent(db_agent.agent_id, &agent_id, &method, dims)
        .await
        .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;

    state.projection_cache.insert(cache_key, result.clone());
    Ok(Json(serde_json::to_value(result).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?))
}

#[derive(Debug, Deserialize)]
pub struct BestiaryProjectionParams {
    method: Option<String>,
    dimensions: Option<u8>,
    limit: Option<usize>,
}

pub async fn get_bestiary_projections(
    State(state): State<AppState>,
    Query(params): Query<BestiaryProjectionParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let dims = params.dimensions.unwrap_or(3);
    let limit = params.limit.unwrap_or(5000);
    let method = parse_projection_method(params.method.as_deref());

    let cache_key = agent_bestiary_projector::CacheKey {
        agent_id: None,
        method: method.name().to_string(),
        dimensions: dims,
    };
    if let Some(cached) = state.projection_cache.get(&cache_key) {
        return Ok(Json(serde_json::to_value(cached).map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?));
    }

    let result = state
        .projection_engine
        .project_bestiary(&method, dims, limit)
        .await
        .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;

    state.projection_cache.insert(cache_key, result.clone());
    Ok(Json(serde_json::to_value(result).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?))
}

#[derive(Debug, Deserialize)]
pub struct TemporalProjectionParams {
    method: Option<String>,
    dimensions: Option<u8>,
    keyframes: Option<usize>,
}

pub async fn get_temporal_projections(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(params): Query<TemporalProjectionParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let dims = params.dimensions.unwrap_or(3);
    let keyframes = params.keyframes.unwrap_or(10);
    let method = parse_projection_method(params.method.as_deref());

    let result = state
        .projection_engine
        .project_agent_temporal(db_agent.agent_id, &agent_id, &method, dims, keyframes)
        .await
        .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;

    Ok(Json(serde_json::to_value(result).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?))
}

pub fn parse_projection_method(method: Option<&str>) -> ProjectionMethod {
    match method {
        Some("tsne") => ProjectionMethod::Tsne { perplexity: 30.0 },
        _ => ProjectionMethod::Pca,
    }
}

// ─── Ontology API (database-backed) ────────────────────────────────

pub async fn get_ontology_history(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    let rows = sqlx::query(
        r#"
        SELECT snapshot_id, version, git_commit_sha, entity_count, fact_count,
               community_count, rule_count, dream_synopsis, created_at
        FROM ontology_snapshots
        WHERE agent_id = $1
        ORDER BY version DESC
        "#,
    )
    .bind(db_agent.agent_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let snapshots: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "snapshot_id": r.get::<uuid::Uuid, _>("snapshot_id"),
                "version": r.get::<i32, _>("version"),
                "git_commit_sha": r.get::<String, _>("git_commit_sha"),
                "entity_count": r.get::<i32, _>("entity_count"),
                "fact_count": r.get::<i32, _>("fact_count"),
                "community_count": r.get::<i32, _>("community_count"),
                "rule_count": r.get::<i32, _>("rule_count"),
                "dream_synopsis": r.get::<Option<String>, _>("dream_synopsis"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            })
        })
        .collect();

    Ok(Json(json!({
        "agent_id": agent_id,
        "agent_uuid": db_agent.agent_id,
        "snapshots": snapshots,
        "total": snapshots.len(),
    })))
}

pub async fn get_ontology_snapshot(
    State(state): State<AppState>,
    Path((agent_id, snapshot_id)): Path<(String, uuid::Uuid)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let _db_agent = resolve_agent(&state, &agent_id).await?;

    let row = sqlx::query(
        r#"
        SELECT snapshot_id, version, git_commit_sha, github_url,
               entity_count, fact_count, community_count, rule_count,
               mermaid_content, dream_synopsis, consolidation_stats, created_at
        FROM ontology_snapshots
        WHERE snapshot_id = $1
        "#,
    )
    .bind(snapshot_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Snapshot not found".to_string()))?;

    Ok(Json(json!({
        "snapshot_id": row.get::<uuid::Uuid, _>("snapshot_id"),
        "agent_id": agent_id,
        "version": row.get::<i32, _>("version"),
        "git_commit_sha": row.get::<String, _>("git_commit_sha"),
        "github_url": row.get::<Option<String>, _>("github_url"),
        "mermaid_content": row.get::<String, _>("mermaid_content"),
        "dream_synopsis": row.get::<Option<String>, _>("dream_synopsis"),
        "consolidation_stats": row.get::<Option<Value>, _>("consolidation_stats"),
        "stats": {
            "entity_count": row.get::<i32, _>("entity_count"),
            "fact_count": row.get::<i32, _>("fact_count"),
            "community_count": row.get::<i32, _>("community_count"),
            "rule_count": row.get::<i32, _>("rule_count"),
        },
        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })))
}

#[derive(Debug, Deserialize)]
pub struct DiffParams {
    from: uuid::Uuid,
    to: uuid::Uuid,
}

pub async fn get_ontology_diff(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(params): Query<DiffParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let _db_agent = resolve_agent(&state, &agent_id).await?;

    // Fetch both snapshots
    let from_row = sqlx::query(
        "SELECT version, mermaid_content, entity_count, fact_count, rule_count, created_at FROM ontology_snapshots WHERE snapshot_id = $1",
    )
    .bind(params.from)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Source snapshot not found".to_string()))?;

    let to_row = sqlx::query(
        "SELECT version, mermaid_content, entity_count, fact_count, rule_count, created_at FROM ontology_snapshots WHERE snapshot_id = $1",
    )
    .bind(params.to)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Target snapshot not found".to_string()))?;

    let from_content: String = from_row.get("mermaid_content");
    let to_content: String = to_row.get("mermaid_content");

    // Line-based diff
    let from_lines: std::collections::HashSet<&str> = from_content.lines().collect();
    let to_lines: std::collections::HashSet<&str> = to_content.lines().collect();

    let added: Vec<&str> = to_lines.difference(&from_lines).copied().collect();
    let removed: Vec<&str> = from_lines.difference(&to_lines).copied().collect();

    Ok(Json(json!({
        "agent_id": agent_id,
        "from": {
            "snapshot_id": params.from,
            "version": from_row.get::<i32, _>("version"),
            "entity_count": from_row.get::<i32, _>("entity_count"),
            "fact_count": from_row.get::<i32, _>("fact_count"),
            "rule_count": from_row.get::<i32, _>("rule_count"),
        },
        "to": {
            "snapshot_id": params.to,
            "version": to_row.get::<i32, _>("version"),
            "entity_count": to_row.get::<i32, _>("entity_count"),
            "fact_count": to_row.get::<i32, _>("fact_count"),
            "rule_count": to_row.get::<i32, _>("rule_count"),
        },
        "diff": {
            "lines_added": added.len(),
            "lines_removed": removed.len(),
            "added": added,
            "removed": removed,
        },
        "deltas": {
            "entity_count": to_row.get::<i32, _>("entity_count") - from_row.get::<i32, _>("entity_count"),
            "fact_count": to_row.get::<i32, _>("fact_count") - from_row.get::<i32, _>("fact_count"),
            "rule_count": to_row.get::<i32, _>("rule_count") - from_row.get::<i32, _>("rule_count"),
        }
    })))
}
