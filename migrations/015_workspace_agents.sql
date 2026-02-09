-- Migration 015: Workspace agents junction table
-- Tracks which agents are available in a workspace.
-- Separate from team_members (which tracks people roles).

BEGIN;

CREATE TABLE IF NOT EXISTS workspace_agents (
    workspace_id  UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    agent_id      UUID NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,
    added_by      TEXT NOT NULL,
    added_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    relationship  TEXT NOT NULL DEFAULT 'hired'
                      CHECK (relationship IN ('hired', 'owned', 'created_here')),
    PRIMARY KEY (workspace_id, agent_id)
);

CREATE INDEX IF NOT EXISTS idx_ws_agents_workspace ON workspace_agents(workspace_id);
CREATE INDEX IF NOT EXISTS idx_ws_agents_agent ON workspace_agents(agent_id);

COMMIT;
