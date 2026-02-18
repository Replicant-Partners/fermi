-- Migration 092: Verify & fix social layer
--
-- 1. Reconcile notification columns — handlers use (type, message) per migration 021
--    but some older code paths may have created rows with wrong columns.
-- 2. Ensure social_visibility column exists on users.
-- 3. Re-create social layer functions (idempotent) in case migration 090 partially failed.
-- 4. Add missing notification type values that the social layer introduced.
--
-- IMPORTANT: No BEGIN/COMMIT — PgBouncer transaction mode.

-- ═══════════════════════════════════════════════════════════════════════════
-- 1. NOTIFICATIONS TABLE RECONCILIATION
-- ═══════════════════════════════════════════════════════════════════════════

-- If someone accidentally created notification_type / body columns, migrate
-- any data back into the canonical columns and drop the extras.

DO $$
BEGIN
    -- Check if 'notification_type' column exists (wrong schema)
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'notifications' AND column_name = 'notification_type'
    ) THEN
        -- Copy any data from notification_type → type where type is NULL
        UPDATE notifications SET type = notification_type
        WHERE type IS NULL AND notification_type IS NOT NULL;

        -- Drop the wrong column
        ALTER TABLE notifications DROP COLUMN IF EXISTS notification_type;
        RAISE NOTICE 'Dropped stale column notifications.notification_type';
    END IF;

    -- Check if 'body' column exists (wrong schema — canonical is 'message')
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'notifications' AND column_name = 'body'
    ) THEN
        -- Copy any data from body → message where message is NULL
        UPDATE notifications SET message = body
        WHERE message IS NULL AND body IS NOT NULL;

        -- Drop the wrong column
        ALTER TABLE notifications DROP COLUMN IF EXISTS body;
        RAISE NOTICE 'Dropped stale column notifications.body';
    END IF;
END $$;

-- Ensure the type constraint allows social notification types.
-- The original migration 021 had no CHECK constraint on type, so new types
-- are fine. But if someone added one, relax it:
DO $$
BEGIN
    -- Drop any CHECK constraint on notifications.type (if it exists)
    IF EXISTS (
        SELECT 1 FROM information_schema.constraint_column_usage ccu
        JOIN information_schema.table_constraints tc
            ON tc.constraint_name = ccu.constraint_name
        WHERE ccu.table_name = 'notifications'
          AND ccu.column_name = 'type'
          AND tc.constraint_type = 'CHECK'
    ) THEN
        EXECUTE (
            SELECT 'ALTER TABLE notifications DROP CONSTRAINT ' || tc.constraint_name
            FROM information_schema.constraint_column_usage ccu
            JOIN information_schema.table_constraints tc
                ON tc.constraint_name = ccu.constraint_name
            WHERE ccu.table_name = 'notifications'
              AND ccu.column_name = 'type'
              AND tc.constraint_type = 'CHECK'
            LIMIT 1
        );
        RAISE NOTICE 'Dropped CHECK constraint on notifications.type to allow social types';
    END IF;
END $$;

-- Add an index on unread notifications per user (for badge counts)
CREATE INDEX IF NOT EXISTS idx_notifications_user_unread
    ON notifications(user_id, created_at DESC)
    WHERE read = false;

-- ═══════════════════════════════════════════════════════════════════════════
-- 2. SOCIAL VISIBILITY ON USERS
-- ═══════════════════════════════════════════════════════════════════════════

ALTER TABLE users ADD COLUMN IF NOT EXISTS social_visibility TEXT
    NOT NULL DEFAULT 'public'
    CHECK (social_visibility IN ('public', 'creature-only', 'private'));

-- ═══════════════════════════════════════════════════════════════════════════
-- 3. CREATURE FRIENDSHIPS TABLE (idempotent)
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS creature_friendships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    creature_a UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    creature_b UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    initiated_by UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'accepted', 'declined', 'blocked')),
    met_in_rabble UUID REFERENCES swarm_events(swarm_id) ON DELETE SET NULL,
    met_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(creature_a, creature_b),
    CHECK (creature_a < creature_b)
);

CREATE INDEX IF NOT EXISTS idx_creature_friendships_a
    ON creature_friendships(creature_a);
CREATE INDEX IF NOT EXISTS idx_creature_friendships_b
    ON creature_friendships(creature_b);
