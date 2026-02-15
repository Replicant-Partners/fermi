-- Migration 082: Rabble operational radius
--
-- Adds radius_meters to swarm_events. Defines the bounded area of operation
-- for a rabble. The anchor creature is the center, radius defines the circle.
-- Default 100m — enough for a café or park. Can be set small (10m) for
-- dense clustering studies or large (1000m) for neighborhood-scale rabbles.
--
-- IMPORTANT: No BEGIN/COMMIT — PgBouncer transaction mode.

ALTER TABLE swarm_events ADD COLUMN IF NOT EXISTS radius_meters INTEGER NOT NULL DEFAULT 100;
