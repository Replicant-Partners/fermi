-- ─────────────────────────────────────────────────────────────────────
-- 151 — pending_cascades: operator-gated cascade queue
-- ─────────────────────────────────────────────────────────────────────
--
-- When a forecast resolves (manually OR via an upstream workspace
-- resolution), the server doesn't auto-propagate to related siblings
-- because every parameter mutation must pass through a human (operator
-- rule). Instead, a pending_cascade row is queued for each non-archived
-- relationship the resolved forecast is part of.
--
-- The console surfaces a "N cascades pending review" badge; the
-- operator clicks each entry to see the proposed deltas (which
-- siblings, by how much) and either Apply (fires propagation) or
-- Dismiss (archives without applying).
--
-- Lifecycle:
--   pending  → queued, waiting for operator
--   applied  → operator clicked Apply; propagation fired
--   dismissed → operator clicked Dismiss; no propagation
--   superseded → trigger forecast got un-resolved / re-resolved,
--                this entry no longer relevant
--
-- The status column drives the queue view: only 'pending' rows show
-- in the badge count.

CREATE TABLE IF NOT EXISTS public.pending_cascades (
    id                      UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    relationship_id         UUID        NOT NULL REFERENCES public.forecast_relationships(id) ON DELETE CASCADE,
    trigger_forecast_id     TEXT        NOT NULL REFERENCES public.fermi_forecasts(id) ON DELETE CASCADE,
    -- 'resolved' (forecast resolved with known outcome) or
    -- 'updated' (probability changed but still active). Driven by
    -- the originating handler. See src/handlers/relationships.rs.
    trigger_kind            TEXT        NOT NULL,
    -- For trigger_kind='resolved': was the outcome true (1) or
    -- false (0). Null for 'updated'.
    outcome                 BOOLEAN,
    -- 'manual' (operator clicked Resolve) | 'workspace_auto' (upstream
    -- workspace resolution propagated to this forecast). Helps the UI
    -- explain why this cascade is waiting and tune trust.
    source                  TEXT        NOT NULL DEFAULT 'manual',
    -- Status — the queue surface only shows 'pending'. See lifecycle
    -- comment above.
    status                  TEXT        NOT NULL DEFAULT 'pending',
    owner_id                TEXT        NOT NULL,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Set on Apply or Dismiss. Lets the queue page show recent
    -- decisions, not just pending ones.
    decided_at              TIMESTAMPTZ,
    decided_by              TEXT,
    -- Free-form notes from the apply/dismiss action. Helpful for
    -- audit + the operator's future self.
    notes                   TEXT,
    -- Snapshot of the proposed deltas at queue time, computed by the
    -- propagate function in dry-run mode. JSONB shape:
    --   { "deltas": [{forecast_id, previous, projected, delta_pp}, ...],
    --     "note": <optional> }
    -- The operator sees this BEFORE clicking Apply so they know what
    -- they're authorising. Recomputed on Apply (so stale snapshots
    -- don't propagate wrong values if siblings shifted in the meantime).
    proposed_snapshot       JSONB
);

CREATE INDEX IF NOT EXISTS idx_pending_cascades_status
    ON public.pending_cascades(status);
CREATE INDEX IF NOT EXISTS idx_pending_cascades_owner
    ON public.pending_cascades(owner_id, status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_pending_cascades_trigger
    ON public.pending_cascades(trigger_forecast_id);
CREATE INDEX IF NOT EXISTS idx_pending_cascades_relationship
    ON public.pending_cascades(relationship_id);

-- Lifecycle CHECK so a typo in status doesn't end up silently in the
-- table.
DO $$ BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'pending_cascades_status_check'
    ) THEN
        ALTER TABLE public.pending_cascades
            ADD CONSTRAINT pending_cascades_status_check
            CHECK (status IN ('pending', 'applied', 'dismissed', 'superseded'));
    END IF;
END $$;
