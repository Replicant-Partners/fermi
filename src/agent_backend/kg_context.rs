//! KG context injection — enrich agent system prompts with semantically
//! relevant knowledge from past dream cycles before any execution.
//!
//! Called at every agent execution entry point so the model can reason
//! over what it has already learned, not just its static system prompt.

use crate::agent_backend::agent_card::AgentCard;
use agent_bestiary_memory::{EmbeddingGenerator, MemoryStore};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Append relevant KG context to the agent card's system prompt.
///
/// Embeds `query`, finds the top semantically similar rules and entities from
/// the agent's KG (populated by dream cycles), and appends them as a
/// "Learned Knowledge" section. Returns the card unchanged if the KG is empty
/// or nothing scores above the 30% cosine similarity threshold.
pub async fn enrich_with_kg_context(
    memory_store: &Arc<MemoryStore>,
    embedder: &Arc<dyn EmbeddingGenerator>,
    agent_uuid: Uuid,
    query: &str,
    mut card: AgentCard,
) -> AgentCard {
    // Fast-path: skip the external embedding call entirely when the agent's
    // knowledge graph is empty (no dream cycles have run yet). This avoids
    // a ~300-800ms external API call that would find nothing and return the
    // card unchanged — the common case for creature agents.
    if card.ontology_stats.entities == 0 && card.ontology_stats.relationships == 0 {
        return card;
    }

    let (rules_res, entities_res) = tokio::join!(
        memory_store.get_agent_semantic_rules(agent_uuid),
        memory_store.get_agent_entities(agent_uuid),
    );

    let rules = rules_res.unwrap_or_default();
    let entities = entities_res.unwrap_or_default();

    if rules.is_empty() && entities.is_empty() {
        return card;
    }

    let query_embedding = match embedder.generate(query).await {
        Ok(e) => e,
        Err(_) => return card,
    };

    const MIN_SIMILARITY: f32 = 0.30;

    let mut scored_rules: Vec<(f32, _)> = rules
        .iter()
        .filter_map(|r| {
            r.embedding
                .as_ref()
                .map(|emb| (cosine_similarity(&query_embedding, emb), r))
        })
        .filter(|(s, _)| *s >= MIN_SIMILARITY)
        .collect();
    scored_rules.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored_rules.truncate(5);

    // Partition: CEP seed entities are reference data (always included); others are episodic.
    let (cep_entities, episodic_entities): (Vec<_>, Vec<_>) = entities
        .iter()
        .partition(|e| e.entity_type.starts_with("cep_"));

    let mut scored_entities: Vec<(f32, _)> = episodic_entities
        .iter()
        .filter_map(|e| {
            e.embedding
                .as_ref()
                .map(|emb| (cosine_similarity(&query_embedding, emb), *e))
        })
        .filter(|(s, _)| *s >= MIN_SIMILARITY)
        .collect();
    scored_entities.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored_entities.truncate(8);

    if scored_rules.is_empty() && scored_entities.is_empty() && cep_entities.is_empty() {
        return card;
    }

    let mut kg_block = String::new();

    // ── CEP calibration reference (structured, not similarity-gated) ──────────
    if !cep_entities.is_empty() {
        kg_block.push_str(
            "\n\n## CEP Calibration Reference (from knowledge graph)\n\
             These are validated reference values seeded into your knowledge graph. \
             Treat them as authoritative priors unless you have stronger current evidence.\n",
        );

        // Group by entity_type for readable output.
        let mut by_type: std::collections::BTreeMap<&str, Vec<_>> =
            std::collections::BTreeMap::new();
        for e in &cep_entities {
            by_type.entry(e.entity_type.as_str()).or_default().push(e);
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
            kg_block.push_str(&format!("\n### {}\n", header));
            for e in items.iter() {
                let summary = e.summary.as_deref().unwrap_or("");
                if let Some(props) = &e.properties {
                    kg_block.push_str(&format!(
                        "- **{}**: {} | data: {}\n",
                        e.entity_name,
                        summary,
                        props
                    ));
                } else {
                    kg_block.push_str(&format!("- **{}**: {}\n", e.entity_name, summary));
                }
            }
        }
    }

    // ── Episodic knowledge (similarity-gated) ────────────────────────────────
    if !scored_rules.is_empty() || !scored_entities.is_empty() {
        kg_block.push_str(
            "\n\n## Learned Knowledge (from past experience)\n\
             The following was distilled from your episodic memory during dream cycles. \
             Use it as context — prioritise your core instructions over these where they conflict.\n",
        );

        if !scored_rules.is_empty() {
            kg_block.push_str("\n### Learned Rules\n");
            for (score, rule) in &scored_rules {
                kg_block.push_str(&format!(
                    "- ({:.0}% match, {:.0}% confidence) {}\n",
                    score * 100.0,
                    rule.confidence_score * 100.0,
                    rule.rule_content
                ));
            }
        }

        if !scored_entities.is_empty() {
            kg_block.push_str("\n### Known Entities\n");
            for (score, entity) in &scored_entities {
                let summary = entity.summary.as_deref().unwrap_or("no summary");
                kg_block.push_str(&format!(
                    "- ({:.0}% match) **{}** ({}): {}\n",
                    score * 100.0,
                    entity.entity_name,
                    entity.entity_type,
                    summary
                ));
            }
        }
    }

    let base = card.system_prompt.unwrap_or_default();
    card.system_prompt = Some(format!("{}{}", base, kg_block));
    card
}

/// Variant for call sites that only have the agent's string name (not its DB UUID).
/// Looks up the UUID, then delegates to `enrich_with_kg_context`.
/// Returns the card unchanged if the DB lookup fails.
pub async fn enrich_with_kg_context_by_name(
    memory_store: &Arc<MemoryStore>,
    embedder: &Arc<dyn EmbeddingGenerator>,
    db: &PgPool,
    agent_name: &str,
    query: &str,
    card: AgentCard,
) -> AgentCard {
    let uuid = match sqlx::query("SELECT agent_id FROM agents WHERE agent_name = $1")
        .bind(agent_name)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .and_then(|row| row.try_get::<Uuid, _>("agent_id").ok())
    {
        Some(u) => u,
        None => return card,
    };
    enrich_with_kg_context(memory_store, embedder, uuid, query, card).await
}
