-- 091: Fermi Forecasting System
--
-- Standalone forecasting tables for the Fermi Console.
-- Forecasts are first-class citizens — they can exist independently
-- of notebooks. Notebooks remain as an optional authoring container.
--
-- This replaces the deferred 048_fermi_notebooks.sql with a design
-- that supports both notebook-based and console-based workflows.

-- ═══════════════════════════════════════════════════════════════════
-- NOTEBOOKS (optional authoring container)
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS fermi_notebooks (
    id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    title TEXT NOT NULL,
    description TEXT,
    owner_id TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    visibility TEXT NOT NULL DEFAULT 'private'
        CHECK (visibility IN ('private', 'shared', 'public')),
    team_id UUID REFERENCES teams(id) ON DELETE SET NULL,
    cells JSONB NOT NULL DEFAULT '[]'::jsonb,
    fpl_source TEXT,  -- raw FPL source (alternative to cells for console-authored)
    execution_state TEXT DEFAULT 'idle'
        CHECK (execution_state IN ('idle', 'running', 'complete', 'error')),
    last_executed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_notebooks_owner ON fermi_notebooks(owner_id);
CREATE INDEX IF NOT EXISTS idx_notebooks_visibility ON fermi_notebooks(visibility)
    WHERE visibility IN ('shared', 'public');
CREATE INDEX IF NOT EXISTS idx_notebooks_team ON fermi_notebooks(team_id)
    WHERE team_id IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- PORTFOLIOS (named collections of forecasts)
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS fermi_portfolios (
    id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    title TEXT NOT NULL,
    description TEXT,
    owner_id TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    visibility TEXT NOT NULL DEFAULT 'private'
        CHECK (visibility IN ('private', 'shared', 'public')),
    team_id UUID REFERENCES teams(id) ON DELETE SET NULL,
    domain TEXT,  -- e.g. 'tech', 'economics', 'geopolitics', 'climate'
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_portfolios_owner ON fermi_portfolios(owner_id);
CREATE INDEX IF NOT EXISTS idx_portfolios_team ON fermi_portfolios(team_id)
    WHERE team_id IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- FORECASTS (first-class, standalone)
-- ═══════════════════════════════════════════════════════════════════
--
-- A forecast is a probabilistic prediction about a future event.
-- It can be created from the console (standalone) or from a notebook.
-- notebook_id is OPTIONAL — console forecasts don't have one.

CREATE TABLE IF NOT EXISTS fermi_forecasts (
    id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    owner_id TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,

    -- The question being forecasted
    question_text TEXT NOT NULL,
    domain TEXT,  -- e.g. 'tech', 'economics', 'geopolitics'
    resolution_criteria TEXT,  -- how will this be resolved?
    target_date TIMESTAMPTZ,  -- when should this resolve?

    -- The prediction
    predicted_probability REAL NOT NULL
        CHECK (predicted_probability >= 0 AND predicted_probability <= 1),
    confidence_interval_low REAL
        CHECK (confidence_interval_low >= 0 AND confidence_interval_low <= 1),
    confidence_interval_high REAL
        CHECK (confidence_interval_high >= 0 AND confidence_interval_high <= 1),

    -- FPL source and simulation results
    fpl_source TEXT,  -- the FPL program that produced this forecast
    notebook_id TEXT REFERENCES fermi_notebooks(id) ON DELETE SET NULL,
    simulation_results JSONB,  -- cached ExecutionResults (mean, p5, p95, etc.)
    iterations INTEGER DEFAULT 10000,

    -- Drivers snapshot (for display without re-parsing FPL)
    drivers JSONB NOT NULL DEFAULT '[]'::jsonb,
    evidence JSONB NOT NULL DEFAULT '[]'::jsonb,

    -- Agent contributions
    agents_used JSONB NOT NULL DEFAULT '[]'::jsonb,  -- [{agent_id, query, model_used, cost}]

    -- Resolution and scoring
    status TEXT NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'active', 'resolved', 'voided')),
    actual_outcome BOOLEAN,  -- NULL until resolved; true = yes, false = no
    brier_score REAL,  -- (predicted - actual)^2, computed on resolution
    resolved_at TIMESTAMPTZ,
    resolved_by TEXT,  -- user_id of resolver (self or oracle)
    resolution_notes TEXT,

    -- Visibility and sharing
    visibility TEXT NOT NULL DEFAULT 'private'
        CHECK (visibility IN ('private', 'shared', 'public')),
    team_id UUID REFERENCES teams(id) ON DELETE SET NULL,

    -- Metadata
    tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_forecasts_owner ON fermi_forecasts(owner_id);
CREATE INDEX IF NOT EXISTS idx_forecasts_status ON fermi_forecasts(status);
CREATE INDEX IF NOT EXISTS idx_forecasts_domain ON fermi_forecasts(domain)
    WHERE domain IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_forecasts_visibility ON fermi_forecasts(visibility)
    WHERE visibility IN ('shared', 'public');
CREATE INDEX IF NOT EXISTS idx_forecasts_team ON fermi_forecasts(team_id)
    WHERE team_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_forecasts_resolved ON fermi_forecasts(resolved_at)
    WHERE resolved_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_forecasts_target_date ON fermi_forecasts(target_date)
    WHERE target_date IS NOT NULL AND status = 'active';
CREATE INDEX IF NOT EXISTS idx_forecasts_notebook ON fermi_forecasts(notebook_id)
    WHERE notebook_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_forecasts_tags ON fermi_forecasts USING gin(tags);

-- ═══════════════════════════════════════════════════════════════════
-- FORECAST UPDATES (probability revision history)
-- ═══════════════════════════════════════════════════════════════════
--
-- Track how a forecast's probability changes over time.
-- Each update records the new probability and what triggered the change.
-- This enables calibration analysis and shows intellectual honesty.

CREATE TABLE IF NOT EXISTS fermi_forecast_updates (
    id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    forecast_id TEXT NOT NULL REFERENCES fermi_forecasts(id) ON DELETE CASCADE,
    previous_probability REAL NOT NULL,
    new_probability REAL NOT NULL,
    reason TEXT,  -- why the update was made
    agent_id TEXT,  -- if an agent triggered the update
    evidence_added JSONB,  -- new evidence that prompted the update
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_forecast_updates_forecast ON fermi_forecast_updates(forecast_id);
CREATE INDEX IF NOT EXISTS idx_forecast_updates_time ON fermi_forecast_updates(created_at);

-- ═══════════════════════════════════════════════════════════════════
-- PORTFOLIO MEMBERSHIP
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS fermi_portfolio_forecasts (
    portfolio_id TEXT NOT NULL REFERENCES fermi_portfolios(id) ON DELETE CASCADE,
    forecast_id TEXT NOT NULL REFERENCES fermi_forecasts(id) ON DELETE CASCADE,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (portfolio_id, forecast_id)
);

CREATE INDEX IF NOT EXISTS idx_pf_portfolio ON fermi_portfolio_forecasts(portfolio_id);
CREATE INDEX IF NOT EXISTS idx_pf_forecast ON fermi_portfolio_forecasts(forecast_id);

-- ═══════════════════════════════════════════════════════════════════
-- LEADERBOARD MATERIALIZED VIEW
-- ═══════════════════════════════════════════════════════════════════
--
-- Materialized for performance — refresh periodically or on resolution.
-- Ranks users by average Brier score (lower is better).

CREATE MATERIALIZED VIEW IF NOT EXISTS fermi_leaderboard AS
SELECT
    f.owner_id,
    u.display_name,
    COUNT(*) AS total_resolved,
    AVG(f.brier_score) AS avg_brier_score,
    MIN(f.brier_score) AS best_brier_score,
    MAX(f.brier_score) AS worst_brier_score,
    STDDEV(f.brier_score) AS brier_stddev,
    -- Calibration buckets: count forecasts in each probability decile
    COUNT(*) FILTER (WHERE f.predicted_probability < 0.1) AS bucket_0_10,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.1 AND f.predicted_probability < 0.2) AS bucket_10_20,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.2 AND f.predicted_probability < 0.3) AS bucket_20_30,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.3 AND f.predicted_probability < 0.4) AS bucket_30_40,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.4 AND f.predicted_probability < 0.5) AS bucket_40_50,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.5 AND f.predicted_probability < 0.6) AS bucket_50_60,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.6 AND f.predicted_probability < 0.7) AS bucket_60_70,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.7 AND f.predicted_probability < 0.8) AS bucket_70_80,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.8 AND f.predicted_probability < 0.9) AS bucket_80_90,
    COUNT(*) FILTER (WHERE f.predicted_probability >= 0.9) AS bucket_90_100,
    -- Accuracy by bucket (fraction of "yes" outcomes in each decile)
    AVG(CASE WHEN f.predicted_probability < 0.2 THEN f.actual_outcome::int END) AS accuracy_0_20,
    AVG(CASE WHEN f.predicted_probability >= 0.2 AND f.predicted_probability < 0.4 THEN f.actual_outcome::int END) AS accuracy_20_40,
    AVG(CASE WHEN f.predicted_probability >= 0.4 AND f.predicted_probability < 0.6 THEN f.actual_outcome::int END) AS accuracy_40_60,
    AVG(CASE WHEN f.predicted_probability >= 0.6 AND f.predicted_probability < 0.8 THEN f.actual_outcome::int END) AS accuracy_60_80,
    AVG(CASE WHEN f.predicted_probability >= 0.8 THEN f.actual_outcome::int END) AS accuracy_80_100,
    -- Streaks and activity
    MAX(f.resolved_at) AS last_resolved_at,
    -- Domain breakdown
    array_agg(DISTINCT f.domain) FILTER (WHERE f.domain IS NOT NULL) AS domains
