-- Rabble.world: creature menagerie tables
-- Minted AR insect specimens with species data, collections, flight logs, and swarm events

CREATE TABLE IF NOT EXISTS creatures (
    creature_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id TEXT NOT NULL,
    workspace_id UUID,

    -- Species reference (GBIF)
    gbif_key BIGINT,
    scientific_name TEXT NOT NULL,
    common_name TEXT,
    species_group TEXT NOT NULL DEFAULT 'butterfly',
    taxonomy JSONB DEFAULT '{}',

    -- Specimen
    specimen_name TEXT,
    asset_path TEXT NOT NULL,
    flight_silhouette_path TEXT,
    variation_notes TEXT,
    generation_params JSONB DEFAULT '{}',

    -- Stats
    mint_number INT NOT NULL DEFAULT 1,
    total_flights INT NOT NULL DEFAULT 0,
    total_flight_time_seconds BIGINT NOT NULL DEFAULT 0,
    unique_locations INT NOT NULL DEFAULT 0,

    -- Data card
    data_card JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_creatures_owner ON creatures(owner_id);
CREATE INDEX IF NOT EXISTS idx_creatures_species ON creatures(scientific_name);
CREATE INDEX IF NOT EXISTS idx_creatures_group ON creatures(species_group);
CREATE INDEX IF NOT EXISTS idx_creatures_gbif ON creatures(gbif_key);

-- Flight log: every time a creature is flown at a location
CREATE TABLE IF NOT EXISTS creature_flights (
    flight_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    creature_id UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    beacon_id UUID REFERENCES ar_beacons(beacon_id) ON DELETE SET NULL,
    owner_id TEXT NOT NULL,

    -- Location
    h3_cell TEXT NOT NULL,
    h3_resolution INT NOT NULL DEFAULT 12,
    center_lat DOUBLE PRECISION NOT NULL,
    center_lng DOUBLE PRECISION NOT NULL,
    location_name TEXT,
    country_code TEXT,

    -- Flight details
    flight_pattern TEXT NOT NULL DEFAULT 'wander',
    choreo_id UUID,
    swarm_id UUID,

    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at TIMESTAMPTZ,
    duration_seconds INT
);

CREATE INDEX IF NOT EXISTS idx_creature_flights_creature ON creature_flights(creature_id);
CREATE INDEX IF NOT EXISTS idx_creature_flights_owner ON creature_flights(owner_id);
CREATE INDEX IF NOT EXISTS idx_creature_flights_h3 ON creature_flights(h3_cell);
CREATE INDEX IF NOT EXISTS idx_creature_flights_swarm ON creature_flights(swarm_id);

-- Swarm events: coordinated multi-user creature gatherings
CREATE TABLE IF NOT EXISTS swarm_events (
    swarm_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    creator_id TEXT NOT NULL,
    workspace_id UUID,

    -- Location
    h3_cell TEXT NOT NULL,
    h3_resolution INT NOT NULL DEFAULT 12,
    center_lat DOUBLE PRECISION NOT NULL,
    center_lng DOUBLE PRECISION NOT NULL,
    location_name TEXT,
    grid_map_id UUID REFERENCES ar_grid_maps(map_id) ON DELETE SET NULL,

    -- Event details
    name TEXT NOT NULL,
    description TEXT,
    species_filter TEXT,
    max_participants INT,

    -- Timing
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL DEFAULT 'scheduled',

    -- Stats
    participant_count INT NOT NULL DEFAULT 0,
    creature_count INT NOT NULL DEFAULT 0,

    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_swarm_events_h3 ON swarm_events(h3_cell);
CREATE INDEX IF NOT EXISTS idx_swarm_events_status ON swarm_events(status);
CREATE INDEX IF NOT EXISTS idx_swarm_events_time ON swarm_events(starts_at);

-- Collections: named groupings of creatures
CREATE TABLE IF NOT EXISTS creature_collections (
    collection_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    creature_ids JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_creature_collections_owner ON creature_collections(owner_id);

-- Add rabble-specific transaction types to the credit ledger constraint
-- (idempotent: these may already exist if the constraint was updated)
DO $$
BEGIN
    ALTER TABLE credit_ledger DROP CONSTRAINT IF EXISTS credit_ledger_tx_type_check;
    ALTER TABLE credit_ledger ADD CONSTRAINT credit_ledger_tx_type_check
        CHECK (tx_type IN (
            'deposit', 'withdrawal',
            'execution_fee', 'gas_fee',
            'education_alloc', 'education_spend',
            'transfer_out', 'transfer_in',
            'grant', 'refund',
            'fork_royalty', 'fork_fee',
            'publish_fee', 'eval_fee',
            'consolidation_fee',
            'marketplace_listing_fee', 'marketplace_match_purchase', 'marketplace_match_payout',
            'creature_mint', 'creature_flight', 'swarm_create', 'swarm_join',
            'gbif_contribution', 'rabble_platform_fee'
        ));
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'tx_type constraint update skipped: %', SQLERRM;
END $$;
