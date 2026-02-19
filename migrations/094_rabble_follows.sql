-- Migration 094: Rabble follows — subscribe to rabble notifications without joining
--
-- Decision D3: Following rabbles = active notifications (not passive bookmarks).
-- Users can follow any rabble and receive notifications when:
--   - A creature joins (notify_on_join)
--   - The rabble starts/becomes active (notify_on_start)
--   - The rabble ends/completes (notify_on_end)
--
-- This is a user-level concept (not creature-level) — you follow a rabble
-- as yourself, not through a specific creature.

CREATE TABLE IF NOT EXISTS rabble_follows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    swarm_id UUID NOT NULL,
    notify_on_join BOOLEAN NOT NULL DEFAULT TRUE,
    notify_on_start BOOLEAN NOT NULL DEFAULT TRUE,
    notify_on_end BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, swarm_id)
);

-- Foreign key to users (user_id is TEXT, matches users.user_id)
-- Using DO block to avoid failure if constraint already exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE constraint_name = 'rabble_follows_user_id_fkey'
    ) THEN
        ALTER TABLE rabble_follows
            ADD CONSTRAINT rabble_follows_user_id_fkey
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- Foreign key to swarm_events
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE constraint_name = 'rabble_follows_swarm_id_fkey'
    ) THEN
        ALTER TABLE rabble_follows
            ADD CONSTRAINT rabble_follows_swarm_id_fkey
            FOREIGN KEY (swarm_id) REFERENCES swarm_events(swarm_id) ON DELETE CASCADE;
    END IF;
END $$;

-- Index for fast lookup: "what rabbles does this user follow?"
CREATE INDEX IF NOT EXISTS idx_rabble_follows_user_id
    ON rabble_follows(user_id);

-- Index for fast lookup: "who follows this rabble?" (used when emitting notifications)
CREATE INDEX IF NOT EXISTS idx_rabble_follows_swarm_id
    ON rabble_follows(swarm_id);

-- Helper function: get followed rabbles with swarm details for a user
CREATE OR REPLACE FUNCTION get_followed_rabbles(p_user_id TEXT, p_limit INT DEFAULT 50)
RETURNS TABLE (
    follow_id UUID,
    swarm_id UUID,
    name TEXT,
    location_name TEXT,
    center_lat DOUBLE PRECISION,
    center_lng DOUBLE PRECISION,
    radius_meters INT,
    creature_count INT,
    participant_count INT,
    starts_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ,
    status TEXT,
    anchor_creature_id UUID,
    anchor_creature_name TEXT,
    anchor_creature_image TEXT,
    notify_on_join BOOLEAN,
    notify_on_start BOOLEAN,
    notify_on_end BOOLEAN,
    followed_at TIMESTAMPTZ
) AS $$
    SELECT
        rf.id AS follow_id,
        se.swarm_id,
        se.name,
        se.location_name,
        se.center_lat,
        se.center_lng,
        se.radius_meters,
        se.creature_count,
        se.participant_count,
        se.starts_at,
        se.ends_at,
        se.status,
        se.anchor_creature_id,
        c.specimen_name AS anchor_creature_name,
        c.asset_path AS anchor_creature_image,
        rf.notify_on_join,
        rf.notify_on_start,
        rf.notify_on_end,
        rf.created_at AS followed_at
    FROM rabble_follows rf
    JOIN swarm_events se ON se.swarm_id = rf.swarm_id
    LEFT JOIN creatures c ON c.creature_id = se.anchor_creature_id
    WHERE rf.user_id = p_user_id
    ORDER BY se.starts_at DESC
    LIMIT p_limit;
$$ LANGUAGE sql STABLE;

-- Helper function: get followers of a rabble (for notification dispatch)
CREATE OR REPLACE FUNCTION get_rabble_followers(
    p_swarm_id UUID,
    p_event_type TEXT DEFAULT 'join'  -- 'join', 'start', or 'end'
)
RETURNS TABLE (
    user_id TEXT,
    follow_id UUID
) AS $$
    SELECT rf.user_id, rf.id AS follow_id
    FROM rabble_follows rf
    WHERE rf.swarm_id = p_swarm_id
      AND CASE p_event_type
            WHEN 'join' THEN rf.notify_on_join
            WHEN 'start' THEN rf.notify_on_start
            WHEN 'end' THEN rf.notify_on_end
            ELSE TRUE
          END;
$$ LANGUAGE sql STABLE;
