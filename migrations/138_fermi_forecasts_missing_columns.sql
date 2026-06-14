-- 138: Add missing columns to fermi_forecasts
--
-- The fermi_forecasts table was originally created by migration 048 which
-- lacked several columns that migration 094 expected to add via
-- CREATE TABLE IF NOT EXISTS (which was a no-op since the table existed).
-- Migration 107 caught some columns but missed status, visibility, and team_id.

-- ── fermi_forecasts: missing columns ─────────────────────────────────
ALTER TABLE fermi_forecasts
    ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'draft',
    ADD COLUMN IF NOT EXISTS visibility TEXT NOT NULL DEFAULT 'private',
    ADD COLUMN IF NOT EXISTS team_id UUID,
    ADD COLUMN IF NOT EXISTS target_date TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS question_version INTEGER NOT NULL DEFAULT 1;

-- Add CHECK constraints only if not already present (idempotent via DO block)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fermi_forecasts_status_check'
    ) THEN
        ALTER TABLE fermi_forecasts
            ADD CONSTRAINT fermi_forecasts_status_check
            CHECK (status IN ('draft', 'active', 'resolved', 'voided'));
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fermi_forecasts_visibility_check'
    ) THEN
        ALTER TABLE fermi_forecasts
            ADD CONSTRAINT fermi_forecasts_visibility_check
            CHECK (visibility IN ('private', 'shared', 'public'));
    END IF;
END
$$;

-- Mark existing forecasts with probabilities as 'active' (they were created
-- before the status column existed, so they're all currently 'draft' default)
UPDATE fermi_forecasts
SET status = 'active'
WHERE status = 'draft'
  AND predicted_probability IS NOT NULL
  AND resolved_at IS NULL;

-- Mark resolved forecasts
UPDATE fermi_forecasts
SET status = 'resolved'
WHERE status = 'draft'
  AND resolved_at IS NOT NULL;

-- Indexes for new columns
CREATE INDEX IF NOT EXISTS idx_forecasts_status ON fermi_forecasts(status);
CREATE INDEX IF NOT EXISTS idx_forecasts_visibility ON fermi_forecasts(visibility);
