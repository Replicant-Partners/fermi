-- Migration 078: Creature Versioned State Model
--
-- Introduces the clean data model from docs/architecture/CREATURE_DATA_MODEL.md:
--   creature_state      — current state pointer (one mutable row per creature)
--   creature_conditions  — social attributes the owner controls
--   creature_versions    — immutable version history (every state transition)
--   flight_telemetry     — observations during FLY state (replaces path_samples)
--
-- Phase 1: Create tables and backfill from existing data.
-- Old tables (creatures columns, creature_flights) remain for dual-write period.
--
-- IMPORTANT: No BEGIN/COMMIT — PgBouncer transaction mode.
-- Multi-statement wrapped in DO blocks where atomicity needed.

-- ═══════════════════════════════════════════════════════════════
-- 1. creature_state — current state pointer
-- ═══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS creature_state (
    creature_id       UUID PRIMARY KEY REFERENCES creatures(creature_id) ON DELETE CASCADE,
    state             TEXT NOT NULL DEFAULT 'perch_solo'
                      CHECK (state IN ('perch_solo', 'fly', 'perch_rabble')),
    location_lat      DOUBLE PRECISION,
    location_lng      DOUBLE PRECISION,
    h3_cell           TEXT,
    rabble_id         UUID REFERENCES swarm_events(swarm_id) ON DELETE SET NULL,
    workspace_id      UUID,
    version_id        UUID,          -- FK added after creature_versions exists
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_cs_state ON creature_state(state);
CREATE INDEX IF NOT EXISTS idx_cs_rabble ON creature_state(rabble_id) WHERE rabble_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_cs_h3 ON creature_state(h3_cell) WHERE h3_cell IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════
-- 2. creature_conditions — social attributes (owner-defined)
-- ═══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS creature_conditions (
    creature_id       UUID PRIMARY KEY REFERENCES creatures(creature_id) ON DELETE CASCADE,
    visibility        TEXT NOT NULL DEFAULT 'public'
                      CHECK (visibility IN ('public', 'contacts_only', 'private')),
    walk_in_price     INTEGER,        -- NULL=private, 0=free, N=cover charge
    sosa_opt_in       BOOLEAN NOT NULL DEFAULT false,
    active_modules    TEXT[] NOT NULL DEFAULT '{}',
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ═══════════════════════════════════════════════════════════════
-- 3. creature_versions — immutable state transition history
-- ═══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS creature_versions (
    version_id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    creature_id       UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    version_number    INTEGER NOT NULL,

    -- State after this transition
    state             TEXT NOT NULL CHECK (state IN ('perch_solo', 'fly', 'perch_rabble')),
    previous_state    TEXT,

    -- Location at this version
    location_lat      DOUBLE PRECISION,
    location_lng      DOUBLE PRECISION,
    h3_cell           TEXT,
    rabble_id         UUID,

    -- Transition metadata
    transition_type   TEXT NOT NULL,   -- perch, fly, land, join, leave
    triggered_by      TEXT NOT NULL,   -- user_id

    -- Agent work product
    episode_ids       UUID[],
    workspace_id      UUID,

    -- Bitemporal
    valid_from        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    recorded_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Immutable enrichment
    metadata          JSONB DEFAULT '{}',

    UNIQUE(creature_id, version_number)
);

CREATE INDEX IF NOT EXISTS idx_cv_creature_version ON creature_versions(creature_id, version_number DESC);
CREATE INDEX IF NOT EXISTS idx_cv_creature_valid ON creature_versions(creature_id, valid_from DESC);
CREATE INDEX IF NOT EXISTS idx_cv_state ON creature_versions(state);
CREATE INDEX IF NOT EXISTS idx_cv_rabble ON creature_versions(rabble_id) WHERE rabble_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_cv_transition ON creature_versions(transition_type);

-- Now add FK from creature_state to creature_versions
DO $$ BEGIN
    ALTER TABLE creature_state
        ADD CONSTRAINT fk_cs_version
        FOREIGN KEY (version_id) REFERENCES creature_versions(version_id);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- ═══════════════════════════════════════════════════════════════
-- 4. flight_telemetry — observations during FLY state
-- ═══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS flight_telemetry (
    telemetry_id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    version_id        UUID NOT NULL REFERENCES creature_versions(version_id) ON DELETE CASCADE,
    creature_id       UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,

    -- Position
    lat               DOUBLE PRECISION NOT NULL,
    lng               DOUBLE PRECISION NOT NULL,
    altitude_m        DOUBLE PRECISION,
    heading           DOUBLE PRECISION,

    -- Source
    data_source       TEXT NOT NULL DEFAULT 'app',
    device_id         UUID,

    -- Temporal
    observed_at       TIMESTAMPTZ NOT NULL,
    recorded_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ft_version ON flight_telemetry(version_id, observed_at);
CREATE INDEX IF NOT EXISTS idx_ft_creature ON flight_telemetry(creature_id, observed_at DESC);

-- ═══════════════════════════════════════════════════════════════
-- 5. Backfill creature_state from current data
-- ═══════════════════════════════════════════════════════════════

-- Derive current state from creature_flights:
--   - active flight with swarm_id → perch_rabble
--   - active flight without swarm_id, pattern='perch' → perch_solo
--   - active flight without swarm_id, pattern IN ('fly','solo','wander') → fly
--   - no active flight → perch_solo (idle)

INSERT INTO creature_state (creature_id, state, location_lat, location_lng, h3_cell, rabble_id, workspace_id, updated_at)
SELECT
    c.creature_id,
    CASE
        WHEN af.flight_id IS NOT NULL AND af.swarm_id IS NOT NULL THEN 'perch_rabble'
        WHEN af.flight_id IS NOT NULL AND af.flight_pattern IN ('fly', 'solo', 'wander') THEN 'fly'
        ELSE 'perch_solo'
    END AS state,
    COALESCE(af.center_lat, lf.center_lat) AS location_lat,
    COALESCE(af.center_lng, lf.center_lng) AS location_lng,
    COALESCE(af.h3_cell, lf.h3_cell) AS h3_cell,
    af.swarm_id AS rabble_id,
    c.workspace_id,
    NOW()
FROM creatures c
-- Active flight (if any)
LEFT JOIN LATERAL (
    SELECT flight_id, center_lat, center_lng, h3_cell, swarm_id, flight_pattern
    FROM creature_flights
    WHERE creature_id = c.creature_id AND ended_at IS NULL
    ORDER BY started_at DESC LIMIT 1
) af ON true
-- Last ended flight (for location if no active flight)
LEFT JOIN LATERAL (
    SELECT center_lat, center_lng, h3_cell
    FROM creature_flights
    WHERE creature_id = c.creature_id AND ended_at IS NOT NULL
    ORDER BY ended_at DESC LIMIT 1
) lf ON true
WHERE c.owner_id IS NOT NULL   -- skip system seed creatures
ON CONFLICT (creature_id) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════
-- 6. Backfill creature_conditions from creatures columns
-- ═══════════════════════════════════════════════════════════════

-- Guarded 2026-08-18. This one-time backfill reads three `creatures` columns
-- that migration 080 drops once the data has moved. On every boot after that it
-- re-ran and failed with `column c.visibility does not exist` — harmless in
-- effect, since the rows it would insert already exist, but it meant this file
-- reported as failing forever and the noise hid the real problem next door: the
-- staging columns were being re-created every boot and permanently consuming
-- slots on a table with a hard 1600-column ceiling. See migration 058.
--
-- Runs only while the staging columns are present, which is exactly when there
-- is anything to copy. PL/pgSQL resolves column references at statement
-- execution rather than block creation, so the guarded branch does not need the
-- columns to exist in order to parse.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'creatures'
           AND column_name = 'visibility'
    ) THEN
        INSERT INTO creature_conditions (creature_id, visibility, sosa_opt_in, active_modules, updated_at)
        SELECT
            c.creature_id,
            COALESCE(c.visibility, 'public'),
            COALESCE(c.sosa_opt_in, false),
            CASE
                WHEN c.presence = 'tracking' THEN ARRAY['tether']
                ELSE ARRAY[]::text[]
            END,
            NOW()
        FROM creatures c
        WHERE c.owner_id IS NOT NULL
        ON CONFLICT (creature_id) DO NOTHING;
    END IF;
END $$;

-- ═══════════════════════════════════════════════════════════════
-- 7. Backfill creature_versions from creature_flights history
-- ═══════════════════════════════════════════════════════════════

-- Each historical flight becomes two versions: a FLY and a LAND (if ended).
-- The initial perch is version 1 for each creature (derived from first flight).

-- Version 1: initial perch for each creature (from their first flight's location
-- or from their creation if they never flew)
INSERT INTO creature_versions (
    version_id, creature_id, version_number, state, previous_state,
    location_lat, location_lng, h3_cell,
    transition_type, triggered_by, workspace_id,
    valid_from, recorded_at, metadata
)
SELECT
    gen_random_uuid(),
    c.creature_id,
    1,
    'perch_solo',
    NULL,
    COALESCE(ff.center_lat, 0),
    COALESCE(ff.center_lng, 0),
    COALESCE(ff.h3_cell, ''),
    'perch',
    c.owner_id,
    c.workspace_id,
    c.created_at,
    c.created_at,
    '{}'::jsonb
FROM creatures c
LEFT JOIN LATERAL (
    SELECT center_lat, center_lng, h3_cell
    FROM creature_flights
    WHERE creature_id = c.creature_id
    ORDER BY started_at ASC LIMIT 1
) ff ON true
WHERE c.owner_id IS NOT NULL
ON CONFLICT (creature_id, version_number) DO NOTHING;

-- Subsequent versions from flight history.
-- Each flight start → FLY version; each flight end → LAND (or JOIN) version.
-- CTE builds event stream, ROW_NUMBER assigns version_number starting at 2.

INSERT INTO creature_versions (
    version_id, creature_id, version_number, state, previous_state,
    location_lat, location_lng, h3_cell, rabble_id,
    transition_type, triggered_by, workspace_id,
    valid_from, recorded_at, metadata
)
SELECT
    gen_random_uuid(),
    ev.creature_id,
    1 + ROW_NUMBER() OVER (PARTITION BY ev.creature_id ORDER BY ev.event_time, ev.event_order),
    ev.state,
    ev.previous_state,
    ev.lat, ev.lng, ev.h3_cell, ev.rabble_id,
    ev.transition_type, ev.triggered_by, NULL,
    ev.event_time, ev.event_time, ev.metadata
FROM (
    -- Flight start → FLY
    SELECT
        cf.creature_id,
        cf.started_at AS event_time,
        1 AS event_order,   -- start before end if same timestamp
        'fly' AS state,
        'perch_solo' AS previous_state,
        cf.center_lat AS lat, cf.center_lng AS lng, cf.h3_cell,
        NULL::uuid AS rabble_id,
        'fly' AS transition_type,
        cf.owner_id AS triggered_by,
        jsonb_build_object('flight_id', cf.flight_id, 'flight_pattern', cf.flight_pattern) AS metadata
    FROM creature_flights cf
    JOIN creatures c ON c.creature_id = cf.creature_id AND c.owner_id IS NOT NULL

    UNION ALL

    -- Flight end → LAND or JOIN
    SELECT
        cf.creature_id,
        cf.ended_at AS event_time,
        2 AS event_order,
        CASE WHEN cf.swarm_id IS NOT NULL THEN 'perch_rabble' ELSE 'perch_solo' END,
        'fly',
        cf.center_lat, cf.center_lng, cf.h3_cell,
        cf.swarm_id,
        CASE WHEN cf.swarm_id IS NOT NULL THEN 'join' ELSE 'land' END,
        cf.owner_id,
        jsonb_build_object('flight_id', cf.flight_id, 'duration_seconds', cf.duration_seconds)
    FROM creature_flights cf
    JOIN creatures c ON c.creature_id = cf.creature_id AND c.owner_id IS NOT NULL
    WHERE cf.ended_at IS NOT NULL
) ev
ON CONFLICT (creature_id, version_number) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════
-- 8. Link creature_state.version_id to latest version
-- ═══════════════════════════════════════════════════════════════

UPDATE creature_state cs
SET version_id = lv.version_id
FROM (
    SELECT DISTINCT ON (creature_id) creature_id, version_id
    FROM creature_versions
    ORDER BY creature_id, version_number DESC
) lv
WHERE cs.creature_id = lv.creature_id;
