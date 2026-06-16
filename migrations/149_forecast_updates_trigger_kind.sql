-- ═══════════════════════════════════════════════════════════════════
-- Migration 149 — fermi_forecast_updates revision_trigger column
--
-- The `forecast_spacetime` table (migration 140) has a `revision_trigger`
-- column with an enum-shaped set of values:
--   'initial' | 'evidence_update' | 'agent_correction' | 'schedule_rerun'
-- | 'manual' | 'bayesops_refit'
--
-- The trigger function `fn_forecast_spacetime_on_update` (migration 140
-- §201) currently hard-codes `'evidence_update'` for every row inserted
-- via `fermi_forecast_updates`, which means BayesOps refits, manual
-- adjustments, scheduled re-runs, and so on all look identical in the
-- spacetime view.
--
-- This migration:
--   1. Adds an optional `revision_trigger` column to fermi_forecast_updates
--      so callers can specify the kind without changing the surrounding
--      schema. Defaults to NULL (interpreted as 'evidence_update' for
--      backward compatibility).
--   2. Adds a corresponding optional `revision_reason` (redundant with the
--      existing `reason` column for now — we keep `reason` for the
--      existing UI; this lets the trigger pass through the right field).
--   3. Replaces the trigger function so it reads the new column when
--      present and falls back to 'evidence_update' otherwise.
--
-- Required by Spec 23 R-3 Piece 1: BayesOps writes here with
-- revision_trigger='bayesops_refit' so the forecast_spacetime endpoint
-- surfaces refits in context with every other rate-moving event.
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE fermi_forecast_updates
    ADD COLUMN IF NOT EXISTS revision_trigger TEXT
        CHECK (revision_trigger IS NULL OR revision_trigger IN (
            'initial', 'evidence_update', 'agent_correction',
            'schedule_rerun', 'manual', 'bayesops_refit'
        ));

COMMENT ON COLUMN fermi_forecast_updates.revision_trigger IS
    'Optional category for the update; passed through to forecast_spacetime.revision_trigger by trg_forecast_spacetime. NULL means the trigger defaults to ''evidence_update'' (the historical behaviour). Set to ''bayesops_refit'' by Spec 23 R-1 refit hook and accept handler.';

-- ─── Updated trigger function ────────────────────────────────────────
-- Identical to migration 140's fn_forecast_spacetime_on_update except
-- the revision_trigger value comes from NEW.revision_trigger (falling
-- back to 'evidence_update'). Safe to replace because:
--   - The column was just added so NEW.revision_trigger is well-defined.
--   - Old code that doesn't set the column still produces
--     'evidence_update' rows (unchanged behaviour).

CREATE OR REPLACE FUNCTION fn_forecast_spacetime_on_update()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO forecast_spacetime (
        forecast_id, revision_seq, predicted_probability, previous_probability,
        revision_trigger, revision_reason, triggering_agent, evidence_delta,
        fpl_snapshot, revision_ts
    )
    SELECT
        NEW.forecast_id,
        COALESCE((
            SELECT MAX(revision_seq) + 1
            FROM forecast_spacetime
            WHERE forecast_id = NEW.forecast_id
        ), 1),
        NEW.new_probability,
        NEW.previous_probability,
        COALESCE(NEW.revision_trigger, 'evidence_update'),
        NEW.reason,
        NEW.agent_id,
        NEW.evidence_added,
        (SELECT fpl_source FROM fermi_forecasts WHERE id = NEW.forecast_id),
        NEW.created_at;
    RETURN NEW;
END;
$$;

-- The trigger itself doesn't need to change; CREATE OR REPLACE on the
-- function above is enough.
