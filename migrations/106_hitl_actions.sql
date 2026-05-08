-- Migration 106: HITL Actions — Phase 4 of the Social Agent
-- Observability Platform.
--
-- Reconciles with:
--   docs/architecture/social_agent_observability_architecture.html (Plane D)
--   docs/architecture/OBSERVABILITY_IMPL.md (Phase 4 entry)
--
-- Phase 3 introduced `anomaly_events.requires_review`/`resolved_at`/
-- `resolved_by` for a coarse "this event needs a human" pointer.
-- Phase 4 layers a richer audit trail on top with this table:
--
--   hitl_actions — append-only log of reviewer actions on anomaly events.
--                  One row per reviewer-action; an anomaly may have multiple
--                  rows (e.g. an "approve" followed by an "intervene").
--
-- The architecture-doc Plane D mock distinguishes three reviewer
-- actions: approve, relabel, intervene. Phase 4 ships approve+relabel
-- only; intervene rejects with a "Phase 5 not yet implemented" payload
-- so the surface is wired but the destructive path is gated.
--
-- All ALTERs are idempotent. No constraint mutations needed.

CREATE TABLE IF NOT EXISTS public.hitl_actions (
    action_id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Linkage to the anomaly being acted on (Phase 3)
    anomaly_event_id       UUID NOT NULL REFERENCES public.anomaly_events(event_id) ON DELETE CASCADE,
    agent_id               UUID NOT NULL REFERENCES public.agents(agent_id)        ON DELETE CASCADE,

    -- Reviewer
    reviewer_id            TEXT NOT NULL,

    -- Action — see ReviewerAction enum (Phase 0). Phase 4 ships
    -- 'approve' and 'relabel' only; 'intervene' is rejected at the
    -- handler with a 501 until Phase 5 lands the full intervention
    -- flow (coherence gate + two-write memory pattern).
    action                 TEXT NOT NULL
        CHECK (action IN ('approve', 'relabel', 'intervene')),

    -- Optional notes from the reviewer
    notes                  TEXT,

    -- For 'relabel': dimension overrides as { dim_name: f64 }
    score_overrides        JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Pointer to a Phase 5 episode_correction row when 'intervene'.
    -- Null in Phase 4 because intervene is gated.
    correction_id          UUID REFERENCES public.episode_corrections(correction_id) ON DELETE SET NULL,

    created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_hitl_actions_anomaly
    ON public.hitl_actions(anomaly_event_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_hitl_actions_agent
    ON public.hitl_actions(agent_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_hitl_actions_reviewer
    ON public.hitl_actions(reviewer_id, created_at DESC);

-- Append-only enforcement — same pattern as episode_corrections.
CREATE OR REPLACE FUNCTION public.hitl_actions_immutable()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'hitl_actions is append-only; row % cannot be modified', OLD.action_id;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_hitl_actions_no_update ON public.hitl_actions;
CREATE TRIGGER trg_hitl_actions_no_update
    BEFORE UPDATE ON public.hitl_actions
    FOR EACH ROW EXECUTE FUNCTION public.hitl_actions_immutable();

-- ═══════════════════════════════════════════════════════════════════
-- End of migration 106
-- ═══════════════════════════════════════════════════════════════════
