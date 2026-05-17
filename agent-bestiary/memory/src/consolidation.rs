//! Consolidation workflow orchestration
//!
//! This module implements the Active Dreaming Memory consolidation process:
//! 1. Acquire lock for agent
//! 2. Fetch unconsolidated episodes
//! 3. Cluster failure episodes using DBSCAN
//! 4. Extract semantic rules from clusters
//! 5. Extract entities and facts from episodes
//! 6. Store consolidated knowledge
//! 7. Mark episodes as consolidated
//! 8. Update job statistics

use crate::{
    generate_structured, Cardinality, ConsolidationLock, DBSCANClustering, EmbeddingGenerator,
    Entity, Episode, EpisodeCluster, ExecutionStatus, Fact, GenerationConfig, LLMProvider,
    MemoryError, MemoryStore, Message, MessageRole, Result, SemanticRule, VerificationStatus,
};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

/// Consolidation workflow orchestrator
pub struct ConsolidationWorker {
    store: Arc<MemoryStore>,
    lock: Arc<ConsolidationLock>,
    embedder: Arc<dyn EmbeddingGenerator>,
    llm: Option<Arc<dyn LLMProvider>>,
    #[allow(dead_code)]
    worker_id: String,
}

impl ConsolidationWorker {
    /// Creates a new consolidation worker
    pub fn new(
        store: Arc<MemoryStore>,
        lock: Arc<ConsolidationLock>,
        embedder: Arc<dyn EmbeddingGenerator>,
        worker_id: String,
    ) -> Self {
        Self {
            store,
            lock,
            embedder,
            llm: None,
            worker_id,
        }
    }

    /// Creates a new consolidation worker with LLM support
    pub fn with_llm(
        store: Arc<MemoryStore>,
        lock: Arc<ConsolidationLock>,
        embedder: Arc<dyn EmbeddingGenerator>,
        llm: Arc<dyn LLMProvider>,
        worker_id: String,
    ) -> Self {
        Self {
            store,
            lock,
            embedder,
            llm: Some(llm),
            worker_id,
        }
    }

    /// Runs consolidation for a specific agent
    pub async fn consolidate_agent(
        &self,
        agent_id: Uuid,
        epsilon: f64,
        min_samples: usize,
    ) -> Result<ConsolidationResult> {
        // Step 1: Acquire lock
        let acquired = self.lock.acquire(agent_id, 30).await?;
        if !acquired {
            return Err(MemoryError::LockUnavailable(format!(
                "Could not acquire lock for agent {}",
                agent_id
            )));
        }

        // Ensure lock is released even if we error
        let result = self
            .consolidate_agent_internal(agent_id, epsilon, min_samples)
            .await;

        // Release lock
        self.lock.release(agent_id).await?;

        result
    }

