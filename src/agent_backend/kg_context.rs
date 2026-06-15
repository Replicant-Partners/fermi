//! KG context injection — enrich agent system prompts with semantically
//! relevant knowledge from past dream cycles before any execution.
//!
//! Called at every agent execution entry point so the model can reason
//! over what it has already learned, not just its static system prompt.
//!
//! ## Performance (Spec 21 Phase 0/1/2)
//!
//! Phase 0: timing spans at enrich entry, embedding call, and DB query.
//! Phase 1: returns the query embedding alongside the enriched card so
//!           callers can reuse it for episode storage without a second
//!           API call.
//! Phase 2: uses pgvector ANN (HNSW) to transfer only top-k rows from
//!           the DB rather than loading the full corpus into Rust and
//!           scoring in-memory. Falls back to load-all path when the
//!           ANN methods are not available (migration 142 pending).

use crate::agent_backend::agent_card::AgentCard;
use agent_bestiary_memory::{EmbeddingGenerator, MemoryStore};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

const MIN_SIMILARITY: f32 = 0.30;

/// Append relevant KG context to the agent card's system prompt.
///
/// Returns `(enriched_card, Option<query_embedding>)`.
/// The query embedding is returned so callers can pass it directly to
/// episode storage, eliminating the second embedding API call per execution.
///
/// Returns `(card, None)` when the KG is empty (fast-path skip) or when
/// the embedding call fails.
pub async fn enrich_with_kg_context(
    memory_store: &Arc<MemoryStore>,
    embedder: &Arc<dyn EmbeddingGenerator>,
    agent_uuid: Uuid,
    query: &str,
    mut card: AgentCard,
) -> (AgentCard, Option<Vec<f32>>) {
    // Phase 0 — baseline timing span
    let t_total = tokio::time::Instant::now();

    // Fast-path: skip external embedding call when KG is empty.
    // Common case for new agents; avoids ~300-800ms API call that finds nothing.
    if card.ontology_stats.entities == 0 && card.ontology_stats.relationships == 0 {
        return (card, None);
    }

    // Generate query embedding (Phase 1: returned to caller)
    let t_embed = tokio::time::Instant::now();
    let query_embedding = match embedder.generate(query).await {
        Ok(e) => e,
        Err(_) => return (card, None),
    };
    tracing::info!(
        elapsed_ms = t_embed.elapsed().as_millis() as u64,
        agent_id = %agent_uuid,
        site = "kg_context_embed",
        "embed_call"
    );

    // Phase 2: try ANN path first; fall back to load-all if unavailable
    let t_db = tokio::time::Instant::now();
    let (top_rules, all_entities) = match try_ann_retrieval(
        memory_store,
        agent_uuid,
        &query_embedding,
    ).await {
        Some((rules, entities)) => {
            tracing::info!(
                elapsed_ms = t_db.elapsed().as_millis() as u64,
                rules = rules.len(),
                entities = entities.len(),
                path = "ann",
                "kg_context_db"
            );
            (rules, entities)
        }
        None => {
            // ANN not available (HNSW index not built yet) — fall back to
            // load-all + in-memory scoring (pre-Spec-21 behaviour)
            let (rules_res, entities_res) = tokio::join!(
                memory_store.get_agent_semantic_rules(agent_uuid),
                memory_store.get_agent_entities(agent_uuid),
            );
            let rules = rules_res.unwrap_or_default();
            let entities = entities_res.unwrap_or_default();
            tracing::info!(
                elapsed_ms = t_db.elapsed().as_millis() as u64,
                rules = rules.len(),
                entities = entities.len(),
                path = "load_all_fallback",
                "kg_context_db"
            );

            if rules.is_empty() && entities.is_empty() {
                return (card, Some(query_embedding));
            }

            // In-memory scoring (legacy path)
            let mut scored_rules: Vec<(f32, _)> = rules
                .iter()
                .filter_map(|r| {
                    r.embedding.as_ref()
                        .map(|emb| (cosine_similarity(&query_embedding, emb), r))
                })
                .filter(|(s, _)| *s >= MIN_SIMILARITY)
                .collect();
            scored_rules.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            scored_rules.truncate(5);

            let (cep_entities, episodic_entities): (Vec<_>, Vec<_>) = entities
                .iter()
                .partition(|e| e.entity_type.starts_with("cep_"));

            let mut scored_episodic: Vec<(f32, _)> = episodic_entities
                .iter()
                .filter_map(|e| {
                    e.embedding.as_ref()
                        .map(|emb| (cosine_similarity(&query_embedding, emb), *e))
                })
                .filter(|(s, _)| *s >= MIN_SIMILARITY)
                .collect();
            scored_episodic.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            scored_episodic.truncate(8);

            if scored_rules.is_empty() && scored_episodic.is_empty() && cep_entities.is_empty() {
                return (card, Some(query_embedding));
            }

            // Build prompt block using legacy scored format
            let kg_block = build_kg_block_scored(&scored_rules, &scored_episodic, &cep_entities);
            if !kg_block.is_empty() {
                let base = card.system_prompt.unwrap_or_default();
                card.system_prompt = Some(format!("{}{}", base, kg_block));
            }
            tracing::info!(
                elapsed_ms = t_total.elapsed().as_millis() as u64,
                agent_id = %agent_uuid,
                "kg_context_enrich"
            );
            return (card, Some(query_embedding));
        }
    };

    // ANN path: top_rules and all_entities already similarity-filtered and limited
    let (cep_entities, episodic_entities): (Vec<_>, Vec<_>) = all_entities
        .iter()
        .partition(|e| e.entity_type.starts_with("cep_"));

    if top_rules.is_empty() && episodic_entities.is_empty() && cep_entities.is_empty() {
        tracing::info!(
            elapsed_ms = t_total.elapsed().as_millis() as u64,
            agent_id = %agent_uuid,
            "kg_context_enrich"
        );
        return (card, Some(query_embedding));
    }

    // Build prompt block (ANN path: no per-item scores available, use confidence)
    let episodic_owned: Vec<_> = episodic_entities.into_iter().cloned().collect();
    let kg_block = build_kg_block_ann(&top_rules, &episodic_owned, &cep_entities);
    if !kg_block.is_empty() {
        let base = card.system_prompt.unwrap_or_default();
        card.system_prompt = Some(format!("{}{}", base, kg_block));
    }

    tracing::info!(
        elapsed_ms = t_total.elapsed().as_millis() as u64,
        agent_id = %agent_uuid,
        "kg_context_enrich"
    );

    (card, Some(query_embedding))
}

