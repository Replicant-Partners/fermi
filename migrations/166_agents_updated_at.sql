-- ═══════════════════════════════════════════════════════════════════
-- Migration 166 — add public.agents.updated_at
--
-- v0.10.18 hotfix. Ivan hit `column "updated_at" of relation "agents"
-- does not exist` on the first successful force-publish under v0.10.15.
-- (v0.10.17 was parallel Activity-panel work, unrelated.)
-- Four write sites reference this column and all have been silently
-- 500'ing since the code shipped:
--
--   1. workflows/publish_pipeline.rs::publish_agent
--   2. workflows/publish_pipeline.rs::archive_agent
--   3. workflows/publish_pipeline.rs::restore_agent
--   4. handlers/lifecycle.rs::update_fork_pricing_handler
--
-- Root cause: `agents` was created in mig-010 with `created_at` only.
-- Every other publishable substrate on the platform carries both
-- (`apps`, `fermi_forecasts`, `fermi_portfolios`, `fermi_notebooks`,
-- `teams`, `wallets`, …). The code was written against the intended
-- invariant "publishable resources have created_at + updated_at", the
-- schema drifted from it, and nothing caught the drift because the
-- write sites are only reachable under a specific auth + admin bypass
-- + publish-checks path that v0.10.13/v0.10.15 unblocked for the
-- first time.
--
-- Fix: add the column, backfill existing rows from `created_at`
-- (better than "all migration time" — preserves relative freshness),
-- then make it NOT NULL DEFAULT NOW() so future INSERTs get it
-- automatically even if the caller doesn't set it.
--
-- Sequenced so no row ever violates NOT NULL:
--   1. ADD COLUMN nullable → all rows get NULL.
--   2. Backfill NULL → created_at.
--   3. SET NOT NULL + DEFAULT NOW().
--
-- Idempotent: guarded by IF NOT EXISTS + catalog probes. Safe to
-- re-run. Individual failures logged as WARNINGs, don't abort.
--
-- PgBouncer-safe: no BEGIN/COMMIT; each DDL in its own DO block.
--
-- This is exactly the kind of drift the v0.11.0 "trust contract"
-- (schema-consistency check that compares pg_get_constraintdef +
-- information_schema.columns against migration files at boot)
-- would catch at deploy time instead of on the first force-publish.
-- ═══════════════════════════════════════════════════════════════════

-- ── Pre-migration diagnostics ──────────────────────────────────────
DO $$
DECLARE
    n_agents INTEGER;
    has_col  BOOLEAN;
BEGIN
    SELECT COUNT(*) INTO n_agents FROM public.agents;
    SELECT EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name   = 'agents'
           AND column_name  = 'updated_at'
    ) INTO has_col;
    RAISE NOTICE '[mig 166] pre-migration — agents rows: %, updated_at present: %',
        n_agents, has_col;
END $$;

-- ── Step 1: add nullable column ────────────────────────────────────
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name   = 'agents'
           AND column_name  = 'updated_at'
    ) THEN
        ALTER TABLE public.agents
            ADD COLUMN updated_at TIMESTAMPTZ;
        RAISE NOTICE '[mig 166] added agents.updated_at (nullable, pre-backfill)';
    ELSE
        RAISE NOTICE '[mig 166] agents.updated_at already present — skipping ADD';
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 166] ADD COLUMN updated_at failed: %', SQLERRM;
END $$;

-- ── Step 2: backfill NULL → created_at ─────────────────────────────
DO $$
DECLARE
    n_backfilled INTEGER;
BEGIN
    UPDATE public.agents
       SET updated_at = created_at
     WHERE updated_at IS NULL;
    GET DIAGNOSTICS n_backfilled = ROW_COUNT;
    RAISE NOTICE '[mig 166] backfilled % rows (updated_at ← created_at)', n_backfilled;
END $$;

-- ── Step 3: NOT NULL + DEFAULT NOW() ───────────────────────────────
DO $$
DECLARE
    is_nullable TEXT;
    col_default TEXT;
BEGIN
    SELECT
        c.is_nullable,
        c.column_default
      INTO is_nullable, col_default
      FROM information_schema.columns c
     WHERE c.table_schema = 'public'
       AND c.table_name   = 'agents'
       AND c.column_name  = 'updated_at';

    IF is_nullable = 'YES' THEN
        ALTER TABLE public.agents
            ALTER COLUMN updated_at SET NOT NULL;
        RAISE NOTICE '[mig 166] agents.updated_at SET NOT NULL';
    END IF;

    IF col_default IS NULL THEN
        ALTER TABLE public.agents
            ALTER COLUMN updated_at SET DEFAULT NOW();
        RAISE NOTICE '[mig 166] agents.updated_at SET DEFAULT NOW()';
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 166] NOT NULL / DEFAULT step failed: %', SQLERRM;
END $$;

-- ── Post-migration validation ──────────────────────────────────────
DO $$
DECLARE
    col_type      TEXT;
    -- Named v_is_nullable, not is_nullable: information_schema.columns has
    -- a column of that name, and the SELECT below reads it unqualified, so
    -- the bare name resolved to both the variable and the column and the
    -- whole block failed with "column reference is ambiguous". The prefix
    -- keeps the variable out of the query's namespace.
    v_is_nullable TEXT;
    col_default   TEXT;
    n_null        INTEGER;
BEGIN
    SELECT data_type, is_nullable, column_default
      INTO col_type, v_is_nullable, col_default
      FROM information_schema.columns
     WHERE table_schema = 'public'
       AND table_name   = 'agents'
       AND column_name  = 'updated_at';

    SELECT COUNT(*) INTO n_null FROM public.agents WHERE updated_at IS NULL;

    RAISE NOTICE '[mig 166] post-migration — updated_at: type=%, nullable=%, default=%, remaining_nulls=%',
        COALESCE(col_type,      '(missing)'),
        COALESCE(v_is_nullable, '(missing)'),
        COALESCE(col_default,   '(none)'),
        n_null;

    IF n_null > 0 THEN
        RAISE WARNING '[mig 166] % rows still have NULL updated_at — investigate before relying on this column', n_null;
    END IF;
END $$;