    async fn consolidate_agent_internal(
        &self,
        agent_id: Uuid,
        epsilon: f64,
        min_samples: usize,
    ) -> Result<ConsolidationResult> {
        // Step 2: Fetch unconsolidated episodes
        let episodes = self.store.get_unconsolidated_episodes(agent_id).await?;

        if episodes.is_empty() {
            return Ok(ConsolidationResult::default());
        }

        let episode_ids: Vec<Uuid> = episodes.iter().map(|e| e.episode_id).collect();

        // Step 3: Create consolidation job
        let job_id = self
            .store
            .create_consolidation_job(agent_id, episode_ids[0], episode_ids[episode_ids.len() - 1])
            .await?;

        // Step 4: Cluster failure episodes
        let failure_episodes: Vec<Episode> = episodes
            .iter()
            .filter(|e| matches!(e.execution_status, crate::ExecutionStatus::Failure))
            .cloned()
            .collect();

        let mut clusters = Vec::new();
        if !failure_episodes.is_empty() {
            let clusterer = DBSCANClustering::new(epsilon, min_samples);
            clusters = clusterer.cluster(failure_episodes)?;
        }

        let mut result = ConsolidationResult {
            episodes_processed: episodes.len(),
            clusters_identified: clusters.len(),
            rules_extracted: 0,
            rules_verified: 0,
            rules_rejected: 0,
            entities_created: 0,
            facts_created: 0,
        };

        // Step 5a: Extract semantic rules from failure clusters
        for cluster in &clusters {
            let rules = self.extract_rules_from_cluster(agent_id, cluster).await?;
            result.rules_extracted += rules.len();

            for rule in rules {
                self.store.store_semantic_rule(rule).await?;
            }
        }

        // Step 5b: Extract knowledge rules from successful episodes (LLM only)
        if let Some(llm) = &self.llm {
            let success_episodes: Vec<&Episode> = episodes
                .iter()
                .filter(|e| matches!(e.execution_status, ExecutionStatus::Success))
                .take(30)
                .collect();

            if !success_episodes.is_empty() {
                match self
                    .extract_knowledge_rules(agent_id, &success_episodes, llm)
                    .await
                {
                    Ok(knowledge_rules) => {
                        result.rules_extracted += knowledge_rules.len();
                        for rule in knowledge_rules {
                            self.store.store_semantic_rule(rule).await?;
                        }
                    }
                    Err(e) => {
                        eprintln!("Knowledge rule extraction failed (non-fatal): {}", e);
                    }
                }
            }
        }

        // Step 6: Extract entities from episodes
        let entities_stored = if let Some(llm) = &self.llm {
            match self
                .extract_entities_with_llm(agent_id, &episodes, llm)
                .await
            {
                Ok(entities) => {
                    result.entities_created = entities.len();
                    let mut stored = Vec::new();
                    for entity in entities {
                        self.store.store_entity(entity.clone()).await?;
                        stored.push(entity);
                    }
                    stored
                }
                Err(e) => {
                    eprintln!(
                        "LLM entity extraction failed, falling back to heuristic: {}",
                        e
                    );
                    let mut stored = Vec::new();
                    for episode in episodes.iter().take(100) {
                        let entities = self
                            .extract_entities_from_episode(agent_id, episode)
                            .await?;
                        result.entities_created += entities.len();
                        for entity in entities {
                            self.store.store_entity(entity.clone()).await?;
                            stored.push(entity);
                        }
                    }
                    stored
                }
            }
        } else {
            let mut stored = Vec::new();
            for episode in episodes.iter().take(100) {
                let entities = self
                    .extract_entities_from_episode(agent_id, episode)
                    .await?;
                result.entities_created += entities.len();
                for entity in entities {
                    self.store.store_entity(entity.clone()).await?;
                    stored.push(entity);
                }
            }
            stored
        };

        // Step 6b: Extract facts (relationships) between entities (LLM only)
        if let Some(llm) = &self.llm {
            if entities_stored.len() >= 2 {
                match self
                    .extract_facts_with_llm(agent_id, &entities_stored, &episodes, llm)
                    .await
                {
                    Ok(facts) => {
                        result.facts_created = facts.len();
                        for fact in facts {
                            self.store.store_fact(fact).await?;
                        }
                    }
                    Err(e) => {
                        eprintln!("Fact extraction failed (non-fatal): {}", e);
                    }
                }
            }
        }

        // Step 7: Mark episodes as consolidated
        self.store
            .mark_episodes_consolidated(&episode_ids, job_id)
            .await?;

        // Step 8: Update job statistics
        self.store
            .update_consolidation_job(
                job_id,
                result.episodes_processed as i32,
                result.clusters_identified as i32,
                result.rules_extracted as i32,
                result.rules_verified as i32,
                result.rules_rejected as i32,
                result.entities_created as i32,
                result.facts_created as i32,
            )
            .await?;

        // Step 9: Complete job
        self.store
            .complete_consolidation_job(job_id, "completed", None)
            .await?;

        Ok(result)
    }

    /// Extracts semantic rules from an episode cluster
    async fn extract_rules_from_cluster(
        &self,
        agent_id: Uuid,
        cluster: &EpisodeCluster,
    ) -> Result<Vec<SemanticRule>> {
        let episode_ids: Vec<Uuid> = cluster.episodes.iter().map(|e| e.episode_id).collect();

        // Use LLM if available, otherwise fall back to pattern-based extraction
        if let Some(llm) = &self.llm {
            self.extract_rules_with_llm(agent_id, cluster, &episode_ids, llm)
                .await
        } else {
            self.extract_rules_pattern_based(agent_id, cluster, &episode_ids)
                .await
        }
    }

