-- Migration 115: Xaman Ek working sessions
--
-- Persistent session state for the xamanEK dungeon master interface.
-- Each session represents a sustained working context — designing an agent,
-- planning a composition, or getting help with a workspace.
--
-- Sessions survive page navigation and browser refresh so xamanEK can
-- pick up where the user left off rather than starting cold.
--
-- PgBouncer-safe: all statements wrapped in DO blocks.

DO $$
BEGIN
    CREATE TABLE IF NOT EXISTS xaman_sessions (
        session_id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        user_id             TEXT NOT NULL,

        -- What kind of work this session is about
        session_type        TEXT NOT NULL DEFAULT 'free'
            CHECK (session_type IN (
                'agent_design',       -- building an agent card
                'composition_design', -- planning a composition
                'workspace_help',     -- helping with a specific workspace
                'free'               -- open conversation
            )),

        -- Short title — auto-set from the first turn's topic
        title               TEXT,

        -- The thing being built: agent card draft, composition plan, etc.
        -- Structured so xamanEK can resume mid-task
        in_progress         JSONB NOT NULL DEFAULT '{}'::jsonb,

        -- Last N message turns for sidebar display (max 20 stored)
        messages            JSONB NOT NULL DEFAULT '[]'::jsonb,

        -- Last known page context (page path + relevant IDs)
        page_context        TEXT,

        -- Status
        status              TEXT NOT NULL DEFAULT 'active'
            CHECK (status IN ('active', 'completed', 'abandoned')),

        created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        last_active_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
    );
EXCEPTION WHEN duplicate_table THEN NULL;
END $$;

DO $$
BEGIN
    CREATE INDEX IF NOT EXISTS idx_xaman_sessions_user
        ON xaman_sessions(user_id, last_active_at DESC);
EXCEPTION WHEN duplicate_table THEN NULL;
END $$;

DO $$
BEGIN
    CREATE INDEX IF NOT EXISTS idx_xaman_sessions_active
        ON xaman_sessions(user_id, status)
        WHERE status = 'active';
EXCEPTION WHEN duplicate_table THEN NULL;
END $$;
