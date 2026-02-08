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
    generate_structured, ConsolidationLock, DBSCANClustering, EmbeddingGenerator, Entity, Episode,
    EpisodeCluster, GenerationConfig, LLMProvider, MemoryError, MemoryStore, Message, MessageRole,
    Result, SemanticRule, VerificationStatus,
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

        // Step 5: Extract semantic rules from clusters
        for cluster in &clusters {
            let rules = self.extract_rules_from_cluster(agent_id, cluster).await?;
            result.rules_extracted += rules.len();

            for rule in rules {
                self.store.store_semantic_rule(rule).await?;
            }
        }

        // Step 6: Extract entities from episodes (sample from all episodes)
        let sample_size = episodes.len().min(100);
        for episode in episodes.iter().take(sample_size) {
            let entities = self
                .extract_entities_from_episode(agent_id, episode)
                .await?;
            result.entities_created += entities.len();

            for entity in entities {
                self.store.store_entity(entity).await?;
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

            let rule_description = if error_messages.len() > 0 {
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
                        extraction_confidence: 0.5, // Low confidence for simple extraction
                        embedding,
                    };

                    entities.push(entity);
                }
            }
        }

        Ok(entities)
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
