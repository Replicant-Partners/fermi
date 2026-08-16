//! Ontology and projection handlers.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use agent_bestiary_memory::{Entity, Fact};
use agent_bestiary_projector::ProjectionMethod;

use crate::{resolve_agent, AppState};

/// Presentation-only fields carried by an `ontology_snapshots` row.
///
/// Deliberately excludes the snapshot's `entity_count` / `fact_count` /
/// `rule_count` / `community_count` columns. Those are a frozen copy of table
/// counts at snapshot time and go stale the moment consolidation next runs;
/// serving them is what made a live graph read as empty. Counts come from the
/// tables, always.
#[derive(Debug, Default, Clone)]
pub struct SnapshotDecorations {
    pub version: i32,
    pub git_commit_sha: Option<String>,
    pub github_url: Option<String>,
    pub mermaid_content: Option<String>,
    pub dream_synopsis: Option<String>,
}
/// Serve an agent's knowledge graph for the viewer and the Knowledge tab.
///
/// ## Why this reads the live tables and not `ontology_snapshots`
///
/// Consolidation writes what it learns to `entities`, `facts` and
/// `semantic_rules` (`memory::store`). It does **not** write an
/// `ontology_snapshots` row — `OntologySnapshotManager::create_snapshot` has
/// exactly one call site, in the standalone `consolidate` CLI, and nothing on
/// the API path invokes it.
///
/// This handler used to read `ontology_snapshots` alone, and to hardcode
/// `entities: []` / `relationships: []` even when it found a row. The two
/// defects compounded into the worst possible presentation of a *working*
/// loop: a consolidation cycle would report "5 rules, 4 entities", write them
/// correctly, and every knowledge surface would then read zero — because the
/// read path pointed at a table the write path never touches, and discarded
/// the arrays regardless. Dreaming looked broken while it was succeeding,
/// which is the same failure mode as `dreaming_maturity` exists to catch,
/// only inverted: there the loop ran and learned nothing, here it learned and
/// could not show it.
///
/// So counts and graph content are now derived from the tables consolidation
/// actually writes. A snapshot row, when one exists, contributes only its
/// decorations — the rendered Mermaid diagram, the git provenance, and the
/// dream synopsis. It can no longer determine whether the graph appears.
pub async fn get_ontology(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let Ok(db_agent) = resolve_agent(&state, &agent_id).await else {
        return Ok(Json(sample_or_empty(&agent_id)));
    };
    let aid = db_agent.agent_id;

    let entities = state
        .memory_store
        .get_agent_entities(aid)
        .await
        .unwrap_or_default();
    let facts = state
        .memory_store
        .get_agent_facts(aid)
        .await
        .unwrap_or_default();
    let rules = state
        .memory_store
        .get_agent_semantic_rules(aid)
        .await
        .unwrap_or_default();
    let communities = state
        .memory_store
        .get_agent_communities(aid)
        .await
        .unwrap_or_default();

    let active_rule_count = rules.iter().filter(|r| r.is_active).count();

    // Snapshot decorations only. Absence must not empty the graph.
    let snapshot = sqlx::query(
        r#"
        SELECT version, git_commit_sha, github_url,
               mermaid_content, dream_synopsis
        FROM ontology_snapshots
        WHERE agent_id = $1
        ORDER BY version DESC LIMIT 1
        "#,
    )
    .bind(aid)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let decorations = snapshot.map(|row| SnapshotDecorations {
        version: row.get::<i32, _>("version"),
        git_commit_sha: row.get::<Option<String>, _>("git_commit_sha"),
        github_url: row.get::<Option<String>, _>("github_url"),
        mermaid_content: row.get::<Option<String>, _>("mermaid_content"),
        dream_synopsis: row.get::<Option<String>, _>("dream_synopsis"),
    });

    Ok(Json(build_ontology_payload(
        &agent_id,
        &entities,
        &facts,
        active_rule_count,
        communities.len(),
        decorations,
    )))
}

