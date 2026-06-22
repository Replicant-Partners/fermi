-- ─────────────────────────────────────────────────────────────────────
-- 152 — object_shares.object_type adds 'portfolio'
-- ─────────────────────────────────────────────────────────────────────
--
-- Spec 24 §3.1.2: portfolios become shareable through the same
-- `object_shares` table that already backs forecasts, agents, indexes,
-- workspaces etc.
--
-- The current CHECK constraint on prod (verified 2026-06-19):
--   CHECK (object_type IN ('agent', 'capability', 'forecast', 'index',
--                          'repo', 'file', 'rabble'))
--
-- ('workspace' is NOT in that set despite migration 117 claiming to
--  add it — leave that bug for whoever needs it; we only add
--  'portfolio'.)
--
-- PostgreSQL CHECK constraints can't be ALTERed in place — only
-- dropped + recreated. The whole transaction is wrapped in a DO block
-- because PgBouncer transaction-mode silently drops the second
-- statement of a multi-statement migration (see callout in
-- migrations/119_teams_mission_defensive.sql:8-12). The DO block is
-- one statement from PgBouncer's perspective.

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'object_shares_object_type_check'
    ) THEN
        ALTER TABLE public.object_shares
            DROP CONSTRAINT object_shares_object_type_check;
    END IF;

    ALTER TABLE public.object_shares
        ADD CONSTRAINT object_shares_object_type_check
        CHECK (object_type IN (
            'agent',
            'capability',
            'forecast',
            'portfolio',
            'index',
            'repo',
            'file',
            'rabble'
        ));
END $$;

COMMENT ON CONSTRAINT object_shares_object_type_check
    ON public.object_shares IS
    'Spec 24 §3.1.2: extended to include ''portfolio''. ''workspace'' is intentionally absent on this prod schema; migration 117''s addition didn''t take effect and is left for a separate fix.';
