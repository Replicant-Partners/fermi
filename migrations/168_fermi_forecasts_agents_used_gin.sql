-- ═══════════════════════════════════════════════════════════════════
-- Migration 168 — GIN index on fermi_forecasts.agents_used
--
-- v0.10.23 hotfix for the abw-cli timeout Ivan hit running
-- `abw admin agents legacy-slugs`. (v0.10.21/v0.10.22 were
-- parallel forecast-save fixes; unrelated.)
--
--     error: GET https://agent-bestiary.world/api/admin/agents/legacy-slugs
--       caused by: operation timed out
--
-- Root cause: the legacy-slug audit handler ran one
-- `SELECT COUNT(*) FROM fermi_forecasts WHERE agents_used @> …`
-- per legacy name (~43 sequential JSONB containment queries on a
-- column with no GIN index). Postgres seq-scanned the whole
-- `fermi_forecasts` table 43 times, blowing past the client's
-- 60-second timeout.
--
-- v0.10.23 also rewrites the handler to make one aggregate query
-- instead of N. Combined with this index, the audit endpoint
-- returns in milliseconds.
--
-- Other read sites on `agents_used @>` that get faster for free:
--   * handlers/eval_brier.rs — Brier lookup via agent_name
--   * handlers/agents.rs::get_agent_calibration_handler
--   * handlers/forecasts.rs — future JSONB containment queries
--
-- PgBouncer-safe: no BEGIN/COMMIT; single DO block with EXCEPTION
-- handler. Idempotent via `IF NOT EXISTS`.
--
-- No CONCURRENTLY: `CREATE INDEX CONCURRENTLY` can't run inside a
-- transaction wrapper, and our migration runner (`sqlx::raw_sql`)
-- wraps each file in an implicit tx. Regular CREATE INDEX takes
-- an ACCESS EXCLUSIVE lock on the table for the duration of the
-- build. For `fermi_forecasts` at current scale (small hundreds of
-- rows) this is sub-second; safe. If the table ever gets large
-- enough to matter, migrate this to an out-of-band rebuild.
-- ═══════════════════════════════════════════════════════════════════

-- ── Pre-migration diagnostics ──────────────────────────────────────
DO $$
DECLARE
    n_rows    INTEGER;
    idx_exists BOOLEAN;
BEGIN
    SELECT COUNT(*) INTO n_rows FROM public.fermi_forecasts;
    SELECT EXISTS (
        SELECT 1 FROM pg_indexes
         WHERE schemaname = 'public'
           AND tablename  = 'fermi_forecasts'
           AND indexname  = 'idx_forecasts_agents_used_gin'
    ) INTO idx_exists;
    RAISE NOTICE '[mig 168] pre-migration — fermi_forecasts rows: %, index exists: %',
        n_rows, idx_exists;
END $$;

-- ── Create the GIN index ───────────────────────────────────────────
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes
         WHERE schemaname = 'public'
           AND tablename  = 'fermi_forecasts'
           AND indexname  = 'idx_forecasts_agents_used_gin'
    ) THEN
        CREATE INDEX idx_forecasts_agents_used_gin
            ON public.fermi_forecasts
            USING gin (agents_used);
        RAISE NOTICE '[mig 168] created idx_forecasts_agents_used_gin (GIN on agents_used)';
    ELSE
        RAISE NOTICE '[mig 168] idx_forecasts_agents_used_gin already exists — skipping';
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 168] CREATE INDEX failed: %', SQLERRM;
END $$;

-- ── Post-migration validation ──────────────────────────────────────
DO $$
DECLARE
    idx_size TEXT;
BEGIN
    SELECT pg_size_pretty(pg_relation_size('idx_forecasts_agents_used_gin'))
      INTO idx_size;
    RAISE NOTICE '[mig 168] post-migration — idx_forecasts_agents_used_gin size: %',
        COALESCE(idx_size, '(missing)');
END $$;
