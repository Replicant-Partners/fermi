use crate::error::{OntologyError, Result};
use crate::git::GitManager;
use crate::mermaid::MermaidGenerator;
use crate::types::OntologyStats;
use agent_bestiary_memory::MemoryStore;
use chrono::Utc;
use sqlx::Row;
use uuid::Uuid;

/// Manages ontology snapshots - combines Mermaid generation, git versioning, and database storage
pub struct SnapshotManager {
    store: MemoryStore,
    mermaid_generator: MermaidGenerator,
    git_manager: GitManager,
}

impl SnapshotManager {
    /// Create a new SnapshotManager
    pub fn new(
        store: MemoryStore,
        mermaid_generator: MermaidGenerator,
        git_manager: GitManager,
    ) -> Self {
        Self {
            store,
            mermaid_generator,
            git_manager,
        }
    }

    /// Create a complete ontology snapshot
    ///
    /// This method orchestrates the full snapshot workflow:
    /// 1. Generates Mermaid ER diagram from agent's ontology
    /// 2. Commits the diagram to git
    /// 3. Stores snapshot metadata in database
    /// 4. Updates agent's current ontology references
    pub async fn create_snapshot(&self, agent_id: Uuid, job_id: Option<Uuid>) -> Result<Uuid> {
        // Fetch agent details
        let agent = self
            .store
            .get_agent(agent_id)
            .await?
            .ok_or_else(|| OntologyError::AgentNotFound(agent_id.to_string()))?;

        // Get ontology statistics
        let (entity_count, fact_count) = self.mermaid_generator.get_stats(agent_id).await?;

        // Get semantic rules count
        let rules = self.store.get_agent_semantic_rules(agent_id).await?;
        let rule_count = rules.len() as i32;

        // Get episode count (if job_id provided, use that; otherwise count all)
        let episode_count = if let Some(_jid) = job_id {
            // Count episodes for this consolidation job
            // For now, use a placeholder - we'd need to track this in the consolidation workflow
            0
        } else {
            0
        };

        let stats = OntologyStats::new(entity_count, fact_count, rule_count, episode_count, job_id);

        // Generate Mermaid diagram
        let mut diagram = self.mermaid_generator.generate(agent_id).await?;
        diagram.metadata.job_id = job_id;

        // Commit to git
        let git_commit =
            self.git_manager
                .commit_ontology(&agent.agent_name, &diagram.content, &stats)?;

        // Store snapshot in database
        let snapshot_id = self
            .store_snapshot(
                agent_id,
                &diagram.content,
                &git_commit.sha,
                git_commit.github_url.as_deref(),
                git_commit.pushed_to_remote,
                job_id,
            )
            .await?;

        // Update agent's current ontology references
        self.update_agent_ontology_refs(agent_id, &git_commit.sha, snapshot_id)
            .await?;

        Ok(snapshot_id)
    }

    /// Update dream synopsis and consolidation stats on an existing snapshot
    pub async fn update_snapshot_synopsis(
        &self,
        snapshot_id: Uuid,
        dream_synopsis: &str,
        consolidation_stats: Option<&serde_json::Value>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE ontology_snapshots
            SET dream_synopsis = $1, consolidation_stats = $2
            WHERE snapshot_id = $3
            "#,
        )
        .bind(dream_synopsis)
        .bind(consolidation_stats)
        .bind(snapshot_id)
        .execute(self.store.pool())
        .await?;

