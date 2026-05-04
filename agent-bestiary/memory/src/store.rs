use crate::{
    Agent, AgentUpdate, AgentVersion, CoherenceEvaluation, Community, ConsolidationJob, Entity,
    Episode, EvalRun, EvalTestCase, Fact, MarketplaceListing, MarketplaceTransaction, MemoryError,
    Result, SemanticRule, ShoppingProfile, VerificationStatus, WorkspaceMessage,
};
use sqlx::{postgres::PgConnectOptions, postgres::PgPoolOptions, PgPool, Row};
use std::str::FromStr;
use uuid::Uuid;

/// Common SELECT columns for agent queries
const AGENT_COLUMNS: &str = r#"
    agent_id, agent_name, agent_type, version, tier,
    executor_type, model, temperature, mcp_servers, description, author,
    system_prompt, visibility, user_id, tags,
    current_ontology_commit, current_ontology_snapshot_id,
    last_consolidated_at, total_executions, successful_executions,
    failed_executions, total_cost_usd, avg_execution_time_ms,
    dreaming_budget_credits, dreaming_credits_used, dreaming_budget_reset_at,
    education_budget_credits, education_credits_used, auto_collect_pct, display_alias,
    llm_provider, embedding_provider, embedding_model, embedding_dimension,
    sample_queries,
    status, fork_pricing, forked_from, fork_count,
    accepts, produces, workflow_template, prompt_template, requires_secrets,
    model_ladder, min_tier, capability_gates
"#;

pub struct MemoryStore {
    pool: PgPool,
}

impl MemoryStore {
    /// Create a new MemoryStore with database connection
    pub async fn new(database_url: &str) -> Result<Self> {
        // Neon uses PgBouncer in transaction mode — disable prepared statement
        // cache to avoid "prepared statement does not exist" errors
        let connect_options = PgConnectOptions::from_str(database_url)?.statement_cache_capacity(0);

        let pool = PgPoolOptions::new()
            .max_connections(20)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .connect_with(connect_options)
            .await?;

        Ok(Self { pool })
    }

    /// Create a MemoryStore from an existing pool (shared pool pattern)
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Store an episode in episodic memory
    pub async fn store_episode(&self, episode: Episode) -> Result<Uuid> {
        let embedding_vec = episode
            .embedding
            .as_ref()
            .map(|e| pgvector::Vector::from(e.clone()));

        let row = sqlx::query(
            r#"
            INSERT INTO episodes (
                episode_id, agent_id, timestamp_ref, query, context,
                execution_status, error_details, execution_time_ms,
                tokens_used, cost_usd, embedding, consolidated, tags
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            RETURNING episode_id
            "#,
        )
        .bind(episode.episode_id)
        .bind(episode.agent_id)
        .bind(episode.timestamp_ref)
        .bind(&episode.query)
        .bind(&episode.context)
        .bind(episode.execution_status.to_string())
        .bind(&episode.error_details)
        .bind(episode.execution_time_ms)
        .bind(episode.tokens_used)
        .bind(episode.cost_usd)
        .bind(embedding_vec)
        .bind(episode.consolidated)
        .bind(&episode.tags)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.try_get("episode_id")?)
    }

    /// Get an episode by ID
    pub async fn get_episode(&self, episode_id: Uuid) -> Result<Episode> {
        let row = sqlx::query(
            r#"
            SELECT
                episode_id, agent_id, timestamp_ref, query, context,
                execution_status, error_details, execution_time_ms,
                tokens_used, cost_usd, embedding, consolidated
            FROM episodes
            WHERE episode_id = $1
            "#,
        )
        .bind(episode_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| MemoryError::NotFound(format!("Episode {} not found", episode_id)))?;

        let embedding: Option<pgvector::Vector> = row.try_get("embedding")?;

        Ok(Episode {
            episode_id: row.try_get("episode_id")?,
            agent_id: row.try_get("agent_id")?,
            timestamp_ref: row.try_get("timestamp_ref")?,
            query: row.try_get("query")?,
            context: row.try_get("context")?,
            execution_status: row
                .try_get::<String, _>("execution_status")?
                .parse()
                .unwrap(),
            error_details: row.try_get("error_details")?,
            execution_time_ms: row.try_get("execution_time_ms")?,
            tokens_used: row.try_get("tokens_used")?,
            cost_usd: row.try_get("cost_usd")?,
            embedding: embedding.map(|v| v.to_vec()),
            consolidated: row.try_get("consolidated")?,
            tags: row.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
        })
    }

    /// Get unconsolidated episodes for an agent
    pub async fn get_unconsolidated_episodes(&self, agent_id: Uuid) -> Result<Vec<Episode>> {
        let rows = sqlx::query(
            r#"
            SELECT
                episode_id, agent_id, timestamp_ref, query, context,
                execution_status, error_details, execution_time_ms,
                tokens_used, cost_usd, embedding, consolidated
            FROM episodes
            WHERE agent_id = $1 AND NOT consolidated
            ORDER BY timestamp_ref DESC
            "#,
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        let mut episodes = Vec::new();
        for row in rows {
            let embedding: Option<pgvector::Vector> = row.try_get("embedding")?;

            episodes.push(Episode {
                episode_id: row.try_get("episode_id")?,
                agent_id: row.try_get("agent_id")?,
                timestamp_ref: row.try_get("timestamp_ref")?,
                query: row.try_get("query")?,
                context: row.try_get("context")?,
                execution_status: row
                    .try_get::<String, _>("execution_status")?
                    .parse()
                    .unwrap(),
                error_details: row.try_get("error_details")?,
                execution_time_ms: row.try_get("execution_time_ms")?,
                tokens_used: row.try_get("tokens_used")?,
                cost_usd: row.try_get("cost_usd")?,
                embedding: embedding.map(|v| v.to_vec()),
                consolidated: row.try_get("consolidated")?,
                tags: row.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
            });
        }

        Ok(episodes)
    }

    /// Create or update an agent (used for seeding curated agents)
    pub async fn upsert_agent(&self, agent: Agent) -> Result<Uuid> {
        let row = sqlx::query(
            r#"
            INSERT INTO agents (
                agent_id, agent_name, agent_type, version, tier,
                executor_type, model, temperature, mcp_servers, description, author,
                system_prompt, visibility, user_id, tags,
                dreaming_budget_credits, dreaming_credits_used,
                education_budget_credits, education_credits_used, display_alias,
                llm_provider, embedding_provider, embedding_model, embedding_dimension,
                sample_queries, status, fork_pricing, forked_from, fork_count,
                accepts, produces, workflow_template, prompt_template, requires_secrets,
                model_ladder, min_tier, capability_gates
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31, $32, $33, $34, $35, $36, $37)
            ON CONFLICT (agent_name)
            DO UPDATE SET
                agent_type = EXCLUDED.agent_type,
                version = EXCLUDED.version,
                tier = EXCLUDED.tier,
                executor_type = EXCLUDED.executor_type,
                model = EXCLUDED.model,
                temperature = EXCLUDED.temperature,
                mcp_servers = EXCLUDED.mcp_servers,
                description = EXCLUDED.description,
                system_prompt = EXCLUDED.system_prompt,
                sample_queries = EXCLUDED.sample_queries,
                status = EXCLUDED.status,
                accepts = EXCLUDED.accepts,
                produces = EXCLUDED.produces,
                workflow_template = EXCLUDED.workflow_template,
                prompt_template = EXCLUDED.prompt_template,
                requires_secrets = EXCLUDED.requires_secrets,
                model_ladder = EXCLUDED.model_ladder,
                min_tier = EXCLUDED.min_tier,
                capability_gates = EXCLUDED.capability_gates
            RETURNING agent_id
            "#,
        )
        .bind(agent.agent_id)
        .bind(&agent.agent_name)
        .bind(&agent.agent_type)
        .bind(&agent.version)
        .bind(&agent.tier)
        .bind(&agent.executor_type)
        .bind(&agent.model)
        .bind(agent.temperature)
        .bind(&agent.mcp_servers)
        .bind(&agent.description)
        .bind(&agent.author)
        .bind(&agent.system_prompt)
        .bind(&agent.visibility)
        .bind(&agent.owner_id)
        .bind(&agent.tags)
        .bind(agent.dreaming_budget_credits)
        .bind(agent.dreaming_credits_used)
        .bind(agent.education_budget_credits)
        .bind(agent.education_credits_used)
        .bind(&agent.display_alias)
        .bind(&agent.llm_provider)
        .bind(&agent.embedding_provider)
        .bind(&agent.embedding_model)
        .bind(agent.embedding_dimension)
        .bind(&agent.sample_queries)
        .bind(&agent.status)
        .bind(&agent.fork_pricing)
        .bind(agent.forked_from)
        .bind(agent.fork_count)
        .bind(&agent.accepts)
        .bind(&agent.produces)
        .bind(&agent.workflow_template)
        .bind(&agent.prompt_template)
        .bind(&agent.requires_secrets)
        .bind(&agent.model_ladder)
        .bind(&agent.min_tier)
        .bind(&agent.capability_gates)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.try_get("agent_id")?)
    }