/// Assemble the ontology payload from live knowledge-table rows.
///
/// Split out from [`get_ontology`] so the read-path invariant — *what the
/// tables hold is what the payload reports* — is unit-testable without a
/// database. See the tests at the bottom of this module.
pub fn build_ontology_payload(
    agent_id: &str,
    entities: &[Entity],
    facts: &[Fact],
    active_rule_count: usize,
    community_count: usize,
    snapshot: Option<SnapshotDecorations>,
) -> Value {
    // Bitemporal: a row with `t_invalid` set has been superseded and is
    // history, not current belief. Same filter `kg_overview_handler` applies.
    let active_entities: Vec<&Entity> = entities.iter().filter(|e| e.t_invalid.is_none()).collect();
    let active_facts: Vec<&Fact> = facts.iter().filter(|f| f.t_invalid.is_none()).collect();

    // Shapes below are the viewer's, not the KG API's: `templates/ontology.html`
    // maps `entities[] -> {id, name, type, properties}` and
    // `relationships[] -> {from, to, type, properties}`. `/api/agents/:id/kg`
    // serves the same data as `{nodes[].label, edges[].source/.target}` for a
    // different consumer; keep both rather than making one contort.
    let entity_json: Vec<Value> = active_entities
        .iter()
        .map(|e| {
            json!({
                "id": e.entity_id,
                "name": e.entity_name,
                "type": e.entity_type,
                "properties": {
                    "definition": e.summary,
                    "confidence": e.extraction_confidence,
                    "source_episodes": e.source_episodes.len(),
                    "attributes": e.properties,
                },
            })
        })
        .collect();

    // Drop edges whose endpoints are not both live nodes. d3's `forceLink`
    // silently drops unresolvable ids, so a dangling edge would otherwise
    // inflate the relationship count above what the graph can draw.
    let live_ids: std::collections::HashSet<_> =
        active_entities.iter().map(|e| e.entity_id).collect();
    let drawable_facts: Vec<&&Fact> = active_facts
        .iter()
        .filter(|f| {
            live_ids.contains(&f.source_entity_id) && live_ids.contains(&f.target_entity_id)
        })
        .collect();

    let relationship_json: Vec<Value> = drawable_facts
        .iter()
        .map(|f| {
            json!({
                "from": f.source_entity_id,
                "to": f.target_entity_id,
                "type": f.relation_type,
                "properties": {
                    "description": f.reasoning,
                    "confidence": f.confidence,
                    "cardinality": f.relation_cardinality.to_mermaid(),
                },
            })
        })
        .collect();

    let had_snapshot = snapshot.is_some();
    let deco = snapshot.unwrap_or_default();

    json!({
        "ontology_id": format!("{}_ontology", agent_id),
        "agent_id": agent_id,
        "version": deco.version,
        "mermaid_content": deco.mermaid_content,
        "git_commit_sha": deco.git_commit_sha,
        "github_url": deco.github_url,
        "dream_synopsis": deco.dream_synopsis,
        "entities": entity_json,
        "relationships": relationship_json,
        "evolution_commits": deco.version,
        // Always present, even at zero. The Knowledge tab gates its DOM write
        // on `stats` existing; omitting it left the fields showing a literal
        // ellipsis, which reads as "loading forever" rather than "empty".
        //
        // `fact_count` counts drawable edges, not active ones, so the header
        // count and the rendered graph cannot disagree.
        "stats": {
            "entity_count": active_entities.len(),
            "fact_count": drawable_facts.len(),
            "community_count": community_count,
            "rule_count": active_rule_count,
        },
        "source": if had_snapshot { "live+snapshot" } else { "live" },
    })
}