CREATE INDEX IF NOT EXISTS idx_creature_friendships_pending
    ON creature_friendships(status) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_creature_friendships_rabble
    ON creature_friendships(met_in_rabble) WHERE met_in_rabble IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════════════
-- 4. CREATURE INVITES TABLE (idempotent)
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS creature_invites (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    from_creature_id UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    to_creature_id UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    rabble_id UUID NOT NULL REFERENCES swarm_events(swarm_id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'accepted', 'declined', 'expired')),
    message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    responded_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ NOT NULL DEFAULT (NOW() + INTERVAL '24 hours')
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_creature_invites_unique_pending
    ON creature_invites(from_creature_id, to_creature_id, rabble_id)
    WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_creature_invites_to
    ON creature_invites(to_creature_id, status);
CREATE INDEX IF NOT EXISTS idx_creature_invites_from
    ON creature_invites(from_creature_id);
CREATE INDEX IF NOT EXISTS idx_creature_invites_expires
    ON creature_invites(expires_at) WHERE status = 'pending';

-- ═══════════════════════════════════════════════════════════════════════════
-- 5. ACTIVITY EVENTS TABLE (idempotent)
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS activity_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_user_id TEXT NOT NULL,
    actor_creature_id UUID REFERENCES creatures(creature_id) ON DELETE SET NULL,
    event_type TEXT NOT NULL CHECK (event_type IN (
        'creature_minted', 'creature_perched', 'creature_flew', 'creature_landed',
        'rabble_created', 'rabble_joined', 'rabble_left', 'rabble_completed',
        'friendship_requested', 'friendship_accepted',
        'creature_invited', 'creature_invite_accepted',
        'flight_planned', 'observation_recorded', 'creature_gifted', 'chat_message'
    )),
    rabble_id UUID REFERENCES swarm_events(swarm_id) ON DELETE SET NULL,
    target_creature_id UUID REFERENCES creatures(creature_id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    body TEXT,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_activity_events_created
    ON activity_events(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_activity_events_actor
    ON activity_events(actor_user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_activity_events_rabble
    ON activity_events(rabble_id, created_at DESC) WHERE rabble_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_activity_events_type
    ON activity_events(event_type, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_activity_events_metadata
    ON activity_events USING gin(metadata);

-- ═══════════════════════════════════════════════════════════════════════════
-- 6. RABBLE CO-PRESENCE TABLE (idempotent)
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS rabble_co_presence (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    rabble_id UUID NOT NULL REFERENCES swarm_events(swarm_id) ON DELETE CASCADE,
    creature_id UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    owner_id TEXT NOT NULL,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at TIMESTAMPTZ,
    overlap_seconds INTEGER DEFAULT 0,
    UNIQUE(rabble_id, creature_id)
);

CREATE INDEX IF NOT EXISTS idx_rabble_co_presence_rabble
    ON rabble_co_presence(rabble_id);
CREATE INDEX IF NOT EXISTS idx_rabble_co_presence_creature
    ON rabble_co_presence(creature_id);
CREATE INDEX IF NOT EXISTS idx_rabble_co_presence_owner
    ON rabble_co_presence(owner_id);

-- ═══════════════════════════════════════════════════════════════════════════
-- 7. RE-CREATE ALL SOCIAL FUNCTIONS (CREATE OR REPLACE = idempotent)
-- ═══════════════════════════════════════════════════════════════════════════

-- Canonical ordering helper
CREATE OR REPLACE FUNCTION canonical_creature_pair(a UUID, b UUID)
RETURNS TABLE(creature_a UUID, creature_b UUID) AS $$
BEGIN
    IF a < b THEN
        RETURN QUERY SELECT a, b;
    ELSE
        RETURN QUERY SELECT b, a;
    END IF;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Creatures met in a rabble (for recap / friend suggestions)
CREATE OR REPLACE FUNCTION get_creatures_met_in_rabble(
    p_rabble_id UUID,
    p_creature_id UUID
) RETURNS TABLE (
    creature_id UUID,
    specimen_name TEXT,
    scientific_name TEXT,
    species_group TEXT,
    asset_path TEXT,
    owner_id TEXT,
    owner_display_name TEXT,
    owner_social_visibility TEXT,
    overlap_seconds INTEGER,
    already_friends BOOLEAN,
    friendship_status TEXT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        cp.creature_id,
        c.specimen_name,
        c.scientific_name,
        c.species_group,
        c.asset_path,
        c.owner_id,
        CASE
            WHEN u.social_visibility = 'private' THEN NULL
            WHEN u.social_visibility = 'creature-only' THEN NULL
            ELSE u.display_name
        END AS owner_display_name,
        u.social_visibility AS owner_social_visibility,
        cp.overlap_seconds,
        EXISTS (
            SELECT 1 FROM creature_friendships cf
            WHERE cf.status = 'accepted'
              AND cf.creature_a = LEAST(p_creature_id, cp.creature_id)
              AND cf.creature_b = GREATEST(p_creature_id, cp.creature_id)
        ) AS already_friends,
        (
            SELECT cf.status FROM creature_friendships cf
            WHERE cf.creature_a = LEAST(p_creature_id, cp.creature_id)
              AND cf.creature_b = GREATEST(p_creature_id, cp.creature_id)
            LIMIT 1
        ) AS friendship_status
    FROM rabble_co_presence cp
    JOIN creatures c ON c.creature_id = cp.creature_id
    LEFT JOIN users u ON u.user_id = c.owner_id
    WHERE cp.rabble_id = p_rabble_id
      AND cp.creature_id != p_creature_id
    ORDER BY cp.overlap_seconds DESC NULLS LAST;
END;
$$ LANGUAGE plpgsql;

-- Pending friendship requests for a user's creatures
CREATE OR REPLACE FUNCTION get_pending_friendship_requests(
    p_user_id TEXT
) RETURNS TABLE (
    friendship_id UUID,
    from_creature_id UUID,
    from_creature_name TEXT,
    from_species_group TEXT,
    from_asset_path TEXT,
    from_owner_id TEXT,
    from_owner_name TEXT,
    to_creature_id UUID,
    to_creature_name TEXT,
    met_in_rabble UUID,
    rabble_name TEXT,
    created_at TIMESTAMPTZ
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        cf.id AS friendship_id,
        cf.initiated_by AS from_creature_id,
        c_from.specimen_name AS from_creature_name,
        c_from.species_group AS from_species_group,
        c_from.asset_path AS from_asset_path,
        c_from.owner_id AS from_owner_id,
        u_from.display_name AS from_owner_name,
        CASE
            WHEN cf.creature_a = cf.initiated_by THEN cf.creature_b
            ELSE cf.creature_a
        END AS to_creature_id,
        c_to.specimen_name AS to_creature_name,
        cf.met_in_rabble,
        s.name AS rabble_name,
        cf.created_at
    FROM creature_friendships cf
    JOIN creatures c_from ON c_from.creature_id = cf.initiated_by
    LEFT JOIN users u_from ON u_from.user_id = c_from.owner_id
    JOIN creatures c_to ON c_to.creature_id = (
        CASE
            WHEN cf.creature_a = cf.initiated_by THEN cf.creature_b
            ELSE cf.creature_a
        END
    )
    LEFT JOIN swarm_events s ON s.swarm_id = cf.met_in_rabble
    WHERE cf.status = 'pending'
      AND c_to.owner_id = p_user_id
      AND c_from.owner_id != p_user_id
    ORDER BY cf.created_at DESC;
END;
$$ LANGUAGE plpgsql;

-- Accepted friends for a creature
CREATE OR REPLACE FUNCTION get_creature_friends(
    p_creature_id UUID,
    p_limit INTEGER DEFAULT 100,
    p_offset INTEGER DEFAULT 0
) RETURNS TABLE (
    friendship_id UUID,
    friend_creature_id UUID,
    friend_name TEXT,
    friend_species_group TEXT,
    friend_asset_path TEXT,
    friend_owner_id TEXT,
    friend_owner_name TEXT,
    friend_social_visibility TEXT,
    met_in_rabble UUID,
    rabble_name TEXT,
    friends_since TIMESTAMPTZ
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        cf.id AS friendship_id,
        CASE
            WHEN cf.creature_a = p_creature_id THEN cf.creature_b
            ELSE cf.creature_a
        END AS friend_creature_id,
        c.specimen_name AS friend_name,
        c.species_group AS friend_species_group,
        c.asset_path AS friend_asset_path,
        c.owner_id AS friend_owner_id,
        CASE
            WHEN u.social_visibility IN ('public') THEN u.display_name
            ELSE NULL
        END AS friend_owner_name,
        u.social_visibility AS friend_social_visibility,
        cf.met_in_rabble,
        s.name AS rabble_name,
        cf.updated_at AS friends_since
    FROM creature_friendships cf
    JOIN creatures c ON c.creature_id = (
        CASE
            WHEN cf.creature_a = p_creature_id THEN cf.creature_b
            ELSE cf.creature_a
        END
    )
    LEFT JOIN users u ON u.user_id = c.owner_id
    LEFT JOIN swarm_events s ON s.swarm_id = cf.met_in_rabble
    WHERE cf.status = 'accepted'
      AND (cf.creature_a = p_creature_id OR cf.creature_b = p_creature_id)
    ORDER BY cf.updated_at DESC
    LIMIT p_limit
    OFFSET p_offset;
END;
$$ LANGUAGE plpgsql;

-- Pending creature invites for a user's creatures
CREATE OR REPLACE FUNCTION get_pending_creature_invites(
    p_user_id TEXT
) RETURNS TABLE (
    invite_id UUID,
    from_creature_id UUID,
    from_creature_name TEXT,
    from_species_group TEXT,
    from_asset_path TEXT,
    from_owner_name TEXT,
    to_creature_id UUID,
    to_creature_name TEXT,
    rabble_id UUID,
    rabble_name TEXT,
    message TEXT,
    created_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        ci.id AS invite_id,
        ci.from_creature_id,
        c_from.specimen_name AS from_creature_name,
        c_from.species_group AS from_species_group,
        c_from.asset_path AS from_asset_path,
        CASE
            WHEN u_from.social_visibility = 'public' THEN u_from.display_name
            ELSE NULL
        END AS from_owner_name,
        ci.to_creature_id,
        c_to.specimen_name AS to_creature_name,
        ci.rabble_id,
        s.name AS rabble_name,
        ci.message,
        ci.created_at,
        ci.expires_at
    FROM creature_invites ci
    JOIN creatures c_from ON c_from.creature_id = ci.from_creature_id
    LEFT JOIN users u_from ON u_from.user_id = c_from.owner_id
    JOIN creatures c_to ON c_to.creature_id = ci.to_creature_id
    JOIN swarm_events s ON s.swarm_id = ci.rabble_id
    WHERE ci.status = 'pending'
      AND ci.expires_at > NOW()
      AND c_to.owner_id = p_user_id
    ORDER BY ci.created_at DESC;
END;
$$ LANGUAGE plpgsql;

-- Activity feed with relationship context
CREATE OR REPLACE FUNCTION get_activity_feed(
    p_user_id TEXT,
    p_before TIMESTAMPTZ DEFAULT NOW(),
    p_limit INTEGER DEFAULT 50
) RETURNS TABLE (
    event_id UUID,
    event_type TEXT,
    actor_user_id TEXT,
    actor_creature_id UUID,
    actor_creature_name TEXT,
    actor_species_group TEXT,
    rabble_id UUID,
    rabble_name TEXT,
    target_creature_id UUID,
    target_creature_name TEXT,
    title TEXT,
    body TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ,
    is_own_creature BOOLEAN,
    is_contact BOOLEAN,
    is_friend_creature BOOLEAN
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        ae.id AS event_id,
        ae.event_type,
        ae.actor_user_id,
        ae.actor_creature_id,
        c_actor.specimen_name AS actor_creature_name,
        c_actor.species_group AS actor_species_group,
        ae.rabble_id,
        s.name AS rabble_name,
        ae.target_creature_id,
        c_target.specimen_name AS target_creature_name,
        ae.title,
        ae.body,
        ae.metadata,
        ae.created_at,
        (c_actor.owner_id = p_user_id) AS is_own_creature,
        EXISTS (
            SELECT 1 FROM contacts ct
            WHERE ct.user_id = p_user_id AND ct.contact_id = ae.actor_user_id
        ) AS is_contact,
        EXISTS (
            SELECT 1 FROM creature_friendships cf
            JOIN creatures mc ON mc.owner_id = p_user_id
            WHERE cf.status = 'accepted'
              AND (
                  (cf.creature_a = ae.actor_creature_id AND cf.creature_b = mc.creature_id)
                  OR
                  (cf.creature_b = ae.actor_creature_id AND cf.creature_a = mc.creature_id)
              )
        ) AS is_friend_creature
    FROM activity_events ae
    LEFT JOIN creatures c_actor ON c_actor.creature_id = ae.actor_creature_id
    LEFT JOIN creatures c_target ON c_target.creature_id = ae.target_creature_id
    LEFT JOIN swarm_events s ON s.swarm_id = ae.rabble_id
    WHERE ae.created_at < p_before
      AND (
          ae.actor_user_id = p_user_id
          OR EXISTS (
              SELECT 1 FROM contacts ct
              WHERE ct.user_id = p_user_id AND ct.contact_id = ae.actor_user_id
          )
          OR EXISTS (
              SELECT 1 FROM creature_friendships cf
              JOIN creatures mc ON mc.owner_id = p_user_id
              WHERE cf.status = 'accepted'
                AND (
                    (cf.creature_a = ae.actor_creature_id AND cf.creature_b = mc.creature_id)
                    OR
                    (cf.creature_b = ae.actor_creature_id AND cf.creature_a = mc.creature_id)
                )
          )
          OR EXISTS (
              SELECT 1 FROM creature_state cs
              JOIN creatures c ON c.creature_id = cs.creature_id
              WHERE c.owner_id = p_user_id
                AND cs.rabble_id = ae.rabble_id
          )
      )
    ORDER BY ae.created_at DESC
    LIMIT p_limit;
END;
$$ LANGUAGE plpgsql;

-- Expire old creature invites (callable from cron or app startup)
CREATE OR REPLACE FUNCTION expire_old_creature_invites()
RETURNS INTEGER AS $$
DECLARE
    expired_count INTEGER;
BEGIN
    UPDATE creature_invites
    SET status = 'expired'
    WHERE status = 'pending'
      AND expires_at < NOW();
    GET DIAGNOSTICS expired_count = ROW_COUNT;
    RETURN expired_count;
END;
$$ LANGUAGE plpgsql;

-- ═══════════════════════════════════════════════════════════════════════════
-- 8. VERIFICATION
-- ═══════════════════════════════════════════════════════════════════════════

DO $$
DECLARE
    missing TEXT := '';
BEGIN
    -- Verify tables
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'creature_friendships') THEN
        missing := missing || 'creature_friendships table, ';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'creature_invites') THEN
        missing := missing || 'creature_invites table, ';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'activity_events') THEN
        missing := missing || 'activity_events table, ';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'rabble_co_presence') THEN
        missing := missing || 'rabble_co_presence table, ';
    END IF;

    -- Verify columns
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'users' AND column_name = 'social_visibility'
    ) THEN
        missing := missing || 'users.social_visibility column, ';
    END IF;

    -- Verify notification columns are correct
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'notifications' AND column_name = 'type'
    ) THEN
        missing := missing || 'notifications.type column, ';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'notifications' AND column_name = 'message'
    ) THEN
        missing := missing || 'notifications.message column, ';
    END IF;

    -- Verify wrong columns are gone
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'notifications' AND column_name = 'notification_type'
    ) THEN
        missing := missing || 'notifications still has stale notification_type column, ';
    END IF;

    -- Verify functions
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.routines
        WHERE routine_schema = 'public' AND routine_name = 'get_pending_friendship_requests'
    ) THEN
        missing := missing || 'get_pending_friendship_requests function, ';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.routines
        WHERE routine_schema = 'public' AND routine_name = 'get_creature_friends'
    ) THEN
        missing := missing || 'get_creature_friends function, ';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.routines
        WHERE routine_schema = 'public' AND routine_name = 'get_pending_creature_invites'
    ) THEN
        missing := missing || 'get_pending_creature_invites function, ';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.routines
        WHERE routine_schema = 'public' AND routine_name = 'get_activity_feed'
    ) THEN
        missing := missing || 'get_activity_feed function, ';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.routines
        WHERE routine_schema = 'public' AND routine_name = 'get_creatures_met_in_rabble'
    ) THEN
        missing := missing || 'get_creatures_met_in_rabble function, ';
    END IF;

    IF missing != '' THEN
        RAISE WARNING 'Social layer verification: MISSING — %', rtrim(missing, ', ');
    ELSE
        RAISE NOTICE 'Social layer verification: ALL OK ✓';
    END IF;
END $$;
