-- Migration 096: Performance indexes for hot-path queries
--
-- Addresses the following hot paths identified in performance audit:
--
--   1. get_creature_handler — 3 correlated subqueries on creature_friendships
--      with OR conditions that defeat single-column indexes.
--   2. list_creatures_handler — 5 correlated subqueries per creature for
--      cognition_level (creature_versions, creature_flights).
--   3. my_rabbles_handler — last_activity_at subquery on activity_events,
--      hosted rabbles lookup by creator_id + status.
--   4. join_swarm / host_rabble — active flight lookup pattern
--      (creature_id + ended_at IS NULL) used everywhere.
--   5. SSE backfill — activity_events by creature_id + created_at.
--   6. Anchor creature guard — swarm_events by anchor_creature_id + status.
--
-- All indexes are created CONCURRENTLY to avoid locking tables in production.
-- This migration must be run outside a transaction (Rails: disable_ddl_transaction!).

-- ═══════════════════════════════════════════════════════════════════════════
-- 1. creature_friendships — friendship counts in get_creature_handler
--
-- The query uses: WHERE (creature_a = $1 OR creature_b = $1) AND status = ...
-- OR conditions can't use a single composite index. We create one index per
-- side so Postgres can bitmap-OR them.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_friendships_a_status
    ON creature_friendships (creature_a, status);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_friendships_b_status
    ON creature_friendships (creature_b, status);

-- Pending requests: also filtered by initiated_by != creature_id
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_friendships_a_pending
    ON creature_friendships (creature_a, status, initiated_by)
    WHERE status = 'pending';

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_friendships_b_pending
    ON creature_friendships (creature_b, status, initiated_by)
    WHERE status = 'pending';

-- ═══════════════════════════════════════════════════════════════════════════
-- 2. creature_versions — cognition_level COUNT in list_creatures_handler
--
-- Two subqueries:
--   COUNT(*) FROM creature_versions WHERE creature_id = $1
--   COUNT(*) FROM creature_versions WHERE creature_id = $1 AND transition_type = 'dream'
-- ═══════════════════════════════════════════════════════════════════════════

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_creature_versions_creature
    ON creature_versions (creature_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_creature_versions_creature_dream
    ON creature_versions (creature_id)
    WHERE transition_type = 'dream';

-- ═══════════════════════════════════════════════════════════════════════════
-- 3. creature_flights — active flight lookups (used everywhere)
--
-- Pattern: WHERE creature_id = $1 AND ended_at IS NULL
-- Used in: join_swarm, host_rabble, tether, untether, perch, get_creature,
--          list_creatures (last_location_name subquery).
--
-- Also: COUNT(DISTINCT swarm_id) for cognition_level.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_creature_flights_active
    ON creature_flights (creature_id, started_at DESC)
    WHERE ended_at IS NULL;

-- Cognition level: COUNT(DISTINCT swarm_id) WHERE swarm_id IS NOT NULL
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_creature_flights_creature_swarm
    ON creature_flights (creature_id, swarm_id)
    WHERE swarm_id IS NOT NULL;

-- Last location name subquery: ORDER BY started_at DESC LIMIT 1
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_creature_flights_creature_started
    ON creature_flights (creature_id, started_at DESC);

-- ═══════════════════════════════════════════════════════════════════════════
-- 4. activity_events — last_activity_at in my_rabbles + SSE backfill
--
-- my_rabbles: MAX(created_at) WHERE rabble_id = $1
-- SSE backfill: WHERE creature_id = $1 AND created_at > $2 ORDER BY created_at
-- ═══════════════════════════════════════════════════════════════════════════

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_activity_events_rabble_time
    ON activity_events (rabble_id, created_at DESC)
    WHERE rabble_id IS NOT NULL;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_activity_events_creature_time
    ON activity_events (creature_id, created_at ASC)
    WHERE creature_id IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════════════
-- 5. swarm_events — my_rabbles (creator + status), anchor guard
--
-- my_rabbles: WHERE creator_id = $1 AND status IN ('scheduled','active','completed')
-- anchor guard: WHERE anchor_creature_id = $1 AND status IN ('active','scheduled')
-- (anchor_creature_id index exists from 062 but is not filtered by status)
-- ═══════════════════════════════════════════════════════════════════════════

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_swarm_events_creator_status
    ON swarm_events (creator_id, status);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_swarm_events_anchor_active
    ON swarm_events (anchor_creature_id, status)
    WHERE anchor_creature_id IS NOT NULL
      AND status IN ('active', 'scheduled');

-- ═══════════════════════════════════════════════════════════════════════════
-- 6. creature_state — primary lookup by creature_id (should be unique)
--
-- Every creature detail load joins creature_state. Ensure the lookup is fast.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS idx_creature_state_creature_unique
    ON creature_state (creature_id);

-- ═══════════════════════════════════════════════════════════════════════════
-- 7. creature_conditions — joined in every creature query
-- ═══════════════════════════════════════════════════════════════════════════

CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS idx_creature_conditions_creature_unique
    ON creature_conditions (creature_id);

-- ═══════════════════════════════════════════════════════════════════════════
-- 8. swarm_participants — co-presence check in SSE auth
--
-- Pattern: WHERE creature_id = $1 AND left_at IS NULL
-- ═══════════════════════════════════════════════════════════════════════════

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_swarm_participants_creature_active
    ON swarm_participants (creature_id, swarm_id)
    WHERE left_at IS NULL;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_swarm_participants_swarm_active
    ON swarm_participants (swarm_id, creature_id)
    WHERE left_at IS NULL;

-- ═══════════════════════════════════════════════════════════════════════════
-- 9. rabble_messages — chat history load (ordered by time, filtered by swarm)
--
-- Already has idx_rabble_messages_swarm from 044, but adding a covering
-- index for the common SELECT pattern.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_rabble_messages_swarm_recent
    ON rabble_messages (swarm_id, created_at DESC);
