-- Migration 141: SimOps Process Benchmark Infrastructure
--
-- The SimOps benchmark is structurally different from the Fermi benchmark:
-- - Ground truth is a continuous physical measurement (not a binary event)
-- - Resolution is triggered by ANY real sensor reading (not a calendar date)
-- - The thing being validated is a model's predicted trajectory vs reality
-- - Process output (yield, carbon, cost) is what matters, not sensor calibration
--
-- Two tables:
--
-- 1. process_projection_commits  — commitment anchor for synthetic predictions.
--    Written the moment a simulation produces a predicted value, before any
--    real measurement arrives. This is the "immutable clock" for SimOps.
--
-- 2. process_spacetime           — one row per (projection × resolution_event).
--    A resolution event is either:
--      a) a real sensor reading that matches the prediction (any time)
--      b) a scheduled sample point check (defined interval per process)
--    Both modes write here with a resolution_mode flag.
--    The trajectory of predictions for a process run lives here.
--
-- Resolution modes:
--   'anomaly_delta' — a real reading arrived AND |predicted-actual|/|actual| > threshold
--   'sample_point'  — a real reading arrived at a scheduled check interval
--   'any_reading'   — a real reading arrived (regardless of delta or schedule)
--
-- The anomaly_delta and sample_point modes are the two you specified.
-- 'any_reading' is logged but not flagged — it gives you the full trajectory.

-- ── 1. Process projection commits ─────────────────────────────────────────
-- One row per synthetic observation write.
-- commitment_hash = sha256(observation_id || predicted_value || model_uri || phenomenon_time)

CREATE TABLE IF NOT EXISTS process_projection_commits (
    commit_id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The synthetic SOSA observation being committed
    sosa_observation_id     UUID NOT NULL,
    projection_id           TEXT,           -- from extra->>'projection_id'
    workspace_id            UUID,           -- the SimOps workspace this belongs to
    session_id              UUID,           -- observation session

    -- What was predicted
    observable_property     TEXT NOT NULL,
    feature_of_interest     TEXT,
    predicted_value         DOUBLE PRECISION NOT NULL,
    model_uri               TEXT,           -- kask:dynamics/kombucha_fermentation@v1
    stage_id                TEXT,

    -- The anchor
    commitment_hash         TEXT NOT NULL UNIQUE,
    committed_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    phenomenon_time_ms      BIGINT,         -- the simulated time this prediction covers

    -- Process context snapshot (what conditions produced this prediction)
    process_context         JSONB,          -- temperature_c, n_instances, step_size, etc.
    harness_snapshot_id     UUID REFERENCES harness_snapshots(snapshot_id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_ppc_projection_id
    ON process_projection_commits(projection_id)
    WHERE projection_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ppc_workspace
    ON process_projection_commits(workspace_id, committed_at DESC);
CREATE INDEX IF NOT EXISTS idx_ppc_property
    ON process_projection_commits(observable_property, feature_of_interest, committed_at DESC);

-- ── 2. Process spacetime ───────────────────────────────────────────────────
-- Written when a real observation resolves against a prior prediction.
-- This is the research artifact: every point where the physical world
-- spoke back to the model.

CREATE TABLE IF NOT EXISTS process_spacetime (
    spacetime_id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    projection_commit_id    UUID REFERENCES process_projection_commits(commit_id),
    workspace_id            UUID,
    session_id              UUID,

    -- The prediction being tested
    projection_id           TEXT,
    observable_property     TEXT NOT NULL,
    feature_of_interest     TEXT,
    predicted_value         DOUBLE PRECISION NOT NULL,
    model_uri               TEXT,
    stage_id                TEXT,

    -- The real measurement
    real_observation_id     UUID NOT NULL,
    actual_value            DOUBLE PRECISION NOT NULL,
    measured_at             TIMESTAMPTZ NOT NULL,

    -- The delta
    absolute_error          DOUBLE PRECISION NOT NULL,  -- |predicted - actual|
    relative_error          DOUBLE PRECISION NOT NULL,  -- |predicted - actual| / |actual|
    accuracy_score          DOUBLE PRECISION NOT NULL,  -- 1 - min(relative_error, 1)
    delta_direction         TEXT NOT NULL CHECK (delta_direction IN ('over', 'under', 'exact')),

    -- Resolution mode
    resolution_mode         TEXT NOT NULL CHECK (resolution_mode IN (
                                'any_reading',      -- every real reading (full trajectory)
                                'sample_point',     -- scheduled check interval
                                'anomaly_delta'     -- threshold breach
                            )),
    anomaly_threshold       DOUBLE PRECISION,   -- the threshold that triggered this (if anomaly)
    sample_interval_hours   DOUBLE PRECISION,   -- the interval that matched (if sample_point)

    -- Process conditions at measurement time
    conditions_at_measure   JSONB,              -- temperature_c, pH, etc. at real reading time

    -- Cross-loop context (RSI proof data)
    loop1_semantic_rules    JSONB,              -- active semantic rules for this model at this time
    loop5_model_accuracy    DOUBLE PRECISION,   -- rolling accuracy score for this model before this reading

    -- Timing
    committed_at            TIMESTAMPTZ,        -- when the prediction was committed
    resolved_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    commit_to_resolve_hours DOUBLE PRECISION GENERATED ALWAYS AS (
        CASE WHEN committed_at IS NOT NULL
             THEN EXTRACT(EPOCH FROM (resolved_at - committed_at)) / 3600.0
             ELSE NULL END
    ) STORED,   -- how long the prediction was "live" before being tested

    -- Was the prediction committed before the measurement arrived?
    -- This is the commit-before-resolve invariant for SimOps.
    committed_before_measured BOOLEAN GENERATED ALWAYS AS (
        committed_at IS NOT NULL AND committed_at < resolved_at
    ) STORED
);

CREATE INDEX IF NOT EXISTS idx_ps_workspace
    ON process_spacetime(workspace_id, resolved_at DESC);
CREATE INDEX IF NOT EXISTS idx_ps_model
    ON process_spacetime(model_uri, resolved_at DESC);
CREATE INDEX IF NOT EXISTS idx_ps_property
    ON process_spacetime(observable_property, feature_of_interest, resolved_at DESC);
CREATE INDEX IF NOT EXISTS idx_ps_resolution_mode
    ON process_spacetime(resolution_mode, resolved_at DESC);
CREATE INDEX IF NOT EXISTS idx_ps_anomaly
    ON process_spacetime(model_uri, accuracy_score)
    WHERE resolution_mode = 'anomaly_delta';

-- ── Sample point configuration ─────────────────────────────────────────────
-- Per-workspace, per-property configuration of when to fire sample_point
-- resolutions. Defaults apply when no row exists.

CREATE TABLE IF NOT EXISTS process_sample_config (
    config_id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id            UUID NOT NULL,
    observable_property     TEXT NOT NULL,      -- or '*' for all properties
    sample_interval_hours   DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    anomaly_threshold       DOUBLE PRECISION NOT NULL DEFAULT 0.15,  -- 15% relative error
    enabled                 BOOLEAN NOT NULL DEFAULT TRUE,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (workspace_id, observable_property)
);

-- Default config: 1-hour sample points, 15% anomaly threshold
INSERT INTO process_sample_config (workspace_id, observable_property, sample_interval_hours, anomaly_threshold)
VALUES ('00000000-0000-0000-0000-000000000000', '*', 1.0, 0.15)
ON CONFLICT DO NOTHING;
