-- Migration 119: defensive backstop for teams composition-identity columns
--
-- Migration 113 added mission, coordination_strategist_id, and
-- strategist_assigned_at to teams via `ADD COLUMN IF NOT EXISTS`.
-- That should have been idempotent and PgBouncer-safe, but
-- production keeps reporting `column "mission" does not exist` —
-- i.e. the columns never landed on the live DB. Best guess: the
-- migration runner sent multiple ALTER statements in one
-- sqlx::raw_sql call and PgBouncer in transaction mode silently
-- dropped them, the same family of issues documented in
-- MEMORY.md → "PgBouncer Pitfalls" for multi-statement
-- constraint mutations.
--
-- This migration is a single DO block so PgBouncer treats it as
-- one statement it cannot split. Inside the block, an explicit
-- pg_attribute lookup decides whether each column needs adding —
-- no reliance on the bare `IF NOT EXISTS` machinery. Safe to run
-- repeatedly; cheap when columns already exist.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_attribute
        WHERE attrelid = 'public.teams'::regclass
          AND attname = 'mission'
          AND NOT attisdropped
    ) THEN
        ALTER TABLE public.teams ADD COLUMN mission TEXT;
        RAISE NOTICE '[migration 119] added teams.mission';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_attribute
        WHERE attrelid = 'public.teams'::regclass
          AND attname = 'coordination_strategist_id'
          AND NOT attisdropped
    ) THEN
        ALTER TABLE public.teams ADD COLUMN coordination_strategist_id UUID;
        RAISE NOTICE '[migration 119] added teams.coordination_strategist_id';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_attribute
        WHERE attrelid = 'public.teams'::regclass
          AND attname = 'strategist_assigned_at'
          AND NOT attisdropped
    ) THEN
        ALTER TABLE public.teams ADD COLUMN strategist_assigned_at TIMESTAMPTZ;
        RAISE NOTICE '[migration 119] added teams.strategist_assigned_at';
    END IF;
END $$;
