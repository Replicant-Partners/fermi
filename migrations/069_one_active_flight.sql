-- Migration 069: Enforce one active flight per creature
-- A creature can only be in ONE place at a time (one active flight OR one active rabble).

-- First, clean up stale flights older than 24h that were never ended
UPDATE creature_flights SET ended_at = NOW(), duration_seconds = 0
WHERE ended_at IS NULL
AND started_at < NOW() - INTERVAL '24 hours';

-- Partial unique index: only one row per creature where ended_at IS NULL
CREATE UNIQUE INDEX IF NOT EXISTS idx_one_active_flight_per_creature
ON creature_flights (creature_id)
WHERE ended_at IS NULL;
