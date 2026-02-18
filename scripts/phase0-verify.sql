-- Phase 0 Verification Script — Rabble UX Unblockers
--
-- Run against production database:
--   psql $DATABASE_URL -f scripts/phase0-verify.sql
--
-- Checks:
--   0.1  Migration 090 (social layer) — tables, columns, functions
--   0.2  PostGIS availability
--   0.3  Notification columns (type/message not notification_type/body)
--   0.4  Migration 091 (swarm_participants) and 092 (fix_social_layer)
--   0.5  Dashboard spatial functions (migration 089)
--
-- Output: PASS / FAIL for each check with actionable remediation.

\echo ''
\echo '═══════════════════════════════════════════════════════════════'
\echo '  Phase 0 — Production Readiness Verification'
\echo '═══════════════════════════════════════════════════════════════'
\echo ''

-- ─── 0.1a: PostGIS Extension ──────────────────────────────────────

\echo '── 0.2 PostGIS ──────────────────────────────────────────────'

SELECT CASE
    WHEN EXISTS (
        SELECT 1 FROM pg_extension WHERE extname = 'postgis'
    )
    THEN '✅ PASS: PostGIS extension is installed (version: ' || (SELECT PostGIS_Version()) || ')'
    ELSE '❌ FAIL: PostGIS NOT installed. Run: CREATE EXTENSION IF NOT EXISTS postgis;'
END AS postgis_check;

-- ─── 0.1b: pgvector Extension ─────────────────────────────────────

SELECT CASE
    WHEN EXISTS (
        SELECT 1 FROM pg_extension WHERE extname = 'vector'
    )
    THEN '✅ PASS: pgvector extension is installed'
    ELSE '⚠️  WARN: pgvector not installed (needed for embedding search, not a blocker for social)'
END AS pgvector_check;

-- ─── 0.1c: uuid-ossp Extension ───────────────────────────────────

SELECT CASE
    WHEN EXISTS (
        SELECT 1 FROM pg_extension WHERE extname = 'uuid-ossp'
    ) OR EXISTS (
        SELECT 1 FROM pg_proc WHERE proname = 'gen_random_uuid'
    )
    THEN '✅ PASS: UUID generation available'
    ELSE '❌ FAIL: No UUID generation. Run: CREATE EXTENSION IF NOT EXISTS "uuid-ossp";'
END AS uuid_check;

\echo ''
\echo '── 0.1 Migration 090: Social Layer Tables ───────────────────'

-- ─── creature_friendships table ───────────────────────────────────

SELECT CASE
    WHEN EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = 'creature_friendships'
    )
    THEN '✅ PASS: creature_friendships table exists'
    ELSE '❌ FAIL: creature_friendships table MISSING. Migration 090 not applied.'
END AS creature_friendships_check;

-- ─── creature_invites table ───────────────────────────────────────

SELECT CASE
    WHEN EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = 'creature_invites'
    )
    THEN '✅ PASS: creature_invites table exists'
    ELSE '❌ FAIL: creature_invites table MISSING. Migration 090 not applied.'
END AS creature_invites_check;

-- ─── activity_events table ────────────────────────────────────────

SELECT CASE
    WHEN EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = 'activity_events'
    )
    THEN '✅ PASS: activity_events table exists'
    ELSE '❌ FAIL: activity_events table MISSING. Migration 090 not applied.'
END AS activity_events_check;

-- ─── rabble_co_presence table ─────────────────────────────────────

SELECT CASE
    WHEN EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = 'rabble_co_presence'
    )
    THEN '✅ PASS: rabble_co_presence table exists'
    ELSE '❌ FAIL: rabble_co_presence table MISSING. Migration 090 not applied.'
END AS rabble_co_presence_check;

-- ─── swarm_participants table (migration 091) ─────────────────────

SELECT CASE
    WHEN EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = 'swarm_participants'
    )
    THEN '✅ PASS: swarm_participants table exists (migration 091)'
    ELSE '❌ FAIL: swarm_participants table MISSING. Migration 091 not applied.'
END AS swarm_participants_check;

\echo ''
\echo '── 0.1 Migration 090: Social Layer Columns ──────────────────'

-- ─── users.social_visibility column ───────────────────────────────

SELECT CASE
    WHEN EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'users' AND column_name = 'social_visibility'
    )
    THEN '✅ PASS: users.social_visibility column exists'
    ELSE '❌ FAIL: users.social_visibility column MISSING. Run: ALTER TABLE users ADD COLUMN IF NOT EXISTS social_visibility TEXT NOT NULL DEFAULT ''public'';'
END AS social_visibility_check;

\echo ''
\echo '── 0.1 Migration 090: SQL Functions ─────────────────────────'

-- ─── Check all 5 required functions ───────────────────────────────

SELECT
    routine_name,
    '✅ PASS' AS status
