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

    // Fast-path: skip the external embedding call when there is nothing to
    // retrieve. Avoids a ~300-800ms API call that would find nothing.
    //
    // ## Why this asks the database and not the card
    //
    // This gate used to read `card.ontology_stats`, which is a field on the
    // agent card that essentially nothing maintains:
    //
    //   * cards reconstructed from a DB row hardcode `entities: 0`
    //     (`api_server.rs`, `db_agent_to_card`)
    //   * 31 of 100 curated card JSONs omit the block entirely, and every
    //     field is `#[serde(default)]`, so it deserialises to zero
    //   * the single code path that ever updated it counted
    //     `SELECT COUNT(*) FROM kg_entities` — a table that does not exist.
    //     The error was swallowed by `.ok().flatten().unwrap_or(0)`, so it
    //     wrote zero every time. Its own comment said it existed "so
    //     enrich_with_kg_context stops fast-pathing this agent".
    //
    // The consequence was that the gate was closed for virtually every agent,
    // permanently. Consolidation extracted entities and rules, stored them
    // correctly, and no execution ever read them back — Loop 1 wrote to memory
    // it could not consult. An agent with a hundred learned rules behaved
    // exactly like one that had never dreamed.
    //
    // So ask the tables. An indexed `EXISTS` costs well under a millisecond
    // against the hundreds we are deciding whether to spend, and it cannot
    // drift from the truth the way a denormalised counter can.
    match retrievable_knowledge(memory_store, agent_uuid).await {
        Retrievable::Nothing => return (card, None),
        Retrievable::PresentButUnembedded => {
            // Distinct from "nothing learned", and worth saying out loud.
            // Retrieval is embedding-based on both the ANN and the fallback
            // path, so a row with a NULL embedding is invisible to every
            // reader. The agent has knowledge it structurally cannot recall.
            tracing::warn!(
                agent_id = %agent_uuid,
                site = "kg_context_gate",
                "agent has knowledge rows but none carry embeddings — nothing is \
                 retrievable; backfill embeddings to close Loop 1 for this agent"
            );
            return (card, None);
        }
        Retrievable::Yes => {}
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
    let (top_rules, all_entities) =
        match try_ann_retrieval(memory_store, agent_uuid, &query_embedding).await {
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
                        r.embedding
                            .as_ref()
                            .map(|emb| (cosine_similarity(&query_embedding, emb), r))
                    })
                    .filter(|(s, _)| *s >= MIN_SIMILARITY)
                    .collect();
                scored_rules
                    .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                scored_rules.truncate(5);

                let (cep_entities, episodic_entities): (Vec<_>, Vec<_>) = entities
                    .iter()
                    .partition(|e| e.entity_type.starts_with("cep_"));

                let mut scored_episodic: Vec<(f32, _)> = episodic_entities
                    .iter()
                    .filter_map(|e| {
                        e.embedding
                            .as_ref()
                            .map(|emb| (cosine_similarity(&query_embedding, emb), *e))
                    })
                    .filter(|(s, _)| *s >= MIN_SIMILARITY)
                    .collect();
                scored_episodic
                    .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                scored_episodic.truncate(8);

                if scored_rules.is_empty() && scored_episodic.is_empty() && cep_entities.is_empty()
                {
                    return (card, Some(query_embedding));
                }

                // Build prompt block using legacy scored format
                let kg_block =
                    build_kg_block_scored(&scored_rules, &scored_episodic, &cep_entities);
                if !kg_block.is_empty() {
                    let base = card.system_prompt.unwrap_or_default();
                    card.system_prompt = Some(format!("{}{}", base, kg_block));
                    record_rule_retrievals(
                        memory_store,
                        scored_rules.iter().map(|(_, r)| r.rule_id).collect(),
                    );
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
    let injected = !kg_block.is_empty();
    if injected {
        card.system_prompt = Some(append_kg_block(card.system_prompt.take(), &kg_block));
        record_rule_retrievals(memory_store, top_rules.iter().map(|r| r.rule_id).collect());
    }

    // Record what actually reached the prompt, not merely that we tried.
    //
    // The previous log fired identically whether the agent received a hundred
    // recalled rules or nothing at all, which made "is retrieval working?"
    // unanswerable from logs — the question that mattered most while the gate
    // was silently closed for every agent. These fields make an execution's
    // recall auditable after the fact.
    tracing::info!(
        elapsed_ms = t_total.elapsed().as_millis() as u64,
        agent_id = %agent_uuid,
        injected,
        rules = top_rules.len(),
        episodic_entities = episodic_owned.len(),
        cep_entities = cep_entities.len(),
        block_chars = kg_block.len(),
        "kg_context_enrich"
    );

    (card, Some(query_embedding))
}

