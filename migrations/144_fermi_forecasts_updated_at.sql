-- 144: Add updated_at to fermi_forecasts
--
-- Same schema-drift pattern as 107 and 138. Migration 048 created
-- fermi_forecasts WITHOUT updated_at. Migration 094 re-CREATE TABLE
-- IF NOT EXISTS'd it with updated_at, which is a no-op when the table
-- already exists. Migrations 107 and 138 patched 18 missing columns
-- between them but missed updated_at.
--
-- Concrete symptom: publish + edit-and-save fail with
--   "column updated_at of relation fermi_forecasts does not exist"
-- because the INSERT/UPDATE in src/handlers/forecasts.rs binds it.
--
-- Idempotent — safe to re-run.

ALTER TABLE fermi_forecasts
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Backfill existing rows: use created_at as the initial updated_at value
-- so the column is meaningful for forecasts that existed before this
-- migration. NOW() default is wrong for those rows because they were not
-- in fact updated at migration time.
UPDATE fermi_forecasts
SET updated_at = created_at
WHERE updated_at >= created_at - INTERVAL '1 minute'
  AND updated_at <= created_at + INTERVAL '1 minute';
-- The bounded clause avoids overwriting any genuinely-fresh updated_at
-- values, in environments where the column was added manually before
-- this migration ran.

-- Defensive: also audit the other fermi_* tables for the same drift.
-- Migration 048 schema for these is known-good but reaffirming is cheap.
ALTER TABLE fermi_notebooks
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

ALTER TABLE fermi_portfolios
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