    /// Get agent by ID
    pub async fn get_agent(&self, agent_id: Uuid) -> Result<Option<Agent>> {
        let query = format!("SELECT {} FROM agents WHERE agent_id = $1", AGENT_COLUMNS);
        let row = sqlx::query(&query)
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => Ok(Some(Self::row_to_agent(&row)?)),
            None => Ok(None),
        }
    }

    /// Get agent by name
    pub async fn get_agent_by_name(&self, agent_name: &str) -> Result<Agent> {
        let query = format!("SELECT {} FROM agents WHERE agent_name = $1", AGENT_COLUMNS);
        let row = sqlx::query(&query)
            .bind(agent_name)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| MemoryError::NotFound(format!("Agent {} not found", agent_name)))?;

        Self::row_to_agent(&row)
    }

    /// List all agents
    pub async fn list_agents(&self) -> Result<Vec<Agent>> {
        let query = format!("SELECT {} FROM agents ORDER BY agent_name", AGENT_COLUMNS);
        let rows = sqlx::query(&query).fetch_all(&self.pool).await?;

        let mut agents = Vec::new();
        for row in rows {
            agents.push(Self::row_to_agent(&row)?);
        }

        Ok(agents)
    }

    /// List public agents (for anonymous catalogue)
    pub async fn list_public_agents(&self) -> Result<Vec<Agent>> {
        let query = format!(
            "SELECT {} FROM agents WHERE visibility = 'public' ORDER BY agent_name",
            AGENT_COLUMNS
        );
        let rows = sqlx::query(&query).fetch_all(&self.pool).await?;

        let mut agents = Vec::new();
        for row in rows {
            agents.push(Self::row_to_agent(&row)?);
        }

        Ok(agents)
    }

    /// List agents owned by a specific user
    pub async fn list_agents_for_owner(&self, owner_id: &str) -> Result<Vec<Agent>> {
        let query = format!(
            "SELECT {} FROM agents WHERE user_id = $1 ORDER BY agent_name",
            AGENT_COLUMNS
        );
        let rows = sqlx::query(&query)
            .bind(owner_id)
            .fetch_all(&self.pool)
            .await?;

        let mut agents = Vec::new();
        for row in rows {
            agents.push(Self::row_to_agent(&row)?);
        }

        Ok(agents)
    }

    /// Create a new agent (INSERT only, no upsert)
    pub async fn create_agent(&self, agent: &Agent) -> Result<Uuid> {
        let row = sqlx::query(
            r#"
            INSERT INTO agents (
                agent_id, agent_name, agent_type, version, tier,
                executor_type, model, temperature, mcp_servers, description, author,
                system_prompt, visibility, user_id, tags,
                dreaming_budget_credits, education_budget_credits, display_alias,
                llm_provider, embedding_provider, embedding_model, embedding_dimension,
                sample_queries, status, fork_pricing, forked_from, fork_count,
                accepts, produces, workflow_template, prompt_template, requires_secrets,
                model_ladder, min_tier, capability_gates
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31, $32, $33, $34, $35)
            RETURNING agent_id
            "#,
        )
        .bind(agent.agent_id)
        .bind(&agent.agent_name)
        .bind(&agent.agent_type)
        .bind(&agent.version)
        .bind(&agent.tier)
        .bind(&agent.executor_type)
        .bind(&agent.model)
        .bind(agent.temperature)
        .bind(&agent.mcp_servers)
        .bind(&agent.description)
        .bind(&agent.author)
        .bind(&agent.system_prompt)
        .bind(&agent.visibility)
        .bind(&agent.owner_id)
        .bind(&agent.tags)
        .bind(agent.dreaming_budget_credits)
        .bind(agent.education_budget_credits)
        .bind(&agent.display_alias)
        .bind(&agent.llm_provider)
        .bind(&agent.embedding_provider)
        .bind(&agent.embedding_model)
        .bind(agent.embedding_dimension)
        .bind(&agent.sample_queries)
        .bind(&agent.status)
        .bind(&agent.fork_pricing)
        .bind(agent.forked_from)
        .bind(agent.fork_count)
        .bind(&agent.accepts)
        .bind(&agent.produces)
        .bind(&agent.workflow_template)
        .bind(&agent.prompt_template)
        .bind(&agent.requires_secrets)
        .bind(&agent.model_ladder)
        .bind(&agent.min_tier)
        .bind(&agent.capability_gates)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.try_get("agent_id")?)
    }

    /// Update an agent with partial fields (owner-only operation)
    pub async fn update_agent(&self, agent_id: Uuid, updates: &AgentUpdate) -> Result<()> {
        let mut set_clauses = Vec::new();
        let mut param_idx = 2u32; // $1 is agent_id

        // Build dynamic SET clause
        if updates.description.is_some() {
            set_clauses.push(format!("description = ${}", param_idx));
            param_idx += 1;
        }
        if updates.system_prompt.is_some() {
            set_clauses.push(format!("system_prompt = ${}", param_idx));
            param_idx += 1;
        }
        if updates.visibility.is_some() {
            set_clauses.push(format!("visibility = ${}", param_idx));
            param_idx += 1;
        }
        if updates.tags.is_some() {
            set_clauses.push(format!("tags = ${}", param_idx));
            param_idx += 1;
        }
        if updates.model.is_some() {
            set_clauses.push(format!("model = ${}", param_idx));
            param_idx += 1;
        }
        if updates.temperature.is_some() {
            set_clauses.push(format!("temperature = ${}", param_idx));
            param_idx += 1;
        }
        if updates.education_budget_credits.is_some() {
            set_clauses.push(format!("education_budget_credits = ${}", param_idx));
            param_idx += 1;
        }
        if updates.display_alias.is_some() {
            set_clauses.push(format!("display_alias = ${}", param_idx));
            param_idx += 1;
        }
        if updates.status.is_some() {
            set_clauses.push(format!("status = ${}", param_idx));
            param_idx += 1;
        }
        if updates.fork_pricing.is_some() {
            set_clauses.push(format!("fork_pricing = ${}", param_idx));
            param_idx += 1;
        }
        if updates.accepts.is_some() {
            set_clauses.push(format!("accepts = ${}", param_idx));
            param_idx += 1;
        }
        if updates.produces.is_some() {
            set_clauses.push(format!("produces = ${}", param_idx));
            param_idx += 1;
        }
        if updates.workflow_template.is_some() {
            set_clauses.push(format!("workflow_template = ${}", param_idx));
            param_idx += 1;
        }
        if updates.prompt_template.is_some() {
            set_clauses.push(format!("prompt_template = ${}", param_idx));
            param_idx += 1;
        }
        if updates.requires_secrets.is_some() {
            set_clauses.push(format!("requires_secrets = ${}", param_idx));
            param_idx += 1;
        }
        if updates.model_ladder.is_some() {
            set_clauses.push(format!("model_ladder = ${}", param_idx));
            param_idx += 1;
        }
        if updates.min_tier.is_some() {
            set_clauses.push(format!("min_tier = ${}", param_idx));
            param_idx += 1;
        }
        if updates.capability_gates.is_some() {
            set_clauses.push(format!("capability_gates = ${}", param_idx));
            let _ = param_idx;
        }

        if set_clauses.is_empty() {
            return Ok(()); // nothing to update
        }

        let sql = format!(
            "UPDATE agents SET {} WHERE agent_id = $1",
            set_clauses.join(", ")
        );
        let mut query = sqlx::query(&sql).bind(agent_id);

        // Bind in same order as set_clauses
        if let Some(ref v) = updates.description {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.system_prompt {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.visibility {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.tags {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.model {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.temperature {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.education_budget_credits {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.display_alias {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.status {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.fork_pricing {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.accepts {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.produces {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.workflow_template {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.prompt_template {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.requires_secrets {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.model_ladder {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.min_tier {
            query = query.bind(v);
        }
        if let Some(ref v) = updates.capability_gates {
            query = query.bind(v);
        }

        query.execute(&self.pool).await?;
        Ok(())
    }

    // ─── Agent Version History ────────────────────────────────────

    /// Snapshot current agent state as a version row
    pub async fn create_agent_version(
        &self,
        agent_id: Uuid,
        changed_by: &str,
    ) -> Result<AgentVersion> {
        let row = sqlx::query(
            "INSERT INTO agent_versions (agent_id, version_number, description, system_prompt, tags, model, temperature, visibility, display_alias, changed_by)
             SELECT agent_id,
                    COALESCE((SELECT MAX(version_number) FROM agent_versions WHERE agent_id = $1), 0) + 1,
                    description, system_prompt, tags, model, temperature, visibility, display_alias, $2
             FROM agents WHERE agent_id = $1
             RETURNING version_id, agent_id, version_number, description, system_prompt, tags, model, temperature, visibility, display_alias, changed_by, created_at"
        )
        .bind(agent_id)
        .bind(changed_by)
        .fetch_one(&self.pool)
        .await?;

        Ok(AgentVersion {
            version_id: row.try_get("version_id")?,
            agent_id: row.try_get("agent_id")?,
            version_number: row.try_get("version_number")?,
            description: row.try_get("description")?,
            system_prompt: row.try_get("system_prompt")?,
            tags: row.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
            model: row.try_get("model")?,
            temperature: row.try_get("temperature")?,
            visibility: row.try_get("visibility")?,
            display_alias: row.try_get("display_alias")?,
            changed_by: row.try_get("changed_by")?,
            created_at: row.try_get("created_at")?,
        })
    }

    /// List all versions for an agent (newest first)
    pub async fn list_agent_versions(&self, agent_id: Uuid) -> Result<Vec<AgentVersion>> {
        let rows = sqlx::query(
            "SELECT version_id, agent_id, version_number, description, system_prompt, tags, model, temperature, visibility, display_alias, changed_by, created_at
             FROM agent_versions WHERE agent_id = $1 ORDER BY version_number DESC"
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|row| AgentVersion {
                version_id: row.try_get("version_id").unwrap_or_default(),
                agent_id: row.try_get("agent_id").unwrap_or_default(),
                version_number: row.try_get("version_number").unwrap_or(0),
                description: row.try_get("description").ok(),
                system_prompt: row.try_get("system_prompt").ok(),
                tags: row.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
                model: row.try_get("model").ok(),
                temperature: row.try_get("temperature").ok(),
                visibility: row.try_get("visibility").ok(),
                display_alias: row.try_get("display_alias").ok(),
                changed_by: row.try_get("changed_by").ok(),
                created_at: row.try_get("created_at").unwrap_or_default(),
            })
            .collect())
    }

    /// Get a specific version by number
    pub async fn get_agent_version(
        &self,
        agent_id: Uuid,
        version_number: i32,
    ) -> Result<AgentVersion> {
        let row = sqlx::query(
            "SELECT version_id, agent_id, version_number, description, system_prompt, tags, model, temperature, visibility, display_alias, changed_by, created_at
             FROM agent_versions WHERE agent_id = $1 AND version_number = $2"
        )
        .bind(agent_id)
        .bind(version_number)
        .fetch_one(&self.pool)
        .await?;

        Ok(AgentVersion {
            version_id: row.try_get("version_id")?,
            agent_id: row.try_get("agent_id")?,
            version_number: row.try_get("version_number")?,
            description: row.try_get("description")?,
            system_prompt: row.try_get("system_prompt")?,
            tags: row.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
            model: row.try_get("model")?,
            temperature: row.try_get("temperature")?,
            visibility: row.try_get("visibility")?,
            display_alias: row.try_get("display_alias")?,
            changed_by: row.try_get("changed_by")?,
            created_at: row.try_get("created_at")?,
        })
    }

    pub async fn delete_agent(&self, agent_id: Uuid) -> Result<()> {
        // Delete in dependency order
        sqlx::query("DELETE FROM facts WHERE agent_id = $1")
            .bind(agent_id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM communities WHERE agent_id = $1")
            .bind(agent_id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM entities WHERE agent_id = $1")
            .bind(agent_id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM semantic_rules WHERE agent_id = $1")
            .bind(agent_id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM consolidation_jobs WHERE agent_id = $1")
            .bind(agent_id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM episodes WHERE agent_id = $1")
            .bind(agent_id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM agents WHERE agent_id = $1")
            .bind(agent_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Map a database row to an Agent struct
    fn row_to_agent(row: &sqlx::postgres::PgRow) -> Result<Agent> {
        Ok(Agent {
            agent_id: row.try_get("agent_id")?,
            agent_name: row.try_get("agent_name")?,
            agent_type: row.try_get("agent_type")?,
            version: row.try_get("version")?,
            tier: row.try_get("tier")?,
            executor_type: row.try_get("executor_type")?,
            model: row.try_get("model")?,
            temperature: row.try_get("temperature")?,
            mcp_servers: row.try_get("mcp_servers")?,
            description: row.try_get("description")?,
            author: row.try_get("author")?,
            system_prompt: row.try_get("system_prompt")?,
            visibility: row
                .try_get::<Option<String>, _>("visibility")?
                .unwrap_or_else(|| "public".to_string()),
            owner_id: row.try_get("user_id")?,
            tags: row
                .try_get::<Option<Vec<String>>, _>("tags")?
                .unwrap_or_default(),
            current_ontology_commit: row.try_get("current_ontology_commit")?,
            current_ontology_snapshot_id: row.try_get("current_ontology_snapshot_id")?,
            last_consolidated_at: row.try_get("last_consolidated_at")?,
            total_executions: row.try_get("total_executions").unwrap_or(0),
            successful_executions: row.try_get("successful_executions").unwrap_or(0),
            failed_executions: row.try_get("failed_executions").unwrap_or(0),
            total_cost_usd: row.try_get("total_cost_usd").ok(),
            avg_execution_time_ms: row.try_get("avg_execution_time_ms").unwrap_or(0),
            dreaming_budget_credits: row.try_get("dreaming_budget_credits")?,
            dreaming_credits_used: row.try_get("dreaming_credits_used")?,
            dreaming_budget_reset_at: row.try_get("dreaming_budget_reset_at")?,
            education_budget_credits: row.try_get("education_budget_credits").unwrap_or(0),
            education_credits_used: row.try_get("education_credits_used").unwrap_or(0),
            auto_collect_pct: row.try_get("auto_collect_pct").unwrap_or(0),
            display_alias: row.try_get("display_alias").unwrap_or(None),
            llm_provider: row
                .try_get("llm_provider")
                .unwrap_or_else(|_| "anthropic".to_string()),
            embedding_provider: row
                .try_get("embedding_provider")
                .unwrap_or_else(|_| "anthropic".to_string()),
            embedding_model: row
                .try_get("embedding_model")
                .unwrap_or_else(|_| "voyage-2".to_string()),
            embedding_dimension: row.try_get("embedding_dimension").unwrap_or(1024),
            sample_queries: row
                .try_get::<Option<Vec<String>>, _>("sample_queries")?
                .unwrap_or_default(),
            status: row
                .try_get("status")
                .unwrap_or_else(|_| "draft".to_string()),
            fork_pricing: row.try_get("fork_pricing").unwrap_or(None),
            forked_from: row.try_get("forked_from").unwrap_or(None),
            fork_count: row.try_get("fork_count").unwrap_or(0),
            accepts: row
                .try_get::<Option<Vec<String>>, _>("accepts")?
                .unwrap_or_default(),
            produces: row
                .try_get::<Option<Vec<String>>, _>("produces")?
                .unwrap_or_default(),
            workflow_template: row.try_get("workflow_template").unwrap_or(None),
            prompt_template: row.try_get("prompt_template").unwrap_or(None),
            requires_secrets: row.try_get("requires_secrets").unwrap_or(None),
            model_ladder: row
                .try_get("model_ladder")
                .unwrap_or_else(|_| serde_json::Value::Array(vec![])),
            min_tier: row
                .try_get("min_tier")
                .unwrap_or_else(|_| "free".to_string()),
            capability_gates: row
                .try_get("capability_gates")
                .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new())),
        })
    }

    /// Search for similar episodes using vector similarity
    pub async fn search_similar_episodes(
        &self,
        agent_id: Uuid,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(Episode, f64)>> {
        let query_vec = pgvector::Vector::from(query_embedding.to_vec());

        let rows = sqlx::query(
            r#"
            SELECT
                episode_id, agent_id, timestamp_ref, query, context,
                execution_status, error_details, execution_time_ms,
                tokens_used, cost_usd, embedding, consolidated,
                embedding <=> $1 AS distance
            FROM episodes
            WHERE agent_id = $2
              AND embedding IS NOT NULL
            ORDER BY embedding <=> $1
            LIMIT $3
            "#,
        )
        .bind(&query_vec)
        .bind(agent_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::new();
        for row in rows {
            let embedding: Option<pgvector::Vector> = row.try_get("embedding")?;
            let distance: f64 = row.try_get("distance")?;

            let episode = Episode {
                episode_id: row.try_get("episode_id")?,
                agent_id: row.try_get("agent_id")?,
                timestamp_ref: row.try_get("timestamp_ref")?,
                query: row.try_get("query")?,
                context: row.try_get("context")?,
                execution_status: row
                    .try_get::<String, _>("execution_status")?
                    .parse()
                    .unwrap(),
                error_details: row.try_get("error_details")?,
                execution_time_ms: row.try_get("execution_time_ms")?,
                tokens_used: row.try_get("tokens_used")?,
                cost_usd: row.try_get("cost_usd")?,
                embedding: embedding.map(|v| v.to_vec()),
                consolidated: row.try_get("consolidated")?,
                tags: row.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
            };

            results.push((episode, distance));
        }

        Ok(results)
    }

    /// Search for similar failure episodes (for clustering)
    pub async fn search_similar_failures(
        &self,
        agent_id: Uuid,
        query_embedding: &[f32],
        max_distance: f32,
        limit: usize,
    ) -> Result<Vec<(Episode, f64)>> {
        let query_vec = pgvector::Vector::from(query_embedding.to_vec());

        let rows = sqlx::query(
            r#"
            SELECT
                episode_id, agent_id, timestamp_ref, query, context,
                execution_status, error_details, execution_time_ms,
                tokens_used, cost_usd, embedding, consolidated,
                embedding <=> $1 AS distance
            FROM episodes
            WHERE agent_id = $2
              AND execution_status = 'failure'
              AND NOT consolidated
              AND embedding IS NOT NULL
              AND embedding <=> $1 < $3
            ORDER BY embedding <=> $1
            LIMIT $4
            "#,
        )
        .bind(&query_vec)
        .bind(agent_id)
        .bind(max_distance)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::new();
        for row in rows {
            let embedding: Option<pgvector::Vector> = row.try_get("embedding")?;
            let distance: f64 = row.try_get("distance")?;

            let episode = Episode {
                episode_id: row.try_get("episode_id")?,
                agent_id: row.try_get("agent_id")?,
                timestamp_ref: row.try_get("timestamp_ref")?,
                query: row.try_get("query")?,
                context: row.try_get("context")?,
                execution_status: row
                    .try_get::<String, _>("execution_status")?
                    .parse()
                    .unwrap(),
                error_details: row.try_get("error_details")?,
                execution_time_ms: row.try_get("execution_time_ms")?,
                tokens_used: row.try_get("tokens_used")?,
                cost_usd: row.try_get("cost_usd")?,
                embedding: embedding.map(|v| v.to_vec()),
                consolidated: row.try_get("consolidated")?,
                tags: row.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
            };

            results.push((episode, distance));
        }

        Ok(results)
    }

    /// Get all unconsolidated failure episodes with embeddings (for DBSCAN clustering)
    pub async fn get_failure_episodes_with_embeddings(
        &self,
        agent_id: Uuid,
    ) -> Result<Vec<Episode>> {
        let rows = sqlx::query(
            r#"
            SELECT
                episode_id, agent_id, timestamp_ref, query, context,
                execution_status, error_details, execution_time_ms,
                tokens_used, cost_usd, embedding, consolidated
            FROM episodes
            WHERE agent_id = $1
              AND execution_status = 'failure'
              AND NOT consolidated
              AND embedding IS NOT NULL
            ORDER BY timestamp_ref DESC
            "#,
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        let mut episodes = Vec::new();
        for row in rows {
            let embedding: Option<pgvector::Vector> = row.try_get("embedding")?;

            episodes.push(Episode {
                episode_id: row.try_get("episode_id")?,
                agent_id: row.try_get("agent_id")?,
                timestamp_ref: row.try_get("timestamp_ref")?,
                query: row.try_get("query")?,
                context: row.try_get("context")?,
                execution_status: row
                    .try_get::<String, _>("execution_status")?
                    .parse()
                    .unwrap(),
                error_details: row.try_get("error_details")?,
                execution_time_ms: row.try_get("execution_time_ms")?,
                tokens_used: row.try_get("tokens_used")?,
                cost_usd: row.try_get("cost_usd")?,
                embedding: embedding.map(|v| v.to_vec()),
                consolidated: row.try_get("consolidated")?,
                tags: row.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
            });
        }

        Ok(episodes)
    }

    /// Marks a batch of episodes as consolidated and links them to a consolidation job
    pub async fn mark_episodes_consolidated(
        &self,
        episode_ids: &[Uuid],
        consolidation_job_id: Uuid,
    ) -> Result<usize> {
        if episode_ids.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query(
            "UPDATE episodes
             SET consolidated = true, consolidation_job_id = $1
             WHERE episode_id = ANY($2)",
        )
        .bind(consolidation_job_id)
        .bind(episode_ids)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() as usize)
    }

    /// Creates a new consolidation job
    pub async fn create_consolidation_job(
        &self,
        agent_id: Uuid,
        episode_range_start: Uuid,
        episode_range_end: Uuid,
    ) -> Result<Uuid> {
        let job_id = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO consolidation_jobs
             (job_id, agent_id, status, started_at, episode_range_start, episode_range_end)
             VALUES ($1, $2, 'running', NOW(), $3, $4)",
        )
        .bind(job_id)
        .bind(agent_id)
        .bind(episode_range_start)
        .bind(episode_range_end)
        .execute(&self.pool)
        .await?;

        Ok(job_id)
    }

    /// Updates consolidation job statistics
    #[allow(clippy::too_many_arguments)]
    pub async fn update_consolidation_job(
        &self,
        job_id: Uuid,
        episodes_processed: i32,
        clusters_identified: i32,
        rules_extracted: i32,
        rules_verified: i32,
        rules_rejected: i32,
        entities_created: i32,
        facts_created: i32,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE consolidation_jobs
             SET episodes_processed = $2,
                 clusters_identified = $3,
                 rules_extracted = $4,
                 rules_verified = $5,
                 rules_rejected = $6,
                 entities_created = $7,
                 facts_created = $8
             WHERE job_id = $1",
        )
        .bind(job_id)
        .bind(episodes_processed)
        .bind(clusters_identified)
        .bind(rules_extracted)
        .bind(rules_verified)
        .bind(rules_rejected)
        .bind(entities_created)
        .bind(facts_created)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Marks a consolidation job as completed
    pub async fn complete_consolidation_job(
        &self,
        job_id: Uuid,
        status: &str, // 'completed' or 'failed'
        error_message: Option<String>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE consolidation_jobs
             SET status = $2,
                 completed_at = NOW(),
                 duration_ms = EXTRACT(EPOCH FROM (NOW() - started_at)) * 1000,
                 error_message = $3
             WHERE job_id = $1",
        )
        .bind(job_id)
        .bind(status)
        .bind(error_message)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Retrieves a consolidation job by ID
    pub async fn get_consolidation_job(&self, job_id: Uuid) -> Result<ConsolidationJob> {
        let row = sqlx::query(
            "SELECT job_id, agent_id, started_at, completed_at, duration_ms,
                    status, error_message, episode_range_start, episode_range_end,
                    episodes_processed, clusters_identified, rules_extracted,
                    rules_verified, rules_rejected, entities_created, facts_created
             FROM consolidation_jobs
             WHERE job_id = $1",
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| MemoryError::NotFound(format!("Consolidation job {} not found", job_id)))?;

        Ok(ConsolidationJob {
            job_id: row.try_get("job_id")?,
            agent_id: row.try_get("agent_id")?,
            started_at: row.try_get("started_at")?,
            completed_at: row.try_get("completed_at")?,
            duration_ms: row.try_get("duration_ms")?,
            status: row.try_get("status")?,
            error_message: row.try_get("error_message")?,
            episode_range_start: row.try_get("episode_range_start")?,
            episode_range_end: row.try_get("episode_range_end")?,
            episodes_processed: row.try_get("episodes_processed")?,
            clusters_identified: row.try_get("clusters_identified")?,
            rules_extracted: row.try_get("rules_extracted")?,
            rules_verified: row.try_get("rules_verified")?,
            rules_rejected: row.try_get("rules_rejected")?,
            entities_created: row.try_get("entities_created")?,
            facts_created: row.try_get("facts_created")?,
        })
    }

    // ========== Semantic Memory Operations ==========

    /// Stores a new semantic rule
    pub async fn store_semantic_rule(&self, rule: SemanticRule) -> Result<()> {
        sqlx::query(
            "INSERT INTO semantic_rules
             (rule_id, agent_id, rule_content, rule_description, confidence_score,
              verification_status, verification_method, source_episode_cluster,
              episode_count, embedding, is_active)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(rule.rule_id)
        .bind(rule.agent_id)
        .bind(&rule.rule_content)
        .bind(&rule.rule_description)
        .bind(rule.confidence_score)
        .bind(rule.verification_status.to_string())
        .bind(&rule.verification_method)
        .bind(&rule.source_episode_cluster)
        .bind(rule.episode_count)
        .bind(
            rule.embedding
                .as_ref()
                .map(|e| pgvector::Vector::from(e.clone())),
        )
        .bind(rule.is_active)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Retrieves a semantic rule by ID
    pub async fn get_semantic_rule(&self, rule_id: Uuid) -> Result<SemanticRule> {
        let row = sqlx::query(
            "SELECT rule_id, agent_id, rule_content, rule_description, confidence_score,
                    verification_status, verification_method, source_episode_cluster,
                    episode_count, embedding, is_active, created_at
             FROM semantic_rules
             WHERE rule_id = $1",
        )
        .bind(rule_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| MemoryError::NotFound(format!("Semantic rule {} not found", rule_id)))?;

        Self::row_to_semantic_rule(&row)
    }

    /// Gets all active semantic rules for an agent
    pub async fn get_agent_semantic_rules(&self, agent_id: Uuid) -> Result<Vec<SemanticRule>> {
        let rows = sqlx::query(
            "SELECT rule_id, agent_id, rule_content, rule_description, confidence_score,
                    verification_status, verification_method, source_episode_cluster,
                    episode_count, embedding, is_active, created_at
             FROM semantic_rules
             WHERE agent_id = $1 AND is_active = true
             ORDER BY confidence_score DESC",
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        let mut rules = Vec::new();
        for row in rows {
            rules.push(Self::row_to_semantic_rule(&row)?);
        }

        Ok(rules)
    }

    /// Map a database row to a SemanticRule struct
    fn row_to_semantic_rule(row: &sqlx::postgres::PgRow) -> Result<SemanticRule> {
        let embedding: Option<pgvector::Vector> = row.try_get("embedding")?;
        Ok(SemanticRule {
            rule_id: row.try_get("rule_id")?,
            agent_id: row.try_get("agent_id")?,
            rule_content: row.try_get("rule_content")?,
            rule_description: row.try_get("rule_description")?,
            confidence_score: row.try_get("confidence_score")?,
            verification_status: row
                .try_get::<String, _>("verification_status")?
                .parse()
                .unwrap(),
            verification_method: row.try_get("verification_method")?,
            source_episode_cluster: row.try_get("source_episode_cluster")?,
            episode_count: row.try_get("episode_count")?,
            embedding: embedding.map(|v| v.to_vec()),
            is_active: row.try_get("is_active")?,
            created_at: row.try_get("created_at")?,
        })
    }

    /// Updates verification status of a semantic rule
    pub async fn update_rule_verification(
        &self,
        rule_id: Uuid,
        status: VerificationStatus,
        method: Option<String>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE semantic_rules
             SET verification_status = $2, verification_method = $3
             WHERE rule_id = $1",
        )
        .bind(rule_id)
        .bind(status.to_string())
        .bind(method)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Deactivates a semantic rule
    pub async fn deactivate_rule(&self, rule_id: Uuid) -> Result<()> {
        sqlx::query(
            "UPDATE semantic_rules
             SET is_active = false
             WHERE rule_id = $1",
        )
        .bind(rule_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ========== Entity Operations ==========

    /// Stores a new entity
    pub async fn store_entity(&self, entity: Entity) -> Result<()> {
        sqlx::query(
            "INSERT INTO entities
             (entity_id, agent_id, entity_name, entity_type, summary,
              t_valid, t_invalid, source_episodes, extraction_confidence, embedding)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(entity.entity_id)
        .bind(entity.agent_id)
        .bind(&entity.entity_name)
        .bind(&entity.entity_type)
        .bind(&entity.summary)
        .bind(entity.t_valid)
        .bind(entity.t_invalid)
        .bind(&entity.source_episodes)
        .bind(entity.extraction_confidence)
        .bind(
            entity
                .embedding
                .as_ref()
                .map(|e| pgvector::Vector::from(e.clone())),
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Retrieves an entity by ID
    pub async fn get_entity(&self, entity_id: Uuid) -> Result<Entity> {
        let row = sqlx::query(
            "SELECT entity_id, agent_id, entity_name, entity_type, summary,
                    t_valid, t_invalid, source_episodes, extraction_confidence, embedding
             FROM entities
             WHERE entity_id = $1 AND (t_invalid IS NULL OR t_invalid > NOW())",
        )
        .bind(entity_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| MemoryError::NotFound(format!("Entity {} not found", entity_id)))?;

        let embedding: Option<pgvector::Vector> = row.try_get("embedding")?;

        Ok(Entity {
            entity_id: row.try_get("entity_id")?,
            agent_id: row.try_get("agent_id")?,
            entity_name: row.try_get("entity_name")?,
            entity_type: row.try_get("entity_type")?,
            summary: row.try_get("summary")?,
            t_valid: row.try_get("t_valid")?,
            t_invalid: row.try_get("t_invalid")?,
            source_episodes: row.try_get("source_episodes")?,
            extraction_confidence: row.try_get("extraction_confidence")?,
            embedding: embedding.map(|v| v.to_vec()),
        })
    }

    /// Gets all active entities for an agent
    pub async fn get_agent_entities(&self, agent_id: Uuid) -> Result<Vec<Entity>> {
        let rows = sqlx::query(
            "SELECT entity_id, agent_id, entity_name, entity_type, summary,
                    t_valid, t_invalid, source_episodes, extraction_confidence, embedding
             FROM entities
             WHERE agent_id = $1 AND (t_invalid IS NULL OR t_invalid > NOW())
             ORDER BY entity_name",
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        let mut entities = Vec::new();
        for row in rows {
            let embedding: Option<pgvector::Vector> = row.try_get("embedding")?;
            entities.push(Entity {
                entity_id: row.try_get("entity_id")?,
                agent_id: row.try_get("agent_id")?,
                entity_name: row.try_get("entity_name")?,
                entity_type: row.try_get("entity_type")?,
                summary: row.try_get("summary")?,
                t_valid: row.try_get("t_valid")?,
                t_invalid: row.try_get("t_invalid")?,
                source_episodes: row.try_get("source_episodes")?,
                extraction_confidence: row.try_get("extraction_confidence")?,
                embedding: embedding.map(|v| v.to_vec()),
            });
        }

        Ok(entities)
    }

    /// Invalidates an entity (soft delete with bi-temporal tracking)
    pub async fn invalidate_entity(&self, entity_id: Uuid) -> Result<()> {
        sqlx::query(
            "UPDATE entities
             SET t_invalid = NOW()
             WHERE entity_id = $1",
        )
        .bind(entity_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ========== Fact Operations ==========

    /// Stores a new fact (relationship between entities)
    pub async fn store_fact(&self, fact: Fact) -> Result<()> {
        sqlx::query(
            "INSERT INTO facts
             (fact_id, agent_id, source_entity_id, target_entity_id, relation_type,
              relation_cardinality, confidence, reasoning, t_valid, t_invalid, source_episodes)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(fact.fact_id)
        .bind(fact.agent_id)
        .bind(fact.source_entity_id)
        .bind(fact.target_entity_id)
        .bind(&fact.relation_type)
        .bind(fact.relation_cardinality.to_string())
        .bind(fact.confidence)
        .bind(&fact.reasoning)
        .bind(fact.t_valid)
        .bind(fact.t_invalid)
        .bind(&fact.source_episodes)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Retrieves a fact by ID
    pub async fn get_fact(&self, fact_id: Uuid) -> Result<Fact> {
        let row = sqlx::query(
            "SELECT fact_id, agent_id, source_entity_id, target_entity_id, relation_type,
                    relation_cardinality, confidence, reasoning, t_valid, t_invalid, source_episodes
             FROM facts
             WHERE fact_id = $1 AND (t_invalid IS NULL OR t_invalid > NOW())",
        )
        .bind(fact_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| MemoryError::NotFound(format!("Fact {} not found", fact_id)))?;

        Ok(Fact {
            fact_id: row.try_get("fact_id")?,
            agent_id: row.try_get("agent_id")?,
            source_entity_id: row.try_get("source_entity_id")?,
            target_entity_id: row.try_get("target_entity_id")?,
            relation_type: row.try_get("relation_type")?,
            relation_cardinality: row
                .try_get::<String, _>("relation_cardinality")?
                .parse()
                .unwrap(),
            confidence: row.try_get("confidence")?,
            reasoning: row.try_get("reasoning")?,
            t_valid: row.try_get("t_valid")?,
            t_invalid: row.try_get("t_invalid")?,
            source_episodes: row.try_get("source_episodes")?,
        })
    }

    /// Gets all active facts for an agent
    pub async fn get_agent_facts(&self, agent_id: Uuid) -> Result<Vec<Fact>> {
        let rows = sqlx::query(
            "SELECT fact_id, agent_id, source_entity_id, target_entity_id, relation_type,
                    relation_cardinality, confidence, reasoning, t_valid, t_invalid, source_episodes
             FROM facts
             WHERE agent_id = $1 AND (t_invalid IS NULL OR t_invalid > NOW())
             ORDER BY confidence DESC",
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        let mut facts = Vec::new();
        for row in rows {
            facts.push(Fact {
                fact_id: row.try_get("fact_id")?,
                agent_id: row.try_get("agent_id")?,
                source_entity_id: row.try_get("source_entity_id")?,
                target_entity_id: row.try_get("target_entity_id")?,
                relation_type: row.try_get("relation_type")?,
                relation_cardinality: row
                    .try_get::<String, _>("relation_cardinality")?
                    .parse()
                    .unwrap(),
                confidence: row.try_get("confidence")?,
                reasoning: row.try_get("reasoning")?,
                t_valid: row.try_get("t_valid")?,
                t_invalid: row.try_get("t_invalid")?,
                source_episodes: row.try_get("source_episodes")?,
            });
        }

        Ok(facts)
    }

    /// Gets facts involving a specific entity
    pub async fn get_entity_facts(&self, entity_id: Uuid) -> Result<Vec<Fact>> {
        let rows = sqlx::query(
            "SELECT fact_id, agent_id, source_entity_id, target_entity_id, relation_type,
                    relation_cardinality, confidence, reasoning, t_valid, t_invalid, source_episodes
             FROM facts
             WHERE (source_entity_id = $1 OR target_entity_id = $1)
               AND (t_invalid IS NULL OR t_invalid > NOW())
             ORDER BY confidence DESC",
        )
        .bind(entity_id)
        .fetch_all(&self.pool)
        .await?;

        let mut facts = Vec::new();
        for row in rows {
            facts.push(Fact {
                fact_id: row.try_get("fact_id")?,
                agent_id: row.try_get("agent_id")?,
                source_entity_id: row.try_get("source_entity_id")?,
                target_entity_id: row.try_get("target_entity_id")?,
                relation_type: row.try_get("relation_type")?,
                relation_cardinality: row
                    .try_get::<String, _>("relation_cardinality")?
                    .parse()
                    .unwrap(),
                confidence: row.try_get("confidence")?,
                reasoning: row.try_get("reasoning")?,
                t_valid: row.try_get("t_valid")?,
                t_invalid: row.try_get("t_invalid")?,
                source_episodes: row.try_get("source_episodes")?,
            });
        }

        Ok(facts)
    }

    /// Invalidates a fact (soft delete with bi-temporal tracking)
    pub async fn invalidate_fact(&self, fact_id: Uuid) -> Result<()> {
        sqlx::query(
            "UPDATE facts
             SET t_invalid = NOW()
             WHERE fact_id = $1",
        )
        .bind(fact_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ========== Paginated Episode Queries ==========

    /// Get paginated episodes for an agent (newest first, no embeddings for performance)
    pub async fn get_episodes_paginated(
        &self,
        agent_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Episode>, i64)> {
        let count: i64 = sqlx::query("SELECT COUNT(*) as cnt FROM episodes WHERE agent_id = $1")
            .bind(agent_id)
            .fetch_one(&self.pool)
            .await?
            .try_get("cnt")?;

        let rows = sqlx::query(
            r#"
            SELECT
                episode_id, agent_id, timestamp_ref, query, context,
                execution_status, error_details, execution_time_ms,
                tokens_used, cost_usd, consolidated
            FROM episodes
            WHERE agent_id = $1
            ORDER BY timestamp_ref DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(agent_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let mut episodes = Vec::new();
        for row in rows {
            episodes.push(Episode {
                episode_id: row.try_get("episode_id")?,
                agent_id: row.try_get("agent_id")?,
                timestamp_ref: row.try_get("timestamp_ref")?,
                query: row.try_get("query")?,
                context: row.try_get("context")?,
                execution_status: row
                    .try_get::<String, _>("execution_status")?
                    .parse()
                    .unwrap(),
                error_details: row.try_get("error_details")?,
                execution_time_ms: row.try_get("execution_time_ms")?,
                tokens_used: row.try_get("tokens_used")?,
                cost_usd: row.try_get("cost_usd")?,
                embedding: None, // omit for performance
                consolidated: row.try_get("consolidated")?,
                tags: row.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
            });
        }

        Ok((episodes, count))
    }

    // ========== Projector Query Methods ==========

    /// Get all episodes with non-null embeddings for an agent
    pub async fn get_all_episodes_with_embeddings(&self, agent_id: Uuid) -> Result<Vec<Episode>> {
        let rows = sqlx::query(
            r#"
            SELECT
                episode_id, agent_id, timestamp_ref, query, context,
                execution_status, error_details, execution_time_ms,
                tokens_used, cost_usd, embedding, consolidated
            FROM episodes
            WHERE agent_id = $1 AND embedding IS NOT NULL
            ORDER BY timestamp_ref ASC
            "#,
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        let mut episodes = Vec::new();
        for row in rows {
            let embedding: Option<pgvector::Vector> = row.try_get("embedding")?;
            episodes.push(Episode {
                episode_id: row.try_get("episode_id")?,
                agent_id: row.try_get("agent_id")?,
                timestamp_ref: row.try_get("timestamp_ref")?,
                query: row.try_get("query")?,
                context: row.try_get("context")?,
                execution_status: row
                    .try_get::<String, _>("execution_status")?
                    .parse()
                    .unwrap(),
                error_details: row.try_get("error_details")?,
                execution_time_ms: row.try_get("execution_time_ms")?,
                tokens_used: row.try_get("tokens_used")?,
                cost_usd: row.try_get("cost_usd")?,
                embedding: embedding.map(|v| v.to_vec()),
                consolidated: row.try_get("consolidated")?,
                tags: row.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
            });
        }

        Ok(episodes)
    }

    /// Get all communities for an agent
    pub async fn get_agent_communities(&self, agent_id: Uuid) -> Result<Vec<Community>> {
        let rows = sqlx::query(
            "SELECT community_id, agent_id, community_name, summary,
                    member_entity_ids, member_count, embedding, created_at
             FROM communities
             WHERE agent_id = $1
             ORDER BY created_at ASC",
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        let mut communities = Vec::new();
        for row in rows {
            let embedding: Option<pgvector::Vector> = row.try_get("embedding")?;
            communities.push(Community {
                community_id: row.try_get("community_id")?,
                agent_id: row.try_get("agent_id")?,
                community_name: row.try_get("community_name")?,
                summary: row.try_get("summary")?,
                member_entity_ids: row.try_get("member_entity_ids")?,
                member_count: row.try_get("member_count")?,
                embedding: embedding.map(|v| v.to_vec()),
                created_at: row.try_get("created_at")?,
            });
        }

        Ok(communities)
    }

    /// Stores a new community
    pub async fn store_community(&self, community: Community) -> Result<()> {
        let embedding_vec = community
            .embedding
            .as_ref()
            .map(|e| pgvector::Vector::from(e.clone()));

        sqlx::query(
            "INSERT INTO communities
             (community_id, agent_id, community_name, summary,
              member_entity_ids, member_count, embedding, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(community.community_id)
        .bind(community.agent_id)
        .bind(&community.community_name)
        .bind(&community.summary)
        .bind(&community.member_entity_ids)
        .bind(community.member_count)
        .bind(embedding_vec)
        .bind(community.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ─── Workspace Messages ────────────────────────────────────────

    pub async fn store_workspace_message(&self, msg: &WorkspaceMessage) -> Result<Uuid> {
        let row = sqlx::query(
            r#"
            INSERT INTO workspace_messages
                (message_id, workspace_id, sender_type, sender_id, sender_name,
                 content, message_type, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING message_id
            "#,
        )
        .bind(msg.message_id)
        .bind(msg.workspace_id)
        .bind(&msg.sender_type)
        .bind(&msg.sender_id)
        .bind(&msg.sender_name)
        .bind(&msg.content)
        .bind(&msg.message_type)
        .bind(&msg.metadata)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.try_get("message_id")?)
    }

    pub async fn get_workspace_messages(
        &self,
        workspace_id: Uuid,
        limit: i64,
        before: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<WorkspaceMessage>> {
        let rows = if let Some(before_ts) = before {
            sqlx::query(
                r#"
                SELECT message_id, workspace_id, sender_type, sender_id, sender_name,
                       content, message_type, metadata, created_at
                FROM workspace_messages
                WHERE workspace_id = $1 AND created_at < $2
                ORDER BY created_at DESC
                LIMIT $3
                "#,
            )
            .bind(workspace_id)
            .bind(before_ts)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT message_id, workspace_id, sender_type, sender_id, sender_name,
                       content, message_type, metadata, created_at
                FROM workspace_messages
                WHERE workspace_id = $1
                ORDER BY created_at DESC
                LIMIT $2
                "#,
            )
            .bind(workspace_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(rows.iter().map(row_to_workspace_message).collect())
    }

    pub async fn get_workspace_messages_since(
        &self,
        workspace_id: Uuid,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<WorkspaceMessage>> {
        let rows = sqlx::query(
            r#"
            SELECT message_id, workspace_id, sender_type, sender_id, sender_name,
                   content, message_type, metadata, created_at
            FROM workspace_messages
            WHERE workspace_id = $1 AND created_at > $2
            ORDER BY created_at ASC
            "#,
        )
        .bind(workspace_id)
        .bind(since)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(row_to_workspace_message).collect())
    }

    // ─── Coherence evaluations ────────────────────────────────────────

    pub async fn store_coherence_evaluation(&self, eval: &CoherenceEvaluation) -> Result<Uuid> {
        let row = sqlx::query(
            r#"
            INSERT INTO coherence_evaluations
                (eval_id, workspace_id, global_score, quality_label,
                 principle_scores, health_indicators, utterance_count, message_window)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING eval_id
            "#,
        )
        .bind(eval.eval_id)
        .bind(eval.workspace_id)
        .bind(eval.global_score)
        .bind(&eval.quality_label)
        .bind(&eval.principle_scores)
        .bind(&eval.health_indicators)
        .bind(eval.utterance_count)
        .bind(&eval.message_window)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.try_get("eval_id")?)
    }

    pub async fn get_latest_coherence(
        &self,
        workspace_id: Uuid,
    ) -> Result<Option<CoherenceEvaluation>> {
        let row = sqlx::query(
            r#"
            SELECT eval_id, workspace_id, global_score, quality_label,
                   principle_scores, health_indicators, utterance_count,
                   message_window, created_at
            FROM coherence_evaluations
            WHERE workspace_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.as_ref().map(row_to_coherence_evaluation))
    }

    pub async fn get_coherence_history(
        &self,
        workspace_id: Uuid,
        limit: i64,
    ) -> Result<Vec<CoherenceEvaluation>> {
        let rows = sqlx::query(
            r#"
            SELECT eval_id, workspace_id, global_score, quality_label,
                   principle_scores, health_indicators, utterance_count,
                   message_window, created_at
            FROM coherence_evaluations
            WHERE workspace_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(workspace_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(row_to_coherence_evaluation).collect())
    }

    pub async fn count_workspace_messages_since(
        &self,
        workspace_id: Uuid,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<i64> {
        let row = sqlx::query(
            "SELECT COUNT(*) as cnt FROM workspace_messages WHERE workspace_id = $1 AND created_at > $2",
        )
        .bind(workspace_id)
        .bind(since)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.try_get::<i64, _>("cnt").unwrap_or(0))
    }

    // ========================================================================
    // EVAL FRAMEWORK
    // ========================================================================

    pub async fn create_eval_test_case(&self, tc: &EvalTestCase) -> Result<Uuid> {
        let row = sqlx::query(
            r#"INSERT INTO eval_test_cases
                (test_case_id, agent_id, query, expected_output, rubric, tags, is_active)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING test_case_id"#,
        )
        .bind(tc.test_case_id)
        .bind(tc.agent_id)
        .bind(&tc.query)
        .bind(&tc.expected_output)
        .bind(&tc.rubric)
        .bind(&tc.tags)
        .bind(tc.is_active)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.try_get("test_case_id")?)
    }

    pub async fn list_eval_test_cases(&self, agent_id: Uuid) -> Result<Vec<EvalTestCase>> {
        let rows = sqlx::query(
            r#"SELECT test_case_id, agent_id, query, expected_output, rubric,
                      tags, is_active, created_at, updated_at
               FROM eval_test_cases
               WHERE agent_id = $1 AND is_active = TRUE
               ORDER BY created_at"#,
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| EvalTestCase {
                test_case_id: r.try_get("test_case_id").unwrap(),
                agent_id: r.try_get("agent_id").unwrap(),
                query: r.try_get("query").unwrap(),
                expected_output: r.try_get("expected_output").unwrap_or(None),
                rubric: r.try_get("rubric").unwrap_or(None),
                tags: r.try_get("tags").unwrap_or_default(),
                is_active: r.try_get("is_active").unwrap(),
                created_at: r.try_get("created_at").unwrap(),
                updated_at: r.try_get("updated_at").unwrap(),
            })
            .collect())
    }

    pub async fn update_eval_test_case(&self, tc: &EvalTestCase) -> Result<()> {
        sqlx::query(
            r#"UPDATE eval_test_cases
               SET query = $2, expected_output = $3, rubric = $4,
                   tags = $5, is_active = $6, updated_at = NOW()
               WHERE test_case_id = $1"#,
        )
        .bind(tc.test_case_id)
        .bind(&tc.query)
        .bind(&tc.expected_output)
        .bind(&tc.rubric)
        .bind(&tc.tags)
        .bind(tc.is_active)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn deactivate_eval_test_case(&self, test_case_id: Uuid) -> Result<()> {
        sqlx::query(
            "UPDATE eval_test_cases SET is_active = FALSE, updated_at = NOW() WHERE test_case_id = $1",
        )
        .bind(test_case_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn create_eval_run(&self, run: &EvalRun) -> Result<Uuid> {
        let row = sqlx::query(
            r#"INSERT INTO eval_runs
                (run_id, agent_id, triggered_by, status, judge_enabled, total_cases)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING run_id"#,
        )
        .bind(run.run_id)
        .bind(run.agent_id)
        .bind(&run.triggered_by)
        .bind(&run.status)
        .bind(run.judge_enabled)
        .bind(run.total_cases)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.try_get("run_id")?)
    }

    pub async fn complete_eval_run(&self, run: &EvalRun) -> Result<()> {
        sqlx::query(
            r#"UPDATE eval_runs SET
                status = $2, total_cases = $3, passed = $4, failed = $5,
                avg_latency_ms = $6, avg_tokens = $7, avg_judge_score = $8,
                total_cost_credits = $9, case_results = $10,
                regression_detected = $11, regression_details = $12,
                completed_at = NOW(),
                duration_ms = EXTRACT(EPOCH FROM (NOW() - started_at))::BIGINT * 1000
               WHERE run_id = $1"#,
        )
        .bind(run.run_id)
        .bind(&run.status)
        .bind(run.total_cases)
        .bind(run.passed)
        .bind(run.failed)
        .bind(run.avg_latency_ms)
        .bind(run.avg_tokens)
        .bind(run.avg_judge_score)
        .bind(run.total_cost_credits)
        .bind(&run.case_results)
        .bind(run.regression_detected)
        .bind(&run.regression_details)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_eval_runs(&self, agent_id: Uuid, limit: i64) -> Result<Vec<EvalRun>> {
        let rows = sqlx::query(
            r#"SELECT run_id, agent_id, triggered_by, status, judge_enabled,
                      total_cases, passed, failed, avg_latency_ms, avg_tokens,
                      avg_judge_score, total_cost_credits, case_results,
                      regression_detected, regression_details,
                      started_at, completed_at, duration_ms
               FROM eval_runs
               WHERE agent_id = $1
               ORDER BY started_at DESC
               LIMIT $2"#,
        )
        .bind(agent_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| EvalRun {
                run_id: r.try_get("run_id").unwrap(),
                agent_id: r.try_get("agent_id").unwrap(),
                triggered_by: r.try_get("triggered_by").unwrap(),
                status: r.try_get("status").unwrap(),
                judge_enabled: r.try_get("judge_enabled").unwrap(),
                total_cases: r.try_get("total_cases").unwrap(),
                passed: r.try_get("passed").unwrap(),
                failed: r.try_get("failed").unwrap(),
                avg_latency_ms: r.try_get("avg_latency_ms").unwrap_or(None),
                avg_tokens: r.try_get("avg_tokens").unwrap_or(None),
                avg_judge_score: r.try_get("avg_judge_score").unwrap_or(None),
                total_cost_credits: r.try_get("total_cost_credits").unwrap(),
                case_results: r
                    .try_get::<serde_json::Value, _>("case_results")
                    .unwrap_or(serde_json::json!([])),
                regression_detected: r.try_get("regression_detected").unwrap(),
                regression_details: r.try_get("regression_details").unwrap_or(None),
                started_at: r.try_get("started_at").unwrap(),
                completed_at: r.try_get("completed_at").unwrap_or(None),
                duration_ms: r.try_get("duration_ms").unwrap_or(None),
            })
            .collect())
    }

    pub async fn seed_eval_test_cases_from_samples(
        &self,
        agent_id: Uuid,
        sample_queries: &[String],
    ) -> Result<i32> {
        let mut seeded = 0i32;
        for q in sample_queries {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM eval_test_cases WHERE agent_id = $1 AND query = $2)",
            )
            .bind(agent_id)
            .bind(q)
            .fetch_one(&self.pool)
            .await
            .unwrap_or(true);

            if !exists {
                sqlx::query(
                    r#"INSERT INTO eval_test_cases (test_case_id, agent_id, query)
                       VALUES ($1, $2, $3)"#,
                )
                .bind(Uuid::new_v4())
                .bind(agent_id)
                .bind(q)
                .execute(&self.pool)
                .await?;
                seeded += 1;
            }
        }
        Ok(seeded)
    }

    // ─── Shopping Profiles & Marketplace ────────────────────────────

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_shopping_profile(
        &self,
        user_id: &str,
        agent_id: Uuid,
        profile_name: &str,
        composite_embedding: Option<&[f32]>,
        episode_count: i32,
        category_tags: &[String],
        price_sensitivity: Option<f64>,
        quality_bias: Option<f64>,
        brand_affinities: &serde_json::Value,
    ) -> Result<Uuid> {
        let embed_vec = composite_embedding.map(|e| pgvector::Vector::from(e.to_vec()));
        let row = sqlx::query(
            r#"INSERT INTO shopping_profiles
                (user_id, agent_id, profile_name, composite_embedding, episode_count,
                 category_tags, price_sensitivity, quality_bias, brand_affinities, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
               ON CONFLICT (user_id, agent_id, profile_name)
               DO UPDATE SET
                 composite_embedding = EXCLUDED.composite_embedding,
                 embedding_version = shopping_profiles.embedding_version + 1,
                 episode_count = EXCLUDED.episode_count,
                 category_tags = EXCLUDED.category_tags,
                 price_sensitivity = EXCLUDED.price_sensitivity,
                 quality_bias = EXCLUDED.quality_bias,
                 brand_affinities = EXCLUDED.brand_affinities,
                 updated_at = NOW()
               RETURNING profile_id"#,
        )
        .bind(user_id)
        .bind(agent_id)
        .bind(profile_name)
        .bind(&embed_vec)
        .bind(episode_count)
        .bind(category_tags)
        .bind(price_sensitivity)
        .bind(quality_bias)
        .bind(brand_affinities)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.try_get("profile_id")?)
    }

    pub async fn get_shopping_profile(
        &self,
        user_id: &str,
        agent_id: Uuid,
        profile_name: &str,
    ) -> Result<Option<ShoppingProfile>> {
        let row = sqlx::query(
            r#"SELECT profile_id, user_id, agent_id, profile_name,
                      embedding_version, episode_count, category_tags,
                      price_sensitivity, quality_bias, brand_affinities,
                      metadata, is_listed, created_at, updated_at
               FROM shopping_profiles
               WHERE user_id = $1 AND agent_id = $2 AND profile_name = $3"#,
        )
        .bind(user_id)
        .bind(agent_id)
        .bind(profile_name)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| row_to_shopping_profile(&r)))
    }

    pub async fn get_user_shopping_profiles(&self, user_id: &str) -> Result<Vec<ShoppingProfile>> {
        let rows = sqlx::query(
            r#"SELECT profile_id, user_id, agent_id, profile_name,
                      embedding_version, episode_count, category_tags,
                      price_sensitivity, quality_bias, brand_affinities,
                      metadata, is_listed, created_at, updated_at
               FROM shopping_profiles WHERE user_id = $1
               ORDER BY updated_at DESC"#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(row_to_shopping_profile).collect())
    }

    pub async fn create_marketplace_listing(
        &self,
        profile_id: Uuid,
        seller_id: &str,
        price_credits: i32,
        max_queries_per_buyer: Option<i32>,
        category_tags: &[String],
        description: Option<&str>,
    ) -> Result<Uuid> {
        // Mark profile as listed
        sqlx::query("UPDATE shopping_profiles SET is_listed = true WHERE profile_id = $1")
            .bind(profile_id)
            .execute(&self.pool)
            .await?;

        let row = sqlx::query(
            r#"INSERT INTO marketplace_listings
                (profile_id, seller_id, price_credits, max_queries_per_buyer,
                 category_tags, description)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING listing_id"#,
        )
        .bind(profile_id)
        .bind(seller_id)
        .bind(price_credits)
        .bind(max_queries_per_buyer)
        .bind(category_tags)
        .bind(description)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.try_get("listing_id")?)
    }

    pub async fn update_marketplace_listing(
        &self,
        listing_id: Uuid,
        seller_id: &str,
        status: Option<&str>,
        price_credits: Option<i32>,
    ) -> Result<()> {
        if let Some(s) = status {
            sqlx::query(
                "UPDATE marketplace_listings SET status = $1, updated_at = NOW() WHERE listing_id = $2 AND seller_id = $3",
            )
            .bind(s)
            .bind(listing_id)
            .bind(seller_id)
            .execute(&self.pool)
            .await?;

            if s == "delisted" {
                // Unmark profile
                sqlx::query(
                    "UPDATE shopping_profiles SET is_listed = false WHERE profile_id = (SELECT profile_id FROM marketplace_listings WHERE listing_id = $1)",
                )
                .bind(listing_id)
                .execute(&self.pool)
                .await?;
            }
        }
        if let Some(p) = price_credits {
            sqlx::query(
                "UPDATE marketplace_listings SET price_credits = $1, updated_at = NOW() WHERE listing_id = $2 AND seller_id = $3",
            )
            .bind(p)
            .bind(listing_id)
            .bind(seller_id)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn get_active_listings(
        &self,
        category_filter: Option<&[String]>,
        limit: i64,
    ) -> Result<Vec<MarketplaceListing>> {
        let rows = if let Some(cats) = category_filter {
            sqlx::query(
                r#"SELECT listing_id, profile_id, seller_id, price_credits,
                          max_queries_per_buyer, total_queries, total_earned,
                          status, category_tags, description, created_at, updated_at
                   FROM marketplace_listings
                   WHERE status = 'active' AND category_tags && $1
                   ORDER BY total_queries DESC
                   LIMIT $2"#,
            )
            .bind(cats)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"SELECT listing_id, profile_id, seller_id, price_credits,
                          max_queries_per_buyer, total_queries, total_earned,
                          status, category_tags, description, created_at, updated_at
                   FROM marketplace_listings
                   WHERE status = 'active'
                   ORDER BY total_queries DESC
                   LIMIT $1"#,
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows.iter().map(row_to_marketplace_listing).collect())
    }

    /// Core marketplace matching: cosine similarity against listed profiles
    pub async fn match_marketplace_profiles(
        &self,
        product_embedding: &[f32],
        category_filter: Option<&[String]>,
        min_similarity: f64,
        limit: i64,
    ) -> Result<Vec<(MarketplaceListing, f64, Option<f64>, Option<f64>)>> {
        let product_vec = pgvector::Vector::from(product_embedding.to_vec());

        let rows = if let Some(cats) = category_filter {
            sqlx::query(
                r#"SELECT l.listing_id, l.profile_id, l.seller_id, l.price_credits,
                          l.max_queries_per_buyer, l.total_queries, l.total_earned,
                          l.status, l.category_tags, l.description,
                          l.created_at, l.updated_at,
                          sp.price_sensitivity, sp.quality_bias,
                          1.0 - (sp.composite_embedding <=> $1) AS similarity
                   FROM marketplace_listings l
                   JOIN shopping_profiles sp ON sp.profile_id = l.profile_id
                   WHERE l.status = 'active'
                     AND sp.composite_embedding IS NOT NULL
                     AND l.category_tags && $2
                   ORDER BY sp.composite_embedding <=> $1
                   LIMIT $3"#,
            )
            .bind(&product_vec)
            .bind(cats)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"SELECT l.listing_id, l.profile_id, l.seller_id, l.price_credits,
                          l.max_queries_per_buyer, l.total_queries, l.total_earned,
                          l.status, l.category_tags, l.description,
                          l.created_at, l.updated_at,
                          sp.price_sensitivity, sp.quality_bias,
                          1.0 - (sp.composite_embedding <=> $1) AS similarity
                   FROM marketplace_listings l
                   JOIN shopping_profiles sp ON sp.profile_id = l.profile_id
                   WHERE l.status = 'active'
                     AND sp.composite_embedding IS NOT NULL
                   ORDER BY sp.composite_embedding <=> $1
                   LIMIT $2"#,
            )
            .bind(&product_vec)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };

        let mut results = Vec::new();
        for row in &rows {
            let similarity: f64 = row.try_get("similarity").unwrap_or(0.0);
            if similarity < min_similarity {
                continue;
            }
            let listing = row_to_marketplace_listing(row);
            let ps: Option<f64> = row.try_get("price_sensitivity").unwrap_or(None);
            let qb: Option<f64> = row.try_get("quality_bias").unwrap_or(None);
            results.push((listing, similarity, ps, qb));
        }
        Ok(results)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn record_marketplace_transaction(
        &self,
        listing_id: Uuid,
        buyer_id: &str,
        seller_id: &str,
        similarity_score: f64,
        product_embedding_hash: Option<&str>,
        credits_charged: i32,
        credits_to_seller: i32,
        platform_fee: i32,
    ) -> Result<Uuid> {
        let row = sqlx::query(
            r#"INSERT INTO marketplace_transactions
                (listing_id, buyer_id, seller_id, similarity_score,
                 product_embedding_hash, credits_charged, credits_to_seller, platform_fee)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING tx_id"#,
        )
        .bind(listing_id)
        .bind(buyer_id)
        .bind(seller_id)
        .bind(similarity_score)
        .bind(product_embedding_hash)
        .bind(credits_charged)
        .bind(credits_to_seller)
        .bind(platform_fee)
        .fetch_one(&self.pool)
        .await?;

        // Update listing counters
        sqlx::query(
            "UPDATE marketplace_listings SET total_queries = total_queries + 1, total_earned = total_earned + $1 WHERE listing_id = $2",
        )
        .bind(credits_to_seller)
        .bind(listing_id)
        .execute(&self.pool)
        .await?;

        Ok(row.try_get("tx_id")?)
    }

    pub async fn get_match_history(
        &self,
        buyer_id: &str,
        limit: i64,
    ) -> Result<Vec<MarketplaceTransaction>> {
        let rows = sqlx::query(
            r#"SELECT tx_id, listing_id, buyer_id, seller_id, similarity_score,
                      product_embedding_hash, credits_charged, credits_to_seller,
                      platform_fee, created_at
               FROM marketplace_transactions
               WHERE buyer_id = $1
               ORDER BY created_at DESC
               LIMIT $2"#,
        )
        .bind(buyer_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(row_to_marketplace_transaction).collect())
    }
}

fn row_to_coherence_evaluation(row: &sqlx::postgres::PgRow) -> CoherenceEvaluation {
    CoherenceEvaluation {
        eval_id: row.try_get("eval_id").unwrap(),
        workspace_id: row.try_get("workspace_id").unwrap(),
        global_score: row.try_get("global_score").unwrap(),
        quality_label: row.try_get("quality_label").unwrap(),
        principle_scores: row
            .try_get::<serde_json::Value, _>("principle_scores")
            .unwrap_or(serde_json::json!({})),
        health_indicators: row
            .try_get::<serde_json::Value, _>("health_indicators")
            .unwrap_or(serde_json::json!({})),
        utterance_count: row.try_get("utterance_count").unwrap_or(0),
        message_window: row.try_get("message_window").unwrap_or(None),
        created_at: row.try_get("created_at").unwrap(),
    }
}

fn row_to_workspace_message(row: &sqlx::postgres::PgRow) -> WorkspaceMessage {
    WorkspaceMessage {
        message_id: row.try_get("message_id").unwrap(),
        workspace_id: row.try_get("workspace_id").unwrap(),
        sender_type: row.try_get("sender_type").unwrap(),
        sender_id: row.try_get("sender_id").unwrap(),
        sender_name: row.try_get("sender_name").unwrap_or(None),
        content: row.try_get("content").unwrap(),
        message_type: row.try_get("message_type").unwrap(),
        metadata: row
            .try_get::<serde_json::Value, _>("metadata")
            .unwrap_or(serde_json::json!({})),
        created_at: row.try_get("created_at").unwrap(),
    }
}

fn row_to_shopping_profile(row: &sqlx::postgres::PgRow) -> ShoppingProfile {
    ShoppingProfile {
        profile_id: row.try_get("profile_id").unwrap(),
        user_id: row.try_get("user_id").unwrap(),
        agent_id: row.try_get("agent_id").unwrap(),
        profile_name: row.try_get("profile_name").unwrap(),
        composite_embedding: None, // Never expose raw embeddings in row helpers
        embedding_version: row.try_get("embedding_version").unwrap_or(0),
        episode_count: row.try_get("episode_count").unwrap_or(0),
        category_tags: row
            .try_get::<Option<Vec<String>>, _>("category_tags")
            .unwrap_or(None)
            .unwrap_or_default(),
        price_sensitivity: row.try_get("price_sensitivity").unwrap_or(None),
        quality_bias: row.try_get("quality_bias").unwrap_or(None),
        brand_affinities: row
            .try_get::<serde_json::Value, _>("brand_affinities")
            .unwrap_or(serde_json::json!({})),
        metadata: row
            .try_get::<serde_json::Value, _>("metadata")
            .unwrap_or(serde_json::json!({})),
        is_listed: row.try_get("is_listed").unwrap_or(false),
        created_at: row.try_get("created_at").unwrap(),
        updated_at: row.try_get("updated_at").unwrap(),
    }
}

fn row_to_marketplace_listing(row: &sqlx::postgres::PgRow) -> MarketplaceListing {
    MarketplaceListing {
        listing_id: row.try_get("listing_id").unwrap(),
        profile_id: row.try_get("profile_id").unwrap(),
        seller_id: row.try_get("seller_id").unwrap(),
        price_credits: row.try_get("price_credits").unwrap_or(1),
        max_queries_per_buyer: row.try_get("max_queries_per_buyer").unwrap_or(None),
        total_queries: row.try_get("total_queries").unwrap_or(0),
        total_earned: row.try_get("total_earned").unwrap_or(0),
        status: row
            .try_get("status")
            .unwrap_or_else(|_| "active".to_string()),
        category_tags: row
            .try_get::<Option<Vec<String>>, _>("category_tags")
            .unwrap_or(None)
            .unwrap_or_default(),
        description: row.try_get("description").unwrap_or(None),
        created_at: row.try_get("created_at").unwrap(),
        updated_at: row.try_get("updated_at").unwrap(),
    }
}

fn row_to_marketplace_transaction(row: &sqlx::postgres::PgRow) -> MarketplaceTransaction {
    MarketplaceTransaction {
        tx_id: row.try_get("tx_id").unwrap(),
        listing_id: row.try_get("listing_id").unwrap(),
        buyer_id: row.try_get("buyer_id").unwrap(),
        seller_id: row.try_get("seller_id").unwrap(),
        similarity_score: row.try_get("similarity_score").unwrap_or(0.0),
        product_embedding_hash: row.try_get("product_embedding_hash").unwrap_or(None),
        credits_charged: row.try_get("credits_charged").unwrap_or(0),
        credits_to_seller: row.try_get("credits_to_seller").unwrap_or(0),
        platform_fee: row.try_get("platform_fee").unwrap_or(0),
        created_at: row.try_get("created_at").unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cardinality, ExecutionStatus};
    use chrono::Utc;
    use rust_decimal::Decimal;
    use serde_json::json;

    async fn get_test_store() -> MemoryStore {
        dotenvy::dotenv().ok();
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
        MemoryStore::new(&database_url).await.unwrap()
    }

    fn test_agent() -> Agent {
        Agent {
            agent_id: Uuid::new_v4(),
            agent_name: format!("test_agent_{}", Uuid::new_v4()),
            agent_type: "test".to_string(),
            version: "1.0.0".to_string(),
            tier: "test".to_string(),
            executor_type: "llm".to_string(),
            model: "test-model".to_string(),
            temperature: 0.3,
            mcp_servers: None,
            description: Some("Test agent".to_string()),
            author: "Test".to_string(),
            current_ontology_commit: None,
            current_ontology_snapshot_id: None,
            last_consolidated_at: None,
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
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            total_cost_usd: None,
            avg_execution_time_ms: 0,
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
        }
    }

    #[tokio::test]
    async fn test_database_connection() {
        let _store = get_test_store().await;
        println!("✅ Database connection successful!");
    }

    #[tokio::test]
    async fn test_store_and_retrieve_episode() {
        let store = get_test_store().await;

        // First create an agent
        let agent = test_agent();
        let agent_id = store.upsert_agent(agent).await.unwrap();

        // Now create an episode for that agent
        let episode = Episode {
            episode_id: Uuid::new_v4(),
            agent_id,
            timestamp_ref: Utc::now(),
            query: "Test query".to_string(),
            context: json!({"test": "data"}),
            execution_status: ExecutionStatus::Success,
            error_details: None,
            execution_time_ms: 1000,
            tokens_used: Some(100),
            cost_usd: Some(Decimal::new(1, 3)), // 0.001
            embedding: None,
            consolidated: false,
            tags: vec![],
        };

        let episode_id = store.store_episode(episode.clone()).await.unwrap();
        let retrieved = store.get_episode(episode_id).await.unwrap();

        assert_eq!(retrieved.episode_id, episode.episode_id);
        assert_eq!(retrieved.query, episode.query);
        println!("✅ Episode storage and retrieval works!");
    }

    #[tokio::test]
    async fn test_vector_similarity_search() {
        use crate::{EmbeddingGenerator, MockEmbeddings};

        let store = get_test_store().await;
        let embedder = MockEmbeddings::new(1024);

        // Create agent
        let agent = test_agent();
        let agent_id = store.upsert_agent(agent).await.unwrap();

        // Create episodes with embeddings
        let queries = vec![
            "What is AMD market share?",
            "AMD datacenter revenue trends",
            "AMD GPU performance benchmarks",
            "Intel processor market share", // Different topic
        ];

        for query in &queries {
            let embedding = embedder.generate(query).await.unwrap();
            let episode = Episode {
                episode_id: Uuid::new_v4(),
                agent_id,
                timestamp_ref: Utc::now(),
                query: query.to_string(),
                context: json!({"test": "data"}),
                execution_status: ExecutionStatus::Success,
                error_details: None,
                execution_time_ms: 1000,
                tokens_used: Some(100),
                cost_usd: Some(Decimal::new(1, 3)),
                embedding: Some(embedding),
                consolidated: false,
                tags: vec![],
            };

            store.store_episode(episode).await.unwrap();
        }

        // Search for AMD-related episodes
        let query_embedding = embedder.generate("AMD market analysis").await.unwrap();
        let results = store
            .search_similar_episodes(agent_id, &query_embedding, 3)
            .await
            .unwrap();

        assert_eq!(results.len(), 3);

        // First result should be most similar (AMD market share)
        assert!(results[0].0.query.contains("AMD"));
        assert!(results[0].1 < results[2].1); // First is closer than third

        println!("✅ Vector similarity search works!");
        println!(
            "   Top result: {} (distance: {})",
            results[0].0.query, results[0].1
        );
    }

    #[tokio::test]
    async fn test_mark_episodes_consolidated() {
        let store = get_test_store().await;

        // Create agent
        let agent = test_agent();
        store.upsert_agent(agent.clone()).await.unwrap();

        // Create a few test episodes
        let mut episode_ids = Vec::new();
        for i in 0..3 {
            let episode = Episode {
                episode_id: Uuid::new_v4(),
                agent_id: agent.agent_id,
                timestamp_ref: Utc::now(),
                query: format!("Test query {}", i),
                context: json!({"test": i}),
                execution_status: ExecutionStatus::Success,
                error_details: None,
                execution_time_ms: 1000,
                tokens_used: Some(100),
                cost_usd: Some(Decimal::new(1, 3)),
                embedding: None,
                consolidated: false,
                tags: vec![],
            };
            episode_ids.push(episode.episode_id);
            store.store_episode(episode).await.unwrap();
        }

        // Create a consolidation job in the database first
        let job_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO consolidation_jobs (job_id, agent_id, status, started_at, episode_range_start, episode_range_end)
             VALUES ($1, $2, 'running', NOW(), $3, $4)",
        )
        .bind(job_id)
        .bind(agent.agent_id)
        .bind(episode_ids[0])
        .bind(episode_ids[episode_ids.len() - 1])
        .execute(&store.pool)
        .await
        .unwrap();

        // Mark episodes as consolidated
        let updated = store
            .mark_episodes_consolidated(&episode_ids, job_id)
            .await
            .unwrap();

        assert_eq!(updated, 3);

        // Verify episodes are marked as consolidated
        for episode_id in episode_ids {
            let episode = store.get_episode(episode_id).await.unwrap();
            assert!(episode.consolidated);
        }

        println!("✅ Mark episodes as consolidated works!");
    }

    #[tokio::test]
    async fn test_consolidation_job_lifecycle() {
        let store = get_test_store().await;

        // Create agent
        let agent = test_agent();
        store.upsert_agent(agent.clone()).await.unwrap();

        // Create some episodes
        let episode1_id = Uuid::new_v4();
        let episode2_id = Uuid::new_v4();

        // Create consolidation job
        let job_id = store
            .create_consolidation_job(agent.agent_id, episode1_id, episode2_id)
            .await
            .unwrap();

        // Verify job was created
        let job = store.get_consolidation_job(job_id).await.unwrap();
        assert_eq!(job.agent_id, agent.agent_id);
        assert_eq!(job.status, "running");
        assert_eq!(job.episodes_processed, 0);

        // Update job statistics
        store
            .update_consolidation_job(job_id, 5, 2, 3, 2, 1, 4, 10)
            .await
            .unwrap();

        // Verify updates
        let job = store.get_consolidation_job(job_id).await.unwrap();
        assert_eq!(job.episodes_processed, 5);
        assert_eq!(job.clusters_identified, 2);
        assert_eq!(job.rules_extracted, 3);
        assert_eq!(job.rules_verified, 2);
        assert_eq!(job.rules_rejected, 1);
        assert_eq!(job.entities_created, 4);
        assert_eq!(job.facts_created, 10);

        // Complete the job
        store
            .complete_consolidation_job(job_id, "completed", None)
            .await
            .unwrap();

        // Verify completion
        let job = store.get_consolidation_job(job_id).await.unwrap();
        assert_eq!(job.status, "completed");
        assert!(job.completed_at.is_some());
        assert!(job.duration_ms.is_some());

        println!("✅ Consolidation job lifecycle works!");
        println!("   Job ID: {}", job_id);
        println!("   Episodes processed: {}", job.episodes_processed);
        println!("   Duration: {} ms", job.duration_ms.unwrap());
    }

    #[tokio::test]
    async fn test_semantic_rule_lifecycle() {
        let store = get_test_store().await;

        // Create agent
        let agent = test_agent();
        store.upsert_agent(agent.clone()).await.unwrap();

        // Create semantic rule
        let rule = SemanticRule {
            rule_id: Uuid::new_v4(),
            agent_id: agent.agent_id,
            rule_content: "When AMD releases datacenter products, stock price increases"
                .to_string(),
            rule_description: Some("Pattern from Q4 2024-Q1 2025".to_string()),
            confidence_score: 0.85,
            verification_status: VerificationStatus::Pending,
            verification_method: None,
            source_episode_cluster: vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()],
            episode_count: 3,
            embedding: None,
            is_active: true,
            created_at: Utc::now(),
        };

        // Store rule
        store.store_semantic_rule(rule.clone()).await.unwrap();

        // Retrieve rule
        let retrieved = store.get_semantic_rule(rule.rule_id).await.unwrap();
        assert_eq!(retrieved.rule_content, rule.rule_content);
        assert_eq!(retrieved.confidence_score, 0.85);
        assert_eq!(retrieved.episode_count, 3);

        // Update verification status
        store
            .update_rule_verification(
                rule.rule_id,
                VerificationStatus::Verified,
                Some("unit_test".to_string()),
            )
            .await
            .unwrap();

        let verified = store.get_semantic_rule(rule.rule_id).await.unwrap();
        assert!(matches!(
            verified.verification_status,
            VerificationStatus::Verified
        ));

        // Get all agent rules
        let rules = store
            .get_agent_semantic_rules(agent.agent_id)
            .await
            .unwrap();
        assert_eq!(rules.len(), 1);

        // Deactivate rule
        store.deactivate_rule(rule.rule_id).await.unwrap();
        let rules = store
            .get_agent_semantic_rules(agent.agent_id)
            .await
            .unwrap();
        assert_eq!(rules.len(), 0);

        println!("✅ Semantic rule lifecycle works!");
    }

    #[tokio::test]
    async fn test_entity_and_fact_storage() {
        let store = get_test_store().await;

        // Create agent
        let agent = test_agent();
        store.upsert_agent(agent.clone()).await.unwrap();

        // Create entities
        let amd_entity = Entity {
            entity_id: Uuid::new_v4(),
            agent_id: agent.agent_id,
            entity_name: "AMD".to_string(),
            entity_type: "Company".to_string(),
            summary: Some("Semiconductor company".to_string()),
            t_valid: Utc::now(),
            t_invalid: None,
            source_episodes: vec![Uuid::new_v4()],
            extraction_confidence: 0.95,
            embedding: None,
        };

        let datacenter_entity = Entity {
            entity_id: Uuid::new_v4(),
            agent_id: agent.agent_id,
            entity_name: "Datacenter".to_string(),
            entity_type: "Market".to_string(),
            summary: Some("Server and datacenter market".to_string()),
            t_valid: Utc::now(),
            t_invalid: None,
            source_episodes: vec![Uuid::new_v4()],
            extraction_confidence: 0.90,
            embedding: None,
        };

        // Store entities
        store.store_entity(amd_entity.clone()).await.unwrap();
        store.store_entity(datacenter_entity.clone()).await.unwrap();

        // Retrieve entities
        let retrieved_amd = store.get_entity(amd_entity.entity_id).await.unwrap();
        assert_eq!(retrieved_amd.entity_name, "AMD");

        // Get all agent entities
        let entities = store.get_agent_entities(agent.agent_id).await.unwrap();
        assert_eq!(entities.len(), 2);

        // Create fact (relationship)
        let fact = Fact {
            fact_id: Uuid::new_v4(),
            agent_id: agent.agent_id,
            source_entity_id: amd_entity.entity_id,
            target_entity_id: datacenter_entity.entity_id,
            relation_type: "operates_in".to_string(),
            relation_cardinality: Cardinality::ManyToMany,
            confidence: 0.92,
            reasoning: Some("AMD produces datacenter processors".to_string()),
            t_valid: Utc::now(),
            t_invalid: None,
            source_episodes: vec![Uuid::new_v4()],
        };

        // Store fact
        store.store_fact(fact.clone()).await.unwrap();

        // Retrieve fact
        let retrieved_fact = store.get_fact(fact.fact_id).await.unwrap();
        assert_eq!(retrieved_fact.relation_type, "operates_in");
        assert_eq!(retrieved_fact.confidence, 0.92);

        // Get entity facts
        let entity_facts = store.get_entity_facts(amd_entity.entity_id).await.unwrap();
        assert_eq!(entity_facts.len(), 1);

        // Get all agent facts
        let all_facts = store.get_agent_facts(agent.agent_id).await.unwrap();
        assert_eq!(all_facts.len(), 1);

        // Invalidate fact
        store.invalidate_fact(fact.fact_id).await.unwrap();
        let facts_after = store.get_agent_facts(agent.agent_id).await.unwrap();
        assert_eq!(facts_after.len(), 0);

        // Invalidate entity
        store.invalidate_entity(amd_entity.entity_id).await.unwrap();
        let entities_after = store.get_agent_entities(agent.agent_id).await.unwrap();
        assert_eq!(entities_after.len(), 1); // Only datacenter remains

        println!("✅ Entity and fact storage works!");
    }
}
