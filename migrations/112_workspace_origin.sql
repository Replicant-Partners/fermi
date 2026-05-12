-- Migration 112: teams.origin — workspace vertical attribution
--
-- Step 1 of the vertical-decoupling work per
-- docs/VERTICAL_HARNESS_SPLIT.md §6. Adds a `origin` column to teams
-- so the harness dashboard, vertical UIs, and admin tooling can
-- segment workspaces by which vertical created them.
--
-- Recognised values (closed-ish enum — new ones are deliberate code
-- changes):
--   - 'bestiary_workspace' (default) — created via the harness UX
--   - 'rabble_swarm'                  — auto-created by rabble flows
--   - 'fermi_forecast'                — future: fermi-console publish
--   - 'kask_*' / 'silat_*'            — future: external verticals
--
-- Backfill identifies existing rabble auto-gens by two signals:
--   1. team description exactly matches the rabble factory string
--      ('Auto-created workspace for rabble' — see rabble_workspace.rs
--      handler)
--   2. team id appears in swarm_events.workspace_id (any swarm-linked
--      workspace, regardless of how it was named)
--
-- Either signal flips origin to 'rabble_swarm'. Everything else stays
-- at the default 'bestiary_workspace'. Idempotent — re-running is a
-- no-op for already-tagged rows. PgBouncer-safe (no BEGIN/COMMIT).

ALTER TABLE public.teams
    ADD COLUMN IF NOT EXISTS origin TEXT NOT NULL DEFAULT 'bestiary_workspace';

UPDATE public.teams
   SET origin = 'rabble_swarm'
 WHERE origin = 'bestiary_workspace'
   AND (
       description = 'Auto-created workspace for rabble'
       OR id IN (
           SELECT workspace_id
             FROM public.swarm_events
            WHERE workspace_id IS NOT NULL
       )
   );

CREATE INDEX IF NOT EXISTS idx_teams_origin ON public.teams(origin);
