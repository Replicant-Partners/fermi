use crate::error::{MemoryError, Result};
use crate::types::{Entity, Episode, Fact, Relationship, SemanticRule};
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

/// Core memory store providing access to episodic and semantic memory
#[derive(Clone)]
pub struct MemoryStore {
    pool: PgPool,
}

impl MemoryStore {
    /// Create a new MemoryStore with a connection pool
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        Ok(Self { pool })
    }

    /// Create a MemoryStore from an existing pool
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    // ========================================================================
    // EPISODE OPERATIONS
    // ========================================================================

    /// Store a new episode
    pub async fn store_episode(&self, episode: Episode) -> Result<Uuid> {
        let episode_id = Uuid::new_v4();

        let rec = sqlx::query!(
            r#"
            INSERT INTO episodes (
                episode_id, agent_id, timestamp_ref, query, context,
                execution_status, error_details, execution_time_ms,
                tokens_used, cost_usd, consolidated
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING episode_id
            "#,
            episode_id,
            episode.agent_id,
            episode.timestamp_ref,
            episode.query,
            episode.context,
            episode.execution_status.to_string(),
            episode.error_details,
            episode.execution_time_ms,
            episode.tokens_used,
            episode.cost_usd,
            episode.consolidated
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(rec.episode_id)
    }

    /// Get an episode by ID
    pub async fn get_episode(&self, episode_id: Uuid) -> Result<Episode> {
        let rec = sqlx::query!(
            r#"
            SELECT
                episode_id, agent_id, user_id, timestamp_ref, timestamp_created,
                query, context, execution_status, error_details,
                execution_time_ms, tokens_used, cost_usd,
                consolidated, consolidation_job_id, cluster_id, created_at
            FROM episodes
            WHERE episode_id = $1
            "#,
            episode_id
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| MemoryError::NotFound(format!("Episode {} not found", episode_id)))?;

        Ok(Episode {
            episode_id: Some(rec.episode_id),
            agent_id: rec.agent_id,
            user_id: rec.user_id.unwrap_or_default(),
            timestamp_ref: rec.timestamp_ref,
            timestamp_created: Some(rec.timestamp_created),
            query: rec.query,
            context: rec.context,
            execution_status: match rec.execution_status.as_str() {
                "success" => crate::types::ExecutionStatus::Success,
                "failure" => crate::types::ExecutionStatus::Failure,
                "partial" => crate::types::ExecutionStatus::Partial,
                _ => crate::types::ExecutionStatus::Success,
            },
            error_details: rec.error_details,
            execution_time_ms: rec.execution_time_ms,
            tokens_used: rec.tokens_used,
            cost_usd: rec.cost_usd,
            consolidated: rec.consolidated,
            consolidation_job_id: rec.consolidation_job_id,
            cluster_id: rec.cluster_id,
            created_at: Some(rec.created_at),
        })
    }

    /// Get unconsolidated episodes for an agent
    pub async fn get_unconsolidated_episodes(
        &self,
        agent_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Episode>> {
        let records = sqlx::query!(
            r#"
            SELECT
                episode_id, agent_id, user_id, timestamp_ref, timestamp_created,
                query, context, execution_status, error_details,
                execution_time_ms, tokens_used, cost_usd,
                consolidated, consolidation_job_id, cluster_id, created_at
            FROM episodes
            WHERE agent_id = $1 AND consolidated = FALSE
            ORDER BY timestamp_ref DESC
            LIMIT $2
            "#,
            agent_id,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(records
            .into_iter()
            .map(|rec| Episode {
                episode_id: Some(rec.episode_id),
                agent_id: rec.agent_id,
                user_id: rec.user_id.unwrap_or_default(),
                timestamp_ref: rec.timestamp_ref,
                timestamp_created: Some(rec.timestamp_created),
                query: rec.query,
                context: rec.context,
                execution_status: match rec.execution_status.as_str() {
                    "success" => crate::types::ExecutionStatus::Success,
                    "failure" => crate::types::ExecutionStatus::Failure,
                    "partial" => crate::types::ExecutionStatus::Partial,
                    _ => crate::types::ExecutionStatus::Success,
                },
                error_details: rec.error_details,
                execution_time_ms: rec.execution_time_ms,
                tokens_used: rec.tokens_used,
                cost_usd: rec.cost_usd,
                consolidated: rec.consolidated,
                consolidation_job_id: rec.consolidation_job_id,
                cluster_id: rec.cluster_id,
                created_at: Some(rec.created_at),
            })
            .collect())
    }

    /// Mark episodes as consolidated
    pub async fn mark_episodes_consolidated(
        &self,
        episode_ids: &[Uuid],
        consolidation_job_id: Uuid,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE episodes
            SET consolidated = TRUE, consolidation_job_id = $1
            WHERE episode_id = ANY($2)
            "#,
            consolidation_job_id,
            episode_ids
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ========================================================================
    // SEMANTIC RULE OPERATIONS
    // ========================================================================

    /// Store a new semantic rule
    pub async fn store_semantic_rule(&self, rule: SemanticRule) -> Result<Uuid> {
        let rule_id = Uuid::new_v4();

        let rec = sqlx::query!(
            r#"
            INSERT INTO semantic_rules (
                rule_id, agent_id, rule_content, rule_description,
                confidence_score, verification_status, verification_method,
                verification_details, source_episode_cluster, episode_count,
                application_count, successful_applications, failed_applications,
                is_active
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING rule_id
            "#,
            rule_id,
            rule.agent_id,
            rule.rule_content,
            rule.rule_description,
            rule.confidence_score,
            rule.verification_status.to_string(),
            rule.verification_method,
            rule.verification_details,
            &rule.source_episode_cluster,
            rule.episode_count,
            rule.application_count,
            rule.successful_applications,
            rule.failed_applications,
            rule.is_active
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(rec.rule_id)
    }

    /// Get a semantic rule by ID
    pub async fn get_semantic_rule(&self, rule_id: Uuid) -> Result<SemanticRule> {
        let rec = sqlx::query!(
            r#"
            SELECT
                rule_id, agent_id, user_id, rule_content, rule_description,
                confidence_score, verification_status, verification_method,
                verification_details, source_episode_cluster, episode_count,
                created_at, last_validated_at, application_count,
                successful_applications, failed_applications, is_active,
                invalidated_at, invalidation_reason
            FROM semantic_rules
            WHERE rule_id = $1
            "#,
            rule_id
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| MemoryError::NotFound(format!("Semantic rule {} not found", rule_id)))?;

        Ok(SemanticRule {
            rule_id: Some(rec.rule_id),
            agent_id: rec.agent_id,
            user_id: rec.user_id.unwrap_or_default(),
            rule_content: rec.rule_content,
            rule_description: rec.rule_description,
            confidence_score: rec.confidence_score,
            verification_status: match rec.verification_status.as_str() {
                "verified" => crate::types::VerificationStatus::Verified,
                "rejected" => crate::types::VerificationStatus::Rejected,
                _ => crate::types::VerificationStatus::Pending,
            },
            verification_method: rec.verification_method,
            verification_details: rec.verification_details,
            source_episode_cluster: rec.source_episode_cluster,
            episode_count: rec.episode_count,
            created_at: Some(rec.created_at),
            last_validated_at: rec.last_validated_at,
            application_count: rec.application_count,
            successful_applications: rec.successful_applications,
            failed_applications: rec.failed_applications,
            is_active: rec.is_active,
            invalidated_at: rec.invalidated_at,
            invalidation_reason: rec.invalidation_reason,
        })
    }

    /// Get active semantic rules for an agent
    pub async fn get_active_semantic_rules(&self, agent_id: Uuid) -> Result<Vec<SemanticRule>> {
        let records = sqlx::query!(
            r#"
            SELECT
                rule_id, agent_id, user_id, rule_content, rule_description,
                confidence_score, verification_status, verification_method,
                verification_details, source_episode_cluster, episode_count,
                created_at, last_validated_at, application_count,
                successful_applications, failed_applications, is_active,
                invalidated_at, invalidation_reason
            FROM semantic_rules
            WHERE agent_id = $1 AND is_active = TRUE
            ORDER BY confidence_score DESC, created_at DESC
            "#,
            agent_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(records
            .into_iter()
            .map(|rec| SemanticRule {
                rule_id: Some(rec.rule_id),
                agent_id: rec.agent_id,
                user_id: rec.user_id.unwrap_or_default(),
                rule_content: rec.rule_content,
                rule_description: rec.rule_description,
                confidence_score: rec.confidence_score,
                verification_status: match rec.verification_status.as_str() {
                    "verified" => crate::types::VerificationStatus::Verified,
                    "rejected" => crate::types::VerificationStatus::Rejected,
                    _ => crate::types::VerificationStatus::Pending,
                },
                verification_method: rec.verification_method,
                verification_details: rec.verification_details,
                source_episode_cluster: rec.source_episode_cluster,
                episode_count: rec.episode_count,
                created_at: Some(rec.created_at),
                last_validated_at: rec.last_validated_at,
                application_count: rec.application_count,
                successful_applications: rec.successful_applications,
                failed_applications: rec.failed_applications,
                is_active: rec.is_active,
                invalidated_at: rec.invalidated_at,
                invalidation_reason: rec.invalidation_reason,
            })
            .collect())
    }

    // ========================================================================
    // HEALTH CHECK
    // ========================================================================

    /// Test database connection
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query!("SELECT 1 as health")
            .fetch_one(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ExecutionStatus;

    #[tokio::test]
    #[ignore] // Requires database connection
    async fn test_store_and_retrieve_episode() {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://localhost/fermi_test".to_string());

        let store = MemoryStore::new(&database_url).await.unwrap();

        let agent_id = Uuid::new_v4();
        let episode = Episode::new(
            agent_id,
            "Test query".to_string(),
            serde_json::json!({"test": "data"}),
            ExecutionStatus::Success,
        );

        let episode_id = store.store_episode(episode).await.unwrap();
        let retrieved = store.get_episode(episode_id).await.unwrap();

        assert_eq!(retrieved.agent_id, agent_id);
        assert_eq!(retrieved.query, "Test query");
    }
}
