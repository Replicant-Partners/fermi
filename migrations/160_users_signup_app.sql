-- Migration 160: Track which app a user first signed up through.
--
-- Motivation: ABW is the shared backend that hosts multiple apps
-- (Fermi Console, Rabble, Silat, etc.). For many users their entire
-- mental model is one app — they never think of ABW. We need the
-- backend to remember which app a user came in through so:
--
--   * Admins can see "who's using Fermi Console" as a distinct cohort
--     without conflating it with direct ABW signups or Rabble users.
--   * Onboarding grants, welcome copy, and future per-app economics
--     can be filtered by signup source.
--   * We can measure app-level activation, retention, and credit burn
--     independently.
--
-- Schema: a single nullable TEXT column stamped ONCE, on user creation.
-- We deliberately do NOT overwrite it on subsequent logins — a user's
-- signup app is a historical fact, not a current-session attribute.
-- Multi-app usage is a separate future problem (see docs/specs for the
-- eventual `user_apps` join table); this column only answers "where
-- did they enter the ecosystem from?".
--
-- The value is a free-form slug matching apps.slug (e.g.
-- 'fermi_console', 'rabble', 'silat'). No FK: apps rows may not exist
-- yet for every entry point, and archiving an app row shouldn't
-- retroactively invalidate historical signups.
--
-- Backfill: NULL means "signed up before this migration" or "signed up
-- through the ABW web landing directly". Left as NULL, not backfilled
-- to a sentinel, so downstream code can distinguish "no data" from
-- "known direct signup".
--
-- PgBouncer-safe. Idempotent.

ALTER TABLE public.users
    ADD COLUMN IF NOT EXISTS signup_app_slug TEXT;

-- Partial index: only the rows we actually want to filter on. Keeps
-- the index small since most historical rows will be NULL.
CREATE INDEX IF NOT EXISTS idx_users_signup_app_slug
    ON public.users(signup_app_slug)
    WHERE signup_app_slug IS NOT NULL;

COMMENT ON COLUMN public.users.signup_app_slug IS
    'App slug (matching apps.slug) that this user first signed up through. '
    'Stamped once at user creation, never overwritten. NULL = direct ABW '
    'signup or pre-migration user.';
