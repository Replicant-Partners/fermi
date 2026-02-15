-- Migration 079: Add presence to creature_conditions
--
-- The `presence` column (active/sleeping/parked/tracking) is a social condition
-- the owner controls, not a spatial state. It belongs in creature_conditions
-- alongside visibility and sosa_opt_in.
--
-- IMPORTANT: No BEGIN/COMMIT — PgBouncer transaction mode.

ALTER TABLE creature_conditions ADD COLUMN IF NOT EXISTS presence TEXT NOT NULL DEFAULT 'active';

-- Backfill from creatures.presence
UPDATE creature_conditions cc
SET presence = COALESCE(
    (SELECT c.presence FROM creatures c WHERE c.creature_id = cc.creature_id),
    'active'
);