/// Try pgvector ANN retrieval. Returns None if HNSW indices aren't ready yet.
async fn try_ann_retrieval(
    store: &Arc<MemoryStore>,
    agent_id: Uuid,
    query_embedding: &[f32],
) -> Option<(Vec<agent_bestiary_memory::SemanticRule>, Vec<agent_bestiary_memory::Entity>)> {
    let (rules_res, entities_res) = tokio::join!(
        store.get_top_k_semantic_rules(agent_id, query_embedding, 5, MIN_SIMILARITY),
        store.get_top_k_entities_with_cep(agent_id, query_embedding, 8, MIN_SIMILARITY),
    );
    match (rules_res, entities_res) {
        (Ok(rules), Ok(entities)) => Some((rules, entities)),
        _ => None, // ANN unavailable — caller falls back to load-all
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { 0.0 } else { dot / (norm_a * norm_b) }
}

fn build_kg_block_scored(
    scored_rules: &[(f32, &agent_bestiary_memory::SemanticRule)],
    scored_entities: &[(f32, &agent_bestiary_memory::Entity)],
    cep_entities: &[&agent_bestiary_memory::Entity],
) -> String {
    build_kg_block_inner(
        &scored_rules.iter().map(|(s, r)| (Some(*s), *r)).collect::<Vec<_>>(),
        &scored_entities.iter().map(|(s, e)| (Some(*s), *e)).collect::<Vec<_>>(),
        cep_entities,
    )
}

fn build_kg_block_ann(
    rules: &[agent_bestiary_memory::SemanticRule],
    episodic: &[agent_bestiary_memory::Entity],
    cep_entities: &[&agent_bestiary_memory::Entity],
) -> String {
    build_kg_block_inner(
        &rules.iter().map(|r| (None::<f32>, r)).collect::<Vec<_>>(),
        &episodic.iter().map(|e| (None::<f32>, e)).collect::<Vec<_>>(),
        cep_entities,
    )
}

fn build_kg_block_inner(
    rules: &[(Option<f32>, &agent_bestiary_memory::SemanticRule)],
    episodic: &[(Option<f32>, &agent_bestiary_memory::Entity)],
    cep_entities: &[&agent_bestiary_memory::Entity],
) -> String {
    let mut block = String::new();

    if !cep_entities.is_empty() {
        block.push_str(
            "\n\n## CEP Calibration Reference (from knowledge graph)\n\
             These are validated reference values seeded into your knowledge graph. \
             Treat them as authoritative priors unless you have stronger current evidence.\n",
        );
        let mut by_type: std::collections::BTreeMap<&str, Vec<_>> = std::collections::BTreeMap::new();
        for e in cep_entities {
            by_type.entry(e.entity_type.as_str()).or_default().push(*e);
        }
        for (etype, items) in &by_type {
            let header = etype.trim_start_matches("cep_").replace('_', " ");
            let header = {
                let mut c = header.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            };
            block.push_str(&format!("\n### {}\n", header));
            for e in items {
                let summary = e.summary.as_deref().unwrap_or("");
                if let Some(props) = &e.properties {
                    block.push_str(&format!("- **{}**: {} | data: {}\n", e.entity_name, summary, props));
                } else {
                    block.push_str(&format!("- **{}**: {}\n", e.entity_name, summary));
                }
            }
        }
    }

    if !rules.is_empty() || !episodic.is_empty() {
        block.push_str(
            "\n\n## Learned Knowledge (from past experience)\n\
             The following was distilled from your episodic memory during dream cycles. \
             Use it as context — prioritise your core instructions over these where they conflict.\n",
        );
        if !rules.is_empty() {
            block.push_str("\n### Learned Rules\n");
            for (score, rule) in rules {
                let score_str = score.map(|s| format!("({:.0}% match, ", s * 100.0)).unwrap_or_default();
                block.push_str(&format!(
                    "- {}{:.0}% confidence) {}\n",
                    score_str,
                    rule.confidence_score * 100.0,
                    rule.rule_content
                ));
            }
        }
        if !episodic.is_empty() {
            block.push_str("\n### Known Entities\n");
            for (score, entity) in episodic {
                let score_str = score.map(|s| format!("({:.0}% match) ", s * 100.0)).unwrap_or_default();
                let summary = entity.summary.as_deref().unwrap_or("no summary");
                block.push_str(&format!(
                    "- {}**{}** ({}): {}\n",
                    score_str, entity.entity_name, entity.entity_type, summary
                ));
            }
        }
    }
    block
}

/// Variant for call sites that only have the agent's string name (not its DB UUID).
/// Returns `(enriched_card, Option<query_embedding>)`.
pub async fn enrich_with_kg_context_by_name(
    memory_store: &Arc<MemoryStore>,
    embedder: &Arc<dyn EmbeddingGenerator>,
    db: &PgPool,
    agent_name: &str,
    query: &str,
    card: AgentCard,
) -> (AgentCard, Option<Vec<f32>>) {
    let uuid = match sqlx::query("SELECT agent_id FROM agents WHERE agent_name = $1")
        .bind(agent_name)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .and_then(|row| row.try_get::<Uuid, _>("agent_id").ok())
    {
        Some(u) => u,
        None => return (card, None),
    };
    enrich_with_kg_context(memory_store, embedder, uuid, query, card).await
}
