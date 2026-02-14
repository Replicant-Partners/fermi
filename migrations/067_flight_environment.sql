-- Migration 067: Add environment JSONB to creature_flights
-- Stores per-waypoint planned conditions (wind, temperature, terrain, elevation)
-- from the flight_coordinator agent. Separate from path_samples (actual telemetry).

ALTER TABLE creature_flights ADD COLUMN IF NOT EXISTS environment JSONB;
