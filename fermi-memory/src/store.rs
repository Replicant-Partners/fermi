use crate::error::{MemoryError, Result};
use crate::types::{Episode, SemanticRule};
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use sqlx::Row;
use std::str::FromStr;
use uuid::Uuid;

/// Core memory store providing access to episodic and semantic memory
#[derive(Clone)]
pub struct MemoryStore {
    pool: PgPool,
}

impl MemoryStore {
    /// Create a new MemoryStore with a connection pool
    pub async fn new(database_url: &str) -> Result<Self> {
        // Neon uses PgBouncer in transaction mode — disable prepared statement
        // cache to avoid "prepared statement does not exist" errors
        let connect_options = PgConnectOptions::from_str(database_url)?.statement_cache_capacity(0);

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .test_before_acquire(true)
            .connect_with(connect_options)
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

        let row = sqlx::query(
            r#"
            INSERT INTO episodes (
                episode_id, agent_id, timestamp_ref, query, context,
                execution_status, error_details, execution_time_ms,
                tokens_used, cost_usd, consolidated
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING episode_id
            "#,
        )
        .bind(episode_id)
        .bind(episode.agent_id)
        .bind(episode.timestamp_ref)
        .bind(&episode.query)
        .bind(&episode.context)
        .bind(episode.execution_status.to_string())
        .bind(&episode.error_details)
        .bind(episode.execution_time_ms)
        .bind(episode.tokens_used)
        .bind(episode.cost_usd)
        .bind(episode.consolidated)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get("episode_id"))
    }

    /// Get an episode by ID
    pub async fn get_episode(&self, episode_id: Uuid) -> Result<Episode> {
        let row = sqlx::query(
            r#"
            SELECT
                episode_id, agent_id, user_id, timestamp_ref, timestamp_created,
                query, context, execution_status, error_details,
                execution_time_ms, tokens_used, cost_usd,
                consolidated, consolidation_job_id, cluster_id, created_at
            FROM episodes
            WHERE episode_id = $1
            "#,
        )
        .bind(episode_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| MemoryError::NotFound(format!("Episode {} not found", episode_id)))?;

        Ok(episode_from_row(&row))
    }

    /// Get unconsolidated episodes for an agent
    pub async fn get_unconsolidated_episodes(
        &self,
        agent_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Episode>> {
        let rows = sqlx::query(
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
        )
        .bind(agent_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(episode_from_row).collect())
    }

    /// Mark episodes as consolidated
    pub async fn mark_episodes_consolidated(
        &self,
        episode_ids: &[Uuid],
        consolidation_job_id: Uuid,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE episodes
            SET consolidated = TRUE, consolidation_job_id = $1
            WHERE episode_id = ANY($2)
            "#,
        )
        .bind(consolidation_job_id)
        .bind(episode_ids)
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

        let row = sqlx::query(
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
        )
        .bind(rule_id)
        .bind(rule.agent_id)
        .bind(&rule.rule_content)
        .bind(&rule.rule_description)
        .bind(rule.confidence_score)
        .bind(rule.verification_status.to_string())
        .bind(&rule.verification_method)
        .bind(&rule.verification_details)
        .bind(&rule.source_episode_cluster)
        .bind(rule.episode_count)
        .bind(rule.application_count)
        .bind(rule.successful_applications)
        .bind(rule.failed_applications)
        .bind(rule.is_active)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get("rule_id"))
    }

    /// Get a semantic rule by ID
    pub async fn get_semantic_rule(&self, rule_id: Uuid) -> Result<SemanticRule> {
        let row = sqlx::query(
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
        )
        .bind(rule_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| MemoryError::NotFound(format!("Semantic rule {} not found", rule_id)))?;

        Ok(rule_from_row(&row))
    }

    /// Get active semantic rules for an agent
    pub async fn get_active_semantic_rules(&self, agent_id: Uuid) -> Result<Vec<SemanticRule>> {
        let rows = sqlx::query(
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
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(rule_from_row).collect())
    }

    // ========================================================================
    // HEALTH CHECK
    // ========================================================================

    /// Test database connection
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1 as health")
            .fetch_one(&self.pool)
            .await?;
        Ok(())
    }
}

// ========================================================================
// Row conversion helpers
// ========================================================================

fn episode_from_row(row: &sqlx::postgres::PgRow) -> Episode {
    let status_str: String = row.get("execution_status");
    Episode {
        episode_id: Some(row.get("episode_id")),
        agent_id: row.get("agent_id"),
        user_id: row.try_get::<String, _>("user_id").unwrap_or_default(),
        timestamp_ref: row.get("timestamp_ref"),
        timestamp_created: Some(row.get("timestamp_created")),
        query: row.get("query"),
        context: row.get("context"),
        execution_status: match status_str.as_str() {
            "success" => crate::types::ExecutionStatus::Success,
            "failure" => crate::types::ExecutionStatus::Failure,
            "partial" => crate::types::ExecutionStatus::Partial,
            _ => crate::types::ExecutionStatus::Success,
        },
        error_details: row.get("error_details"),
        execution_time_ms: row.get("execution_time_ms"),
        tokens_used: row.get("tokens_used"),
        cost_usd: row.get("cost_usd"),
        consolidated: row.get("consolidated"),
        consolidation_job_id: row.get("consolidation_job_id"),
        cluster_id: row.get("cluster_id"),
        created_at: Some(row.get("created_at")),
    }
}

fn rule_from_row(row: &sqlx::postgres::PgRow) -> SemanticRule {
    let status_str: String = row.get("verification_status");
    SemanticRule {
        rule_id: Some(row.get("rule_id")),
        agent_id: row.get("agent_id"),
        user_id: row.try_get::<String, _>("user_id").unwrap_or_default(),
        rule_content: row.get("rule_content"),
        rule_description: row.get("rule_description"),
        confidence_score: row.get("confidence_score"),
        verification_status: match status_str.as_str() {
            "verified" => crate::types::VerificationStatus::Verified,
            "rejected" => crate::types::VerificationStatus::Rejected,
            _ => crate::types::VerificationStatus::Pending,
        },
        verification_method: row.get("verification_method"),
        verification_details: row.get("verification_details"),
        source_episode_cluster: row.get("source_episode_cluster"),
        episode_count: row.get("episode_count"),
        created_at: Some(row.get("created_at")),
        last_validated_at: row.get("last_validated_at"),
        application_count: row.get("application_count"),
        successful_applications: row.get("successful_applications"),
        failed_applications: row.get("failed_applications"),
        is_active: row.get("is_active"),
        invalidated_at: row.get("invalidated_at"),
        invalidation_reason: row.get("invalidation_reason"),
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
            String::new(),
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
