-- Migration 088: Backfill creature_versions from creature_flights
-- This seeds the versioned state system with historical flight data
-- so the activity feed has content to display.

-- For each flight, create a 'fly' transition (start) and optionally a 'land' (end)
-- For flights with swarm_id, create a 'join' transition instead
-- Use ROW_NUMBER to assign sequential version_numbers per creature

INSERT INTO creature_versions (
    creature_id, version_number, state, previous_state,
    location_lat, location_lng, h3_cell, rabble_id,
    transition_type, triggered_by, valid_from, recorded_at, metadata
)
SELECT
    sub.creature_id,
    ROW_NUMBER() OVER (PARTITION BY sub.creature_id ORDER BY sub.ts) AS version_number,
    sub.state,
    sub.previous_state,
    sub.center_lat,
    sub.center_lng,
    sub.h3_cell,
    sub.swarm_id,
    sub.transition_type,
    sub.triggered_by,
    sub.ts,
    sub.ts,
    '{}'::jsonb
FROM (
    -- Flight start: fly or join
    SELECT
        cf.creature_id,
        CASE WHEN cf.swarm_id IS NOT NULL THEN 'in_rabble' ELSE 'fly' END AS state,
        'perched' AS previous_state,
        cf.center_lat,
        cf.center_lng,
        cf.h3_cell,
        cf.swarm_id,
        CASE WHEN cf.swarm_id IS NOT NULL THEN 'join' ELSE 'fly' END AS transition_type,
        cf.owner_id AS triggered_by,
        cf.started_at AS ts
    FROM creature_flights cf

    UNION ALL

    -- Flight end: land
    SELECT
        cf.creature_id,
        'perched' AS state,
        CASE WHEN cf.swarm_id IS NOT NULL THEN 'in_rabble' ELSE 'fly' END AS previous_state,
        cf.center_lat,
        cf.center_lng,
        cf.h3_cell,
        NULL AS swarm_id,
        'land' AS transition_type,
        cf.owner_id AS triggered_by,
        cf.ended_at AS ts
    FROM creature_flights cf
    WHERE cf.ended_at IS NOT NULL
) sub
ON CONFLICT (creature_id, version_number) DO NOTHING;

-- Seed creature_state for creatures that have flights but no state row yet
INSERT INTO creature_state (creature_id, version_id, state, rabble_id)
SELECT DISTINCT ON (cf.creature_id)
    cf.creature_id,
    cv.version_id,
    CASE
        WHEN cf.ended_at IS NULL AND cf.swarm_id IS NOT NULL THEN 'in_rabble'
        WHEN cf.ended_at IS NULL THEN 'fly'
        ELSE 'perched'
    END,
    CASE WHEN cf.ended_at IS NULL THEN cf.swarm_id ELSE NULL END
FROM creature_flights cf
JOIN creature_versions cv ON cv.creature_id = cf.creature_id
    AND cv.version_number = (
        SELECT MAX(cv2.version_number) FROM creature_versions cv2
        WHERE cv2.creature_id = cf.creature_id
    )
WHERE NOT EXISTS (
    SELECT 1 FROM creature_state cs WHERE cs.creature_id = cf.creature_id
)
ORDER BY cf.creature_id, cf.started_at DESC;
