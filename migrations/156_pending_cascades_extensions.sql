-- ─────────────────────────────────────────────────────────────────────
-- 156 — Pending cascades extensions for undo/supersede (Spec 25 §4.3-4.6)
-- ─────────────────────────────────────────────────────────────────────
--
-- Extends the pending_cascades table (mig 153) to support:
--   - applied_deltas: captures what was actually written at Apply
--     time, for undo support
--   - group_id: replaces relationship_id as the foreign key to the
--     new forecast_relationship_groups table (group tag model)
--   - superseded_by: points at the newer cascade row when re-queued
--   - 'undone' status: terminal state for undid cascades
--   - 'cascade_undo' revision_trigger value on fermi_forecast_updates
--
-- The existing relationship_id (UUID FK to forecast_relationships) is
-- kept for backward compatibility but a new group_id column is added.
-- At the application layer, group_id takes precedence when present.

-- §4.4: applied_deltas — what was actually written at Apply time.
-- Shape: [{forecast_id, prev_pp, new_pp, delta_pp}, ...]
ALTER TABLE public.pending_cascades
    ADD COLUMN IF NOT EXISTS applied_deltas JSONB;

-- §4.6: superseded_by — points at the replacement cascade row.
ALTER TABLE public.pending_cascades
    ADD COLUMN IF NOT EXISTS superseded_by UUID
    REFERENCES public.pending_cascades(id);

-- group_id — the new group tag reference. When present, takes
-- precedence over relationship_id at the application layer.
ALTER TABLE public.pending_cascades
    ADD COLUMN IF NOT EXISTS group_id TEXT
    REFERENCES public.forecast_relationship_groups(group_id);

CREATE INDEX IF NOT EXISTS idx_pending_cascades_group_id
    ON public.pending_cascades(group_id) WHERE group_id IS NOT NULL;

-- §4.5: Extend status CHECK to include 'undone'.
-- Drop + recreate (PG can't ALTER CHECK in place).
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'pending_cascades_status_check'
    ) THEN
        ALTER TABLE public.pending_cascades
            DROP CONSTRAINT pending_cascades_status_check;
    END IF;
END $$;

ALTER TABLE public.pending_cascades
    ADD CONSTRAINT pending_cascades_status_check
    CHECK (status IN ('pending', 'applied', 'dismissed', 'superseded', 'undone'));

-- Add 'cascade_undo' to the legal revision_trigger values on
-- fermi_forecast_updates so undo writes can tag their rows.
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fermi_forecast_updates_revision_trigger_check'
    ) THEN
        ALTER TABLE public.fermi_forecast_updates
            DROP CONSTRAINT fermi_forecast_updates_revision_trigger_check;
    END IF;
END $$;

ALTER TABLE public.fermi_forecast_updates
    ADD CONSTRAINT fermi_forecast_updates_revision_trigger_check
    CHECK (
        revision_trigger IS NULL OR revision_trigger IN (
            'initial',
            'evidence_update',
            'agent_correction',
            'schedule_rerun',
            'manual',
            'bayesops_refit',
            'cascade',
            'cascade_undo'
        )
    );
