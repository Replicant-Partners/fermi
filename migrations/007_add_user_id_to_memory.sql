-- Migration: Add user ownership to memory tables
-- Date: 2026-02-08
-- Description: Adds user_id to episodes, semantic_rules, entities, facts, relationships

-- Add user_id to episodes
ALTER TABLE public.episodes
    ADD COLUMN IF NOT EXISTS user_id TEXT REFERENCES public.users(user_id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS idx_episodes_user_id ON public.episodes(user_id);
CREATE INDEX IF NOT EXISTS idx_episodes_user_agent ON public.episodes(user_id, agent_id);

-- Add user_id to semantic_rules
ALTER TABLE public.semantic_rules
    ADD COLUMN IF NOT EXISTS user_id TEXT REFERENCES public.users(user_id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS idx_semantic_rules_user_id ON public.semantic_rules(user_id);
CREATE INDEX IF NOT EXISTS idx_semantic_rules_user_agent ON public.semantic_rules(user_id, agent_id);

-- Add user_id to entities
ALTER TABLE public.entities
    ADD COLUMN IF NOT EXISTS user_id TEXT REFERENCES public.users(user_id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS idx_entities_user_id ON public.entities(user_id);

-- Add user_id to facts
ALTER TABLE public.facts
    ADD COLUMN IF NOT EXISTS user_id TEXT REFERENCES public.users(user_id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS idx_facts_user_id ON public.facts(user_id);

-- Add user_id to relationships
ALTER TABLE public.relationships
    ADD COLUMN IF NOT EXISTS user_id TEXT REFERENCES public.users(user_id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS idx_relationships_user_id ON public.relationships(user_id);

-- Comments
COMMENT ON COLUMN public.episodes.user_id IS 'Owner - for multi-tenant isolation';
COMMENT ON COLUMN public.semantic_rules.user_id IS 'Owner - for multi-tenant isolation';
COMMENT ON COLUMN public.entities.user_id IS 'Owner - for multi-tenant isolation';
COMMENT ON COLUMN public.facts.user_id IS 'Owner - for multi-tenant isolation';
COMMENT ON COLUMN public.relationships.user_id IS 'Owner - for multi-tenant isolation';

-- NOTE: Backfill strategy - derive from agents table
-- Example:
-- UPDATE public.episodes e
-- SET user_id = a.user_id
-- FROM public.agents a
-- WHERE e.agent_id = a.agent_id AND e.user_id IS NULL;