    /// LLM-powered rule extraction
    async fn extract_rules_with_llm(
        &self,
        agent_id: Uuid,
        cluster: &EpisodeCluster,
        episode_ids: &[Uuid],
        llm: &Arc<dyn LLMProvider>,
    ) -> Result<Vec<SemanticRule>> {
        let mut rules = Vec::new();

        // Prepare cluster summary for LLM
        let error_messages: Vec<String> = cluster
            .episodes
            .iter()
            .filter_map(|e| e.error_details.clone())
            .take(10) // Limit to avoid token overflow
            .collect();

        let queries: Vec<String> = cluster
            .episodes
            .iter()
            .map(|e| e.query.clone())
            .take(10)
            .collect();

        if error_messages.is_empty() {
            return Ok(rules);
        }

        // Build prompt for LLM
        let system_prompt = "You are an expert at analyzing failure patterns in AI agent execution logs. \
            Your task is to identify common patterns, root causes, and actionable rules from clusters of failed episodes. \
            Generate 1-3 concise, actionable semantic rules that capture the essence of the failure pattern. \
            Each rule should be a clear statement about what went wrong and ideally suggest how to avoid it.";

        let user_prompt = format!(
            "Analyze this cluster of {} failed episodes and extract semantic rules:\n\n\
            Sample Queries:\n{}\n\n\
            Sample Errors:\n{}\n\n\
            Generate 1-3 semantic rules in JSON format:\n\
            [{{\n  \
              \"rule\": \"<concise rule statement>\",\n  \
              \"description\": \"<detailed explanation>\",\n  \
              \"confidence\": <0.0-1.0>\n\
            }}]",
            cluster.episodes.len(),
            queries.join("\n"),
            error_messages.join("\n")
        );

        let messages = vec![
            Message {
                role: MessageRole::System,
                content: system_prompt.to_string(),
            },
            Message {
                role: MessageRole::User,
                content: user_prompt,
            },
        ];

        let config = GenerationConfig {
            temperature: 0.3, // Lower temperature for more consistent analysis
            max_tokens: Some(2048),
            ..Default::default()
        };

        // Define expected structure
        #[derive(serde::Deserialize)]
        struct LLMRule {
            rule: String,
            description: String,
            confidence: f64,
        }

        // Call LLM with structured output (automatic parsing + graceful degradation)
        let llm_rules: Vec<LLMRule> = generate_structured(llm.as_ref(), messages, &config).await?;

        // Convert to SemanticRule objects
        for llm_rule in llm_rules {
            let embedding = self.embedder.generate(&llm_rule.rule).await.ok();

            let rule = SemanticRule {
                rule_id: Uuid::new_v4(),
                agent_id,
                rule_content: llm_rule.rule,
                rule_description: Some(llm_rule.description),
                confidence_score: llm_rule.confidence.clamp(0.0, 1.0),
                verification_status: VerificationStatus::Pending,
                verification_method: Some(format!("llm_extraction:{}", llm.model_name())),
                source_episode_cluster: episode_ids.to_vec(),
                episode_count: cluster.episodes.len() as i32,
                embedding,
                is_active: true,
                created_at: chrono::Utc::now(),
            };

            rules.push(rule);
        }

        Ok(rules)
    }

    /// Pattern-based rule extraction (fallback)
    async fn extract_rules_pattern_based(
        &self,
        agent_id: Uuid,
        cluster: &EpisodeCluster,
        episode_ids: &[Uuid],
    ) -> Result<Vec<SemanticRule>> {
        let mut rules = Vec::new();

        // Extract common error patterns
        let error_messages: Vec<String> = cluster
            .episodes
            .iter()
            .filter_map(|e| e.error_details.clone())
            .collect();

        if !error_messages.is_empty() {
            let rule_content = format!(
                "Common failure pattern identified across {} episodes",
                cluster.episodes.len()
            );

            let rule_description = if !error_messages.is_empty() {
                Some(format!("Error example: {}", &error_messages[0]))
            } else {
                None
            };

            // Generate embedding for the rule content
            let embedding = self.embedder.generate(&rule_content).await.ok();

            let rule = SemanticRule {
                rule_id: Uuid::new_v4(),
                agent_id,
                rule_content,
                rule_description,
                confidence_score: calculate_confidence(&cluster.episodes),
                verification_status: VerificationStatus::Pending,
                verification_method: Some("pattern_based".to_string()),
                source_episode_cluster: episode_ids.to_vec(),
                episode_count: cluster.episodes.len() as i32,
                embedding,
                is_active: true,
                created_at: chrono::Utc::now(),
            };

            rules.push(rule);
        }

        Ok(rules)
    }

