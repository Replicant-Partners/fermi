-- Migration 133: dyad_profiles — named, intentional human-agent relationships
--
-- dyad_state (migration 105) holds the running scores (rapport/trust/reciprocity)
-- computed from interaction history. dyad_profiles is the identity layer on top:
-- a human-readable name, metadata, and auto-formation tracking.
--
-- Auto-formation: when an agent-user pair accumulates >= 3 episodes with a
-- non-null dyad_id, the application layer creates a dyad_profile row.
-- The operator can then name and annotate the relationship.
--
-- This table is the foundation for:
--   - The "Relationships" tab in the observatory (agent's social graph)
--   - Customer service continuity (recognise returning users)
--   - Dyad-scoped knowledge graph entries (what the agent knows about this person)

CREATE TABLE IF NOT EXISTS public.dyad_profiles (
    -- Natural key matches dyad_state.dyad_id
    dyad_id             TEXT PRIMARY KEY,

    -- Participants
    agent_id            UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    human_id            TEXT NOT NULL,      -- user_id string

    -- Identity
    display_name        TEXT,               -- operator-assigned: "Ivan", "Customer #447"
    notes               TEXT,               -- operator freetext: context, preferences, flags
    tags                TEXT[] DEFAULT '{}',

    -- Formation
    auto_formed         BOOLEAN NOT NULL DEFAULT TRUE,
    formed_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    formation_threshold INTEGER NOT NULL DEFAULT 3,  -- episodes required for auto-formation

    -- Activity
    first_interaction_at TIMESTAMPTZ,
    last_interaction_at  TIMESTAMPTZ,
    total_interactions   INTEGER NOT NULL DEFAULT 0,

    -- Lifecycle
    archived_at         TIMESTAMPTZ,        -- soft-delete / end of relationship
    archived_reason     TEXT,

    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_dyad_profiles_agent_id
    ON public.dyad_profiles(agent_id, last_interaction_at DESC);

CREATE INDEX IF NOT EXISTS idx_dyad_profiles_human_id
    ON public.dyad_profiles(human_id);

CREATE INDEX IF NOT EXISTS idx_dyad_profiles_active
    ON public.dyad_profiles(agent_id)
    WHERE archived_at IS NULL;
