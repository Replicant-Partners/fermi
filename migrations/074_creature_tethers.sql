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

-- NOTE: tx_type constraint removed in migration 076. No constraint update needed here.
