-- Migration 229: select_agent_decisions — typed selection trace (Phase 3.4)
--
-- Each call to the select_agent tool writes one row here recording what
-- candidates were available, what was chosen, and what criteria weights
-- were used. Feeds the competition-stats endpoint and Loop 4.
--
-- best-effort write: the tool succeeds even if this insert fails.
CREATE TABLE IF NOT EXISTS select_agent_decisions (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    input_schema_id  TEXT        NOT NULL,
    scope_level      TEXT        NOT NULL DEFAULT 'workspace',
    scope_fleet_id   TEXT,
    workspace_id     UUID,
    criteria_weights JSONB,
    candidates       JSONB       NOT NULL DEFAULT '[]',
    selected         TEXT,
    parent_episode_id UUID,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_sad_agent ON select_agent_decisions (selected);
CREATE INDEX IF NOT EXISTS idx_sad_schema ON select_agent_decisions (input_schema_id);
CREATE INDEX IF NOT EXISTS idx_sad_ws    ON select_agent_decisions (workspace_id);
