-- Migration: Add user ownership to agents
-- Date: 2026-02-08
-- Description: Adds user_id and visibility columns to agents table for multi-tenant isolation

-- Add columns (nullable initially for backfill)
ALTER TABLE public.agents
    ADD COLUMN IF NOT EXISTS user_id TEXT REFERENCES public.users(user_id) ON DELETE CASCADE,
    ADD COLUMN IF NOT EXISTS is_public BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS visibility TEXT NOT NULL DEFAULT 'private'
        CHECK (visibility IN ('private', 'unlisted', 'public'));

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_agents_user_id ON public.agents(user_id);
CREATE INDEX IF NOT EXISTS idx_agents_visibility ON public.agents(visibility);
CREATE INDEX IF NOT EXISTS idx_agents_public ON public.agents(is_public) WHERE is_public = TRUE;
CREATE INDEX IF NOT EXISTS idx_agents_user_visibility ON public.agents(user_id, visibility);

-- Comments
COMMENT ON COLUMN public.agents.user_id IS 'Owner of this agent - references users table';
COMMENT ON COLUMN public.agents.is_public IS 'Quick check for public visibility';
COMMENT ON COLUMN public.agents.visibility IS 'private: owner only, unlisted: link only, public: catalog listed';

-- NOTE: Backfill will be done separately after creating admin user
-- Example backfill:
-- UPDATE public.agents
-- SET user_id = '<admin-user-id>', is_public = TRUE, visibility = 'public'
-- WHERE user_id IS NULL;
