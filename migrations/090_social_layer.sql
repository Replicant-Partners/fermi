-- Migration 090: Social Layer
--
-- Creature-to-creature friendships (symmetric, canonical order)
-- Creature-to-creature invites ("come fly with me")
-- User social visibility preference
-- Activity events for SSE feed
--
-- IMPORTANT: No BEGIN/COMMIT — PgBouncer transaction mode.

-- ═══════════════════════════════════════════════════════════════════════════
-- 1. CREATURE FRIENDSHIPS
-- ═══════════════════════════════════════════════════════════════════════════
-- Symmetric relationship with canonical ordering (creature_a < creature_b).
-- Tracks where the creatures met (which rabble) for post-rabble recap.

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

-- Index for looking up all friendships for a creature (either side)
CREATE INDEX IF NOT EXISTS idx_creature_friendships_a
    ON creature_friendships(creature_a);
CREATE INDEX IF NOT EXISTS idx_creature_friendships_b
    ON creature_friendships(creature_b);

-- Index for pending requests (for notification badge counts)
CREATE INDEX IF NOT EXISTS idx_creature_friendships_pending
    ON creature_friendships(status)
    WHERE status = 'pending';

-- Index for looking up friendships by rabble (for recap screen)
CREATE INDEX IF NOT EXISTS idx_creature_friendships_rabble
    ON creature_friendships(met_in_rabble)
    WHERE met_in_rabble IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════════════
-- 2. CREATURE INVITES ("come fly with me")
-- ═══════════════════════════════════════════════════════════════════════════
-- Layer 2 action: creature-to-creature active invitation to join a rabble.
-- Distinct from social invites (user-to-user, Layer 1) which grant visibility.

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

-- Prevent duplicate pending invites for the same creature pair + rabble
CREATE UNIQUE INDEX IF NOT EXISTS idx_creature_invites_unique_pending
    ON creature_invites(from_creature_id, to_creature_id, rabble_id)
    WHERE status = 'pending';

-- Invites received by a creature (for notification listing)
CREATE INDEX IF NOT EXISTS idx_creature_invites_to
    ON creature_invites(to_creature_id, status);

-- Invites sent from a creature
CREATE INDEX IF NOT EXISTS idx_creature_invites_from
    ON creature_invites(from_creature_id);

-- Expire old invites efficiently
CREATE INDEX IF NOT EXISTS idx_creature_invites_expires
    ON creature_invites(expires_at)
    WHERE status = 'pending';

-- ═══════════════════════════════════════════════════════════════════════════
-- 3. USER SOCIAL VISIBILITY
-- ═══════════════════════════════════════════════════════════════════════════
-- Controls how a user appears in social contexts:
--   public:        name + creatures visible to everyone
--   creature-only: only creature identities shown, owner anonymous
--   private:       hidden from search, discoverable only via direct link

ALTER TABLE users ADD COLUMN IF NOT EXISTS social_visibility TEXT
    NOT NULL DEFAULT 'public'
    CHECK (social_visibility IN ('public', 'creature-only', 'private'));

