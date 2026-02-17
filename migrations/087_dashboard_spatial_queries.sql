-- Migration 087: Dashboard Spatial Queries
--
-- Creates functions and indexes for dashboard spatial queries.
-- Supports: creature deployment tracking, boundary violation detection,
-- nearby rabble discovery, and spatial presence indicators.
--
-- IMPORTANT: No BEGIN/COMMIT — PgBouncer transaction mode.

-- ═══════════════════════════════════════════════════════════════════════════
-- 1. SPATIAL INDEXES
-- ═══════════════════════════════════════════════════════════════════════════

-- Index on creature locations for spatial queries
CREATE INDEX IF NOT EXISTS idx_creature_state_location
  ON creature_state(location_lat, location_lng)
  WHERE location_lat IS NOT NULL AND location_lng IS NOT NULL;

-- Index on rabble centers for spatial queries
CREATE INDEX IF NOT EXISTS idx_swarm_events_location
  ON swarm_events(center_lat, center_lng);

-- Index on rabble status for filtering active rabbles
CREATE INDEX IF NOT EXISTS idx_swarm_events_status
  ON swarm_events(status)
  WHERE status IN ('active', 'pending');

-- ═══════════════════════════════════════════════════════════════════════════
-- 2. GET MY RABBLES WITH STATUS
-- ═══════════════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION get_my_rabbles_with_status(
  p_user_id TEXT,
  p_limit INTEGER DEFAULT 50
) RETURNS TABLE (
  swarm_id UUID,
  name TEXT,
  location_name TEXT,
  center_lat DOUBLE PRECISION,
  center_lng DOUBLE PRECISION,
  radius_meters INTEGER,
  creature_count INTEGER,
  participant_count INTEGER,
  starts_at TIMESTAMPTZ,
  ends_at TIMESTAMPTZ,
  status TEXT,
  anchor_creature_id UUID,
  anchor_creature_name TEXT,
  anchor_creature_image TEXT,
  my_creatures JSONB
) AS $$
BEGIN
  RETURN QUERY
  SELECT
    s.swarm_id,
    s.name,
    s.location_name,
    s.center_lat,
    s.center_lng,
    s.radius_meters,
    s.creature_count,
    s.participant_count,
    s.starts_at,
    s.ends_at,
    s.status,
    s.anchor_creature_id,
    c.specimen_name AS anchor_creature_name,
    c.asset_path AS anchor_creature_image,
    (
      SELECT jsonb_agg(jsonb_build_object(
        'creature_id', cs.creature_id,
        'specimen_name', c2.specimen_name,
        'scientific_name', c2.scientific_name,
        'location_lat', cs.location_lat,
        'location_lng', cs.location_lng,
        'distance_meters',
          CASE
            WHEN cs.location_lat IS NOT NULL AND cs.location_lng IS NOT NULL
            THEN ST_Distance(
              ST_MakePoint(cs.location_lng, cs.location_lat)::geography,
              ST_MakePoint(s.center_lng, s.center_lat)::geography
            )
            ELSE NULL
          END,
        'in_area',
          CASE
            WHEN cs.location_lat IS NOT NULL AND cs.location_lng IS NOT NULL
            THEN ST_DWithin(
              ST_MakePoint(cs.location_lng, cs.location_lat)::geography,
              ST_MakePoint(s.center_lng, s.center_lat)::geography,
              s.radius_meters
            )
            ELSE NULL
          END
      ))
      FROM creature_state cs
      JOIN creatures c2 ON c2.creature_id = cs.creature_id
      WHERE cs.rabble_id = s.swarm_id
        AND c2.owner_id = p_user_id
    ) AS my_creatures
  FROM swarm_events s
  LEFT JOIN creatures c ON c.creature_id = s.anchor_creature_id
  WHERE s.status IN ('active', 'pending')
    AND EXISTS (
      SELECT 1
      FROM creature_state cs
      JOIN creatures c2 ON c2.creature_id = cs.creature_id
      WHERE cs.rabble_id = s.swarm_id
        AND c2.owner_id = p_user_id
    )
  ORDER BY s.starts_at DESC
  LIMIT p_limit;
END;
$$ LANGUAGE plpgsql;

-- ═══════════════════════════════════════════════════════════════════════════
-- 3. GET NEARBY RABBLES
-- ═══════════════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION get_nearby_rabbles(
  p_user_lat DOUBLE PRECISION,
  p_user_lng DOUBLE PRECISION,
  p_radius_meters INTEGER DEFAULT 1000,
  p_limit INTEGER DEFAULT 50
) RETURNS TABLE (
  swarm_id UUID,
  name TEXT,
  location_name TEXT,
  center_lat DOUBLE PRECISION,
  center_lng DOUBLE PRECISION,
  radius_meters INTEGER,
  creature_count INTEGER,
  participant_count INTEGER,
  starts_at TIMESTAMPTZ,
  ends_at TIMESTAMPTZ,
  status TEXT,
  anchor_creature_id UUID,
  anchor_creature_name TEXT,
  anchor_creature_image TEXT,
  distance_meters DOUBLE PRECISION,
  user_in_area BOOLEAN
) AS $$
BEGIN
  RETURN QUERY
  SELECT
    s.swarm_id,
    s.name,
    s.location_name,
    s.center_lat,
    s.center_lng,
    s.radius_meters,
    s.creature_count,
    s.participant_count,
    s.starts_at,
    s.ends_at,
    s.status,
    s.anchor_creature_id,
    c.specimen_name AS anchor_creature_name,
    c.asset_path AS anchor_creature_image,
    ST_Distance(
      ST_MakePoint(p_user_lng, p_user_lat)::geography,
      ST_MakePoint(s.center_lng, s.center_lat)::geography
    ) AS distance_meters,
    ST_DWithin(
      ST_MakePoint(p_user_lng, p_user_lat)::geography,
      ST_MakePoint(s.center_lng, s.center_lat)::geography,
      s.radius_meters
    ) AS user_in_area
  FROM swarm_events s
  LEFT JOIN creatures c ON c.creature_id = s.anchor_creature_id
  WHERE s.status IN ('active', 'pending')
    AND ST_DWithin(
      ST_MakePoint(p_user_lng, p_user_lat)::geography,
      ST_MakePoint(s.center_lng, s.center_lat)::geography,
      p_radius_meters
    )
  ORDER BY distance_meters ASC
  LIMIT p_limit;
