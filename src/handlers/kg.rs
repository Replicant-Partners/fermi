//! Knowledge graph query handlers — entities, facts, rules, communities.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{resolve_agent, AppState};

// ─── Query params ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct EntityFilter {
    pub entity_type: Option<String>,
    pub confidence_min: Option<f64>,
    pub include_invalid: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct FactFilter {
    pub relation_type: Option<String>,
    pub confidence_min: Option<f64>,
    pub include_invalid: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct RuleFilter {
    pub confidence_min: Option<f64>,
    pub status: Option<String>, // verified, unverified, rejected
    pub active_only: Option<bool>,
}

// ─── Entities ───────────────────────────────────────────────────────

pub async fn list_entities_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(filter): Query<EntityFilter>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    let mut entities = state
        .memory_store
        .get_agent_entities(db_agent.agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Filter out invalidated unless requested
    if !filter.include_invalid.unwrap_or(false) {
        entities.retain(|e| e.t_invalid.is_none());
    }

    if let Some(ref etype) = filter.entity_type {
        let q = etype.to_lowercase();
        entities.retain(|e| e.entity_type.to_lowercase() == q);
    }

    if let Some(min) = filter.confidence_min {
        entities.retain(|e| e.extraction_confidence >= min);
    }

    let items: Vec<Value> = entities
        .iter()
        .map(|e| {
            json!({
                "entity_id": e.entity_id,
                "entity_name": e.entity_name,
                "entity_type": e.entity_type,
                "summary": e.summary,
                "extraction_confidence": e.extraction_confidence,
                "source_episodes": e.source_episodes,
                "t_valid": e.t_valid.to_rfc3339(),
                "t_invalid": e.t_invalid.map(|t| t.to_rfc3339()),
                "has_embedding": e.embedding.is_some(),
            })
        })
        .collect();

    // Summarize entity types
    let mut type_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for e in &entities {
        *type_counts.entry(e.entity_type.clone()).or_insert(0) += 1;
    }

    Ok(Json(json!({
        "agent_id": agent_id,
        "entities": items,
        "total": items.len(),
        "type_summary": type_counts,
    })))
}

pub async fn get_entity_handler(
    State(state): State<AppState>,
    Path((agent_id, entity_id)): Path<(String, Uuid)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let _db_agent = resolve_agent(&state, &agent_id).await?;

    let entity = state
        .memory_store
        .get_entity(entity_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Entity not found: {}", e)))?;

    // Get related facts (both as source and target)
    let facts = state
        .memory_store
        .get_entity_facts(entity_id)
        .await
        .unwrap_or_default();

    let relationships: Vec<Value> = facts
        .iter()
        .map(|f| {
            json!({
                "fact_id": f.fact_id,
                "relation_type": f.relation_type,
                "source_entity_id": f.source_entity_id,
                "target_entity_id": f.target_entity_id,
                "confidence": f.confidence,
                "direction": if f.source_entity_id == entity_id { "outgoing" } else { "incoming" },
            })
        })
        .collect();

    Ok(Json(json!({
        "entity_id": entity.entity_id,
        "entity_name": entity.entity_name,
        "entity_type": entity.entity_type,
        "summary": entity.summary,
        "extraction_confidence": entity.extraction_confidence,
        "source_episodes": entity.source_episodes,
        "t_valid": entity.t_valid.to_rfc3339(),
        "t_invalid": entity.t_invalid.map(|t| t.to_rfc3339()),
        "has_embedding": entity.embedding.is_some(),
        "relationships": relationships,
        "relationship_count": relationships.len(),
    })))
}

// ─── Facts ──────────────────────────────────────────────────────────

pub async fn list_facts_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(filter): Query<FactFilter>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    let mut facts = state
        .memory_store
        .get_agent_facts(db_agent.agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !filter.include_invalid.unwrap_or(false) {
        facts.retain(|f| f.t_invalid.is_none());
    }

    if let Some(ref rtype) = filter.relation_type {
        let q = rtype.to_lowercase();
        facts.retain(|f| f.relation_type.to_lowercase().contains(&q));
    }

    if let Some(min) = filter.confidence_min {
        facts.retain(|f| f.confidence >= min);
    }

    // Collect entity IDs for eager-loading names
    let entity_ids: std::collections::HashSet<Uuid> = facts
        .iter()
        .flat_map(|f| vec![f.source_entity_id, f.target_entity_id])
        .collect();

    let mut entity_names: std::collections::HashMap<Uuid, String> =
        std::collections::HashMap::new();
    for eid in &entity_ids {
        if let Ok(entity) = state.memory_store.get_entity(*eid).await {
            entity_names.insert(*eid, entity.entity_name);
        }
    }

    let items: Vec<Value> = facts
        .iter()
        .map(|f| {
            json!({
                "fact_id": f.fact_id,
                "source_entity_id": f.source_entity_id,
                "source_entity_name": entity_names.get(&f.source_entity_id),
                "target_entity_id": f.target_entity_id,
                "target_entity_name": entity_names.get(&f.target_entity_id),
                "relation_type": f.relation_type,
                "relation_cardinality": format!("{:?}", f.relation_cardinality),
                "confidence": f.confidence,
                "reasoning": f.reasoning,
                "source_episodes": f.source_episodes,
                "t_valid": f.t_valid.to_rfc3339(),
                "t_invalid": f.t_invalid.map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    // Summarize relation types
    let mut relation_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for f in &facts {
        *relation_counts.entry(f.relation_type.clone()).or_insert(0) += 1;
    }

    Ok(Json(json!({
        "agent_id": agent_id,
        "facts": items,
        "total": items.len(),
        "relation_summary": relation_counts,
    })))
}

pub async fn get_entity_facts_handler(
    State(state): State<AppState>,
    Path((agent_id, entity_id)): Path<(String, Uuid)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let _db_agent = resolve_agent(&state, &agent_id).await?;

    let facts = state
        .memory_store
        .get_entity_facts(entity_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Eager-load entity names
    let entity_ids: std::collections::HashSet<Uuid> = facts
        .iter()
        .flat_map(|f| vec![f.source_entity_id, f.target_entity_id])
        .collect();

    let mut entity_names: std::collections::HashMap<Uuid, String> =
        std::collections::HashMap::new();
    for eid in &entity_ids {
        if let Ok(entity) = state.memory_store.get_entity(*eid).await {
            entity_names.insert(*eid, entity.entity_name);
        }
    }

    let items: Vec<Value> = facts
        .iter()
        .map(|f| {
            json!({
                "fact_id": f.fact_id,
                "source_entity_id": f.source_entity_id,
                "source_entity_name": entity_names.get(&f.source_entity_id),
                "target_entity_id": f.target_entity_id,
                "target_entity_name": entity_names.get(&f.target_entity_id),
                "relation_type": f.relation_type,
                "relation_cardinality": format!("{:?}", f.relation_cardinality),
                "confidence": f.confidence,
                "reasoning": f.reasoning,
            })
        })
        .collect();

    Ok(Json(json!({
        "entity_id": entity_id,
        "facts": items,
        "total": items.len(),
    })))
}

// ─── Semantic Rules ─────────────────────────────────────────────────

pub async fn list_rules_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(filter): Query<RuleFilter>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    let mut rules = state
        .memory_store
        .get_agent_semantic_rules(db_agent.agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if filter.active_only.unwrap_or(true) {
        rules.retain(|r| r.is_active);
    }

    if let Some(ref status) = filter.status {
        let s = status.to_lowercase();
        rules.retain(|r| format!("{:?}", r.verification_status).to_lowercase() == s);
    }

    if let Some(min) = filter.confidence_min {
        rules.retain(|r| r.confidence_score >= min);
    }

    let items: Vec<Value> = rules
        .iter()
        .map(|r| {
            json!({
                "rule_id": r.rule_id,
                "rule_content": r.rule_content,
                "rule_description": r.rule_description,
                "confidence_score": r.confidence_score,
                "verification_status": format!("{:?}", r.verification_status),
                "verification_method": r.verification_method,
                "episode_count": r.episode_count,
                "is_active": r.is_active,
                "has_embedding": r.embedding.is_some(),
                "created_at": r.created_at.to_rfc3339(),
            })
        })
        .collect();

    // Summarize verification status
    let mut status_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for r in &rules {
        *status_counts
            .entry(format!("{:?}", r.verification_status))
            .or_insert(0) += 1;
    }

    Ok(Json(json!({
        "agent_id": agent_id,
        "rules": items,
        "total": items.len(),
        "status_summary": status_counts,
    })))
}

pub async fn get_rule_handler(
    State(state): State<AppState>,
    Path((agent_id, rule_id)): Path<(String, Uuid)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let _db_agent = resolve_agent(&state, &agent_id).await?;

    let rule = state
        .memory_store
        .get_semantic_rule(rule_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Rule not found: {}", e)))?;

    Ok(Json(json!({
        "rule_id": rule.rule_id,
        "rule_content": rule.rule_content,
        "rule_description": rule.rule_description,
        "confidence_score": rule.confidence_score,
        "verification_status": format!("{:?}", rule.verification_status),
        "verification_method": rule.verification_method,
        "source_episode_cluster": rule.source_episode_cluster,
        "episode_count": rule.episode_count,
        "is_active": rule.is_active,
        "has_embedding": rule.embedding.is_some(),
        "created_at": rule.created_at.to_rfc3339(),
    })))
}

// ─── Communities ────────────────────────────────────────────────────

pub async fn list_communities_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    let communities = state
        .memory_store
        .get_agent_communities(db_agent.agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let items: Vec<Value> = communities
        .iter()
        .map(|c| {
            json!({
                "community_id": c.community_id,
                "community_name": c.community_name,
                "summary": c.summary,
                "member_entity_ids": c.member_entity_ids,
                "member_count": c.member_count,
                "has_embedding": c.embedding.is_some(),
                "created_at": c.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "agent_id": agent_id,
        "communities": items,
        "total": items.len(),
    })))
}

// ─── Graph overview ─────────────────────────────────────────────────

/// Returns a full KG summary: entity/fact/rule/community counts,
/// type distributions, and a graph structure for visualization.
pub async fn kg_overview_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
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

    // Active only
    let active_entities: Vec<_> = entities.iter().filter(|e| e.t_invalid.is_none()).collect();
    let active_facts: Vec<_> = facts.iter().filter(|f| f.t_invalid.is_none()).collect();
    let active_rules: Vec<_> = rules.iter().filter(|r| r.is_active).collect();

    // Entity type distribution
    let mut entity_types: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for e in &active_entities {
        *entity_types.entry(e.entity_type.clone()).or_insert(0) += 1;
    }

    // Relation type distribution
    let mut relation_types: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for f in &active_facts {
        *relation_types.entry(f.relation_type.clone()).or_insert(0) += 1;
    }

    // Build graph nodes + edges for visualization
    let nodes: Vec<Value> = active_entities
        .iter()
        .map(|e| {
            json!({
                "id": e.entity_id,
                "label": e.entity_name,
                "type": e.entity_type,
                "confidence": e.extraction_confidence,
            })
        })
        .collect();

    let edges: Vec<Value> = active_facts
        .iter()
        .map(|f| {
            json!({
                "id": f.fact_id,
                "source": f.source_entity_id,
                "target": f.target_entity_id,
                "label": f.relation_type,
                "confidence": f.confidence,
            })
        })
        .collect();

    // Avg confidence
    let avg_entity_confidence = if active_entities.is_empty() {
        0.0
    } else {
        active_entities
            .iter()
            .map(|e| e.extraction_confidence)
            .sum::<f64>()
            / active_entities.len() as f64
    };
    let avg_fact_confidence = if active_facts.is_empty() {
        0.0
    } else {
        active_facts.iter().map(|f| f.confidence).sum::<f64>() / active_facts.len() as f64
    };
    let avg_rule_confidence = if active_rules.is_empty() {
        0.0
    } else {
        active_rules.iter().map(|r| r.confidence_score).sum::<f64>() / active_rules.len() as f64
    };

    Ok(Json(json!({
        "agent_id": agent_id,
        "counts": {
            "entities": active_entities.len(),
            "facts": active_facts.len(),
            "rules": active_rules.len(),
            "communities": communities.len(),
        },
        "confidence": {
            "avg_entity": (avg_entity_confidence * 100.0).round() / 100.0,
            "avg_fact": (avg_fact_confidence * 100.0).round() / 100.0,
            "avg_rule": (avg_rule_confidence * 100.0).round() / 100.0,
        },
        "distributions": {
            "entity_types": entity_types,
            "relation_types": relation_types,
        },
        "graph": {
            "nodes": nodes,
            "edges": edges,
        },
    })))
}
