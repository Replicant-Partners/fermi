-- Fermi Active Dreaming Memory Schema
-- PostgreSQL with pgvector extension
-- Bi-temporal tracking, episodic/semantic memory, knowledge graph

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "vector";
CREATE EXTENSION IF NOT EXISTS "pg_trgm"; -- For full-text search

-- ============================================================================
-- AGENT REGISTRY
-- ============================================================================

CREATE TABLE agents (
    agent_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_name TEXT UNIQUE NOT NULL,
    agent_type TEXT NOT NULL,
    version TEXT NOT NULL DEFAULT '1.0.0',
    tier TEXT NOT NULL DEFAULT 'curated', -- 'curated' or 'community'

    -- Capabilities
    executor_type TEXT NOT NULL, -- 'llm', 'mcp', 'manual', 'skill'
    model TEXT NOT NULL,
    temperature FLOAT NOT NULL DEFAULT 0.3,
    mcp_servers JSONB, -- Array of MCP server configurations for this agent

    -- Metadata
    description TEXT,
    author TEXT NOT NULL DEFAULT 'Fermi Team',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Ontology references (bidirectional linking)
    current_ontology_commit TEXT, -- Git commit SHA
    current_ontology_snapshot_id UUID, -- Database snapshot
    last_consolidated_at TIMESTAMPTZ,

    -- Performance stats (cached from database)
    total_executions INTEGER NOT NULL DEFAULT 0,
    successful_executions INTEGER NOT NULL DEFAULT 0,
    failed_executions INTEGER NOT NULL DEFAULT 0,
    total_cost_usd DECIMAL(10, 6) NOT NULL DEFAULT 0.0,
    avg_execution_time_ms BIGINT NOT NULL DEFAULT 0
);

CREATE INDEX idx_agents_name ON agents(agent_name);
CREATE INDEX idx_agents_type ON agents(agent_type);

-- ============================================================================
-- EPISODIC MEMORY (Wake Phase)
-- ============================================================================

