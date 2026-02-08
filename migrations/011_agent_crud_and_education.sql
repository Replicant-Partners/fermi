-- Migration 011: Agent CRUD support + education budget (AKP)
-- Adds system_prompt, tags, and education fields to agents table.
-- The user_id, is_public, and visibility columns already exist from migration 010
-- but were unused. This migration adds the remaining fields needed for user-created agents.

BEGIN;

-- System prompt for custom agent behavior
ALTER TABLE public.agents ADD COLUMN IF NOT EXISTS system_prompt TEXT;

-- Searchable tags
ALTER TABLE public.agents ADD COLUMN IF NOT EXISTS tags TEXT[] DEFAULT '{}';

-- AKP education budget (separate from dreaming credits)
ALTER TABLE public.agents ADD COLUMN IF NOT EXISTS education_budget_credits INTEGER NOT NULL DEFAULT 0;
ALTER TABLE public.agents ADD COLUMN IF NOT EXISTS education_credits_used INTEGER NOT NULL DEFAULT 0;

-- Ensure curated agents are public
UPDATE public.agents SET visibility = 'public' WHERE visibility IS NULL;

-- Index for visibility filtering
CREATE INDEX IF NOT EXISTS idx_agents_visibility ON public.agents(visibility);

-- Index for owner lookups (user_id column from migration 010)
CREATE INDEX IF NOT EXISTS idx_agents_user_id ON public.agents(user_id);

COMMIT;
