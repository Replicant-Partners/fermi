-- Migration 079: Add presence to creature_conditions
--
-- The `presence` column (active/sleeping/parked/tracking) is a social condition
-- the owner controls, not a spatial state. It belongs in creature_conditions
-- alongside visibility and sosa_opt_in.
--
-- IMPORTANT: No BEGIN/COMMIT — PgBouncer transaction mode.

ALTER TABLE creature_conditions ADD COLUMN IF NOT EXISTS presence TEXT NOT NULL DEFAULT 'active';

-- Backfill from creatures.presence
--
-- Guarded 2026-08-18. Migration 080 drops `creatures.presence` once this has
-- run, after which this statement failed on every boot with `column c.presence
-- does not exist`. Worse than the noise: an unguarded `COALESCE(..., 'active')`
-- over a missing source would, if it had somehow parsed, have reset every
-- creature's presence to 'active' on each restart. Running it only while the
-- source exists is both the fix and the correct semantics — there is nothing to
-- copy once the column is gone. See migration 058 for why these columns were
-- being re-created every boot.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'creatures'
           AND column_name = 'presence'
    ) THEN
        UPDATE creature_conditions cc
        SET presence = COALESCE(
            (SELECT c.presence FROM creatures c WHERE c.creature_id = cc.creature_id),
            'active'
        );
    END IF;
END $$;