CREATE TABLE episodes (
    episode_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,

    -- Temporal tracking
    timestamp_ref TIMESTAMPTZ NOT NULL, -- Event time (when it happened)
    timestamp_created TIMESTAMPTZ NOT NULL DEFAULT NOW(), -- Transaction time (when recorded)

    -- Execution context
    query TEXT NOT NULL,
    context JSONB NOT NULL, -- Full input/output, parameters, etc.
    execution_status TEXT NOT NULL, -- 'success', 'failure', 'partial'
    error_details TEXT,

    -- Execution metrics
    execution_time_ms BIGINT NOT NULL,
    tokens_used INTEGER,
    cost_usd DECIMAL(10, 6),

    -- Vector embedding for similarity search
    embedding vector(1024), -- 1024-dimensional embeddings

    -- Consolidation tracking
    consolidated BOOLEAN NOT NULL DEFAULT FALSE,
    consolidation_job_id UUID, -- FK added later
    cluster_id UUID, -- Which cluster this episode belongs to

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_episodes_agent ON episodes(agent_id, timestamp_ref DESC);
CREATE INDEX idx_episodes_status ON episodes(execution_status);
CREATE INDEX idx_episodes_consolidated ON episodes(consolidated) WHERE NOT consolidated;
CREATE INDEX idx_episodes_embedding ON episodes USING ivfflat (embedding vector_cosine_ops);
CREATE INDEX idx_episodes_cluster ON episodes(cluster_id) WHERE cluster_id IS NOT NULL;

-- ============================================================================
-- SEMANTIC MEMORY (Sleep Phase - Consolidated Rules)
-- ============================================================================

CREATE TABLE semantic_rules (
    rule_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,

    -- Rule content
    rule_content TEXT NOT NULL,
    rule_description TEXT,
    confidence_score FLOAT NOT NULL,

    -- Verification tracking
    verification_status TEXT NOT NULL DEFAULT 'pending', -- 'pending', 'verified', 'rejected'
    verification_method TEXT, -- 'contradiction', 'historical', 'counterfactual'
    verification_details JSONB,

    -- Source episodes (which failures led to this rule)
    source_episode_cluster UUID[], -- Array of episode_ids
    episode_count INTEGER NOT NULL,

    -- Usage tracking
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_validated_at TIMESTAMPTZ,
    application_count INTEGER NOT NULL DEFAULT 0,
    successful_applications INTEGER NOT NULL DEFAULT 0,
    failed_applications INTEGER NOT NULL DEFAULT 0,

    -- Vector embedding for retrieval
    embedding vector(1024),

    -- Invalidation
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    invalidated_at TIMESTAMPTZ,
    invalidation_reason TEXT
);

CREATE INDEX idx_semantic_rules_agent ON semantic_rules(agent_id);
CREATE INDEX idx_semantic_rules_status ON semantic_rules(verification_status);
CREATE INDEX idx_semantic_rules_active ON semantic_rules(is_active) WHERE is_active;
CREATE INDEX idx_semantic_rules_embedding ON semantic_rules USING ivfflat (embedding vector_cosine_ops);
CREATE INDEX idx_semantic_rules_content ON semantic_rules USING gin (to_tsvector('english', rule_content));

-- ============================================================================
-- KNOWLEDGE GRAPH - ENTITIES
-- ============================================================================

CREATE TABLE entities (
    entity_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,

    -- Entity identification
    entity_name TEXT NOT NULL,
    entity_type TEXT NOT NULL, -- 'company', 'product', 'market', 'technology', etc.
    summary TEXT,

    -- Bi-temporal tracking
    t_valid TIMESTAMPTZ NOT NULL, -- When this entity became valid (event time)
    t_invalid TIMESTAMPTZ, -- When this entity became invalid (event time, NULL = still valid)
    t_created TIMESTAMPTZ NOT NULL DEFAULT NOW(), -- Transaction time (when recorded)
    t_expired TIMESTAMPTZ, -- Transaction end (NULL = current version)

    -- Source tracking
    source_episodes UUID[], -- Which episodes mentioned this entity
    extraction_confidence FLOAT NOT NULL,

    -- Vector embedding
    embedding vector(1024),

    -- Version tracking
    version INTEGER NOT NULL DEFAULT 1,
    replaces_entity_id UUID REFERENCES entities(entity_id) -- Previous version of this entity
);

CREATE INDEX idx_entities_agent ON entities(agent_id);
CREATE INDEX idx_entities_name ON entities(entity_name);
CREATE INDEX idx_entities_type ON entities(entity_type);
CREATE INDEX idx_entities_valid ON entities(t_valid, t_invalid) WHERE t_invalid IS NULL;
CREATE INDEX idx_entities_current ON entities(t_expired) WHERE t_expired IS NULL;
CREATE INDEX idx_entities_embedding ON entities USING ivfflat (embedding vector_cosine_ops);
CREATE INDEX idx_entities_name_trgm ON entities USING gin (entity_name gin_trgm_ops);

-- ============================================================================
-- KNOWLEDGE GRAPH - FACTS (Relationships)
-- ============================================================================

CREATE TABLE facts (
    fact_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,

    -- Relationship
    source_entity_id UUID NOT NULL REFERENCES entities(entity_id) ON DELETE CASCADE,
    target_entity_id UUID NOT NULL REFERENCES entities(entity_id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL, -- 'PRODUCES', 'COMPETES_IN', 'USES', etc.
    relation_cardinality TEXT NOT NULL, -- '||--||', '||--o{', '}o--||', '}o--o{'

    -- Fact metadata
    confidence FLOAT NOT NULL,
    reasoning TEXT,

    -- Bi-temporal tracking
    t_valid TIMESTAMPTZ NOT NULL,
    t_invalid TIMESTAMPTZ,
    t_created TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    t_expired TIMESTAMPTZ,

    -- Source tracking (bidirectional)
    source_episodes UUID[],

    -- Version tracking
    version INTEGER NOT NULL DEFAULT 1,
    replaces_fact_id UUID REFERENCES facts(fact_id)
);

CREATE INDEX idx_facts_agent ON facts(agent_id);
CREATE INDEX idx_facts_source ON facts(source_entity_id);
CREATE INDEX idx_facts_target ON facts(target_entity_id);
CREATE INDEX idx_facts_relation ON facts(relation_type);
CREATE INDEX idx_facts_valid ON facts(t_valid, t_invalid) WHERE t_invalid IS NULL;
CREATE INDEX idx_facts_current ON facts(t_expired) WHERE t_expired IS NULL;
CREATE INDEX idx_facts_bidirectional ON facts(source_entity_id, target_entity_id, relation_type);

-- ============================================================================
-- KNOWLEDGE GRAPH - COMMUNITIES
-- ============================================================================

CREATE TABLE communities (
    community_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,

    -- Community metadata
    community_name TEXT,
    summary TEXT,

    -- Members
    member_entity_ids UUID[], -- Array of entity_ids
    member_count INTEGER NOT NULL DEFAULT 0,

    -- Tracking
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_propagation_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Vector embedding
    embedding vector(1024)
);

CREATE INDEX idx_communities_agent ON communities(agent_id);
CREATE INDEX idx_communities_propagation ON communities(last_propagation_at);
CREATE INDEX idx_communities_embedding ON communities USING ivfflat (embedding vector_cosine_ops);

-- ============================================================================
-- ONTOLOGY SNAPSHOTS (Git Integration)
-- ============================================================================

CREATE TABLE ontology_snapshots (
    snapshot_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,

    -- Git tracking
    git_commit_sha TEXT NOT NULL,
    git_repository TEXT NOT NULL,
    git_path TEXT NOT NULL, -- e.g., 'agents/market_research/ontology.mermaid' (per-agent repo)
    github_url TEXT, -- GitHub URL e.g., 'https://github.com/Replicant-Partners/fermi-agent-market-research'
    pushed_to_remote BOOLEAN NOT NULL DEFAULT false, -- Whether successfully pushed to GitHub

    -- Snapshot metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    consolidation_job_id UUID, -- FK added later

    -- Stats at snapshot time
    entity_count INTEGER NOT NULL,
    fact_count INTEGER NOT NULL,
    community_count INTEGER NOT NULL,
    rule_count INTEGER NOT NULL,

    -- Full Mermaid ER diagram
    mermaid_content TEXT NOT NULL,

    -- Version tracking
    version INTEGER NOT NULL,
    previous_snapshot_id UUID REFERENCES ontology_snapshots(snapshot_id)
);

CREATE INDEX idx_ontology_snapshots_agent ON ontology_snapshots(agent_id, created_at DESC);
CREATE INDEX idx_ontology_snapshots_commit ON ontology_snapshots(git_commit_sha);
CREATE UNIQUE INDEX idx_ontology_snapshots_unique ON ontology_snapshots(agent_id, git_commit_sha);
CREATE INDEX idx_ontology_snapshots_github_url ON ontology_snapshots(github_url) WHERE github_url IS NOT NULL;

-- ============================================================================
-- CONSOLIDATION JOBS (Sleep Phase Tracking)
-- ============================================================================

CREATE TABLE consolidation_jobs (
    job_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,

    -- Timing
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    duration_ms BIGINT,

    -- Status
    status TEXT NOT NULL DEFAULT 'running', -- 'running', 'completed', 'failed'
    error_message TEXT,

    -- Episode range
    episode_range_start UUID NOT NULL,
    episode_range_end UUID NOT NULL,
    episodes_processed INTEGER NOT NULL DEFAULT 0,

    -- Results
    clusters_identified INTEGER NOT NULL DEFAULT 0,
    rules_extracted INTEGER NOT NULL DEFAULT 0,
    rules_verified INTEGER NOT NULL DEFAULT 0,
    rules_rejected INTEGER NOT NULL DEFAULT 0,
    entities_created INTEGER NOT NULL DEFAULT 0,
    facts_created INTEGER NOT NULL DEFAULT 0,

    -- Snapshot reference
    ontology_snapshot_id UUID REFERENCES ontology_snapshots(snapshot_id)
);

CREATE INDEX idx_consolidation_jobs_agent ON consolidation_jobs(agent_id, started_at DESC);
CREATE INDEX idx_consolidation_jobs_status ON consolidation_jobs(status);

-- Add FK from episodes to consolidation_jobs (circular dependency)
ALTER TABLE episodes ADD CONSTRAINT fk_episodes_consolidation
    FOREIGN KEY (consolidation_job_id) REFERENCES consolidation_jobs(job_id);

-- ============================================================================
-- VERIFICATION TESTS (Counterfactual Validation)
-- ============================================================================

CREATE TABLE verification_tests (
    test_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    rule_id UUID NOT NULL REFERENCES semantic_rules(rule_id) ON DELETE CASCADE,
    consolidation_job_id UUID REFERENCES consolidation_jobs(job_id),

    -- Test type
    test_type TEXT NOT NULL, -- 'contradiction', 'historical', 'counterfactual'

    -- Test details
    scenario_description TEXT NOT NULL,
    test_input JSONB NOT NULL,
    test_expected_output JSONB,

    -- Results
    test_result BOOLEAN NOT NULL,
    confidence FLOAT NOT NULL,
    reasoning TEXT,

    -- LLM tracking
    model_used TEXT,
    tokens_used INTEGER,
    cost_usd DECIMAL(10, 6),

    tested_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_verification_tests_rule ON verification_tests(rule_id);
CREATE INDEX idx_verification_tests_type ON verification_tests(test_type);
CREATE INDEX idx_verification_tests_result ON verification_tests(test_result);

-- ============================================================================
-- RACE CONDITION PREVENTION
-- ============================================================================

-- Lock table for critical sections (agent consolidation)
CREATE TABLE consolidation_locks (
    agent_id UUID PRIMARY KEY REFERENCES agents(agent_id) ON DELETE CASCADE,
    locked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    locked_by TEXT NOT NULL, -- Worker instance ID
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_consolidation_locks_expires ON consolidation_locks(expires_at);

-- Function to acquire lock with timeout
CREATE OR REPLACE FUNCTION acquire_consolidation_lock(
    p_agent_id UUID,
    p_worker_id TEXT,
    p_timeout_minutes INTEGER DEFAULT 60
) RETURNS BOOLEAN AS $$
DECLARE
    lock_acquired BOOLEAN;
BEGIN
    -- Try to insert lock
    INSERT INTO consolidation_locks (agent_id, locked_by, expires_at)
    VALUES (p_agent_id, p_worker_id, NOW() + (p_timeout_minutes || ' minutes')::INTERVAL)
    ON CONFLICT (agent_id) DO NOTHING;

    -- Check if we got the lock
    SELECT EXISTS (
        SELECT 1 FROM consolidation_locks
        WHERE agent_id = p_agent_id
          AND locked_by = p_worker_id
          AND expires_at > NOW()
    ) INTO lock_acquired;

    RETURN lock_acquired;
END;
$$ LANGUAGE plpgsql;

-- Function to release lock
CREATE OR REPLACE FUNCTION release_consolidation_lock(
    p_agent_id UUID,
    p_worker_id TEXT
) RETURNS BOOLEAN AS $$
BEGIN
    DELETE FROM consolidation_locks
    WHERE agent_id = p_agent_id
      AND locked_by = p_worker_id;

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;

-- Function to clean expired locks
CREATE OR REPLACE FUNCTION clean_expired_locks() RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM consolidation_locks
    WHERE expires_at < NOW();

    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- HELPER FUNCTIONS
-- ============================================================================

-- Function to get current agent state (for agent card)
CREATE OR REPLACE FUNCTION get_agent_state(p_agent_id UUID)
RETURNS TABLE (
    agent_name TEXT,
    total_episodes BIGINT,
    unconsolidated_episodes BIGINT,
    total_rules BIGINT,
    verified_rules BIGINT,
    total_entities BIGINT,
    current_entities BIGINT,
    total_facts BIGINT,
    current_facts BIGINT,
    last_consolidated_at TIMESTAMPTZ,
    current_ontology_commit TEXT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        a.agent_name,
        COUNT(DISTINCT e.episode_id)::BIGINT,
        COUNT(DISTINCT e.episode_id) FILTER (WHERE NOT e.consolidated)::BIGINT,
        COUNT(DISTINCT sr.rule_id)::BIGINT,
        COUNT(DISTINCT sr.rule_id) FILTER (WHERE sr.verification_status = 'verified')::BIGINT,
        COUNT(DISTINCT ent.entity_id)::BIGINT,
        COUNT(DISTINCT ent.entity_id) FILTER (WHERE ent.t_expired IS NULL)::BIGINT,
        COUNT(DISTINCT f.fact_id)::BIGINT,
        COUNT(DISTINCT f.fact_id) FILTER (WHERE f.t_expired IS NULL)::BIGINT,
        a.last_consolidated_at,
        a.current_ontology_commit
    FROM agents a
    LEFT JOIN episodes e ON a.agent_id = e.agent_id
    LEFT JOIN semantic_rules sr ON a.agent_id = sr.agent_id
    LEFT JOIN entities ent ON a.agent_id = ent.agent_id
    LEFT JOIN facts f ON a.agent_id = f.agent_id
    WHERE a.agent_id = p_agent_id
    GROUP BY a.agent_name, a.last_consolidated_at, a.current_ontology_commit;
END;
$$ LANGUAGE plpgsql;

-- Function to get temporal entity state at specific time
CREATE OR REPLACE FUNCTION get_entity_at_time(
    p_agent_id UUID,
    p_entity_name TEXT,
    p_timestamp TIMESTAMPTZ
)
RETURNS TABLE (
    entity_id UUID,
    entity_name TEXT,
    entity_type TEXT,
    summary TEXT,
    t_valid TIMESTAMPTZ,
    t_invalid TIMESTAMPTZ
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        e.entity_id,
        e.entity_name,
        e.entity_type,
        e.summary,
        e.t_valid,
        e.t_invalid
    FROM entities e
    WHERE e.agent_id = p_agent_id
      AND e.entity_name = p_entity_name
      AND e.t_valid <= p_timestamp
      AND (e.t_invalid IS NULL OR e.t_invalid > p_timestamp)
      AND e.t_created <= p_timestamp
      AND (e.t_expired IS NULL OR e.t_expired > p_timestamp)
    ORDER BY e.version DESC
    LIMIT 1;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- VIEWS FOR COMMON QUERIES
-- ============================================================================

-- View: Current knowledge graph state per agent
CREATE VIEW current_knowledge_graph AS
SELECT
    e.agent_id,
    e.entity_id,
    e.entity_name,
    e.entity_type,
    e.summary,
    f.fact_id,
    f.relation_type,
    f.relation_cardinality,
    f.target_entity_id,
    te.entity_name AS target_entity_name,
    f.confidence
FROM entities e
LEFT JOIN facts f ON e.entity_id = f.source_entity_id
    AND f.t_expired IS NULL
    AND f.t_valid <= NOW()
    AND (f.t_invalid IS NULL OR f.t_invalid > NOW())
LEFT JOIN entities te ON f.target_entity_id = te.entity_id
WHERE e.t_expired IS NULL
  AND e.t_valid <= NOW()
  AND (e.t_invalid IS NULL OR e.t_invalid > NOW());

-- View: Agent performance summary
CREATE VIEW agent_performance AS
SELECT
    a.agent_id,
    a.agent_name,
    a.agent_type,
    COUNT(DISTINCT e.episode_id) AS total_episodes,
    COUNT(DISTINCT e.episode_id) FILTER (WHERE e.execution_status = 'success') AS successful_episodes,
    COUNT(DISTINCT e.episode_id) FILTER (WHERE e.execution_status = 'failure') AS failed_episodes,
    AVG(e.execution_time_ms) AS avg_execution_time_ms,
    SUM(e.cost_usd) AS total_cost_usd,
    COUNT(DISTINCT sr.rule_id) FILTER (WHERE sr.is_active) AS active_rules,
    COUNT(DISTINCT ent.entity_id) FILTER (WHERE ent.t_expired IS NULL) AS current_entities,
    MAX(cj.completed_at) AS last_consolidation
FROM agents a
LEFT JOIN episodes e ON a.agent_id = e.agent_id
LEFT JOIN semantic_rules sr ON a.agent_id = sr.agent_id
LEFT JOIN entities ent ON a.agent_id = ent.agent_id
LEFT JOIN consolidation_jobs cj ON a.agent_id = cj.agent_id AND cj.status = 'completed'
GROUP BY a.agent_id, a.agent_name, a.agent_type;

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON TABLE episodes IS 'Episodic memory - raw agent execution traces (wake phase)';
COMMENT ON TABLE semantic_rules IS 'Semantic memory - consolidated verified rules (sleep phase)';
COMMENT ON TABLE entities IS 'Knowledge graph nodes with bi-temporal tracking';
COMMENT ON TABLE facts IS 'Knowledge graph edges (relationships) with bi-temporal tracking';
COMMENT ON TABLE communities IS 'High-level entity clusters (label propagation)';
COMMENT ON TABLE ontology_snapshots IS 'Git-committed Mermaid ER snapshots of agent worldview';
COMMENT ON TABLE consolidation_jobs IS 'Sleep phase execution tracking';
COMMENT ON TABLE verification_tests IS 'Counterfactual and validation tests for rules';
COMMENT ON TABLE consolidation_locks IS 'Distributed lock for race condition prevention';
