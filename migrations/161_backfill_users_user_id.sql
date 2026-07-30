-- ═══════════════════════════════════════════════════════════════════
-- Migration 161 — backfill users.user_id for legacy / half-provisioned rows
--
-- Root cause: sync_user_from_app's UPDATE branch (fermi-auth/src/oidc.rs)
-- historically did NOT touch users.user_id when it hit an existing row
-- matched by email or google_id. If that row had `user_id = NULL` or
-- `user_id = ''` (from an older INSERT path, a partial provisioning, or
-- a legacy row that migration 004b's backfill missed), the row stayed
-- broken forever. The tail of sync_user_from_app then minted a JWT with
--   sub = record.user_id.unwrap_or_else(|| record.id.to_string())
-- so `Some("")` skipped the fallback and the session carried an empty
-- user_id, while `None` produced `id::text` — a value NOT stored in the
-- user_id column, so `fermi_forecasts.owner_id → users(user_id)` FK
-- fires on every save. `ensure_user_row`'s email-heal (v0.9.1) could
-- fix it on the next write, but only if the row's `auth_provider`
-- looked legacy-shaped, which non-legacy Google/GitHub rows didn't.
--
-- Symptom: every account except the one lucky INSERT-path row (ivan@)
-- gets "Backend save failed: this server is running an older version…"
-- on save + 403 on invite accept.
--
-- Fix: one-shot backfill `user_id = id::text` wherever it's NULL or ''.
-- Combined with the v0.10.3 UPDATE clause in sync_user_from_app that
-- keeps it filled going forward, sessions and FK targets stay aligned.
--
-- Idempotent + safe: only touches rows that are already broken. The
-- users_user_id_unique constraint (mig 093) already guarantees no two
-- rows can collide on the backfilled value because `id` is the PK.
--
-- PgBouncer-safe: every write is wrapped in its own DO $$ … END $$;
-- block so multi-statement splitting in transaction mode doesn't drop
-- any of them. No BEGIN/COMMIT.
-- ═══════════════════════════════════════════════════════════════════

-- ── Pre-count how many rows we're about to heal so operators can grep it.
DO $$
DECLARE
    n_null INTEGER;
    n_empty INTEGER;
BEGIN
    SELECT COUNT(*) INTO n_null FROM users WHERE user_id IS NULL;
    SELECT COUNT(*) INTO n_empty FROM users WHERE user_id = '';
    RAISE NOTICE '[mig 161] backfilling users.user_id: % NULL rows, % empty rows',
        n_null, n_empty;
END $$;

-- ── Backfill NULL/empty user_id with the row's own PK.
--
-- id::text is chosen because:
--   * It's unique by construction (PK), so no collision with any
--     other existing user_id value (mig 093 UNIQUE).
--   * It matches what sync_user_from_app's `unwrap_or_else` fallback
--     would have produced, so any JWT already minted against this
--     resolved value keeps working post-heal.
--   * It matches what mig 004b intended for legacy rows.
DO $$
DECLARE
    n_healed INTEGER;
BEGIN
    UPDATE users
    SET user_id = id::text
    WHERE user_id IS NULL OR user_id = '';
    GET DIAGNOSTICS n_healed = ROW_COUNT;
    RAISE NOTICE '[mig 161] healed % users rows (NULL/empty user_id -> id::text)', n_healed;
END $$;

-- ── Normalize UUID-shaped user_ids to lowercase.
--
-- The `owner_id::uuid` cast in fermi_forecasts / fermi_portfolios
-- INSERTs round-trips text through UUID, which lowercases hyphenated
-- UUIDs. If users.user_id was stored uppercase (rare, but seen), the
-- FK compare stored-vs-cast returns NOT EQUAL and the FK trips
-- despite the row existing. Normalize once so the compare always
-- matches going forward.
DO $$
DECLARE
    n_lc INTEGER;
BEGIN
    UPDATE users
    SET user_id = LOWER(user_id)
    WHERE user_id ~ '^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$'
      AND user_id <> LOWER(user_id);
    GET DIAGNOSTICS n_lc = ROW_COUNT;
    RAISE NOTICE '[mig 161] lowercased % UUID-shaped user_ids for FK round-trip parity', n_lc;
END $$;

-- ── Sanity check: no NULLs / empties remain. If this fires post-heal
--    something is very wrong (concurrent INSERT of a NULL user_id?),
--    and we want the deploy log to make it loud.
DO $$
DECLARE
    n_bad INTEGER;
BEGIN
    SELECT COUNT(*) INTO n_bad FROM users WHERE user_id IS NULL OR user_id = '';
    IF n_bad > 0 THEN
        RAISE WARNING '[mig 161] % users rows still have NULL/empty user_id post-backfill', n_bad;
    ELSE
        RAISE NOTICE '[mig 161] backfill complete; all users have a non-empty user_id';
    END IF;
END $$;