/// Demo ontologies for agents that do not exist in the database.
fn sample_or_empty(agent_id: &str) -> Value {
    let sample_path = format!("ontologies/samples/{}_ontology.json", agent_id);
    if let Ok(content) = std::fs::read_to_string(&sample_path) {
        if let Ok(ontology) = serde_json::from_str::<Value>(&content) {
            return ontology;
        }
    }

    json!({
        "ontology_id": format!("{}_ontology", agent_id),
        "agent_id": agent_id,
        "version": 0,
        "entities": [],
        "relationships": [],
        "evolution_commits": 0,
        "stats": {
            "entity_count": 0,
            "fact_count": 0,
            "community_count": 0,
            "rule_count": 0,
        },
        "metadata": {
            "status": "empty",
            "message": "No ontology data available for this agent"
        }
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use agent_bestiary_memory::Cardinality;
    use chrono::Utc;
    use uuid::Uuid;

    fn entity(name: &str) -> Entity {
        Entity {
            entity_id: Uuid::new_v4(),
            agent_id: Uuid::nil(),
            entity_name: name.to_string(),
            entity_type: "concept".to_string(),
            summary: Some(format!("what {name} means")),
            t_valid: Utc::now(),
            t_invalid: None,
            source_episodes: vec![Uuid::new_v4()],
            extraction_confidence: 0.9,
            embedding: None,
            properties: None,
        }
    }

    fn fact(source: &Entity, target: &Entity) -> Fact {
        Fact {
            fact_id: Uuid::new_v4(),
            agent_id: Uuid::nil(),
            source_entity_id: source.entity_id,
            target_entity_id: target.entity_id,
            relation_type: "relates_to".to_string(),
            relation_cardinality: Cardinality::OneToMany,
            confidence: 0.8,
            reasoning: Some("because".to_string()),
            t_valid: Utc::now(),
            t_invalid: None,
            source_episodes: vec![],
            data: None,
        }
    }

    /// The invariant this whole module exists to uphold.
    ///
    /// Consolidation writes to `entities` / `facts` / `semantic_rules`. If the
    /// read path reports zero while those tables hold rows, a working dreaming
    /// cycle is indistinguishable from a broken one on every knowledge
    /// surface. That is precisely what shipped: the handler read
    /// `ontology_snapshots` (which nothing on the API path writes) and
    /// hardcoded `entities: []` even when it found a row.
    #[test]
    fn payload_reports_what_the_tables_hold() {
        let a = entity("kelly_criterion");
        let b = entity("bankroll");
        let f = fact(&a, &b);

        let payload = build_ontology_payload("fermi", &[a, b], &[f], 5, 2, None);

        assert_eq!(payload["stats"]["entity_count"], 2);
        assert_eq!(payload["stats"]["fact_count"], 1);
        assert_eq!(payload["stats"]["rule_count"], 5);
        assert_eq!(payload["stats"]["community_count"], 2);

        // Counts alone are not enough — the arrays feed the graph.
        assert_eq!(payload["entities"].as_array().unwrap().len(), 2);
        assert_eq!(payload["relationships"].as_array().unwrap().len(), 1);
    }

    /// A snapshot is decoration. It must never be able to empty a live graph,
    /// which is how the previous implementation failed: no snapshot row meant
    /// the handler fell through to a terminal `status: "empty"` payload
    /// regardless of how much the agent had actually learned.
    #[test]
    fn snapshot_absence_does_not_empty_the_graph() {
        let a = entity("a");
        let b = entity("b");
        let f = fact(&a, &b);

        let without =
            build_ontology_payload("fermi", &[a.clone(), b.clone()], &[f.clone()], 3, 0, None);
        let with = build_ontology_payload(
            "fermi",
            &[a, b],
            &[f],
            3,
            0,
            Some(SnapshotDecorations {
                version: 7,
                mermaid_content: Some("erDiagram".to_string()),
                ..Default::default()
            }),
        );

        assert_eq!(without["stats"], with["stats"]);
        assert_eq!(without["entities"], with["entities"]);
        assert_eq!(without["relationships"], with["relationships"]);

        // Only the decorations differ.
        assert_eq!(without["evolution_commits"], 0);
        assert_eq!(with["evolution_commits"], 7);
        assert_eq!(without["source"], "live");
        assert_eq!(with["source"], "live+snapshot");
    }

    /// `stats` must be present on every response. The Knowledge tab renders a
    /// literal ellipsis and replaces it from this block; when the block was
    /// missing the placeholder stayed forever, which reads as a hung request
    /// rather than an honest zero.
    #[test]
    fn stats_block_is_present_even_when_empty() {
        let payload = build_ontology_payload("fermi", &[], &[], 0, 0, None);
        assert!(payload["stats"].is_object());
        assert_eq!(payload["stats"]["entity_count"], 0);
        assert_eq!(payload["entities"].as_array().unwrap().len(), 0);

        // Same guarantee on the no-such-agent path.
        let empty = sample_or_empty("does_not_exist");
        assert!(empty["stats"].is_object());
        assert_eq!(empty["stats"]["entity_count"], 0);
    }

    /// Superseded rows are history, not current belief.
    #[test]
    fn invalidated_rows_are_excluded() {
        let a = entity("current");
        let mut b = entity("superseded");
        b.t_invalid = Some(Utc::now());

        let payload = build_ontology_payload("fermi", &[a, b], &[], 0, 0, None);

        assert_eq!(payload["stats"]["entity_count"], 1);
        assert_eq!(payload["entities"][0]["name"], "current");
    }

    /// The header count and the drawn graph must agree. d3 silently drops an
    /// edge whose endpoints it cannot resolve, so counting active facts rather
    /// than drawable ones would advertise relationships the viewer never
    /// renders — a smaller version of the same "reported but not readable"
    /// defect.
    #[test]
    fn dangling_edges_are_excluded_from_both_count_and_array() {
        let a = entity("a");
        let b = entity("b");
        let orphan = entity("orphan");
        let good = fact(&a, &b);
        let dangling = fact(&a, &orphan);

        // `orphan` is not passed in, so `dangling` has no resolvable target.
        let payload = build_ontology_payload("fermi", &[a, b], &[good, dangling], 0, 0, None);

        assert_eq!(payload["relationships"].as_array().unwrap().len(), 1);
        assert_eq!(payload["stats"]["fact_count"], 1);
    }

    /// The viewer maps `{id, name, type}` and `{from, to, type}`. The KG API
    /// at `/api/agents/:id/kg` uses `{id, label}` and `{source, target}` for a
    /// different consumer. Getting these confused renders an empty canvas with
    /// no error, so pin the field names.
    #[test]
    fn field_names_match_what_the_viewer_maps() {
        let a = entity("a");
        let b = entity("b");
        let f = fact(&a, &b);
        let payload = build_ontology_payload("fermi", &[a, b], &[f], 0, 0, None);

        let node = &payload["entities"][0];
        assert!(node["id"].is_string(), "viewer joins edges on `id`");
        assert!(node["name"].is_string(), "viewer labels nodes from `name`");
        assert!(node["type"].is_string());

        let edge = &payload["relationships"][0];
        assert!(
            edge["from"].is_string(),
            "viewer reads `from`, not `source`"
        );
        assert!(edge["to"].is_string(), "viewer reads `to`, not `target`");
        assert!(edge["type"].is_string());

        // Edge endpoints must be resolvable against node ids.
        let ids: Vec<_> = payload["entities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap().to_string())
            .collect();
        assert!(ids.contains(&edge["from"].as_str().unwrap().to_string()));
        assert!(ids.contains(&edge["to"].as_str().unwrap().to_string()));
    }
}
