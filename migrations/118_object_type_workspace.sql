-- Migration 117: Add 'workspace' to object_shares.object_type
--
-- Extends the polymorphic sharing primitive to allow workspace-level sharing.
-- Required for SimOps collaborative workspaces (Doc 1 §6.2).
--
-- The DB CHECK constraint must be dropped and recreated because PostgreSQL
-- does not support ALTER CHECK inline. Wrapped in a DO block for PgBouncer
-- compatibility (transaction-mode safe).

DO $$ BEGIN
    ALTER TABLE public.object_shares
        DROP CONSTRAINT IF EXISTS object_shares_object_type_check;

    ALTER TABLE public.object_shares
        ADD CONSTRAINT object_shares_object_type_check
            CHECK (object_type IN (
                'agent', 'capability', 'forecast', 'index', 'repo', 'file', 'workspace'
            ));
END $$;
