-- Migration 191: fermi_forecast_schedules.forecast_id → fermi_forecasts(id)
--
-- Every other table keyed on a forecast declares the reference and lets
-- Postgres enforce it: fermi_portfolio_forecasts, fermi_forecast_updates,
-- forecast_spacetime, forecast_commitments, forecast_splits and
-- driver_annotations all CASCADE, and fermi_market_observations SET NULLs
-- on purpose (the observation of a market outlives the forecast that
-- referenced it).
--
-- `fermi_forecast_schedules` (mig 106/109) declared `forecast_id TEXT NOT
-- NULL` and no reference at all. So `DELETE /api/forecasts/:id`, which is
-- a bare `DELETE FROM fermi_forecasts` relying entirely on cascade, leaves
-- that forecast's recurring agent schedules behind. The admin purge path
-- (handlers/admin.rs) already deletes them explicitly, which is the
-- clearest evidence that the missing constraint was known at one call site
-- and not the other — exactly the failure mode a foreign key exists to
-- prevent.
--
-- The orphans are inert today: schedules only fire when the console opens
-- the forecast they belong to (`load_schedules` → overdue → fire), and a
-- deleted forecast cannot be opened. There is no server-side runner. So
-- this is accumulating dead rows rather than runaway agent spend — but the
-- row still names an agent, a driver and a query, and it will be found by
-- anything that scans the table on its own terms.
--
-- Idempotent: safe to re-run on every boot, which is how this project
-- applies migrations.

DO $$
DECLARE
    v_orphans BIGINT;
BEGIN
    IF to_regclass('public.fermi_forecast_schedules') IS NULL THEN
        RAISE NOTICE '[mig 191] fermi_forecast_schedules absent — nothing to do';
        RETURN;
    END IF;

    -- Already applied?
    IF EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'public.fermi_forecast_schedules'::regclass
           AND conname  = 'fermi_forecast_schedules_forecast_id_fkey'
    ) THEN
        RETURN;
    END IF;

    -- Clear the backlog first. ADD CONSTRAINT validates existing rows and
    -- would abort the whole boot migration pass on the first orphan.
    DELETE FROM public.fermi_forecast_schedules s
     WHERE NOT EXISTS (
        SELECT 1 FROM public.fermi_forecasts f WHERE f.id = s.forecast_id
     );
    GET DIAGNOSTICS v_orphans = ROW_COUNT;

    IF v_orphans > 0 THEN
        RAISE NOTICE '[mig 191] removed % schedule(s) for deleted forecasts', v_orphans;
    END IF;

    ALTER TABLE public.fermi_forecast_schedules
        ADD CONSTRAINT fermi_forecast_schedules_forecast_id_fkey
        FOREIGN KEY (forecast_id)
        REFERENCES public.fermi_forecasts(id)
        ON DELETE CASCADE;

    RAISE NOTICE '[mig 191] fermi_forecast_schedules now cascades from fermi_forecasts';
END $$;