        Ok(())
    }

    /// Store snapshot metadata in database
    async fn store_snapshot(
        &self,
        agent_id: Uuid,
        mermaid_content: &str,
        git_commit_sha: &str,
        github_url: Option<&str>,
        pushed_to_remote: bool,
        job_id: Option<Uuid>,
    ) -> Result<Uuid> {
        let snapshot_id = Uuid::new_v4();

        // Get stats for this snapshot
        let (entity_count, fact_count) = self.mermaid_generator.get_stats(agent_id).await?;
        let rules = self.store.get_agent_semantic_rules(agent_id).await?;
        let rule_count = rules.len() as i32;

        // Get community count
        let community_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM communities WHERE agent_id = $1")
                .bind(agent_id)
                .fetch_one(self.store.pool())
                .await?;

        // Next version for this agent.
        //
        // `MAX(version)` with no GROUP BY always returns exactly one row, and
        // that row is NULL when the agent has no snapshots yet. The previous
        // form decoded it into `(i32,)` rather than `(Option<i32>,)`, so the
        // NULL failed to decode, `?` propagated the error, and the FIRST
        // snapshot for any agent always failed.
        //
        // Since the first snapshot always failed, no agent ever reached a
        // second, so `create_snapshot` never once succeeded on this platform.
        // The single row in `ontology_snapshots` carries
        // `git_commit_sha = 'seed-034'` — inserted by migration 034, not
        // created here. That is why ontologies never developed, and why the
        // dream narrator's `UPDATE ontology_snapshots` had nothing to update.
        //
        // `fetch_optional` also hid it: it reads as "there may be no row",
        // which is the plausible-but-wrong mental model for an aggregate.
        let previous_version: Option<i32> =
            sqlx::query_scalar("SELECT MAX(version) FROM ontology_snapshots WHERE agent_id = $1")
                .bind(agent_id)
                .fetch_one(self.store.pool())
                .await?;

        let version = previous_version.unwrap_or(0) + 1;

        let git_repository = github_url.unwrap_or("local");
        let git_path = "ontology.mermaid".to_string();

        sqlx::query(
            r#"
            INSERT INTO ontology_snapshots (
                snapshot_id, agent_id, git_commit_sha, git_repository, git_path,
                github_url, pushed_to_remote, mermaid_content,
                entity_count, fact_count, community_count, rule_count,
                version, consolidation_job_id, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            "#,
        )
        .bind(snapshot_id)
        .bind(agent_id)
        .bind(git_commit_sha)
        .bind(git_repository)
        .bind(git_path)
        .bind(github_url)
        .bind(pushed_to_remote)
        .bind(mermaid_content)
        .bind(entity_count)
        .bind(fact_count)
        .bind(community_count.0 as i32)
        .bind(rule_count)
        .bind(version)
        .bind(job_id)
        .bind(Utc::now())
        .execute(self.store.pool())
        .await?;

        Ok(snapshot_id)
    }

    /// Update agent's current ontology references
    async fn update_agent_ontology_refs(
        &self,
        agent_id: Uuid,
        git_commit_sha: &str,
        snapshot_id: Uuid,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE agents
            SET current_ontology_commit = $1,
                current_ontology_snapshot_id = $2
            WHERE agent_id = $3
            "#,
        )
        .bind(git_commit_sha)
        .bind(snapshot_id)
        .bind(agent_id)
        .execute(self.store.pool())
        .await?;

        Ok(())
    }

    /// Get the latest snapshot for an agent
    pub async fn get_latest_snapshot(&self, agent_id: Uuid) -> Result<Option<OntologySnapshot>> {
        let row = sqlx::query(
            r#"
            SELECT snapshot_id, agent_id, git_commit_sha, mermaid_content,
                   consolidation_job_id, dream_synopsis, consolidation_stats, created_at
            FROM ontology_snapshots
            WHERE agent_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(agent_id)
        .fetch_optional(self.store.pool())
        .await?;

        match row {
            Some(row) => Ok(Some(row_to_snapshot(&row)?)),
            None => Ok(None),
        }
    }

    /// Get a specific snapshot by ID
    pub async fn get_snapshot(&self, snapshot_id: Uuid) -> Result<Option<OntologySnapshot>> {
        let row = sqlx::query(
            r#"
            SELECT snapshot_id, agent_id, git_commit_sha, mermaid_content,
                   consolidation_job_id, dream_synopsis, consolidation_stats, created_at
            FROM ontology_snapshots
            WHERE snapshot_id = $1
            "#,
        )
        .bind(snapshot_id)
        .fetch_optional(self.store.pool())
        .await?;

        match row {
            Some(row) => Ok(Some(row_to_snapshot(&row)?)),
            None => Ok(None),
        }
    }

    /// List all snapshots for an agent
    pub async fn list_snapshots(&self, agent_id: Uuid) -> Result<Vec<OntologySnapshot>> {
        let rows = sqlx::query(
            r#"
            SELECT snapshot_id, agent_id, git_commit_sha, mermaid_content,
                   consolidation_job_id, dream_synopsis, consolidation_stats, created_at
            FROM ontology_snapshots
            WHERE agent_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(agent_id)
        .fetch_all(self.store.pool())
        .await?;

        let mut snapshots = Vec::new();
        for row in rows {
            snapshots.push(row_to_snapshot(&row)?);
        }

        Ok(snapshots)
    }

    /// Get git manager reference
    pub fn git_manager(&self) -> &GitManager {
        &self.git_manager
    }

    /// Get mermaid generator reference
    pub fn mermaid_generator(&self) -> &MermaidGenerator {
        &self.mermaid_generator
    }
}

/// Helper to construct OntologySnapshot from a database row
fn row_to_snapshot(row: &sqlx::postgres::PgRow) -> Result<OntologySnapshot> {
    Ok(OntologySnapshot {
        snapshot_id: row.try_get("snapshot_id")?,
        agent_id: row.try_get("agent_id")?,
        git_commit_sha: row.try_get("git_commit_sha")?,
        mermaid_content: row.try_get("mermaid_content")?,
        consolidation_job_id: row.try_get("consolidation_job_id")?,
        dream_synopsis: row.try_get("dream_synopsis")?,
        consolidation_stats: row.try_get("consolidation_stats")?,
        created_at: row.try_get("created_at")?,
    })
}

/// Represents an ontology snapshot stored in the database
#[derive(Debug, Clone)]
pub struct OntologySnapshot {
    pub snapshot_id: Uuid,
    pub agent_id: Uuid,
    pub git_commit_sha: String,
    pub mermaid_content: String,
    pub consolidation_job_id: Option<Uuid>,
    pub dream_synopsis: Option<String>,
    pub consolidation_stats: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GitConfig;
    use tempfile::TempDir;

    // Note: These tests require a database connection and are integration tests
    // They are disabled by default and should be run with a test database

    #[test]
    fn test_ontology_snapshot_struct() {
        let snapshot = OntologySnapshot {
            snapshot_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            git_commit_sha: "abc123".to_string(),
            mermaid_content: "erDiagram\n".to_string(),
            consolidation_job_id: None,
            dream_synopsis: None,
            consolidation_stats: None,
            created_at: Utc::now(),
        };

        assert!(!snapshot.git_commit_sha.is_empty());
        assert!(!snapshot.mermaid_content.is_empty());
    }
}
