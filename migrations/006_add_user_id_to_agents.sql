-- Migration: Add user ownership to agents
-- Date: 2026-02-08
-- Description: Adds user_id and visibility columns to agents table for multi-tenant isolation
-- Note: No foreign keys for simpler migration

BEGIN;

-- Add columns (nullable initially for backfill)
ALTER TABLE public.agents
    ADD COLUMN IF NOT EXISTS user_id TEXT,
    ADD COLUMN IF NOT EXISTS is_public BOOLEAN DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS visibility TEXT DEFAULT 'private'
        CHECK (visibility IN ('private', 'unlisted', 'public'));

-- Set default admin user for existing agents (using first user in DB)
UPDATE public.agents
SET user_id = (SELECT user_id FROM public.users LIMIT 1),
    is_public = TRUE,
    visibility = 'public'
WHERE user_id IS NULL;

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_agents_user_id ON public.agents(user_id) WHERE user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_agents_visibility ON public.agents(visibility);
CREATE INDEX IF NOT EXISTS idx_agents_public ON public.agents(is_public) WHERE is_public = TRUE;
CREATE INDEX IF NOT EXISTS idx_agents_user_visibility ON public.agents(user_id, visibility) WHERE user_id IS NOT NULL;

-- Comments
COMMENT ON COLUMN public.agents.user_id IS 'Owner of this agent - references users.user_id';
COMMENT ON COLUMN public.agents.is_public IS 'Quick check for public visibility';
COMMENT ON COLUMN public.agents.visibility IS 'private: owner only, unlisted: link only, public: catalog listed';

COMMIT;
