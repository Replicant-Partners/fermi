-- ═══════════════════════════════════════════════════════════════════
-- Migration 167 — cast MIN/MAX(brier_score) to DOUBLE PRECISION in
--                 the fermi_leaderboard materialized view
--
-- v0.10.19 hotfix. Same family as the resolve_forecast() REAL/f64
-- mismatch that surfaced in Mo's Resolve Forecast dialog:
--
--     Server error: error occurred while decoding column 0:
--     mismatched types; Rust type f64 (as SQL type FLOAT8) is not
--     compatible with SQL type FLOAT4
--
-- fermi_leaderboard was created in mig-094 with:
--
--     MIN(f.brier_score) AS best_brier_score,   -- REAL (FLOAT4)
--     MAX(f.brier_score) AS worst_brier_score,  -- REAL (FLOAT4)
--     AVG(f.brier_score) AS avg_brier_score,    -- DOUBLE PRECISION (OK)
--     STDDEV(f.brier_score) AS brier_stddev,    -- DOUBLE PRECISION (OK)
--
-- MIN/MAX preserve the input type (REAL), AVG/STDDEV widen to DOUBLE
-- PRECISION. The leaderboard_handler in src/handlers/forecasts.rs
-- reads both MIN and MAX as `Option<f64>` — which 400s the moment
-- the view has any data. The v0.10.19 substrate rule closing this
-- family: every numeric aggregate published to Rust is DOUBLE
-- PRECISION, either naturally (AVG, STDDEV) or by explicit cast
-- (MIN, MAX).
--
-- CREATE OR REPLACE doesn't exist for materialized views — column
-- types are fixed at creation. This migration DROPs and recreates.
-- The view is derived from `fermi_forecasts` so no data is lost;
-- WITH DATA rebuilds it in-place. Indexes recreated too.
--
-- PgBouncer-safe: no BEGIN/COMMIT; each DDL in its own DO block.
-- Idempotent via IF EXISTS / IF NOT EXISTS.
-- ═══════════════════════════════════════════════════════════════════

-- ── Pre-migration diagnostics ──────────────────────────────────────
DO $$
DECLARE
    n_rows       INTEGER;
    view_exists  BOOLEAN;
    min_col_type TEXT;
BEGIN
    SELECT EXISTS (
        SELECT 1 FROM pg_matviews
         WHERE schemaname = 'public' AND matviewname = 'fermi_leaderboard'
    ) INTO view_exists;

    IF view_exists THEN
        EXECUTE 'SELECT COUNT(*) FROM public.fermi_leaderboard' INTO n_rows;
        SELECT data_type INTO min_col_type
          FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name   = 'fermi_leaderboard'
           AND column_name  = 'best_brier_score';
        RAISE NOTICE '[mig 167] pre-migration — view exists, % rows, best_brier_score type: %',
            n_rows, COALESCE(min_col_type, '(unknown)');
    ELSE
        RAISE NOTICE '[mig 167] pre-migration — fermi_leaderboard does not exist (will CREATE fresh)';
    END IF;
END $$;

-- ── Step 1: drop the drifted materialized view (if present) ────────
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_matviews
         WHERE schemaname = 'public' AND matviewname = 'fermi_leaderboard'
    ) THEN
        DROP MATERIALIZED VIEW public.fermi_leaderboard CASCADE;
        RAISE NOTICE '[mig 167] dropped fermi_leaderboard (CASCADE — indexes go with it)';
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 167] DROP MATERIALIZED VIEW skipped: %', SQLERRM;
END $$;