    /// Extracts entities from an episode
    async fn extract_entities_from_episode(
        &self,
        agent_id: Uuid,
        episode: &Episode,
    ) -> Result<Vec<Entity>> {
        let mut entities = Vec::new();

        // For now, simple keyword extraction
        // In production, this would use NER or LLM-based extraction
        let text = format!("{} {:?}", episode.query, episode.context);

        // Simple heuristic: extract capitalized words as potential entities
        for word in text.split_whitespace() {
            if word.len() > 3 && word.chars().next().unwrap().is_uppercase() {
                let entity_name = word
                    .trim_matches(|c: char| !c.is_alphanumeric())
                    .to_string();

                if entity_name.len() > 3 {
                    let embedding = self.embedder.generate(&entity_name).await.ok();

                    let entity = Entity {
                        entity_id: Uuid::new_v4(),
                        agent_id,
                        entity_name: entity_name.clone(),
                        entity_type: "Unknown".to_string(),
                        summary: Some(format!("Extracted from: {}", episode.query)),
                        t_valid: Utc::now(),
                        t_invalid: None,
                        source_episodes: vec![episode.episode_id],
                        extraction_confidence: 0.5,
                        embedding,
                        properties: None,
                    };

                    entities.push(entity);
                }
            }
        }

        Ok(entities)
    }

    /// LLM-powered entity extraction from a batch of episodes
    async fn extract_entities_with_llm(
        &self,
        agent_id: Uuid,
        episodes: &[Episode],
        llm: &Arc<dyn LLMProvider>,
    ) -> Result<Vec<Entity>> {
        let mut all_entities = Vec::new();

        // Batch episodes into groups of 20 for LLM calls
        for chunk in episodes.chunks(20) {
            let episode_summaries: Vec<String> = chunk
                .iter()
                .map(|e| {
                    let ctx = serde_json::to_string(&e.context).unwrap_or_default();
                    let ctx_preview = if ctx.len() > 200 {
                        format!("{}...", &ctx[..200])
                    } else {
                        ctx
                    };
                    format!("- Query: {}\n  Context: {}", e.query, ctx_preview)
                })
                .collect();

            let system_prompt = "You are an expert knowledge graph constructor. \
                Extract named entities from AI agent execution logs. \
                Identify specific people, organizations, concepts, technologies, locations, \
                events, metrics, and domain-specific terms that represent distinct knowledge nodes. \
                Return ONLY a JSON array. Do not extract generic words — focus on proper nouns and domain concepts.";

            let user_prompt = format!(
                "Extract named entities from these {} agent execution episodes:\n\n{}\n\n\
                Return a JSON array:\n\
                [{{\"name\": \"<entity name>\", \"type\": \"<Person|Organization|Concept|Technology|Location|Event|Metric|Domain>\", \"summary\": \"<one-sentence description>\"}}]",
                chunk.len(),
                episode_summaries.join("\n")
            );

            let messages = vec![
                Message {
                    role: MessageRole::System,
                    content: system_prompt.to_string(),
                },
                Message {
                    role: MessageRole::User,
                    content: user_prompt,
                },
            ];

            let config = GenerationConfig {
                temperature: 0.2,
                max_tokens: Some(2048),
                ..Default::default()
            };

            #[derive(serde::Deserialize)]
            struct LLMEntity {
                name: String,
                #[serde(rename = "type")]
                entity_type: String,
                summary: String,
            }

            let llm_entities: Vec<LLMEntity> =
                match generate_structured(llm.as_ref(), messages, &config).await {
                    Ok(e) => e,
                    Err(e) => {
                        eprintln!("Entity extraction batch failed: {}", e);
                        continue;
                    }
                };

            // Deduplicate by name (case-insensitive) within batch
            let mut seen = std::collections::HashSet::new();
            let episode_ids: Vec<Uuid> = chunk.iter().map(|e| e.episode_id).collect();

            for llm_entity in llm_entities {
                let key = llm_entity.name.to_lowercase();
                if seen.contains(&key) || llm_entity.name.len() < 2 {
                    continue;
                }
                seen.insert(key);

                let embedding = self.embedder.generate(&llm_entity.name).await.ok();

                all_entities.push(Entity {
                    entity_id: Uuid::new_v4(),
                    agent_id,
                    entity_name: llm_entity.name,
                    entity_type: llm_entity.entity_type,
                    summary: Some(llm_entity.summary),
                    t_valid: Utc::now(),
                    t_invalid: None,
                    source_episodes: episode_ids.clone(),
                    extraction_confidence: 0.8,
                    embedding,
                    properties: None,
                });
            }
        }

        Ok(all_entities)
    }

