-- Migration 070: Clean up stale active flights
-- The one-active-flight constraint (069) only cleaned up flights > 24h old.
-- This catches ALL remaining un-ended flights older than 2 hours.
-- The unique index prevents new duplicates; this cleans up legacy ones.

-- End all active flights older than 2 hours (generous window)
UPDATE creature_flights SET ended_at = NOW(), duration_seconds = 0
WHERE ended_at IS NULL
AND started_at < NOW() - INTERVAL '2 hours';

-- For creatures with MULTIPLE active flights (legacy duplicates),
-- keep only the most recent one and end the rest.
UPDATE creature_flights SET ended_at = NOW(), duration_seconds = 0
WHERE flight_id IN (
    SELECT flight_id FROM (
        SELECT flight_id, creature_id,
               ROW_NUMBER() OVER (PARTITION BY creature_id ORDER BY started_at DESC) as rn
        FROM creature_flights
        WHERE ended_at IS NULL
    ) ranked
    WHERE rn > 1
);
