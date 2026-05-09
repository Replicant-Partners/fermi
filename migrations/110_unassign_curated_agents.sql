-- Migration 110: Repair curated agent ownership
--
-- Migration 006 contained a backfill that ran on every startup and
-- silently reassigned every NULL user_id to "the first user returned
-- by SELECT user_id FROM users LIMIT 1" (no ORDER BY — deterministic
-- but arbitrary, in practice the oldest user). The seeder inserts
-- curated agents with user_id = NULL, so every new curated agent
-- added to the repo got wrongly assigned to that user on the next
-- deploy. Migration 006 has now been patched to drop the UPDATE.
--
-- This migration repairs the resulting damage: every agent whose
-- tier marks it as system-owned (curated / system) gets its
-- user_id reset to NULL so it shows up correctly as catalogue/system
-- and not under any individual user's profile.
--
-- Idempotent — safe to re-run. PgBouncer-safe (no BEGIN/COMMIT).
-- Only affects rows that match the tier filter; user-created
-- (community / personal) agents are untouched.

UPDATE public.agents
   SET user_id = NULL
 WHERE tier IN ('curated', 'system')
   AND user_id IS NOT NULL;