    /// LLM-powered fact (relationship) extraction between entities
    async fn extract_facts_with_llm(
        &self,
        agent_id: Uuid,
        entities: &[Entity],
        episodes: &[Episode],
        llm: &Arc<dyn LLMProvider>,
    ) -> Result<Vec<Fact>> {
        let entity_list: Vec<String> = entities
            .iter()
            .map(|e| format!("- {} ({})", e.entity_name, e.entity_type))
            .collect();

        let episode_context: Vec<String> = episodes
            .iter()
            .take(15)
            .map(|e| format!("- {}", e.query))
            .collect();

        let system_prompt = "You are an expert at identifying relationships between entities \
            in a knowledge domain. Given a list of entities and context from agent execution logs, \
            identify meaningful relationships between them. \
            Return ONLY a JSON array. Only include relationships you are confident about.";

        let user_prompt = format!(
            "Entities:\n{}\n\nContext (sample queries):\n{}\n\n\
            Identify relationships between these entities. Return a JSON array:\n\
            [{{\"source\": \"<source entity name>\", \"target\": \"<target entity name>\", \
            \"relation\": \"<relationship type>\", \
            \"cardinality\": \"one_to_one\"|\"one_to_many\"|\"many_to_one\"|\"many_to_many\", \
            \"confidence\": <0.0-1.0>, \
            \"reasoning\": \"<brief explanation>\"}}]",
            entity_list.join("\n"),
            episode_context.join("\n")
        );

        let messages = vec![
            Message {
                role: MessageRole::System,
                content: system_prompt.to_string(),
            },
            Message {
                role: MessageRole::User,
                content: user_prompt,
            },
        ];

        let config = GenerationConfig {
            temperature: 0.2,
            max_tokens: Some(2048),
            ..Default::default()
        };

        #[derive(serde::Deserialize)]
        struct LLMFact {
            source: String,
            target: String,
            relation: String,
            #[serde(default = "default_cardinality")]
            cardinality: String,
            #[serde(default = "default_confidence")]
            confidence: f64,
            reasoning: Option<String>,
        }
        fn default_cardinality() -> String {
            "many_to_many".to_string()
        }
        fn default_confidence() -> f64 {
            0.7
        }

        let llm_facts: Vec<LLMFact> = generate_structured(llm.as_ref(), messages, &config).await?;

        // Build name -> entity lookup (case-insensitive)
        let entity_map: std::collections::HashMap<String, &Entity> = entities
            .iter()
            .map(|e| (e.entity_name.to_lowercase(), e))
            .collect();

        let episode_ids: Vec<Uuid> = episodes.iter().take(15).map(|e| e.episode_id).collect();
        let mut facts = Vec::new();

        for llm_fact in llm_facts {
            let source = entity_map.get(&llm_fact.source.to_lowercase());
            let target = entity_map.get(&llm_fact.target.to_lowercase());

            if let (Some(src), Some(tgt)) = (source, target) {
                let cardinality = match llm_fact.cardinality.as_str() {
                    "one_to_one" => Cardinality::OneToOne,
                    "one_to_many" => Cardinality::OneToMany,
                    "many_to_one" => Cardinality::ManyToOne,
                    _ => Cardinality::ManyToMany,
                };

                facts.push(Fact {
                    fact_id: Uuid::new_v4(),
                    agent_id,
                    source_entity_id: src.entity_id,
                    target_entity_id: tgt.entity_id,
                    relation_type: llm_fact.relation,
                    relation_cardinality: cardinality,
                    confidence: llm_fact.confidence.clamp(0.0, 1.0),
                    reasoning: llm_fact.reasoning,
                    t_valid: Utc::now(),
                    t_invalid: None,
                    source_episodes: episode_ids.clone(),
                    data: None,
                });
            }
        }

        Ok(facts)
    }

