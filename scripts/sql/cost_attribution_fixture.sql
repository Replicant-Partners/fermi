-- Minimal fixture for migrations 194 + 195.
--
-- Only the tables those two migrations read or write, at their PRE-194 shape,
-- so applying the migrations exercises the real ALTER/CREATE paths rather than
-- a schema that already has the columns. Deliberately no FKs to tables outside
-- this set — the point is to test the views' arithmetic, not referential
-- integrity that production already enforces.
--
-- Consumed by scripts/smoke_cost_attribution.sh against a throwaway database.

CREATE TABLE IF NOT EXISTS public.agents (
    agent_id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_name TEXT NOT NULL,
    tier       TEXT NOT NULL DEFAULT 'curated'
);

-- Pre-194 shape: tokens_used + cost_usd, no split and no basis.
CREATE TABLE IF NOT EXISTS public.episodes (
    episode_id       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id         UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    timestamp_ref    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    query            TEXT NOT NULL DEFAULT '',
    context          JSONB NOT NULL DEFAULT '{}'::jsonb,
    execution_status TEXT NOT NULL DEFAULT 'success',
    execution_time_ms BIGINT NOT NULL DEFAULT 0,
    tokens_used      INTEGER,
    cost_usd         DECIMAL(10, 6),
    provider_used    TEXT,
    model_used       TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Pre-195 shape: no episode_id.
CREATE TABLE IF NOT EXISTS public.forecast_agent_claims (
    claim_id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id  UUID NOT NULL,
    agent_id      UUID REFERENCES public.agents(agent_id) ON DELETE SET NULL,
    agent_name    TEXT NOT NULL,
    driver        TEXT NOT NULL,
    p5            REAL,
    p50           REAL NOT NULL,
    p95           REAL,
    neutral_value REAL NOT NULL DEFAULT 1.0,
    source        TEXT,
    raw_evidence  TEXT,
    claimed_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS public.fermi_forecasts (
    id                     TEXT PRIMARY KEY,
    question_text          TEXT NOT NULL,
    status                 TEXT NOT NULL DEFAULT 'active',
    predicted_probability  DOUBLE PRECISION,
    actual_outcome         DOUBLE PRECISION,
    brier_score            DOUBLE PRECISION,
    resolved_at            TIMESTAMPTZ,
    workspace_id           UUID,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS public.forecast_agent_credit (
    forecast_id    TEXT NOT NULL,
    agent_name     TEXT NOT NULL,
    shapley_value  DOUBLE PRECISION,
    neutralisation DOUBLE PRECISION,
    PRIMARY KEY (forecast_id, agent_name)
);