/// Mark rules as having actually reached a prompt.
///
/// ## Why retrieval is the resolution event
///
/// A rule's value is not whether it looked plausible when it was written — that
/// is the judgement of the same model that wrote it — but whether it later
/// turned out to be worth recalling. Retrieval is the first moment that becomes
/// observable, and it is exactly analogous to a forecast resolving: delayed,
/// outcome-based, and not available at write time.
///
/// This is what gives the extractor a signal. `semantic_rules.extracted_by`
/// (migration 201) says who wrote a rule; `application_count` says whether the
/// platform ever wanted it back. Together they answer "how good is the
/// ontologist at extraction?", which nothing could answer before.
///
/// ## Why the counters were already there
///
/// `application_count` and `last_validated_at` have existed since migration 010
/// and had **zero** non-test references in the codebase — declared, never
/// written, never read. The schema anticipated this signal and nothing ever
/// populated it. This is the missing write.
///
/// ## Off the hot path, deliberately
///
/// `enrich_with_kg_context` runs on every execution and its latency is already
/// dominated by an embedding call. Bookkeeping must not add to that, and must
/// never fail a run: the update is spawned and its result logged, not awaited
/// and not propagated. A lost increment slightly understates a rule's utility;
/// a blocked execution is a user-visible outage.
fn record_rule_retrievals(memory_store: &Arc<MemoryStore>, rule_ids: Vec<Uuid>) {
    if rule_ids.is_empty() {
        return;
    }
    let store = memory_store.clone();
    tokio::spawn(async move {
        // `last_validated_at` doubles as "first seen useful": a rule never
        // retrieved keeps NULL, which is how the utility query tells "unused"
        // from "used once, long ago".
        let res = sqlx::query(
            "UPDATE semantic_rules
                SET application_count = application_count + 1,
                    last_validated_at = NOW()
              WHERE rule_id = ANY($1)",
        )
        .bind(&rule_ids)
        .execute(store.pool())
        .await;

        match res {
            Ok(r) if r.rows_affected() as usize != rule_ids.len() => {
                // Retrieved a rule that no longer exists. Worth a line: it
                // means a reader served knowledge that has since been deleted.
                tracing::warn!(
                    retrieved = rule_ids.len(),
                    updated = r.rows_affected(),
                    "kg_retrieval_credit_partial"
                );
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(
                error = %e,
                rules = rule_ids.len(),
                "kg_retrieval_credit_failed — extraction utility will understate these rules"
            ),
        }
    });
}

/// Try pgvector ANN retrieval. Returns None if HNSW indices aren't ready yet.
/// What the knowledge tables can actually serve for an agent.
#[derive(Debug, PartialEq, Eq)]
enum Retrievable {
    /// No entities and no active rules. A new agent, or one that has never
    /// consolidated. Skipping is correct and cheap.
    Nothing,
    /// Rows exist, but none carry an embedding, so no reader can reach them.
    /// Distinguished from `Nothing` because it is a defect, not a lifecycle
    /// stage — something wrote knowledge without embedding it.
    PresentButUnembedded,
    /// At least one embedded entity or active rule. Worth paying for a query
    /// embedding.
    Yes,
}

/// Single indexed round-trip answering "is there anything to retrieve?".
///
/// Embedded rows count because that is what similarity retrieval requires: the
/// ANN path matches on the `embedding` column and the load-all fallback does
/// `filter_map(|r| r.embedding.as_ref())`.
///
/// `cep_*` entities count **without** an embedding. They are seed reference
/// data — `get_top_k_entities_with_cep` returns them via a `UNION ALL` branch
/// that is neither similarity-gated nor embedding-filtered, so they are always
/// injected. Requiring an embedding of them would suppress the one class of
/// knowledge that is deliberately stored without one.
async fn retrievable_knowledge(store: &Arc<MemoryStore>, agent_id: Uuid) -> Retrievable {
    let row = sqlx::query(
        r#"
        SELECT
          EXISTS(SELECT 1 FROM entities
                  WHERE agent_id = $1
                    AND (t_invalid IS NULL OR t_invalid > NOW())) AS any_entity,
          EXISTS(SELECT 1 FROM semantic_rules
                  WHERE agent_id = $1 AND is_active)              AS any_rule,
          EXISTS(SELECT 1 FROM entities
                  WHERE agent_id = $1 AND embedding IS NOT NULL
                    AND (t_invalid IS NULL OR t_invalid > NOW())) AS embedded_entity,
          EXISTS(SELECT 1 FROM semantic_rules
                  WHERE agent_id = $1 AND is_active
                    AND embedding IS NOT NULL)                    AS embedded_rule,
          EXISTS(SELECT 1 FROM entities
                  WHERE agent_id = $1 AND entity_type LIKE 'cep\_%'
                    AND (t_invalid IS NULL OR t_invalid > NOW())) AS any_cep
        "#,
    )
    .bind(agent_id)
    .fetch_optional(store.pool())
    .await;

    let Ok(Some(row)) = row else {
        // A failed probe must not silently disable learning. Assume there is
        // something to retrieve and let the real query decide — the cost of
        // being wrong is one embedding call, and the cost of the opposite
        // default is the bug this function was written to fix.
        return Retrievable::Yes;
    };

    classify_retrievable(
        row.get::<bool, _>("any_entity") || row.get::<bool, _>("any_rule"),
        row.get::<bool, _>("embedded_entity") || row.get::<bool, _>("embedded_rule"),
        row.get::<bool, _>("any_cep"),
    )
}

/// Append the retrieved-knowledge block to an agent's system prompt.
///
/// Trivial, and extracted anyway: this single concatenation is the whole
/// mechanism by which everything Loop 1 learns reaches the model. From here it
/// travels `card.system_prompt` → `ExecutionContext.agent_card` →
/// `LlmExecutor::build_system_prompt` → the `system` field of the provider
/// request. If this appends to the wrong thing, or the caller drops the
/// returned card, every embedding on the platform is dead weight.
fn append_kg_block(system_prompt: Option<String>, block: &str) -> String {
    format!("{}{}", system_prompt.unwrap_or_default(), block)
}

/// Decide what the counts mean. Split out so the semantics are pinned by test
/// rather than by a DB fixture.
fn classify_retrievable(any_rows: bool, any_embedded: bool, any_cep: bool) -> Retrievable {
    match (any_rows, any_embedded || any_cep) {
        (_, true) => Retrievable::Yes,
        (true, false) => Retrievable::PresentButUnembedded,
        (false, false) => Retrievable::Nothing,
    }
}

#[cfg(test)]
mod gate_tests {
    use super::*;

    /// The gate that broke Loop 1. An agent with learned knowledge must not be
    /// classified as having nothing to retrieve — that is what a stale
    /// `ontology_stats` counter did for every agent on the platform.
    #[test]
    fn embedded_knowledge_opens_the_gate() {
        assert_eq!(classify_retrievable(true, true, false), Retrievable::Yes);
    }

    /// A genuinely new agent. Skipping is correct and saves a 300-800ms call.
    #[test]
    fn no_knowledge_closes_the_gate() {
        assert_eq!(
            classify_retrievable(false, false, false),
            Retrievable::Nothing
        );
    }

    /// The state worth naming: rows exist, none are reachable. Retrieval is
    /// embedding-based on both the ANN and fallback paths, so an unembedded
    /// row is invisible — paying for a query embedding would find nothing.
    /// Distinguished from `Nothing` so it can be logged as the defect it is.
    #[test]
    fn unembedded_knowledge_is_distinguished_from_none() {
        assert_eq!(
            classify_retrievable(true, false, false),
            Retrievable::PresentButUnembedded
        );
        assert_ne!(
            classify_retrievable(true, false, false),
            classify_retrievable(false, false, false),
            "an agent that learned but cannot recall is not the same as a new agent"
        );
    }

    /// CEP seed entities are deliberately stored without embeddings and are
    /// injected unconditionally by `get_top_k_entities_with_cep`'s second
    /// UNION branch. Requiring an embedding of them would suppress the only
    /// knowledge class designed not to have one — on this deployment that is
    /// 107 rows across agents like `biotech_analyst`, whose entire ontology is
    /// CEP seed data.
    use agent_bestiary_memory::{Entity, SemanticRule};
    use chrono::Utc;

    fn rule(content: &str) -> SemanticRule {
        SemanticRule {
            rule_id: Uuid::new_v4(),
            agent_id: Uuid::nil(),
            rule_content: content.to_string(),
            rule_description: None,
            confidence_score: 0.9,
            verification_status: agent_bestiary_memory::VerificationStatus::Pending,
            verification_method: None,
            source_episode_cluster: vec![],
            episode_count: 3,
            embedding: None,
            is_active: true,
            created_at: Utc::now(),
            extracted_by: None,
            provenance_floor: None,
            provenance_floor_basis: None,
        }
    }

    fn seed_entity(name: &str, etype: &str, summary: &str) -> Entity {
        Entity {
            entity_id: Uuid::new_v4(),
            agent_id: Uuid::nil(),
            entity_name: name.to_string(),
            entity_type: etype.to_string(),
            summary: Some(summary.to_string()),
            t_valid: Utc::now(),
            t_invalid: None,
            source_episodes: vec![],
            extraction_confidence: 0.85,
            embedding: None,
            properties: None,
        }
    }

    /// The end of the chain, and the reason embeddings are worth generating.
    ///
    /// Retrieved knowledge is only useful if its *text* reaches the model. This
    /// asserts the actual rule content and entity names appear in the block —
    /// not merely that a block was produced. A block that renders headings and
    /// drops the content would satisfy every count-based check while teaching
    /// the agent nothing.
    #[test]
    fn retrieved_knowledge_reaches_the_prompt_text() {
        let r = rule("kombucha_fermentation overestimates yield above 65C");
        let learned = seed_entity("Kelly criterion", "concept", "bet sizing rule");
        let cep = seed_entity(
            "AFC confederation strength",
            "cep_base_rate",
            "coefficient 0.82",
        );

        let block = build_kg_block_inner(&[(None, &r)], &[(None, &learned)], &[&cep]);

        assert!(
            block.contains("kombucha_fermentation overestimates yield above 65C"),
            "learned rule content must reach the prompt, got: {block}"
        );
        assert!(
            block.contains("Kelly criterion"),
            "retrieved entity name must reach the prompt, got: {block}"
        );
        assert!(
            block.contains("AFC confederation strength"),
            "CEP seed must reach the prompt, got: {block}"
        );

        // And the block must survive the append onto an existing prompt.
        let enriched = append_kg_block(Some("You are a forecaster.".into()), &block);
        assert!(
            enriched.starts_with("You are a forecaster."),
            "base prompt preserved"
        );
        assert!(
            enriched.contains("kombucha_fermentation overestimates yield above 65C"),
            "knowledge must survive the append into system_prompt"
        );
    }

    /// An agent with no prior system prompt must still receive its knowledge.
    #[test]
    fn append_works_without_a_base_prompt() {
        let out = append_kg_block(None, "\n\n## Learned Knowledge\n- something");
        assert!(out.contains("## Learned Knowledge"));
        assert!(out.contains("- something"));
    }

    // ─── the prompt must not launder a rule ───
    //
    // This is the boundary where a stored claim becomes another agent's
    // premise. Everything upstream — the floor column, the oracle, the
    // ceiling — exists to reach these four tests, and if the rendering drops
    // the floor then all of it is bookkeeping nobody reads.

    fn rule_with_floor(content: &str, floor: Option<&str>) -> SemanticRule {
        let mut r = rule(content);
        r.provenance_floor = floor.map(|s| s.to_string());
        r
    }

    /// The invariant, stated over every value the vocabulary permits plus the
    /// unknown case, so it holds by exhaustion rather than by inspection of
    /// the branches someone remembered to check.
    #[test]
    fn no_rule_can_ever_render_as_tool_verified() {
        let mut floors: Vec<Option<&str>> = vec![None];
        floors.extend(
            crate::grounding_trust::PROVENANCE_VALUES
                .iter()
                .map(|v| Some(*v)),
        );

        for floor in floors {
            let r = rule_with_floor("Aeshna cyanea preys on Lepidoptera", floor);
            let block = build_kg_block_inner(&[(Some(0.9), &r)], &[], &[]);
            let lower = block.to_lowercase();
            for needle in ["tool_verified", "tool-verified", "verified", "measured"] {
                assert!(
                    !lower.contains(needle),
                    "floor {floor:?} rendered a rule containing `{needle}`. A rule \
                     cannot be tool-verified — EXTRACTION_CEILING is \
                     model_inference — so any wording a model could read that way \
                     reports a fact about the sources as a fact about the rule.\n{block}"
                );
            }
        }
    }

    /// The number that used to carry all the weight must not read as
    /// calibration. `confidence_score` is the extraction model rating its own
    /// output; labelling it plain "confidence" next to a similarity score is
    /// what made the block persuasive.
    #[test]
    fn the_self_report_is_labelled_as_a_self_report() {
        let r = rule_with_floor("something", Some("model_inference"));
        let block = build_kg_block_inner(&[(None, &r)], &[], &[]);
        assert!(
            block.contains("self-rated"),
            "the model\'s own confidence must be named as such, got: {block}"
        );
    }

    /// Known-bad and unknown must be distinguishable in the prompt, because
    /// the remedy differs: one rule should be retracted, the other is waiting
    /// on retention and contracts. Collapsing them would make the honest
    /// state of the corpus unreadable to the agent and to us.
    #[test]
    fn ungrounded_and_unknown_are_not_the_same_word() {
        let bad = rule_with_floor("x", Some("unavailable_no_tool_source"));
        let unknown = rule_with_floor("x", None);
        let a = build_kg_block_inner(&[(None, &bad)], &[], &[]);
        let b = build_kg_block_inner(&[(None, &unknown)], &[], &[]);
        assert_ne!(a, b, "absence is not a verdict, and must not print as one");
        assert!(a.to_uppercase().contains("UNGROUNDED"), "{a}");
        assert!(b.contains("grounding unknown"), "{b}");
    }

    /// The floor is worthless if the reading model is not told what to do
    /// about it. A label it cannot act on is decoration.
    #[test]
    fn the_block_tells_the_model_not_to_cite_an_ungrounded_rule() {
        let r = rule_with_floor("x", None);
        let block = build_kg_block_inner(&[(None, &r)], &[], &[]);
        assert!(
            block.contains("must not cite it as established"),
            "the rules section must carry its own reading instructions: {block}"
        );
    }

    /// Nothing retrieved must produce nothing appended, so an empty recall
    /// cannot silently pad every prompt with an empty heading.
    #[test]
    fn empty_retrieval_produces_no_block() {
        assert!(build_kg_block_inner(&[], &[], &[]).is_empty());
    }

    #[test]
    fn cep_seeds_open_the_gate_without_embeddings() {
        assert_eq!(
            classify_retrievable(true, false, true),
            Retrievable::Yes,
            "an agent holding only CEP seeds has retrievable knowledge"
        );
    }
}

async fn try_ann_retrieval(
    store: &Arc<MemoryStore>,
    agent_id: Uuid,
    query_embedding: &[f32],
) -> Option<(
    Vec<agent_bestiary_memory::SemanticRule>,
    Vec<agent_bestiary_memory::Entity>,
)> {
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
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

fn build_kg_block_scored(
    scored_rules: &[(f32, &agent_bestiary_memory::SemanticRule)],
    scored_entities: &[(f32, &agent_bestiary_memory::Entity)],
    cep_entities: &[&agent_bestiary_memory::Entity],
) -> String {
    build_kg_block_inner(
        &scored_rules
            .iter()
            .map(|(s, r)| (Some(*s), *r))
            .collect::<Vec<_>>(),
        &scored_entities
            .iter()
            .map(|(s, e)| (Some(*s), *e))
            .collect::<Vec<_>>(),
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
        &episodic
            .iter()
            .map(|e| (None::<f32>, e))
            .collect::<Vec<_>>(),
        cep_entities,
    )
}

/// How a learned rule was grounded, in words the reading model will act on.
///
/// # Why the prompt has to say this
///
/// The line above this function used to read `- (72% match, 90% confidence)
/// <rule>`. Both numbers are real and neither is a measurement.
/// `confidence_score` is the extraction model's own self-report about a
/// generalisation it had just written, and `match` is cosine similarity
/// between two embeddings. Rendered side by side and labelled "confidence",
/// they read as calibration — to a model, `90% confidence` is a strong signal
/// to assert the content downstream.
///
/// That is the last step of a laundering path, and it is the step that
/// matters, because it is where the claim leaves the database and enters
/// another agent's reasoning. A rule extracted from ten paragraphs of prose
/// arrives in the prompt looking exactly like one extracted from ten tool
/// calls. Worse than a bare hallucination: the citation is real, because
/// `source_episode_cluster` genuinely points at episodes that genuinely said
/// that.
///
/// # Why never "verified"
///
/// No branch returns anything a model could read as tool-backed, and that is
/// not caution, it is arithmetic: `EXTRACTION_CEILING` is `model_inference`,
/// so a rule *cannot* hold `tool_verified` however well-sourced its episodes
/// were. Reading well-grounded episodes and writing a generalisation about
/// them is judgement, and judgement does not inherit retrieval. A rule
/// claiming otherwise would be reporting a fact about its sources as a fact
/// about itself.
///
/// Guarded by `no_rule_can_ever_render_as_tool_verified`.
fn grounding_note(rule: &agent_bestiary_memory::SemanticRule) -> &'static str {
    use crate::grounding_trust::{PROV_INFERRED, PROV_NO_MATCH, PROV_UNAVAILABLE};
    match rule.provenance_floor.as_deref() {
        // The best an extracted rule can be: reasoned from evidence something
        // could actually check.
        Some(PROV_INFERRED) => "inferred from sourced evidence",
        // Known bad. The episodes it came from asserted things no tool could
        // supply, so the rule inherits nothing to stand on.
        Some(PROV_UNAVAILABLE) | Some(PROV_NO_MATCH) => "UNGROUNDED - no tool could confirm this",
        // Unknown, and it must not read as either of the above. Distinct
        // wording because the remedy is different: retention and contracts,
        // not retracting the rule.
        None => "grounding unknown",
        // Any other value is a vocabulary the runtime has grown without
        // updating this function. Refuse to characterise it rather than
        // guessing upward; `PROVENANCE_VALUES` is closed and tested, so this
        // is reachable only mid-change.
        Some(_) => "grounding unrecognised - treat as unknown",
    }
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
        let mut by_type: std::collections::BTreeMap<&str, Vec<_>> =
            std::collections::BTreeMap::new();
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
                    block.push_str(&format!(
                        "- **{}**: {} | data: {}\n",
                        e.entity_name, summary, props
                    ));
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
            block.push_str(
                "\n### Learned Rules\n\
                 Each rule carries how it was grounded. A rule marked ungrounded \
                 or unknown was distilled from text that no tool could confirm; \
                 it is a hypothesis, and you must not cite it as established.\n",
            );
            for (score, rule) in rules {
                let score_str = score
                    .map(|s| format!("{:.0}% match, ", s * 100.0))
                    .unwrap_or_default();
                block.push_str(&format!(
                    "- ({}{:.0}% self-rated, {}) {}\n",
                    score_str,
                    rule.confidence_score * 100.0,
                    grounding_note(rule),
                    rule.rule_content
                ));
            }
        }
        if !episodic.is_empty() {
            block.push_str("\n### Known Entities\n");
            for (score, entity) in episodic {
                let score_str = score
                    .map(|s| format!("({:.0}% match) ", s * 100.0))
                    .unwrap_or_default();
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