    /// LLM-powered knowledge rule extraction from successful episodes
    async fn extract_knowledge_rules(
        &self,
        agent_id: Uuid,
        episodes: &[&Episode],
        llm: &Arc<dyn LLMProvider>,
    ) -> Result<Vec<SemanticRule>> {
        let episode_summaries: Vec<String> = episodes
            .iter()
            .map(|e| {
                let ctx = serde_json::to_string(&e.context).unwrap_or_default();
                let ctx_preview = if ctx.len() > 300 {
                    format!("{}...", &ctx[..300])
                } else {
                    ctx
                };
                format!("- Query: {}\n  Context: {}", e.query, ctx_preview)
            })
            .collect();

        let episode_ids: Vec<Uuid> = episodes.iter().map(|e| e.episode_id).collect();

        let system_prompt =
            "You are an expert at distilling knowledge from AI agent execution logs. \
            Extract 2-5 semantic rules — reusable insights, patterns, or domain knowledge that \
            the agent has learned through its successful executions. \
            Each rule should be a clear, actionable insight that could improve future performance. \
            Return ONLY a JSON array.";

        let user_prompt = format!(
            "Analyze these {} successful agent executions and extract knowledge rules:\n\n{}\n\n\
            Return a JSON array:\n\
            [{{\"rule\": \"<concise rule statement>\", \
            \"description\": \"<detailed explanation>\", \
            \"confidence\": <0.0-1.0>}}]",
            episodes.len(),
            episode_summaries.join("\n")
        );

        let messages = vec![
            Message {
                role: MessageRole::System,
                content: system_prompt.to_string(),
            },
            Message {
                role: MessageRole::User,
                content: user_prompt,
            },
        ];

        let config = GenerationConfig {
            temperature: 0.3,
            max_tokens: Some(2048),
            ..Default::default()
        };

        #[derive(serde::Deserialize)]
        struct LLMRule {
            rule: String,
            description: String,
            #[serde(default = "default_rule_confidence")]
            confidence: f64,
        }
        fn default_rule_confidence() -> f64 {
            0.7
        }

        let llm_rules: Vec<LLMRule> = generate_structured(llm.as_ref(), messages, &config).await?;

        let mut rules = Vec::new();
        for llm_rule in llm_rules {
            let embedding = self.embedder.generate(&llm_rule.rule).await.ok();

            rules.push(SemanticRule {
                rule_id: Uuid::new_v4(),
                agent_id,
                rule_content: llm_rule.rule,
                rule_description: Some(llm_rule.description),
                confidence_score: llm_rule.confidence.clamp(0.0, 1.0),
                verification_status: VerificationStatus::Pending,
                verification_method: Some(format!("llm_knowledge_extraction:{}", llm.model_name())),
                source_episode_cluster: episode_ids.clone(),
                episode_count: episodes.len() as i32,
                embedding,
                is_active: true,
                created_at: Utc::now(),
            });
        }

        Ok(rules)
    }
}

/// Result of a consolidation run
#[derive(Debug, Clone, Default)]
pub struct ConsolidationResult {
    pub episodes_processed: usize,
    pub clusters_identified: usize,
    pub rules_extracted: usize,
    pub rules_verified: usize,
    pub rules_rejected: usize,
    pub entities_created: usize,
    pub facts_created: usize,
}