FROM fermi_forecasts f
JOIN users u ON u.user_id = f.owner_id
WHERE f.status = 'resolved'
  AND f.brier_score IS NOT NULL
GROUP BY f.owner_id, u.display_name
HAVING COUNT(*) >= 5  -- minimum 5 resolved forecasts to appear on leaderboard
WITH DATA;

CREATE UNIQUE INDEX IF NOT EXISTS idx_leaderboard_owner ON fermi_leaderboard(owner_id);
CREATE INDEX IF NOT EXISTS idx_leaderboard_brier ON fermi_leaderboard(avg_brier_score ASC);

-- ═══════════════════════════════════════════════════════════════════
-- HELPER FUNCTION: Compute Brier score
-- ═══════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION compute_brier_score(
    predicted REAL,
    actual BOOLEAN
) RETURNS REAL AS $$
BEGIN
    RETURN (predicted - (CASE WHEN actual THEN 1.0 ELSE 0.0 END)) ^ 2;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- ═══════════════════════════════════════════════════════════════════
-- HELPER FUNCTION: Resolve a forecast
-- ═══════════════════════════════════════════════════════════════════
--
-- Atomically sets the outcome, computes Brier score, and updates status.
-- Returns the computed Brier score.

CREATE OR REPLACE FUNCTION resolve_forecast(
    p_forecast_id TEXT,
    p_actual_outcome BOOLEAN,
    p_resolved_by TEXT,
    p_resolution_notes TEXT DEFAULT NULL
) RETURNS REAL AS $$
DECLARE
    v_predicted REAL;
    v_brier REAL;
    v_status TEXT;
