-- 146: Make fermi_forecasts.notebook_id nullable.
--
-- Same schema-drift family as 138 and 144 — the original migration 048
-- declared notebook_id as NOT NULL, but the modern fermi-as-app workflow
-- creates forecasts without any notebook (a forecast can now be its own
-- workspace, with workspace_id from 139 serving as the grouping concept).
--
-- Concrete symptom: every publish from the fermi-console fails with
--   "null value in column \"notebook_id\" violates not-null constraint"
-- because the console's CreateForecastRequest leaves notebook_id as None
-- and the INSERT binds NULL.
--
-- Idempotent: DROP NOT NULL on an already-nullable column is a no-op in
-- Postgres ≥ 9.4 — it just emits a notice. Safe to re-run.

DO $$
BEGIN
    -- The FK constraint stays — a forecast that DOES have a notebook
    -- still references a real one; we just no longer require notebook
    -- membership at insert time.
    ALTER TABLE fermi_forecasts
        ALTER COLUMN notebook_id DROP NOT NULL;

    COMMENT ON COLUMN fermi_forecasts.notebook_id IS
        'Optional notebook membership (legacy fermi forecasting layer). '
        'Forecasts created via the fermi-as-app workflow use workspace_id '
        '(139) instead. Nullable since migration 146.';
END
$$;
