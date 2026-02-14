-- Migration 068: Add data_source to creature_flights
-- Distinguishes synthetic (simulation) from device (real GPS) telemetry.
-- Flights from device-paired creatures are tagged 'device'; all others default 'synthetic'.

ALTER TABLE creature_flights ADD COLUMN IF NOT EXISTS data_source TEXT NOT NULL DEFAULT 'synthetic';

-- Backfill: flights with a beacon_id are device-sourced
UPDATE creature_flights SET data_source = 'device' WHERE beacon_id IS NOT NULL;
