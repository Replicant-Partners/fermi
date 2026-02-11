-- Fermi Notebook System
-- Supports private/shared/public visibility with team/org access control

CREATE TABLE IF NOT EXISTS fermi_notebooks (
    id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    title TEXT NOT NULL,
    description TEXT,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    visibility TEXT NOT NULL DEFAULT 'private' CHECK (visibility IN ('private', 'shared', 'public')),
    team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
    org_id TEXT, -- Future-proofing for organization-level sharing
    cells JSONB NOT NULL DEFAULT '[]'::jsonb,
    execution_state TEXT DEFAULT 'idle' CHECK (execution_state IN ('idle', 'running', 'complete', 'error')),
    last_executed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for ownership and visibility queries
CREATE INDEX IF NOT EXISTS idx_notebooks_owner_visibility ON fermi_notebooks(owner_id, visibility);
CREATE INDEX IF NOT EXISTS idx_notebooks_team ON fermi_notebooks(team_id) WHERE team_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_notebooks_public ON fermi_notebooks(visibility) WHERE visibility = 'public';

-- Portfolio management tables
CREATE TABLE IF NOT EXISTS fermi_portfolios (
    id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    title TEXT NOT NULL,
    description TEXT,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    visibility TEXT NOT NULL DEFAULT 'private' CHECK (visibility IN ('private', 'shared', 'public')),
    team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
    org_id TEXT,
    notebook_ids TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb, -- For custom indices, tags, categories
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_portfolios_owner_visibility ON fermi_portfolios(owner_id, visibility);
CREATE INDEX IF NOT EXISTS idx_portfolios_team ON fermi_portfolios(team_id) WHERE team_id IS NOT NULL;

-- Brier scoring and calibration tracking
CREATE TABLE IF NOT EXISTS fermi_forecasts (
    id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    notebook_id TEXT NOT NULL REFERENCES fermi_notebooks(id) ON DELETE CASCADE,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    question_text TEXT NOT NULL,
    predicted_probability REAL NOT NULL CHECK (predicted_probability >= 0 AND predicted_probability <= 1),
    confidence_interval_low REAL CHECK (confidence_interval_low >= 0 AND confidence_interval_low <= 1),
    confidence_interval_high REAL CHECK (confidence_interval_high >= 0 AND confidence_interval_high <= 1),
    resolution_date TIMESTAMPTZ,
    actual_outcome BOOLEAN, -- NULL until resolved
    brier_score REAL, -- Computed as (predicted - actual)^2 when resolved
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb, -- For tags, evidence links, etc.
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_forecasts_notebook ON fermi_forecasts(notebook_id);
CREATE INDEX IF NOT EXISTS idx_forecasts_owner ON fermi_forecasts(owner_id);
CREATE INDEX IF NOT EXISTS idx_forecasts_resolved ON fermi_forecasts(resolved_at) WHERE resolved_at IS NOT NULL;

-- Portfolio membership for Brier aggregation
CREATE TABLE IF NOT EXISTS fermi_portfolio_forecasts (
    portfolio_id TEXT NOT NULL REFERENCES fermi_portfolios(id) ON DELETE CASCADE,
    forecast_id TEXT NOT NULL REFERENCES fermi_forecasts(id) ON DELETE CASCADE,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (portfolio_id, forecast_id)
);

CREATE INDEX IF NOT EXISTS idx_portfolio_forecasts_portfolio ON fermi_portfolio_forecasts(portfolio_id);