END;
$$ LANGUAGE plpgsql;

-- ═══════════════════════════════════════════════════════════════════════════
-- 4. GET CREATURES WITH DEPLOYMENT
-- ═══════════════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION get_creatures_with_deployment(
  p_user_id TEXT,
  p_status TEXT DEFAULT 'active',
  p_limit INTEGER DEFAULT 200
) RETURNS TABLE (
  creature_id UUID,
  specimen_name TEXT,
  scientific_name TEXT,
  species_group TEXT,
  asset_path TEXT,
  rabble_id UUID,
  rabble_name TEXT,
  location_lat DOUBLE PRECISION,
  location_lng DOUBLE PRECISION,
  h3_cell TEXT,
  state TEXT,
  presence TEXT,
  distance_from_rabble_center DOUBLE PRECISION,
  in_rabble_area BOOLEAN
) AS $$
BEGIN
  RETURN QUERY
  SELECT
    c.creature_id,
    c.specimen_name,
    c.scientific_name,
    c.species_group,
    c.asset_path,
    cs.rabble_id,
    s.name AS rabble_name,
    cs.location_lat,
    cs.location_lng,
    cs.h3_cell,
    cs.state,
    cc.presence,
    CASE
      WHEN cs.rabble_id IS NOT NULL
        AND cs.location_lat IS NOT NULL
        AND cs.location_lng IS NOT NULL
      THEN ST_Distance(
        ST_MakePoint(cs.location_lng, cs.location_lat)::geography,
        ST_MakePoint(s.center_lng, s.center_lat)::geography
      )
      ELSE NULL
    END AS distance_from_rabble_center,
    CASE
      WHEN cs.rabble_id IS NOT NULL
        AND cs.location_lat IS NOT NULL
        AND cs.location_lng IS NOT NULL
      THEN ST_DWithin(
        ST_MakePoint(cs.location_lng, cs.location_lat)::geography,
        ST_MakePoint(s.center_lng, s.center_lat)::geography,
        s.radius_meters
      )
      ELSE NULL
    END AS in_rabble_area
  FROM creatures c
  LEFT JOIN creature_state cs ON cs.creature_id = c.creature_id
  LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
  LEFT JOIN swarm_events s ON s.swarm_id = cs.rabble_id
  WHERE c.owner_id = p_user_id
    AND c.status = p_status
  ORDER BY
    cs.rabble_id NULLS LAST,
    c.specimen_name
  LIMIT p_limit;
END;
$$ LANGUAGE plpgsql;

-- ═══════════════════════════════════════════════════════════════════════════
-- 5. CHECK BOUNDARY VIOLATIONS
-- ═══════════════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION check_boundary_violations(
  p_user_id TEXT
) RETURNS TABLE (
  creature_id UUID,
  specimen_name TEXT,
  rabble_id UUID,
  rabble_name TEXT,
  distance_meters DOUBLE PRECISION,
  rabble_radius INTEGER
) AS $$
BEGIN
  RETURN QUERY
  SELECT
    c.creature_id,
    c.specimen_name,
    cs.rabble_id,
    s.name AS rabble_name,
    ST_Distance(
      ST_MakePoint(cs.location_lng, cs.location_lat)::geography,
      ST_MakePoint(s.center_lng, s.center_lat)::geography
    ) AS distance_meters,
    s.radius_meters AS rabble_radius
  FROM creatures c
  JOIN creature_state cs ON cs.creature_id = c.creature_id
  JOIN swarm_events s ON s.swarm_id = cs.rabble_id
  WHERE c.owner_id = p_user_id
    AND cs.location_lat IS NOT NULL
    AND cs.location_lng IS NOT NULL
    AND NOT ST_DWithin(
      ST_MakePoint(cs.location_lng, cs.location_lat)::geography,
      ST_MakePoint(s.center_lng, s.center_lat)::geography,
      s.radius_meters
    );
END;
$$ LANGUAGE plpgsql;

-- ═══════════════════════════════════════════════════════════════════════════
-- 6. VERIFICATION
-- ═══════════════════════════════════════════════════════════════════════════

-- Verify PostGIS is available
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'postgis') THEN
    RAISE EXCEPTION 'PostGIS extension not found. Please install PostGIS.';
  END IF;
END $$;

-- Log migration completion
INSERT INTO migrations_log (migration_id, applied_at, description)
VALUES (
  '087',
  NOW(),
  'Dashboard spatial queries: functions and indexes for creature deployment tracking'
)
ON CONFLICT (migration_id) DO NOTHING;

