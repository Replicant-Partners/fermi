-- Migration 052: Universal sensor observations (W3C SSN/SOSA)
-- Domain-agnostic telemetry ingestion: any sensor, any property, one table.
-- See: https://www.w3.org/TR/vocab-ssn/

-- Platforms host sensors (drone, weather station, greenhouse, vehicle, wearable)
CREATE TABLE IF NOT EXISTS sosa_platforms (
    platform_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id TEXT NOT NULL,
    name TEXT NOT NULL,
    platform_type TEXT NOT NULL,
    description TEXT,
    location JSONB DEFAULT '{}',
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Sensors belong to platforms and observe properties
CREATE TABLE IF NOT EXISTS sosa_sensors (
    sensor_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    platform_id UUID NOT NULL REFERENCES sosa_platforms(platform_id),
    name TEXT NOT NULL,
    observable_property TEXT NOT NULL,
    unit TEXT,
    description TEXT,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Observation sessions (collection windows, like swarm_sessions but universal)
CREATE TABLE IF NOT EXISTS observation_sessions (
    session_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id TEXT NOT NULL,
    platform_id UUID NOT NULL REFERENCES sosa_platforms(platform_id),
    name TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    started_at TIMESTAMPTZ DEFAULT NOW(),
    ended_at TIMESTAMPTZ,
    metadata JSONB DEFAULT '{}'
);

-- Individual observations (append-only, high-frequency)
-- Each row = one sosa:Observation
CREATE TABLE IF NOT EXISTS sosa_observations (
    observation_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES observation_sessions(session_id),
    sensor_id UUID REFERENCES sosa_sensors(sensor_id),
    platform_id UUID NOT NULL,
    observable_property TEXT NOT NULL,
    feature_of_interest TEXT,
    result_value DOUBLE PRECISION NOT NULL,
    result_unit TEXT,
    phenomenon_time BIGINT NOT NULL,
    result_time BIGINT,
    procedure TEXT,
    extra JSONB DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_sosa_obs_session ON sosa_observations(session_id);
CREATE INDEX IF NOT EXISTS idx_sosa_obs_session_time ON sosa_observations(session_id, phenomenon_time);
CREATE INDEX IF NOT EXISTS idx_sosa_obs_property ON sosa_observations(session_id, observable_property);
CREATE INDEX IF NOT EXISTS idx_sosa_obs_sensor ON sosa_observations(sensor_id);
CREATE INDEX IF NOT EXISTS idx_sosa_obs_platform ON sosa_observations(platform_id);
CREATE INDEX IF NOT EXISTS idx_sosa_platforms_owner ON sosa_platforms(owner_id);
CREATE INDEX IF NOT EXISTS idx_sosa_sensors_platform ON sosa_sensors(platform_id);
CREATE INDEX IF NOT EXISTS idx_obs_sessions_owner ON observation_sessions(owner_id);
CREATE INDEX IF NOT EXISTS idx_obs_sessions_platform ON observation_sessions(platform_id);

-- Opt-in flag for Rabble creatures: owner must explicitly enable SOSA telemetry sharing
-- Defaults to false — respects AKP consent model (agent_interaction_policies roadmap)
ALTER TABLE creatures ADD COLUMN IF NOT EXISTS sosa_opt_in BOOLEAN NOT NULL DEFAULT false;

-- Add new tx_types
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
        'avatar_generate', 'embedding_import',
        'ontology_generation', 'prompt_generation', 'file_write',
        'creature_mint', 'creature_flight', 'swarm_create', 'swarm_join',
        'collection_create', 'rabble_chat',
        'gbif_contribution', 'rabble_platform_fee',
        'akp_alignment', 'akp_transfer', 'akp_bootstrap', 'akp_diff',
        'swarm_session_create', 'swarm_telemetry_ingest',
        'observation_session_create', 'observation_ingest'
    ));
