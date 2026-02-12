-- Creature device pairing: GPS trackers, smart tags, BLE beacons
CREATE TABLE IF NOT EXISTS creature_devices (
    device_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    creature_id UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    owner_id TEXT NOT NULL,
    device_type TEXT NOT NULL,
    device_identifier TEXT NOT NULL,
    device_name TEXT,
    is_active BOOLEAN NOT NULL DEFAULT true,
    last_lat DOUBLE PRECISION,
    last_lng DOUBLE PRECISION,
    last_seen_at TIMESTAMPTZ,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(owner_id, device_identifier)
);
CREATE INDEX IF NOT EXISTS idx_creature_devices_creature ON creature_devices(creature_id);
CREATE INDEX IF NOT EXISTS idx_creature_devices_owner ON creature_devices(owner_id);
