-- Migration 058: Creature presence states (active, sleeping, parked)
--
-- ## Why this file is guarded (edited 2026-08-18)
--
-- `run_migrations` replays every file on every boot. Migration 079 copies
-- `creatures.presence` into `creature_conditions.presence`, and migration 080
-- then drops the `creatures` columns. So on an already-migrated database this
-- file was re-adding two columns that 080 immediately dropped again — every
-- boot, for the life of the deployment.
--
-- **Postgres never reclaims a dropped column's slot.** A dropped column keeps
-- its `attnum` forever, counts against the hard 1600-column ceiling, and is only
-- released by rewriting the table. Together with 052 (`sosa_opt_in`) and 065
-- (`visibility`) this cycle burned five slots per boot, and by the time it was
-- found `creatures` stood at **1600 of 1600 attnums — 1,575 dropped, 25 live**,
-- unable to accept another column ever again. Roughly 315 boots of leak.
--
-- The leak was invisible because `run_migrations` logged failures and continued,
-- so the eventual "tables can have at most 1600 columns" error was one line in a
-- boot log. It surfaced the first time the migration ledger recorded a result
-- (migration 207).
--
-- ## The guard, and why it is shaped this way
--
-- These `creatures` columns are pure **staging**: their only purpose is to be
-- read once by 079's backfill and then discarded. So the honest condition for
-- creating them is *"the destination does not have this data yet."*
--
--   fresh database   `creature_conditions.presence` does not exist (078 has not
--                    run) -> stage the column, 079 copies, 080 drops. Unchanged.
--   migrated database  the destination exists -> skip. No slot consumed, and
--                    nothing to drop.
--
-- Editing an old migration is normally wrong, but there is no "already applied"
-- state here to protect: every file runs every boot, so the running behaviour IS
-- the file. A new migration cannot undo an `ALTER` that a previous file reissues
-- moments later. The ledger records `content_sha256`, so this edit is visible as
-- a hash change rather than a silent rewrite of history.

DO $$
BEGIN
    -- Stage only while 079's destination is still absent.
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'creature_conditions'
           AND column_name = 'presence'
    ) THEN
        ALTER TABLE public.creatures
            ADD COLUMN IF NOT EXISTS presence TEXT NOT NULL DEFAULT 'active';
        ALTER TABLE public.creatures
            ADD COLUMN IF NOT EXISTS presence_changed_at TIMESTAMPTZ DEFAULT NOW();

        -- Only meaningful while the staging column exists. Dropping a column
        -- drops its indexes, so on a migrated database this index was being
        -- created and destroyed every boot alongside the column.
        CREATE INDEX IF NOT EXISTS idx_creatures_presence
            ON public.creatures(presence);
    END IF;
END $$;

-- `parked_at_workspace` is not staged and not recreated.
--
-- Migration 080 drops it with the note "unused (never read or written)", and
-- that is still true: no reference exists anywhere in `src/`, `templates/` or
-- `crates/`. It had no destination in `creature_conditions`, so unlike the two
-- above it was never migration staging — it was a column that was added, never
-- used, dropped, and then re-added on every boot purely to be dropped again.
-- One slot per boot for nothing at all.