/// Calculates confidence score based on cluster characteristics
fn calculate_confidence(episodes: &[Episode]) -> f64 {
    let base_confidence = 0.5;
    let episode_boost = (episodes.len() as f64 * 0.1).min(0.3);
    (base_confidence + episode_boost).min(0.95)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Agent, MockEmbeddings};
    use serde_json::json;

    async fn get_test_store() -> MemoryStore {
        dotenvy::dotenv().ok();
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
        MemoryStore::new(&database_url).await.unwrap()
    }

    #[tokio::test]
    async fn test_consolidation_workflow() {
        let store = Arc::new(get_test_store().await);
        let pool = Arc::new(store.pool().clone());
        let lock = Arc::new(ConsolidationLock::new(pool, "test-worker".to_string()));
        let embedder = Arc::new(MockEmbeddings::new(1024));

        let worker =
            ConsolidationWorker::new(store.clone(), lock, embedder, "test-worker".to_string());

        // Create agent
        let agent = Agent {
            agent_id: Uuid::new_v4(),
            agent_name: format!("test_agent_{}", Uuid::new_v4()),
            agent_type: "test".to_string(),
            version: "1.0.0".to_string(),
            tier: "test".to_string(),
            executor_type: "llm".to_string(),
            model: "test-model".to_string(),
            temperature: 0.3,
            mcp_servers: None,
            description: None,
            author: "test".to_string(),
            current_ontology_commit: None,
            current_ontology_snapshot_id: None,
            last_consolidated_at: None,
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            total_cost_usd: None,
            avg_execution_time_ms: 0,
            dreaming_budget_credits: 0,
            dreaming_credits_used: 0,
            dreaming_budget_reset_at: None,
            system_prompt: None,
            visibility: "public".to_string(),
            owner_id: None,
            tags: vec![],
            education_budget_credits: 0,
            education_credits_used: 0,
            display_alias: None,
            llm_provider: "anthropic".to_string(),
            embedding_provider: "anthropic".to_string(),
            embedding_model: "voyage-2".to_string(),
            embedding_dimension: 1024,
            sample_queries: vec![],
            status: "draft".to_string(),
            fork_pricing: None,
            forked_from: None,
            fork_count: 0,
            accepts: vec![],
            produces: vec![],
            workflow_template: None,
            prompt_template: None,
            requires_secrets: None,
            auto_collect_pct: 0,
            model_ladder: serde_json::Value::Array(vec![]),
            min_tier: "free".to_string(),
            capability_gates: serde_json::Value::Object(serde_json::Map::new()),
            persona_version: 1,
            fermi_contract: None,
                    output_contract: None,
            model_params: serde_json::Value::Object(serde_json::Map::new()),
            valence: None,
        };
        store.upsert_agent(agent.clone()).await.unwrap();

        // Create test episodes with failures
        for i in 0..10 {
            let episode = Episode {
                episode_id: Uuid::new_v4(),
                agent_id: agent.agent_id,
                timestamp_ref: Utc::now(),
                query: format!("Test query {}", i),
                context: json!({"test": i}),
                execution_status: if i % 3 == 0 {
                    crate::ExecutionStatus::Failure
                } else {
                    crate::ExecutionStatus::Success
                },
                error_details: if i % 3 == 0 {
                    Some(format!("Error {}", i))
                } else {
                    None
                },
                execution_time_ms: 1000,
                tokens_used: Some(100),
                cost_usd: Some(rust_decimal::Decimal::new(1, 3)),
                embedding: Some(vec![0.1; 1024]),
                consolidated: false,
                tags: vec![],
                provenance: crate::Provenance::AutoPass,
                authority_weight: 0.5,
                dyad_id: None,
                persona_version_at_write: None,
                provider_used: None,
                model_used: None,
            };
            store.store_episode(episode).await.unwrap();
        }

        // Run consolidation
        let result = worker
            .consolidate_agent(agent.agent_id, 0.5, 2)
            .await
            .unwrap();

        assert_eq!(result.episodes_processed, 10);
        assert!(result.rules_extracted > 0 || result.clusters_identified == 0);

        // Verify episodes are marked as consolidated
        let remaining = store
            .get_unconsolidated_episodes(agent.agent_id)
            .await
            .unwrap();
        assert_eq!(remaining.len(), 0);

        println!("✅ Consolidation workflow works!");
        println!("   Episodes processed: {}", result.episodes_processed);
        println!("   Clusters identified: {}", result.clusters_identified);
        println!("   Rules extracted: {}", result.rules_extracted);
        println!("   Entities created: {}", result.entities_created);
    }
}
