-- Migration: Add user ownership to memory tables
-- Date: 2026-02-08
-- Description: Adds user_id to episodes, semantic_rules, entities, facts
-- Note: No foreign keys to avoid complexity with existing schema

BEGIN;

-- Add user_id to episodes (nullable for now)
ALTER TABLE public.episodes
    ADD COLUMN IF NOT EXISTS user_id TEXT;

-- Backfill from agents table if possible
UPDATE public.episodes e
SET user_id = a.user_id
FROM public.agents a
WHERE e.agent_id = a.agent_id AND e.user_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_episodes_user_id ON public.episodes(user_id) WHERE user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_episodes_user_agent ON public.episodes(user_id, agent_id) WHERE user_id IS NOT NULL;

-- Add user_id to semantic_rules (nullable for now)
ALTER TABLE public.semantic_rules
    ADD COLUMN IF NOT EXISTS user_id TEXT;

-- Backfill from agents table if possible
UPDATE public.semantic_rules sr
SET user_id = a.user_id
FROM public.agents a
WHERE sr.agent_id = a.agent_id AND sr.user_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_semantic_rules_user_id ON public.semantic_rules(user_id) WHERE user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_semantic_rules_user_agent ON public.semantic_rules(user_id, agent_id) WHERE user_id IS NOT NULL;

-- Add user_id to entities if table exists (nullable for now)
DO $$
BEGIN
    IF EXISTS (SELECT FROM pg_tables WHERE schemaname = 'public' AND tablename = 'entities') THEN
        ALTER TABLE public.entities ADD COLUMN IF NOT EXISTS user_id TEXT;
        CREATE INDEX IF NOT EXISTS idx_entities_user_id ON public.entities(user_id) WHERE user_id IS NOT NULL;
    END IF;
END $$;

-- Add user_id to facts if table exists (nullable for now)
DO $$
BEGIN
    IF EXISTS (SELECT FROM pg_tables WHERE schemaname = 'public' AND tablename = 'facts') THEN
        ALTER TABLE public.facts ADD COLUMN IF NOT EXISTS user_id TEXT;
        CREATE INDEX IF NOT EXISTS idx_facts_user_id ON public.facts(user_id) WHERE user_id IS NOT NULL;
    END IF;
END $$;

-- Comments
COMMENT ON COLUMN public.episodes.user_id IS 'Owner - for multi-tenant isolation (derived from agent owner)';
COMMENT ON COLUMN public.semantic_rules.user_id IS 'Owner - for multi-tenant isolation (derived from agent owner)';

COMMIT;
