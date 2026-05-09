-- Migration: Add user ownership to agents
-- Date: 2026-02-08
-- Description: Adds user_id and visibility columns to agents table for multi-tenant isolation
-- Note: No foreign keys for simpler migration
--
-- 2026-05-09 patch: removed the original "backfill NULL user_id to first user
-- in the table" UPDATE block. That block ran on every startup (no migration
-- tracking table) and the seeder inserts curated agents with user_id = NULL,
-- so every new curated agent was being silently re-assigned to whichever user
-- the database happened to return first from SELECT LIMIT 1. Migration 110
-- repairs the resulting damage.

-- Add columns (nullable initially for backfill).
-- No BEGIN/COMMIT — PgBouncer manages transactions in transaction mode.
ALTER TABLE public.agents
    ADD COLUMN IF NOT EXISTS user_id TEXT,
    ADD COLUMN IF NOT EXISTS is_public BOOLEAN DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS visibility TEXT DEFAULT 'private'
        CHECK (visibility IN ('private', 'unlisted', 'public'));

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_agents_user_id ON public.agents(user_id) WHERE user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_agents_visibility ON public.agents(visibility);
CREATE INDEX IF NOT EXISTS idx_agents_public ON public.agents(is_public) WHERE is_public = TRUE;
CREATE INDEX IF NOT EXISTS idx_agents_user_visibility ON public.agents(user_id, visibility) WHERE user_id IS NOT NULL;

-- Comments
COMMENT ON COLUMN public.agents.user_id IS 'Owner of this agent - references users.user_id';
COMMENT ON COLUMN public.agents.is_public IS 'Quick check for public visibility';
COMMENT ON COLUMN public.agents.visibility IS 'private: owner only, unlisted: link only, public: catalog listed';
