-- ─────────────────────────────────────────────────────────────────────
-- 157 — object_shares.object_type: restore the COMPLETE set
-- ─────────────────────────────────────────────────────────────────────
--
-- Regression fix. Migrations run on every startup in *list order* (no
-- tracking table), so the last migration to touch this CHECK wins:
--
--   060 → adds 'rabble'
--   118 → adds 'workspace', drops 'rabble'
--   152 → adds 'portfolio' + 'rabble', DROPS 'workspace'  ← last to run
--
-- Because 152 runs after 118 and intentionally omitted 'workspace'
-- ("leave that bug for whoever needs it" — 152's own comment), every
-- boot ends with a constraint that rejects object_type='workspace'.
-- That breaks kask.bio's SimOps collaborative workspaces: sharing a
-- workspace (POST /api/shares, object_type='workspace') INSERTs into
-- object_shares, hits the CHECK, and 500s.
--
-- This migration recreates the constraint with the FULL union of every
-- object_type the code emits, and is registered LAST in the runner so
-- no earlier migration can clobber it again. Add new types here, not in
-- a fresh drop/recreate migration, to keep this the single source of
-- truth.
--
-- DO block = one statement to PgBouncer (transaction-mode safe); a bare
-- multi-statement DROP+ADD would silently lose the ADD.

DO $$
BEGIN
    ALTER TABLE public.object_shares
        DROP CONSTRAINT IF EXISTS object_shares_object_type_check;

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
            'rabble',
            'workspace'
        ));
END $$;

COMMENT ON CONSTRAINT object_shares_object_type_check
    ON public.object_shares IS
    'Complete object_type set. Registered last in the migration runner so it is the final word on every startup — add new types HERE, not in a new drop/recreate migration. Restores ''workspace'' dropped by 152 (broke kask.bio workspace sharing).';
