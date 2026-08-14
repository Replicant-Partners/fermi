-- Migration 097: Governance — Block + Eject + Report
--
-- Three governance primitives for social safety:
--   1. Block (creature-level + user-level escalation)
--   2. Eject (host removes creature from rabble)
--   3. Report (flag content/behavior for review)
--
-- Design: docs/DESIGN_GOVERNANCE.md

-- ═══════════════════════════════════════════════════════════════════════════
-- 1. CREATURE BLOCKS — light boundary between specific creatures
--
-- "Luna doesn't want to interact with Bad Bunny"
-- The blocked creature does NOT know they are blocked (privacy).
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS creature_blocks (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    blocker_creature_id UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    blocked_creature_id UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    blocker_user_id     TEXT NOT NULL,       -- denormalized for fast user-level queries
    blocked_user_id     TEXT NOT NULL,       -- denormalized for fast user-level queries
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(blocker_creature_id, blocked_creature_id),
    CHECK(blocker_creature_id != blocked_creature_id)
);

CREATE INDEX IF NOT EXISTS idx_creature_blocks_blocker
    ON creature_blocks(blocker_creature_id);
CREATE INDEX IF NOT EXISTS idx_creature_blocks_blocked
    ON creature_blocks(blocked_creature_id);
CREATE INDEX IF NOT EXISTS idx_creature_blocks_blocker_user
    ON creature_blocks(blocker_user_id);
CREATE INDEX IF NOT EXISTS idx_creature_blocks_blocked_user
    ON creature_blocks(blocked_user_id);

-- ═══════════════════════════════════════════════════════════════════════════
-- 2. USER BLOCKS — escalation, hides ALL creatures from ALL of yours
--
-- "I don't want any interaction with this person"
-- Nuclear option for harassment. The blocked user does NOT know.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS user_blocks (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    blocker_user_id TEXT NOT NULL,
    blocked_user_id TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(blocker_user_id, blocked_user_id),
    CHECK(blocker_user_id != blocked_user_id)
);

CREATE INDEX IF NOT EXISTS idx_user_blocks_blocker
    ON user_blocks(blocker_user_id);
CREATE INDEX IF NOT EXISTS idx_user_blocks_blocked
    ON user_blocks(blocked_user_id);