FROM information_schema.routines
WHERE routine_schema = 'public'
  AND routine_name IN (
    'get_pending_friendship_requests',
    'get_creature_friends',
    'get_pending_creature_invites',
    'get_activity_feed',
    'get_creatures_met_in_rabble',
    'canonical_creature_pair'
  )
ORDER BY routine_name;

-- Show which functions are missing
SELECT
    fn_name AS missing_function,
    '❌ FAIL: Function not found. Re-run migration 090.' AS status
FROM (
    VALUES
        ('canonical_creature_pair'),
        ('get_creature_friends'),
        ('get_creatures_met_in_rabble'),
        ('get_pending_creature_invites'),
        ('get_pending_friendship_requests')
) AS required(fn_name)
WHERE fn_name NOT IN (
    SELECT routine_name FROM information_schema.routines
    WHERE routine_schema = 'public'
);

\echo ''
\echo '── 0.3 Notification Columns ─────────────────────────────────'

-- ─── Verify correct columns exist ─────────────────────────────────

SELECT CASE
    WHEN EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'notifications' AND column_name = 'type'
    )
    THEN '✅ PASS: notifications.type column exists (correct)'
    ELSE '❌ FAIL: notifications.type column MISSING'
END AS notif_type_check;

SELECT CASE
    WHEN EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'notifications' AND column_name = 'message'
    )
    THEN '✅ PASS: notifications.message column exists (correct)'
    ELSE '❌ FAIL: notifications.message column MISSING'
END AS notif_message_check;

-- ─── Verify stale columns are gone ────────────────────────────────

SELECT CASE
    WHEN EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'notifications' AND column_name = 'notification_type'
    )
    THEN '❌ FAIL: notifications.notification_type column still exists (stale). Migration 092 should have removed it.'
    ELSE '✅ PASS: notifications.notification_type column does not exist (correct)'
END AS notif_stale_type_check;

SELECT CASE
    WHEN EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'notifications'
        AND column_name = 'body'
        AND table_name NOT IN (
            SELECT table_name FROM information_schema.columns
            WHERE column_name = 'body' AND table_name = 'activity_events'
        )
    )
    THEN '⚠️  WARN: notifications.body column exists — might be stale. Check if migration 092 ran.'
    ELSE '✅ PASS: notifications table has no stale body column'
END AS notif_stale_body_check;

\echo ''
\echo '── 0.5 Dashboard Spatial Functions (Migration 089) ──────────'

-- ─── get_my_rabbles_with_status ───────────────────────────────────

SELECT CASE
    WHEN EXISTS (
        SELECT 1 FROM information_schema.routines
        WHERE routine_schema = 'public'
          AND routine_name = 'get_my_rabbles_with_status'
    )
    THEN '✅ PASS: get_my_rabbles_with_status function exists (migration 089)'
    ELSE '❌ FAIL: get_my_rabbles_with_status MISSING. Migration 089 not applied.'
END AS dashboard_fn_check;

\echo ''
\echo '── Row Counts (sanity check) ────────────────────────────────'

SELECT 'notifications' AS table_name, COUNT(*) AS row_count FROM notifications
UNION ALL
SELECT 'creature_friendships', COUNT(*) FROM creature_friendships
UNION ALL
SELECT 'creature_invites', COUNT(*) FROM creature_invites
UNION ALL
SELECT 'activity_events', COUNT(*) FROM activity_events
UNION ALL
SELECT 'rabble_co_presence', COUNT(*) FROM rabble_co_presence
UNION ALL
SELECT 'swarm_participants', COUNT(*) FROM swarm_participants
UNION ALL
SELECT 'creatures', COUNT(*) FROM creatures
UNION ALL
SELECT 'swarm_events', COUNT(*) FROM swarm_events
UNION ALL
SELECT 'users', COUNT(*) FROM users
ORDER BY table_name;

\echo ''
\echo '── Recent Notifications (last 5, verify data shape) ─────────'

SELECT id, user_id, type, title,
       LEFT(COALESCE(message, ''), 60) AS message_preview,
       read, created_at
FROM notifications
ORDER BY created_at DESC
LIMIT 5;

\echo ''
\echo '═══════════════════════════════════════════════════════════════'
\echo '  Phase 0 Verification Complete'
\echo ''
\echo '  If any checks show ❌ FAIL:'
\echo '    1. Migrations run on server startup (api_server.rs)'
\echo '    2. Restart the server to re-run migrations'
\echo '    3. Or run manually: psql $DATABASE_URL -f migrations/090_social_layer.sql'
\echo '    4. Then re-run: psql $DATABASE_URL -f scripts/phase0-verify.sql'
\echo ''
\echo '  If PostGIS is missing:'
\echo '    - Neon: Enable via dashboard → Extensions → PostGIS'
\echo '    - Self-hosted: CREATE EXTENSION IF NOT EXISTS postgis;'
\echo ''
\echo '  Next: Once all checks pass, proceed to Phase 1'
\echo '═══════════════════════════════════════════════════════════════'
\echo ''
