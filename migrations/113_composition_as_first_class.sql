-- Migration 113: Composition as a first-class element
--
-- Implements Step 1 of docs/COMPOSITION_AS_FIRST_CLASS.md §10.
-- Adds the minimum data model that supports the composition
-- creation arc (mission + strategist) and the tune-the-team RSI
-- loop (composition_versions snapshot history).
--
-- Backfill: existing teams keep working — mission and
-- coordination_strategist_id default to NULL. The runtime currently
-- doesn't use these fields, so existing workspaces are unaffected.
-- The new UX prompts users to fill them in when they engage with
-- existing workspaces.
--
-- PgBouncer-safe — idempotent, no BEGIN/COMMIT, no multi-statement
-- transactions that PgBouncer would split.

-- ─── §1 — composition identity ─────────────────────────────────────

ALTER TABLE public.teams
    ADD COLUMN IF NOT EXISTS mission                      TEXT,
    ADD COLUMN IF NOT EXISTS coordination_strategist_id   UUID,
    ADD COLUMN IF NOT EXISTS strategist_assigned_at       TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_teams_strategist
    ON public.teams(coordination_strategist_id)
    WHERE coordination_strategist_id IS NOT NULL;

COMMENT ON COLUMN public.teams.mission IS
    'Free-text declaration of what this composition accomplishes. ' ||
    'Captured during creation via the xamanEK-guided arc. ' ||
    'See docs/COMPOSITION_AS_FIRST_CLASS.md §2 step 1.';

COMMENT ON COLUMN public.teams.coordination_strategist_id IS
    'Pointer to an agent tagged ''coordination_strategy'' that embodies ' ||
    'how this composition coordinates work across its members. ' ||
    'See docs/COMPOSITION_AS_FIRST_CLASS.md §3.';

-- ─── §4 — tune-the-team RSI history ────────────────────────────────

CREATE TABLE IF NOT EXISTS public.composition_versions (
    composition_version_id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id                UUID NOT NULL REFERENCES public.teams(id) ON DELETE CASCADE,
    version_number              INT NOT NULL,
    mission                     TEXT,
    coordination_strategist_id  UUID,
    member_agent_ids            UUID[],
    member_weights              JSONB,
    diff_summary                TEXT,
    proposed_by                 TEXT,            -- 'user' or strategist agent_id::text
    accepted_by                 TEXT,            -- user_id of approving human
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_composition_versions_workspace
    ON public.composition_versions(workspace_id, version_number DESC);

COMMENT ON TABLE public.composition_versions IS
    'Snapshot history of (mission + strategist + members + weights). ' ||
    'Each row is a version of the composition produced either by user ' ||
    'edits or by tune-the-team RSI proposals from the strategist. ' ||
    'See docs/COMPOSITION_AS_FIRST_CLASS.md §4 + §6.';
