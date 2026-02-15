-- Migration 083: Cache genome profile on creature_conditions
--
-- The genome profiler is a one-time informational action. Once the LLM
-- generates a phylogenetic profile, it's stored here so subsequent reads
-- are free and instant. No BEGIN/COMMIT — PgBouncer transaction mode.

ALTER TABLE creature_conditions ADD COLUMN IF NOT EXISTS genome_profile JSONB;
