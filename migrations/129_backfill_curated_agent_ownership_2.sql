-- Migration 129: Backfill ownership for curated agents seeded after migration 122.
--
-- Same pattern as 122 (which fixed agents added after 111). Any curated agent
-- seeded after 122 ran (e.g. simops_dynamics_runner, future additions) lands
-- with user_id = NULL because seed_agents_to_database does not set owner_id.
--
-- This migration assigns the earliest admin user to any curated/system agent
-- that still has a null owner. Idempotent — won't touch agents that already
-- have an owner. Safe to re-run.
--
-- PgBouncer-safe: single UPDATE, no BEGIN/COMMIT.

UPDATE public.agents
   SET user_id = (
       SELECT user_id
         FROM public.users
        WHERE role = 'admin'
        ORDER BY created_at ASC
        LIMIT 1
   )
 WHERE tier IN ('curated', 'system')
   AND user_id IS NULL
   AND EXISTS (SELECT 1 FROM public.users WHERE role = 'admin');
