-- Migration 080: Drop redundant creature columns
--
-- These columns are now served by creature_conditions (migration 078+079):
--   visibility     → creature_conditions.visibility
--   sosa_opt_in    → creature_conditions.sosa_opt_in
--   presence        → creature_conditions.presence
--   presence_changed_at → creature_conditions.updated_at
--   parked_at_workspace → unused (never read or written)
--
-- IMPORTANT: No BEGIN/COMMIT — PgBouncer transaction mode.

ALTER TABLE creatures DROP COLUMN IF EXISTS visibility;
ALTER TABLE creatures DROP COLUMN IF EXISTS sosa_opt_in;
ALTER TABLE creatures DROP COLUMN IF EXISTS presence;
ALTER TABLE creatures DROP COLUMN IF EXISTS presence_changed_at;
ALTER TABLE creatures DROP COLUMN IF EXISTS parked_at_workspace;
