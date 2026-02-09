-- Agent version history: snapshot mutable fields before each update
CREATE TABLE IF NOT EXISTS agent_versions (
    version_id    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id      UUID NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,
    version_number INTEGER NOT NULL,
    description   TEXT,
    system_prompt TEXT,
    tags          TEXT[] DEFAULT '{}',
    model         TEXT,
    temperature   DOUBLE PRECISION,
    visibility    TEXT,
    display_alias TEXT,
    changed_by    TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_agent_versions_agent ON agent_versions(agent_id, version_number DESC);
