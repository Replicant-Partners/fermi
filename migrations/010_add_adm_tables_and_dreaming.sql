-- Migration: ADM tables + dreaming budget + dream synopses
-- Date: 2026-02-08
-- Description: Create ADM schema (agents, episodes, semantic_rules, entities, facts,
--              communities, ontology_snapshots, consolidation_jobs, verification_tests,
--              consolidation_locks) if not exists, then add dreaming budget columns.

BEGIN;

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "vector";

-- ============================================================================
-- AGENT REGISTRY
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.agents (
    agent_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_name TEXT UNIQUE NOT NULL,
    agent_type TEXT NOT NULL,
    version TEXT NOT NULL DEFAULT '1.0.0',
    tier TEXT NOT NULL DEFAULT 'curated',
    executor_type TEXT NOT NULL,
    model TEXT NOT NULL,
    temperature FLOAT NOT NULL DEFAULT 0.3,
    mcp_servers JSONB,
    description TEXT,
    author TEXT NOT NULL DEFAULT 'Fermi Team',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    current_ontology_commit TEXT,
    current_ontology_snapshot_id UUID,
    last_consolidated_at TIMESTAMPTZ,
    total_executions INTEGER NOT NULL DEFAULT 0,
    successful_executions INTEGER NOT NULL DEFAULT 0,
    failed_executions INTEGER NOT NULL DEFAULT 0,
    total_cost_usd DECIMAL(10, 6) NOT NULL DEFAULT 0.0,
    avg_execution_time_ms BIGINT NOT NULL DEFAULT 0,
    user_id TEXT,
    is_public BOOLEAN DEFAULT TRUE,
    visibility TEXT DEFAULT 'public'
);

CREATE INDEX IF NOT EXISTS idx_agents_name ON public.agents(agent_name);
CREATE INDEX IF NOT EXISTS idx_agents_type ON public.agents(agent_type);