-- ═══════════════════════════════════════════════════════════════════════════
-- 3. RABBLE EJECTIONS — host removes creature from their rabble
--
-- Ejected creatures face a 24h cooldown before rejoining (unless permanent).
-- The ejection reason is visible to admins only, not the ejected user.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS rabble_ejections (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    swarm_id            UUID NOT NULL REFERENCES swarm_events(swarm_id) ON DELETE CASCADE,
    ejected_creature_id UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    ejected_user_id     TEXT NOT NULL,           -- owner of the ejected creature
    ejected_by_user     TEXT NOT NULL,           -- the host who ejected
    reason              TEXT,                    -- admin-visible only
    permanent           BOOLEAN NOT NULL DEFAULT false,
    cooldown_until      TIMESTAMPTZ,             -- null if permanent, otherwise +24h
    ejected_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ejections_swarm
    ON rabble_ejections(swarm_id);
CREATE INDEX IF NOT EXISTS idx_ejections_creature
    ON rabble_ejections(ejected_creature_id);
-- No WHERE clause here on purpose. This index used to be partial on
-- `permanent = true OR cooldown_until > NOW()`, which Postgres rejects
-- outright (NOW() is STABLE, and index predicates must be IMMUTABLE) —
-- so this migration had never applied. The predicate was also wrong on
-- its own terms: it would be evaluated once at index-build time and
-- then frozen, so rows would silently drop out of the index as their
-- cooldown elapsed and the index would quietly stop matching reality.
-- The full index still serves the (swarm_id, ejected_creature_id)
-- lookup; callers filter on permanent / cooldown_until at query time,
-- where NOW() is evaluated fresh.
CREATE INDEX IF NOT EXISTS idx_ejections_active
    ON rabble_ejections(swarm_id, ejected_creature_id);

-- ═══════════════════════════════════════════════════════════════════════════
-- 4. REPORTS — flag content/behavior for review
--
-- Reports capture a snapshot of the reported content so it can be reviewed
-- even if the original is edited or deleted.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS reports (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reporter_user_id TEXT NOT NULL,
    report_type     TEXT NOT NULL,               -- 'creature', 'message', 'user', 'rabble'
    target_id       TEXT NOT NULL,               -- UUID as text (polymorphic)
    target_type     TEXT NOT NULL,               -- same as report_type
    reason          TEXT NOT NULL,               -- standardized reason code
    description     TEXT,                        -- free-text from reporter
    context         JSONB DEFAULT '{}',          -- snapshot of reported content
    status          TEXT NOT NULL DEFAULT 'pending',
    reviewed_by     TEXT,                        -- admin user_id
    review_notes    TEXT,
    action_taken    TEXT,                        -- 'none', 'warned', 'muted', 'suspended', 'banned', 'deleted'
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reviewed_at     TIMESTAMPTZ,

    CHECK(report_type IN ('creature', 'message', 'user', 'rabble')),
    CHECK(reason IN ('inappropriate_content', 'harassment', 'spam', 'impersonation', 'other')),
    CHECK(status IN ('pending', 'reviewed', 'action_taken', 'dismissed'))
);

CREATE INDEX IF NOT EXISTS idx_reports_status
    ON reports(status) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_reports_target
    ON reports(target_type, target_id);
CREATE INDEX IF NOT EXISTS idx_reports_reporter
    ON reports(reporter_user_id);
CREATE INDEX IF NOT EXISTS idx_reports_created
    ON reports(created_at DESC);

-- ═══════════════════════════════════════════════════════════════════════════
-- 5. HELPER FUNCTIONS
-- ═══════════════════════════════════════════════════════════════════════════

-- Check if any block exists between two creatures (either direction, either level)
CREATE OR REPLACE FUNCTION is_blocked(
    p_creature_a UUID,
    p_creature_b UUID
) RETURNS BOOLEAN AS $$
DECLARE
    v_blocked BOOLEAN;
BEGIN
    -- Creature-level block (either direction)
    SELECT EXISTS(
        SELECT 1 FROM creature_blocks
        WHERE (blocker_creature_id = p_creature_a AND blocked_creature_id = p_creature_b)
           OR (blocker_creature_id = p_creature_b AND blocked_creature_id = p_creature_a)
    ) INTO v_blocked;

    IF v_blocked THEN RETURN true; END IF;

    -- User-level block (either direction) — look up owners
    SELECT EXISTS(
        SELECT 1 FROM user_blocks ub
        WHERE (ub.blocker_user_id = (SELECT owner_id FROM creatures WHERE creature_id = p_creature_a)
           AND ub.blocked_user_id = (SELECT owner_id FROM creatures WHERE creature_id = p_creature_b))
           OR (ub.blocker_user_id = (SELECT owner_id FROM creatures WHERE creature_id = p_creature_b)
           AND ub.blocked_user_id = (SELECT owner_id FROM creatures WHERE creature_id = p_creature_a))
    ) INTO v_blocked;

    RETURN v_blocked;
END;
$$ LANGUAGE plpgsql STABLE;

-- Check if a user is blocked by another user (user-level only)
CREATE OR REPLACE FUNCTION is_user_blocked(
    p_user_a TEXT,
    p_user_b TEXT
) RETURNS BOOLEAN AS $$
    SELECT EXISTS(
        SELECT 1 FROM user_blocks
        WHERE (blocker_user_id = p_user_a AND blocked_user_id = p_user_b)
           OR (blocker_user_id = p_user_b AND blocked_user_id = p_user_a)
    );
$$ LANGUAGE sql STABLE;

-- Check if a creature is ejected from a rabble (and cooldown hasn't expired)
CREATE OR REPLACE FUNCTION is_ejected(
    p_swarm_id UUID,
    p_creature_id UUID
) RETURNS BOOLEAN AS $$
    SELECT EXISTS(
        SELECT 1 FROM rabble_ejections
        WHERE swarm_id = p_swarm_id
          AND ejected_creature_id = p_creature_id
          AND (permanent = true OR cooldown_until > NOW())
    );
$$ LANGUAGE sql STABLE;

-- Get block list for a user (both creature-level and user-level)
CREATE OR REPLACE FUNCTION get_user_blocks(
    p_user_id TEXT
) RETURNS TABLE (
    block_id UUID,
    block_level TEXT,          -- 'creature' or 'user'
    blocked_entity_id TEXT,    -- creature_id or user_id
    blocked_name TEXT,         -- creature specimen_name or user display_name
    blocked_image TEXT,        -- creature asset_path or user avatar
    created_at TIMESTAMPTZ
) AS $$
    -- Creature-level blocks by this user's creatures
    SELECT
        cb.id AS block_id,
        'creature'::TEXT AS block_level,
        cb.blocked_creature_id::TEXT AS blocked_entity_id,
        c.specimen_name AS blocked_name,
        c.asset_path AS blocked_image,
        cb.created_at
    FROM creature_blocks cb
    JOIN creatures c ON c.creature_id = cb.blocked_creature_id
    WHERE cb.blocker_user_id = p_user_id

    UNION ALL

    -- User-level blocks
    SELECT
        ub.id AS block_id,
        'user'::TEXT AS block_level,
        ub.blocked_user_id AS blocked_entity_id,
        u.display_name AS blocked_name,
        u.avatar_url AS blocked_image,
        ub.created_at
    FROM user_blocks ub
    LEFT JOIN users u ON u.user_id = ub.blocked_user_id
    WHERE ub.blocker_user_id = p_user_id

    ORDER BY created_at DESC;
$$ LANGUAGE sql STABLE;