-- ── Step 2: recreate with ::float8 casts on MIN/MAX ────────────────
CREATE MATERIALIZED VIEW IF NOT EXISTS public.fermi_leaderboard AS
SELECT
    f.owner_id,
    u.display_name,
    COUNT(*) AS total_resolved,
    -- AVG/STDDEV widen to DOUBLE PRECISION naturally — no cast needed.
    -- MIN/MAX preserve the input type (REAL), so cast explicitly.
    AVG(f.brier_score)         AS avg_brier_score,
    MIN(f.brier_score)::float8 AS best_brier_score,
    MAX(f.brier_score)::float8 AS worst_brier_score,
    STDDEV(f.brier_score)      AS brier_stddev,
    -- Calibration buckets: count forecasts in each probability decile
    COUNT(*) FILTER (WHERE f.predicted_probability < 0.1) AS bucket_0_10,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.1 AND f.predicted_probability < 0.2) AS bucket_10_20,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.2 AND f.predicted_probability < 0.3) AS bucket_20_30,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.3 AND f.predicted_probability < 0.4) AS bucket_30_40,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.4 AND f.predicted_probability < 0.5) AS bucket_40_50,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.5 AND f.predicted_probability < 0.6) AS bucket_50_60,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.6 AND f.predicted_probability < 0.7) AS bucket_60_70,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.7 AND f.predicted_probability < 0.8) AS bucket_70_80,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.8 AND f.predicted_probability < 0.9) AS bucket_80_90,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.9) AS bucket_90_100,
    -- Accuracy by bucket (fraction of "yes" outcomes in each decile)
    AVG(CASE WHEN f.predicted_probability < 0.2 THEN f.actual_outcome::int END) AS accuracy_0_20,
    AVG(CASE WHEN f.predicted_probability >= 0.2 AND f.predicted_probability < 0.4 THEN f.actual_outcome::int END) AS accuracy_20_40,
    AVG(CASE WHEN f.predicted_probability >= 0.4 AND f.predicted_probability < 0.6 THEN f.actual_outcome::int END) AS accuracy_40_60,
    AVG(CASE WHEN f.predicted_probability >= 0.6 AND f.predicted_probability < 0.8 THEN f.actual_outcome::int END) AS accuracy_60_80,
    AVG(CASE WHEN f.predicted_probability >= 0.8 THEN f.actual_outcome::int END) AS accuracy_80_100,
    -- Streaks and activity
    MAX(f.resolved_at) AS last_resolved_at,
    -- Domain breakdown
    array_agg(DISTINCT f.domain) FILTER (WHERE f.domain IS NOT NULL) AS domains
FROM public.fermi_forecasts f
JOIN public.users u ON u.user_id = f.owner_id
WHERE f.status = 'resolved'
  AND f.brier_score IS NOT NULL
GROUP BY f.owner_id, u.display_name
HAVING COUNT(*) >= 5
WITH DATA;

-- ── Step 3: recreate indexes (CASCADE dropped them) ────────────────
CREATE UNIQUE INDEX IF NOT EXISTS idx_leaderboard_owner
    ON public.fermi_leaderboard(owner_id);
CREATE INDEX IF NOT EXISTS idx_leaderboard_brier
    ON public.fermi_leaderboard(avg_brier_score ASC);

-- ── Post-migration validation ──────────────────────────────────────
DO $$
DECLARE
    n_rows        INTEGER;
    min_col_type  TEXT;
    max_col_type  TEXT;
    avg_col_type  TEXT;
BEGIN
    SELECT COUNT(*) INTO n_rows FROM public.fermi_leaderboard;

    SELECT data_type INTO min_col_type
      FROM information_schema.columns
     WHERE table_schema = 'public'
       AND table_name   = 'fermi_leaderboard'
       AND column_name  = 'best_brier_score';
    SELECT data_type INTO max_col_type
      FROM information_schema.columns
     WHERE table_schema = 'public'
       AND table_name   = 'fermi_leaderboard'
       AND column_name  = 'worst_brier_score';
    SELECT data_type INTO avg_col_type
      FROM information_schema.columns
     WHERE table_schema = 'public'
       AND table_name   = 'fermi_leaderboard'
       AND column_name  = 'avg_brier_score';

    RAISE NOTICE '[mig 167] post-migration — % rows, types: best=%, worst=%, avg=%',
        n_rows,
        COALESCE(min_col_type, '(missing)'),
        COALESCE(max_col_type, '(missing)'),
        COALESCE(avg_col_type, '(missing)');

    IF min_col_type <> 'double precision' OR max_col_type <> 'double precision' THEN
        RAISE WARNING '[mig 167] best/worst still not DOUBLE PRECISION — leaderboard reads will 400';
    END IF;
END $$;
