-- Migration 142: Performance indices (Spec 21 Phase 2 + Phase 2.4)
--
-- Phase 2: HNSW vector indices for KG hot-path retrieval
-- Phase 2.4: partial composite index on episodes for unconsolidated scan
--
-- HNSW requires pgvector >= 0.5.0 (Neon ships >= 0.5.0 since late 2023).
-- Wrapped in DO blocks for PgBouncer compatibility.
--
-- These were originally written CONCURRENTLY (no table lock, safe in
-- production), but CONCURRENTLY cannot run inside a function body or a
-- transaction, and a DO block is a function body — so every statement
-- below errored with "CREATE INDEX CONCURRENTLY cannot be executed from
-- a function" and this migration had never applied anywhere. The DO
-- blocks are what make this file PgBouncer-safe, so they stay and the
-- CONCURRENTLY goes: what actually runs is a plain
-- CREATE INDEX IF NOT EXISTS, still guarded by the pg_indexes check
-- above each one, so it remains idempotent. It takes a write lock for
-- the duration of the build; if a table ever grows large enough for
-- that to matter, build that index by hand outside a DO block instead.

DO $$ BEGIN
    -- Episodes: partial composite index for unconsolidated subset
    -- Replaces full agent-episode scan in get_unconsolidated_episodes.
    -- Without this, Postgres scans ALL episodes for an agent to find the subset.
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes WHERE indexname = 'idx_episodes_agent_unconsolidated'
    ) THEN
        CREATE INDEX IF NOT EXISTS idx_episodes_agent_unconsolidated
            ON episodes(agent_id, timestamp_ref DESC)
            WHERE NOT consolidated;
    END IF;
END $$;

DO $$ BEGIN
    -- Semantic rules: composite partial index for active rules per agent
    -- Replaces separate agent_id scan + is_active filter.
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes WHERE indexname = 'idx_semantic_rules_agent_active_conf'
    ) THEN
        CREATE INDEX IF NOT EXISTS idx_semantic_rules_agent_active_conf
            ON semantic_rules(agent_id, confidence_score DESC)
            WHERE is_active = true;
    END IF;
END $$;

-- HNSW indices for vector similarity (pgvector ANN)
-- These replace in-memory cosine scoring over full corpus loads.
-- ef_construction=64, m=16: conservative start; tune upward if recall insufficient.
-- Built non-concurrently for the reason described at the top of this file.
DO $$ BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes WHERE indexname = 'idx_semantic_rules_embedding_hnsw'
    ) THEN
        CREATE INDEX IF NOT EXISTS idx_semantic_rules_embedding_hnsw
            ON semantic_rules
            USING hnsw (embedding vector_cosine_ops)
            WITH (m = 16, ef_construction = 64)
            WHERE is_active = true;
    END IF;
EXCEPTION WHEN OTHERS THEN
    -- HNSW may fail if pgvector < 0.5.0; non-fatal, code falls back to load-all path.
    RAISE WARNING 'HNSW index creation skipped: %', SQLERRM;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes WHERE indexname = 'idx_entities_embedding_hnsw'
    ) THEN
        CREATE INDEX IF NOT EXISTS idx_entities_embedding_hnsw
            ON entities
            USING hnsw (embedding vector_cosine_ops)
            WITH (m = 16, ef_construction = 64);
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING 'HNSW entity index creation skipped: %', SQLERRM;
END $$;
