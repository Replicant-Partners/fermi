-- 139: Link fermi_forecasts to ABW workspaces
--
-- Each forecast can optionally be backed by a workspace (team) for full
-- OODA loop observability: workspace_messages track agent research,
-- workspace_action_log tracks decomposition/parameter changes/resolution,
-- and Loop 3 coherence evaluations run against the workspace.

-- ── fermi_forecasts: workspace link ──────────────────────────────────
ALTER TABLE fermi_forecasts
    ADD COLUMN IF NOT EXISTS workspace_id UUID REFERENCES teams(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_forecasts_workspace
    ON fermi_forecasts(workspace_id) WHERE workspace_id IS NOT NULL;

-- ── Convenience view: forecast workspaces with origin ────────────────
-- Lets the dashboard filter fermi workspaces and join forecast metadata.
CREATE OR REPLACE VIEW fermi_forecast_workspaces AS
SELECT
    f.id           AS forecast_id,
    f.question_text,
    f.predicted_probability,
    f.status       AS forecast_status,
    f.brier_score,
    f.created_at   AS forecast_created_at,
    t.id           AS workspace_id,
    t.name         AS workspace_name,
    t.origin,
    t.workspace_budget,
    t.workspace_spent,
    (SELECT COUNT(*) FROM workspace_messages wm WHERE wm.workspace_id = t.id) AS message_count,
    (SELECT COUNT(*) FROM workspace_agents wa WHERE wa.workspace_id = t.id) AS agent_count
FROM fermi_forecasts f
JOIN teams t ON t.id = f.workspace_id
WHERE f.workspace_id IS NOT NULL;
