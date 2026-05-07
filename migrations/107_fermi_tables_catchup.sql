-- 107: Fermi tables catch-up columns
--
-- Production ran migration 094 before domain/metadata/tags columns were
-- added to the CREATE TABLE statements. This migration safely adds any
-- missing columns using IF NOT EXISTS guards.

-- ── fermi_portfolios ───────────────────────────────────────────────
ALTER TABLE fermi_portfolios
    ADD COLUMN IF NOT EXISTS domain TEXT,
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb;

-- ── fermi_forecasts ────────────────────────────────────────────────
ALTER TABLE fermi_forecasts
    ADD COLUMN IF NOT EXISTS domain TEXT,
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    ADD COLUMN IF NOT EXISTS resolution_criteria TEXT,
    ADD COLUMN IF NOT EXISTS resolved_by TEXT,
    ADD COLUMN IF NOT EXISTS resolution_notes TEXT,
    ADD COLUMN IF NOT EXISTS confidence_interval_low REAL,
    ADD COLUMN IF NOT EXISTS confidence_interval_high REAL,
    ADD COLUMN IF NOT EXISTS fpl_source TEXT,
    ADD COLUMN IF NOT EXISTS notebook_id TEXT,
    ADD COLUMN IF NOT EXISTS simulation_results JSONB,
    ADD COLUMN IF NOT EXISTS iterations INTEGER DEFAULT 10000,
    ADD COLUMN IF NOT EXISTS drivers JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS evidence JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS agents_used JSONB NOT NULL DEFAULT '[]'::jsonb;

-- Indexes that may be missing if columns were just added
CREATE INDEX IF NOT EXISTS idx_forecasts_domain ON fermi_forecasts(domain)
    WHERE domain IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_forecasts_tags ON fermi_forecasts USING gin(tags);