BEGIN
    -- Lock the row and get current state
    SELECT predicted_probability, status
    INTO v_predicted, v_status
    FROM fermi_forecasts
    WHERE id = p_forecast_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'Forecast % not found', p_forecast_id;
    END IF;

    IF v_status != 'active' THEN
        RAISE EXCEPTION 'Forecast % is not active (status: %)', p_forecast_id, v_status;
    END IF;

    -- Compute Brier score
    v_brier := compute_brier_score(v_predicted, p_actual_outcome);

    -- Update the forecast
    UPDATE fermi_forecasts SET
        actual_outcome = p_actual_outcome,
        brier_score = v_brier,
        status = 'resolved',
        resolved_at = NOW(),
        resolved_by = p_resolved_by,
        resolution_notes = p_resolution_notes,
        updated_at = NOW()
    WHERE id = p_forecast_id;

    -- Refresh leaderboard (async in production, inline here for correctness)
    -- In production, this should be triggered by a background job.
    -- REFRESH MATERIALIZED VIEW CONCURRENTLY fermi_leaderboard;

    RETURN v_brier;
END;
$$ LANGUAGE plpgsql;

-- ═══════════════════════════════════════════════════════════════════
-- HELPER FUNCTION: Refresh leaderboard
-- ═══════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION refresh_fermi_leaderboard()
RETURNS void AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY fermi_leaderboard;
END;
$$ LANGUAGE plpgsql;
