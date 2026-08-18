-- Migration 065: Creature visibility
-- Controls who can see a creature on the Flights map
-- Values: 'public' (anyone), 'contacts' (owner's contacts only), 'private' (hidden)
--
-- ## Why the `creatures` column is guarded (edited 2026-08-18)
--
-- `run_migrations` replays every file on every boot. Migration 078 copies
-- `creatures.visibility` into `creature_conditions.visibility`, and 080 then
-- drops the `creatures` column — so on an already-migrated database this file
-- re-added a column that was dropped again moments later, every boot.
--
-- Postgres never reclaims a dropped column's slot: the `attnum` is held forever
-- and counts against the hard 1600-column ceiling. With 052 (`sosa_opt_in`) and
-- 058 (`presence`, `presence_changed_at`, `parked_at_workspace`) this consumed
-- five slots per boot until `creatures` reached **1600 of 1600 — 1,575 dropped,
-- 25 live** and could no longer accept any column at all.
--
-- The condition below is the honest one for a staging column: create it only
-- while the destination is still missing. See migration 058 for the full
-- reasoning, including why editing a replayed migration is the only fix
-- available.
--
-- `creature_flights.visibility` is NOT staging. Nothing drops it, it is read by
-- the visible-flights query, and it stays unguarded.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'creature_conditions'
           AND column_name = 'visibility'
    ) THEN
        ALTER TABLE public.creatures
            ADD COLUMN IF NOT EXISTS visibility TEXT NOT NULL DEFAULT 'public';
    END IF;
END $$;

-- Flights inherit visibility from creature at creation time. Real column on a
-- table nothing drops from.
ALTER TABLE creature_flights
  ADD COLUMN IF NOT EXISTS visibility TEXT NOT NULL DEFAULT 'public';

-- Index for efficient visible-flights queries
CREATE INDEX IF NOT EXISTS idx_creature_flights_visibility
  ON creature_flights (visibility)
  WHERE ended_at IS NULL;
