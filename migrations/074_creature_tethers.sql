-- Migration 074: Creature tethering — link creatures to live signal sources
-- A tethered creature tracks automatically instead of flying simulated routes.
-- Tether types: phone_gps, meshtastic, gps_tracker, fixed_sensor

CREATE TABLE IF NOT EXISTS creature_tethers (
    tether_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    creature_id UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    owner_id TEXT NOT NULL,
    tether_type TEXT NOT NULL DEFAULT 'phone_gps',
    device_label TEXT,
    config JSONB DEFAULT '{}',
    active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deactivated_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_creature_tethers_creature ON creature_tethers(creature_id);
CREATE INDEX IF NOT EXISTS idx_creature_tethers_active ON creature_tethers(creature_id, active) WHERE active = true;

-- Telemetry points: timestamped position stream from tethered creatures
CREATE TABLE IF NOT EXISTS telemetry_points (
    point_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tether_id UUID NOT NULL REFERENCES creature_tethers(tether_id) ON DELETE CASCADE,
    creature_id UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    lat DOUBLE PRECISION NOT NULL,
    lng DOUBLE PRECISION NOT NULL,
    altitude DOUBLE PRECISION,
    accuracy DOUBLE PRECISION,
    speed DOUBLE PRECISION,
    heading DOUBLE PRECISION,
    metadata JSONB DEFAULT '{}',
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_telemetry_creature ON telemetry_points(creature_id, recorded_at DESC);
CREATE INDEX IF NOT EXISTS idx_telemetry_tether ON telemetry_points(tether_id, recorded_at DESC);

-- Add tether tx_type
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
        'flight_plan',
        'perch', 'fly', 'walk_in_fee', 'walk_in_revenue',
        'tether'
    ));
