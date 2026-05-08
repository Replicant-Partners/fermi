-- Migration 106: Forecast agent schedules
-- Persists recurring agent research schedules so the cockpit can
-- auto-fire agents on open and users can track when agents last ran.

CREATE TABLE IF NOT EXISTS fermi_forecast_schedules (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    forecast_id     TEXT NOT NULL,
    agent_id        TEXT NOT NULL,
    driver_name     TEXT NOT NULL,
    query           TEXT NOT NULL,
    interval_hours  INT NOT NULL DEFAULT 24,
    last_run_at     TIMESTAMPTZ,
    next_run_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    enabled         BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_ffs_unique
    ON fermi_forecast_schedules (forecast_id, agent_id, driver_name);

CREATE INDEX IF NOT EXISTS idx_ffs_forecast_id
    ON fermi_forecast_schedules (forecast_id);

CREATE INDEX IF NOT EXISTS idx_ffs_next_run
    ON fermi_forecast_schedules (next_run_at)
    WHERE enabled = true;
