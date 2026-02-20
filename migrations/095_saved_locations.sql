-- Migration 095: Saved locations — favourite places, drop pins, creature waypoints
--
-- Users can save locations from the explore map (long-press to drop pin),
-- from rabble locations, or from creature waypoints. These appear as ⭐ pins
-- on the Environment tab map and can be used as targets for "Move Creature Here".
--
-- Source types:
--   'pin'                — user dropped a pin manually on the map
--   'rabble'             — saved from a rabble's location
--   'creature_waypoint'  — saved from a creature's flight path or perch point

CREATE TABLE IF NOT EXISTS saved_locations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    lat DOUBLE PRECISION NOT NULL,
    lng DOUBLE PRECISION NOT NULL,
    radius_meters INT NOT NULL DEFAULT 500,
    h3_cell TEXT,
    source TEXT NOT NULL DEFAULT 'pin',
    -- Optional reference to the rabble or creature that originated this save
    source_id UUID,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Foreign key to users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE constraint_name = 'saved_locations_user_id_fkey'
    ) THEN
        ALTER TABLE saved_locations
            ADD CONSTRAINT saved_locations_user_id_fkey
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- Index: "what locations has this user saved?"
CREATE INDEX IF NOT EXISTS idx_saved_locations_user_id
    ON saved_locations(user_id);

-- Index: spatial lookup by H3 cell (for nearby queries)
CREATE INDEX IF NOT EXISTS idx_saved_locations_h3_cell
    ON saved_locations(h3_cell)
    WHERE h3_cell IS NOT NULL;

-- Limit: max 200 saved locations per user (prevent abuse)
-- Enforced at application layer, not DB constraint.

-- Helper function: list saved locations for a user, ordered by most recent
CREATE OR REPLACE FUNCTION get_saved_locations(
    p_user_id TEXT,
    p_limit INT DEFAULT 100
)
RETURNS TABLE (
    id UUID,
    name TEXT,
    lat DOUBLE PRECISION,
    lng DOUBLE PRECISION,
    radius_meters INT,
    h3_cell TEXT,
    source TEXT,
    source_id UUID,
    notes TEXT,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ,
    -- If source is 'rabble', try to resolve rabble name
    rabble_name TEXT,
    rabble_status TEXT
) AS $$
    SELECT
        sl.id,
        sl.name,
        sl.lat,
        sl.lng,
        sl.radius_meters,
        sl.h3_cell,
        sl.source,
        sl.source_id,
        sl.notes,
        sl.created_at,
        sl.updated_at,
        se.name AS rabble_name,
        se.status AS rabble_status
    FROM saved_locations sl
    LEFT JOIN swarm_events se ON se.swarm_id = sl.source_id AND sl.source = 'rabble'
    WHERE sl.user_id = p_user_id
    ORDER BY sl.created_at DESC
    LIMIT p_limit;
$$ LANGUAGE sql STABLE;

-- Helper function: get nearby creatures (public or contact-visible)
-- Uses PostGIS ST_DWithin for spatial filtering.
-- Returns creatures within `p_radius_meters` of the given point,
-- excluding the requesting user's own creatures.
CREATE OR REPLACE FUNCTION get_nearby_creatures(
    p_user_id TEXT,
    p_lat DOUBLE PRECISION,
    p_lng DOUBLE PRECISION,
    p_radius_meters INT DEFAULT 1000,
    p_limit INT DEFAULT 50
)
RETURNS TABLE (
    creature_id UUID,
    owner_id TEXT,
    specimen_name TEXT,
    scientific_name TEXT,
    species_group TEXT,
    asset_path TEXT,
    creature_state TEXT,
    rabble_id UUID,
    rabble_name TEXT,
    location_lat DOUBLE PRECISION,
    location_lng DOUBLE PRECISION,
    distance_meters DOUBLE PRECISION,
    is_contact BOOLEAN
) AS $$
    SELECT
        c.creature_id,
        c.owner_id,
        c.specimen_name,
        c.scientific_name,
        c.species_group,
        c.asset_path,
        cs.state AS creature_state,
        cf_active.swarm_id AS rabble_id,
        se.name AS rabble_name,
        cf_active.center_lat AS location_lat,
        cf_active.center_lng AS location_lng,
        ST_Distance(
            ST_SetSRID(ST_MakePoint(p_lng, p_lat), 4326)::geography,
            ST_SetSRID(ST_MakePoint(cf_active.center_lng, cf_active.center_lat), 4326)::geography
        ) AS distance_meters,
        EXISTS(
            SELECT 1 FROM contacts ct
            WHERE ct.user_id = p_user_id AND ct.contact_id = c.owner_id
        ) AS is_contact
    FROM creatures c
    JOIN creature_flights cf_active
        ON cf_active.creature_id = c.creature_id
        AND cf_active.ended_at IS NULL
    LEFT JOIN creature_state cs ON cs.creature_id = c.creature_id
    LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
    LEFT JOIN swarm_events se ON se.swarm_id = cf_active.swarm_id
    WHERE c.owner_id != p_user_id
      AND c.status = 'active'
      -- Visibility filter: public, or contacts-only if user is a contact
      AND (
          COALESCE(cc.visibility, 'public') = 'public'
          OR (
              COALESCE(cc.visibility, 'public') = 'contacts'
              AND EXISTS(
                  SELECT 1 FROM contacts ct
                  WHERE ct.user_id = p_user_id AND ct.contact_id = c.owner_id
              )
          )
      )
      -- Spatial filter: within radius
      AND cf_active.center_lat IS NOT NULL
      AND cf_active.center_lng IS NOT NULL
      AND ST_DWithin(
          ST_SetSRID(ST_MakePoint(p_lng, p_lat), 4326)::geography,
          ST_SetSRID(ST_MakePoint(cf_active.center_lng, cf_active.center_lat), 4326)::geography,
          p_radius_meters
      )
    ORDER BY distance_meters ASC
    LIMIT p_limit;
$$ LANGUAGE sql STABLE;
