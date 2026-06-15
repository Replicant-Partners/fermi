-- 140: Workspace outputs — typed key-value store for workspace results
--
-- Enables cross-workspace data consumption: workspace A publishes outputs,
-- workspace B reads them via the dependency graph. This is the foundation
-- for the Fermi forecast system where team priors feed tournament paths
-- and H2H match forecasts.
--
-- Also adds workspace status lifecycle and dependency tracking.

-- ═══════════════════════════════════════════════════════════════════
-- Workspace Outputs — typed KV store per workspace
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS workspace_outputs (
    workspace_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    key          TEXT NOT NULL,
    value        JSONB NOT NULL,
    version      INTEGER NOT NULL DEFAULT 1,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by   TEXT,           -- user_id or agent_id that last wrote this output
    PRIMARY KEY (workspace_id, key)
);

CREATE INDEX IF NOT EXISTS idx_workspace_outputs_workspace
    ON workspace_outputs(workspace_id);

-- ═══════════════════════════════════════════════════════════════════
-- Workspace Dependencies — DAG of workspace-to-workspace edges
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS workspace_dependencies (
    upstream_id     UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    downstream_id   UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    dependency_type TEXT NOT NULL DEFAULT 'output'
        CHECK (dependency_type IN ('output', 'event', 'parameter')),
    key_filter      TEXT,           -- optional: only propagate when this key changes
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (upstream_id, downstream_id)
);

-- Prevent self-references
ALTER TABLE workspace_dependencies
    ADD CONSTRAINT no_self_dependency CHECK (upstream_id != downstream_id);

CREATE INDEX IF NOT EXISTS idx_workspace_deps_downstream
    ON workspace_dependencies(downstream_id);

CREATE INDEX IF NOT EXISTS idx_workspace_deps_upstream
    ON workspace_dependencies(upstream_id);

-- ═══════════════════════════════════════════════════════════════════
-- Workspace Status Lifecycle
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE teams
    ADD COLUMN IF NOT EXISTS workspace_status TEXT NOT NULL DEFAULT 'active'
        CHECK (workspace_status IN ('active', 'completed', 'failed', 'archived'));
