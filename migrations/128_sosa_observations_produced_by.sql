-- Doc 12 § Capability 1 — agent version stamp on observations.
--
-- Three new columns on sosa_observations carry the agent provenance for any
-- observation that was produced by an agent (vs typed directly by a user or
-- streamed from a deterministic sensor).
--
--   * `produced_by_agent_id` — denormalised string for fast filtering
--   * `produced_by_version_id` — foreign key into `agent_versions`
--   * `produced_by_version_number` — denormalised int for display
--
-- All three are nullable: observations from human input or non-agent ingest
-- carry no provenance, and that's fine.
--
-- The version stamp lets RSI Loop 5 partition Brier scoring by agent version
-- (see GET /api/agents/<id>/calibration?partition_by=version), enables A/B
-- testing of prompt/model changes, and lets every ABW app answer "which
-- version of the agent made this observation" without bespoke schema.
--
-- PgBouncer-safe: each statement wrapped in its own DO block so the entire
-- migration runs as a single command from sqlx's perspective.

DO $$
BEGIN
    ALTER TABLE sosa_observations
        ADD COLUMN IF NOT EXISTS produced_by_agent_id       TEXT,
        ADD COLUMN IF NOT EXISTS produced_by_version_id     UUID
            REFERENCES agent_versions(version_id) ON DELETE SET NULL,
        ADD COLUMN IF NOT EXISTS produced_by_version_number INTEGER;
END $$;

DO $$
BEGIN
    CREATE INDEX IF NOT EXISTS idx_obs_produced_by_version
        ON sosa_observations (produced_by_agent_id, produced_by_version_number)
        WHERE produced_by_agent_id IS NOT NULL;
END $$;
