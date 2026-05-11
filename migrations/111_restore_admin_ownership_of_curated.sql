-- Migration 111: Restore sys admin ownership of curated/system agents.
--
-- Migration 110 (yesterday) nulled out user_id on every agent with
-- tier IN ('curated', 'system'), undoing the buggy migration-006
-- backfill that was reassigning them to whichever user the database
-- returned first. That was correct for community/forked agents (which
-- belong to users) but over-corrected for curated/system agents — the
-- intended model is for the sys admin (role='admin') to own them so
-- they have the Eval / Intelligence / Manage tabs to maintain them.
--
-- This migration restores that: every curated/system agent that
-- currently has user_id = NULL gets assigned to the earliest-created
-- admin user. Idempotent — won't touch agents that already have an
-- owner.
--
-- If no admin exists, this is a no-op (no rows updated). Setting up
-- the admin role on the right user is a manual step (UPDATE users
-- SET role='admin' WHERE ...) — the migration can't guess.
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