-- ============================================================================
-- EPISODIC MEMORY (Wake Phase)
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.episodes (
    episode_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    timestamp_ref TIMESTAMPTZ NOT NULL,
    timestamp_created TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    query TEXT NOT NULL,
    context JSONB NOT NULL,
    execution_status TEXT NOT NULL,
    error_details TEXT,
    execution_time_ms BIGINT NOT NULL,
    tokens_used INTEGER,
    cost_usd DECIMAL(10, 6),
    embedding vector(1024),
    consolidated BOOLEAN NOT NULL DEFAULT FALSE,
    consolidation_job_id UUID,
    cluster_id UUID,
    user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_episodes_agent ON public.episodes(agent_id, timestamp_ref DESC);
CREATE INDEX IF NOT EXISTS idx_episodes_status ON public.episodes(execution_status);
CREATE INDEX IF NOT EXISTS idx_episodes_consolidated ON public.episodes(consolidated) WHERE NOT consolidated;

-- ============================================================================
-- SEMANTIC MEMORY (Sleep Phase)
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.semantic_rules (
    rule_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    rule_content TEXT NOT NULL,
    rule_description TEXT,
    confidence_score FLOAT NOT NULL,
    verification_status TEXT NOT NULL DEFAULT 'pending',
    verification_method TEXT,
    verification_details JSONB,
    source_episode_cluster UUID[],
    episode_count INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_validated_at TIMESTAMPTZ,
    application_count INTEGER NOT NULL DEFAULT 0,
    successful_applications INTEGER NOT NULL DEFAULT 0,
    failed_applications INTEGER NOT NULL DEFAULT 0,
    embedding vector(1024),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    invalidated_at TIMESTAMPTZ,
    invalidation_reason TEXT,
    user_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_semantic_rules_agent ON public.semantic_rules(agent_id);
CREATE INDEX IF NOT EXISTS idx_semantic_rules_active ON public.semantic_rules(is_active) WHERE is_active;

-- ============================================================================
-- KNOWLEDGE GRAPH - ENTITIES
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.entities (
    entity_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    entity_name TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    summary TEXT,
    t_valid TIMESTAMPTZ NOT NULL,
    t_invalid TIMESTAMPTZ,
    t_created TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    t_expired TIMESTAMPTZ,
    source_episodes UUID[],
    extraction_confidence FLOAT NOT NULL,
    embedding vector(1024),
    version INTEGER NOT NULL DEFAULT 1,
    replaces_entity_id UUID REFERENCES public.entities(entity_id)
);

CREATE INDEX IF NOT EXISTS idx_entities_agent ON public.entities(agent_id);
CREATE INDEX IF NOT EXISTS idx_entities_name ON public.entities(entity_name);
CREATE INDEX IF NOT EXISTS idx_entities_current ON public.entities(t_expired) WHERE t_expired IS NULL;

-- ============================================================================
-- KNOWLEDGE GRAPH - FACTS
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.facts (
    fact_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    source_entity_id UUID NOT NULL REFERENCES public.entities(entity_id) ON DELETE CASCADE,
    target_entity_id UUID NOT NULL REFERENCES public.entities(entity_id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL,
    relation_cardinality TEXT NOT NULL,
    confidence FLOAT NOT NULL,
    reasoning TEXT,
    t_valid TIMESTAMPTZ NOT NULL,
    t_invalid TIMESTAMPTZ,
    t_created TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    t_expired TIMESTAMPTZ,
    source_episodes UUID[],
    version INTEGER NOT NULL DEFAULT 1,
    replaces_fact_id UUID REFERENCES public.facts(fact_id)
);

CREATE INDEX IF NOT EXISTS idx_facts_agent ON public.facts(agent_id);
CREATE INDEX IF NOT EXISTS idx_facts_source ON public.facts(source_entity_id);
CREATE INDEX IF NOT EXISTS idx_facts_target ON public.facts(target_entity_id);

-- ============================================================================
-- COMMUNITIES
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.communities (
    community_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    community_name TEXT,
    summary TEXT,
    member_entity_ids UUID[],
    member_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_propagation_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    embedding vector(1024)
);

CREATE INDEX IF NOT EXISTS idx_communities_agent ON public.communities(agent_id);

-- ============================================================================
-- ONTOLOGY SNAPSHOTS
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.ontology_snapshots (
    snapshot_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    git_commit_sha TEXT NOT NULL,
    git_repository TEXT NOT NULL,
    git_path TEXT NOT NULL,
    github_url TEXT,
    pushed_to_remote BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    consolidation_job_id UUID,
    entity_count INTEGER NOT NULL,
    fact_count INTEGER NOT NULL,
    community_count INTEGER NOT NULL,
    rule_count INTEGER NOT NULL,
    mermaid_content TEXT NOT NULL,
    version INTEGER NOT NULL,
    previous_snapshot_id UUID REFERENCES public.ontology_snapshots(snapshot_id)
);

CREATE INDEX IF NOT EXISTS idx_ontology_snapshots_agent ON public.ontology_snapshots(agent_id, created_at DESC);

-- ============================================================================
-- CONSOLIDATION JOBS
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.consolidation_jobs (
    job_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    duration_ms BIGINT,
    status TEXT NOT NULL DEFAULT 'running',
    error_message TEXT,
    episode_range_start UUID NOT NULL,
    episode_range_end UUID NOT NULL,
    episodes_processed INTEGER NOT NULL DEFAULT 0,
    clusters_identified INTEGER NOT NULL DEFAULT 0,
    rules_extracted INTEGER NOT NULL DEFAULT 0,
    rules_verified INTEGER NOT NULL DEFAULT 0,
    rules_rejected INTEGER NOT NULL DEFAULT 0,
    entities_created INTEGER NOT NULL DEFAULT 0,
    facts_created INTEGER NOT NULL DEFAULT 0,
    ontology_snapshot_id UUID REFERENCES public.ontology_snapshots(snapshot_id)
);

CREATE INDEX IF NOT EXISTS idx_consolidation_jobs_agent ON public.consolidation_jobs(agent_id, started_at DESC);

-- ============================================================================
-- CONSOLIDATION LOCKS
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.consolidation_locks (
    agent_id UUID PRIMARY KEY REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    locked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    locked_by TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);

-- ============================================================================
-- DREAMING BUDGET (new columns)
-- ============================================================================

ALTER TABLE public.agents ADD COLUMN IF NOT EXISTS dreaming_budget_credits INTEGER NOT NULL DEFAULT 0;
ALTER TABLE public.agents ADD COLUMN IF NOT EXISTS dreaming_credits_used INTEGER NOT NULL DEFAULT 0;
ALTER TABLE public.agents ADD COLUMN IF NOT EXISTS dreaming_budget_reset_at TIMESTAMPTZ;

-- ============================================================================
-- DREAM SYNOPSES (new columns)
-- ============================================================================

ALTER TABLE public.consolidation_jobs ADD COLUMN IF NOT EXISTS dream_synopsis TEXT;
ALTER TABLE public.ontology_snapshots ADD COLUMN IF NOT EXISTS dream_synopsis TEXT;
ALTER TABLE public.ontology_snapshots ADD COLUMN IF NOT EXISTS consolidation_stats JSONB;

COMMIT;
