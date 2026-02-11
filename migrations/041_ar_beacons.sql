-- AR Beacons: spatial AR asset placement records
-- Uses H3 hexagonal grid cell IDs for location addressing

CREATE TABLE IF NOT EXISTS ar_beacons (
    beacon_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL,
    creator_id TEXT NOT NULL,
    agent_name TEXT NOT NULL DEFAULT 'ar_beacon',

    -- H3 location
    h3_cell TEXT NOT NULL,
    h3_resolution INT NOT NULL DEFAULT 12,
    center_lat DOUBLE PRECISION NOT NULL,
    center_lng DOUBLE PRECISION NOT NULL,

    -- Asset
    asset_path TEXT NOT NULL,
    asset_type TEXT NOT NULL DEFAULT 'image',

    -- Orientation
    azimuth_deg DOUBLE PRECISION NOT NULL DEFAULT 0,
    elevation_deg DOUBLE PRECISION NOT NULL DEFAULT 0,
    billboard BOOLEAN NOT NULL DEFAULT true,
    scale DOUBLE PRECISION NOT NULL DEFAULT 1.0,

    -- TTL
    ttl_seconds INT NOT NULL DEFAULT 86400,
    decay_style TEXT NOT NULL DEFAULT 'fade',
    expires_at TIMESTAMPTZ NOT NULL,

    -- Metadata
    visibility TEXT NOT NULL DEFAULT 'public',
    tags JSONB DEFAULT '[]',
    interaction JSONB DEFAULT '{}',
    metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ar_beacons_h3 ON ar_beacons(h3_cell);
CREATE INDEX IF NOT EXISTS idx_ar_beacons_workspace ON ar_beacons(workspace_id);
CREATE INDEX IF NOT EXISTS idx_ar_beacons_expires ON ar_beacons(expires_at);
CREATE INDEX IF NOT EXISTS idx_ar_beacons_creator ON ar_beacons(creator_id);

-- AR Choreographies: motion sequences attached to beacons
CREATE TABLE IF NOT EXISTS ar_choreographies (
    choreo_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    beacon_id UUID NOT NULL REFERENCES ar_beacons(beacon_id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL,
    name TEXT,
    description TEXT,
    motion JSONB NOT NULL,
    duration_total_ms INT,
    loop_motion BOOLEAN NOT NULL DEFAULT true,
    active BOOLEAN NOT NULL DEFAULT true,
    priority INT NOT NULL DEFAULT 1,
    triggers JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ar_choreo_beacon ON ar_choreographies(beacon_id);
CREATE INDEX IF NOT EXISTS idx_ar_choreo_workspace ON ar_choreographies(workspace_id);

-- AR Grid Maps: named spatial grids defined by ar_cartographer
CREATE TABLE IF NOT EXISTS ar_grid_maps (
    map_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL,
    creator_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,

    -- Center
    center_lat DOUBLE PRECISION NOT NULL,
    center_lng DOUBLE PRECISION NOT NULL,
    center_h3 TEXT NOT NULL,
    center_resolution INT NOT NULL DEFAULT 9,

    -- Grid config
    grid_resolution INT NOT NULL DEFAULT 12,
    radius_rings INT NOT NULL DEFAULT 5,
    total_cells INT NOT NULL DEFAULT 0,

    -- Named quadrants and zones
    quadrants JSONB NOT NULL DEFAULT '[]',
    zones JSONB NOT NULL DEFAULT '[]',

    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ar_grid_maps_workspace ON ar_grid_maps(workspace_id);
