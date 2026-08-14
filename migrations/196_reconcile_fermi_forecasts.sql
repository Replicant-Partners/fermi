-- Migration 196: give fermi_forecasts the columns 094 believes it created
--
-- Runs between 048 and 094 in `api_server::run_migrations()`, despite its
-- number. The runner order is that list, not the filename order, and this file
-- has to sit in the gap between the migration that creates the table and the
-- migration that assumes a wider version of it.
--
-- ─── The defect ────────────────────────────────────────────────────────
--
-- 048 creates `fermi_forecasts` with 13 columns. 094 then runs
-- `CREATE TABLE IF NOT EXISTS fermi_forecasts (...)` declaring 28 — and
-- because the table already exists, **the entire statement is skipped**. Its
-- 15 extra columns never appear. 094 then aborts three statements later on
--
--     CREATE INDEX idx_forecasts_status ON fermi_forecasts(status)
--     ERROR: column "status" does not exist
--
-- and never reaches its own `CREATE TABLE fermi_forecast_updates` near the
-- bottom of the file. That single skipped statement is the root of six further
-- failures — 140, 149, 150, 156, 174 and 176 all abort on
-- `fermi_forecast_updates` not existing — and 140's failure takes 175 with it,
-- because 140 is what creates `forecast_spacetime`.
--
-- Eight migrations, one `IF NOT EXISTS` that quietly meant "do nothing". It is
-- the same shape as the `users.id` ghost (004 edited in place), the
-- `password_hash` ghost, and the `auth_provider` CHECK that 004b tried to
-- widen with `ADD COLUMN IF NOT EXISTS`. See
-- docs/plans/CI_MIGRATION_RATCHET.md.
--
-- ─── Why here and not in 094 ───────────────────────────────────────────
--
-- 094 has already been applied to production, where `fermi_forecasts` does
-- carry all 28 columns. Editing an applied migration in place is precisely
-- what created the `users.id` ghost, so the fix is additive and external:
-- supply the missing columns and let 094 succeed unchanged on the next boot.
--
-- No-op against production, which already has every column below.

DO $$
BEGIN
    IF to_regclass('public.fermi_forecasts') IS NULL THEN
        RAISE NOTICE '[mig 196] fermi_forecasts not present yet; nothing to reconcile on this pass';
        RETURN;
    END IF;

    -- Definitions copied from 094's CREATE TABLE, minus the FK on team_id:
    -- `teams` may not exist at this point in the order, and 165 realigns that
    -- FK later anyway. A column of the right type with no constraint is what
    -- makes 094's indexes buildable; the FK is 094's and 165's business.
    ALTER TABLE public.fermi_forecasts
        ADD COLUMN IF NOT EXISTS domain              TEXT,
        ADD COLUMN IF NOT EXISTS resolution_criteria TEXT,
        ADD COLUMN IF NOT EXISTS target_date         TIMESTAMPTZ,
        ADD COLUMN IF NOT EXISTS fpl_source          TEXT,
        ADD COLUMN IF NOT EXISTS simulation_results  JSONB,
        ADD COLUMN IF NOT EXISTS iterations          INTEGER DEFAULT 10000,
        ADD COLUMN IF NOT EXISTS drivers             JSONB NOT NULL DEFAULT '[]'::jsonb,
        ADD COLUMN IF NOT EXISTS evidence            JSONB NOT NULL DEFAULT '[]'::jsonb,
        ADD COLUMN IF NOT EXISTS agents_used         JSONB NOT NULL DEFAULT '[]'::jsonb,
        ADD COLUMN IF NOT EXISTS status              TEXT NOT NULL DEFAULT 'draft',
        ADD COLUMN IF NOT EXISTS resolved_by         TEXT,
        ADD COLUMN IF NOT EXISTS resolution_notes    TEXT,
        ADD COLUMN IF NOT EXISTS visibility          TEXT NOT NULL DEFAULT 'private',
        ADD COLUMN IF NOT EXISTS team_id             UUID,
        ADD COLUMN IF NOT EXISTS tags                TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
        ADD COLUMN IF NOT EXISTS updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW();

    -- The CHECKs 094 attaches inline. Added separately, guarded by constraint
    -- name so a second boot does not fail on a duplicate and a production
    -- table that already has them is untouched.
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fermi_forecasts_status_check'
    ) THEN
        ALTER TABLE public.fermi_forecasts
            ADD CONSTRAINT fermi_forecasts_status_check
            CHECK (status IN ('draft', 'active', 'resolved', 'voided'));
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fermi_forecasts_visibility_check'
    ) THEN
        ALTER TABLE public.fermi_forecasts
            ADD CONSTRAINT fermi_forecasts_visibility_check
            CHECK (visibility IN ('private', 'shared', 'public'));
    END IF;
END $$;