-- ═══════════════════════════════════════════════════════════════════════════
-- 4. ACTIVITY EVENTS (for SSE feed)
-- ═══════════════════════════════════════════════════════════════════════════
-- Denormalized event stream for the activity feed. Written on every
-- significant action. The feed SSE endpoint streams from this table
-- and annotates events with relationship context (your creature,
-- contact's creature, friend creature, etc.)

CREATE TABLE IF NOT EXISTS activity_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Who did it
    actor_user_id TEXT NOT NULL,
    actor_creature_id UUID REFERENCES creatures(creature_id) ON DELETE SET NULL,

    -- What happened
    event_type TEXT NOT NULL CHECK (event_type IN (
        'creature_minted',
        'creature_perched',
        'creature_flew',
        'creature_landed',
        'rabble_created',
        'rabble_joined',
        'rabble_left',
        'rabble_completed',
        'friendship_requested',
        'friendship_accepted',
        'creature_invited',
        'creature_invite_accepted',
        'flight_planned',
        'observation_recorded',
        'creature_gifted',
        'chat_message'
    )),

    -- Where it happened (optional)
    rabble_id UUID REFERENCES swarm_events(swarm_id) ON DELETE SET NULL,
    target_creature_id UUID REFERENCES creatures(creature_id) ON DELETE SET NULL,

    -- Display data (denormalized for fast reads)
    title TEXT NOT NULL,
    body TEXT,
    metadata JSONB DEFAULT '{}',

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Primary query: feed for a user (their own + contacts' + friends' events)
CREATE INDEX IF NOT EXISTS idx_activity_events_created
    ON activity_events(created_at DESC);

-- Filter by actor for "my activity" view
CREATE INDEX IF NOT EXISTS idx_activity_events_actor
    ON activity_events(actor_user_id, created_at DESC);

-- Filter by rabble for rabble-scoped feeds
CREATE INDEX IF NOT EXISTS idx_activity_events_rabble
    ON activity_events(rabble_id, created_at DESC)
    WHERE rabble_id IS NOT NULL;

-- Filter by event type
CREATE INDEX IF NOT EXISTS idx_activity_events_type
    ON activity_events(event_type, created_at DESC);

-- GIN index on metadata for flexible queries
CREATE INDEX IF NOT EXISTS idx_activity_events_metadata
    ON activity_events USING gin(metadata);

-- ═══════════════════════════════════════════════════════════════════════════
-- 5. RABBLE CO-PRESENCE TRACKING
-- ═══════════════════════════════════════════════════════════════════════════
-- Records which creatures were co-present in a rabble. Drives the
-- "You met these creatures" post-rabble recap screen.

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

-- Fast lookup: all creatures that were in a rabble
CREATE INDEX IF NOT EXISTS idx_rabble_co_presence_rabble
    ON rabble_co_presence(rabble_id);

-- Fast lookup: all rabbles a creature participated in
CREATE INDEX IF NOT EXISTS idx_rabble_co_presence_creature
    ON rabble_co_presence(creature_id);

-- Fast lookup: all rabbles a user's creatures participated in
CREATE INDEX IF NOT EXISTS idx_rabble_co_presence_owner
    ON rabble_co_presence(owner_id);

-- ═══════════════════════════════════════════════════════════════════════════
-- 6. HELPER FUNCTIONS
-- ═══════════════════════════════════════════════════════════════════════════

-- Canonical ordering for friendship pairs
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

-- Get creatures a given creature has met in rabbles (for recap/friend suggestions)
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
              AND (
                  (cf.creature_a = LEAST(p_creature_id, cp.creature_id)
                   AND cf.creature_b = GREATEST(p_creature_id, cp.creature_id))
              )
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

-- Get pending friendship requests for all creatures owned by a user
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

-- Get accepted friends for a creature
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

-- Get pending creature invites for a user's creatures
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

-- Get activity feed with relationship context
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
    -- Relationship context annotations
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
        -- Is this one of the user's own creatures?
        (c_actor.owner_id = p_user_id) AS is_own_creature,
        -- Is the actor a contact of the user?
        EXISTS (
            SELECT 1 FROM contacts ct
            WHERE ct.user_id = p_user_id AND ct.contact_id = ae.actor_user_id
        ) AS is_contact,
        -- Is the actor creature friends with any of the user's creatures?
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
          -- Own activity
          ae.actor_user_id = p_user_id
          -- Contact activity
          OR EXISTS (
              SELECT 1 FROM contacts ct
              WHERE ct.user_id = p_user_id AND ct.contact_id = ae.actor_user_id
          )
          -- Friend creature activity
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
          -- Public events in rabbles the user participates in
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

-- ═══════════════════════════════════════════════════════════════════════════
-- 7. AUTO-EXPIRE INVITES (callable from cron or app startup)
-- ═══════════════════════════════════════════════════════════════════════════

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
BEGIN
    -- Verify tables created
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'creature_friendships') THEN
        RAISE EXCEPTION 'creature_friendships table not created';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'creature_invites') THEN
        RAISE EXCEPTION 'creature_invites table not created';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'activity_events') THEN
        RAISE EXCEPTION 'activity_events table not created';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'rabble_co_presence') THEN
        RAISE EXCEPTION 'rabble_co_presence table not created';
    END IF;

    -- Verify social_visibility column on users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'users' AND column_name = 'social_visibility'
    ) THEN
        RAISE EXCEPTION 'social_visibility column not added to users';
    END IF;
END $$;

-- Log migration
INSERT INTO migrations_log (migration_id, applied_at, description)
VALUES (
    '090',
    NOW(),
    'Social layer: creature_friendships, creature_invites, social_visibility, activity_events, rabble_co_presence'
)
ON CONFLICT (migration_id) DO NOTHING;
