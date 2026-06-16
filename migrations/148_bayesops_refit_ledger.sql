-- ═══════════════════════════════════════════════════════════════════
-- Migration 148 — BayesOps Refit Ledger & Pending Queue
--
-- Two tables to support Phase R-1 of the BayesOps World Cup demo:
--
--   1. bayesops_posterior_snapshots — every fit ever computed by the
--      refit hook, regardless of whether it was auto-accepted, staged
--      for review, or hard-blocked. This is the spacetime view's data
--      source (Phase R-3) and the audit trail of "how the prior moved
--      over time" for any learnable driver on any workspace.
--
--   2. bayesops_pending_fits — fits whose impact exceeded the auto-
--      accept threshold, waiting on a forecaster decision via the
--      sparkline accept/dismiss affordance (Phase R-2). One row per
--      staged fit; transitions to accepted or rejected as the user
--      decides.
--
-- The hook itself lives in src/handlers/workspace/refit.rs and is
-- called from the TODO insertion point at
-- src/handlers/workspace/resolution.rs (post-commit, tokio::spawn) and
-- from POST /api/workspaces/:id/refit.
--
-- See docs/specs/23_BAYESOPS_WORLD_CUP_DEMO.md §3 for the design.
-- ═══════════════════════════════════════════════════════════════════

-- ─── Snapshots: one row per fit, ever ────────────────────────────────
--
-- Always written. The decision column records what the impact gate
-- decided to do with the fit:
--   • auto_accepted — gate said the impact was small; params written
--                     immediately by the refit hook
--   • staged        — gate said the impact warranted human review;
--                     a row in bayesops_pending_fits points back here
--   • hard_blocked  — gate said the impact was implausibly large
--                     (likely a fitting bug); no params written, no
--                     row in bayesops_pending_fits, but the snapshot
--                     is kept for diagnostics
--
-- rate_before / rate_after capture the Monte Carlo rate-of-interest
-- with the current prior vs the proposed fitted posterior, run by the
-- impact gate. They are nullable because not every refit context can
-- compute them (e.g. multi-driver forecasts with no single rate metric
-- — for the demo every forecast has one; this is forward-compatible).
--
-- synthetic_n records how many of the n_observations were synthetic
-- (from a ConditionalPosterior sample) vs real (from upstream
-- resolutions). Lets the spacetime view colour real vs synthetic
-- evidence dots distinctly without re-parsing the fit metadata.
CREATE TABLE IF NOT EXISTS bayesops_posterior_snapshots (
    snapshot_id     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID NOT NULL,
    driver_name     TEXT NOT NULL,

    -- The fitted distribution (posterior::FittedDistribution) and
    -- its metadata (posterior::FitMetadata), persisted as the
    -- canonical JSONB form. These round-trip cleanly through serde
    -- — verified by the Phase 1 test suite.
    fitted          JSONB NOT NULL,
    metadata        JSONB NOT NULL,

    n_observations  INT NOT NULL,
    synthetic_n     INT NOT NULL DEFAULT 0,
    ci_width        DOUBLE PRECISION NOT NULL,
    n_eff           DOUBLE PRECISION NOT NULL,
    quality         TEXT NOT NULL
                    CHECK (quality IN ('sufficient', 'sparse', 'insufficient')),

    -- Impact-gate outputs. NULL when the gate could not run (e.g. no
    -- forecast on this workspace).
    rate_before     DOUBLE PRECISION,
    rate_after      DOUBLE PRECISION,

    decision        TEXT NOT NULL
                    CHECK (decision IN ('auto_accepted', 'staged', 'hard_blocked')),

    -- Provenance: what fired the refit. Examples:
    --   "resolution:upstream:<workspace_id>"
    --   "manual:<user_id>"
    --   "scheduled:<job_id>"
    triggered_by    TEXT NOT NULL,

    fitted_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Spacetime view's primary access pattern: per workspace + driver,
-- newest first.
CREATE INDEX IF NOT EXISTS idx_bayesops_snapshots_workspace_driver
    ON bayesops_posterior_snapshots(workspace_id, driver_name, fitted_at DESC);

-- Audit pattern: every snapshot since time T (for backfill / replay).
CREATE INDEX IF NOT EXISTS idx_bayesops_snapshots_fitted_at
    ON bayesops_posterior_snapshots(fitted_at DESC);

-- ─── Pending fits: one row per staged-for-review fit ─────────────────
--
-- Lifecycle:
--   pending  → accepted (forecaster clicked Accept; params written;
--              snapshot's decision stays 'staged' for history)
--           → rejected (forecaster clicked Dismiss; no params change)
--           → expired  (system cron after N days of inactivity; the
--                       spec doesn't ship this yet but the status is
--                       reserved)
--
-- snapshot_id references the bayesops_posterior_snapshots row that
-- was written when the gate staged this fit, so the UI can show
-- full posterior detail (CI sparkline overlay etc.) without a join
-- to fitted/metadata JSONB on every render.
CREATE TABLE IF NOT EXISTS bayesops_pending_fits (
    pending_id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID NOT NULL,
    driver_name     TEXT NOT NULL,
    snapshot_id     UUID NOT NULL
                    REFERENCES bayesops_posterior_snapshots(snapshot_id)
                    ON DELETE CASCADE,

    status          TEXT NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending', 'accepted', 'rejected', 'expired')),

    staged_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    decided_at      TIMESTAMPTZ,
    decided_by      TEXT,
    decision_notes  TEXT,

    -- Only one pending fit per (workspace, driver) at any time. If a
    -- new refit arrives while one is already pending, the existing
    -- pending row is auto-expired (decided_at set, status='expired')
    -- and a new one is inserted. The handler enforces this; the
    -- index ensures we can find the existing row efficiently.
    CONSTRAINT bayesops_pending_fits_workspace_driver_status_unique
        EXCLUDE USING btree (
            workspace_id WITH =,
            driver_name WITH =
        ) WHERE (status = 'pending')
);

CREATE INDEX IF NOT EXISTS idx_bayesops_pending_workspace_status
    ON bayesops_pending_fits(workspace_id, status, staged_at DESC);

-- ─── Comments ────────────────────────────────────────────────────────

COMMENT ON TABLE bayesops_posterior_snapshots IS
    'Append-only ledger of every posterior fit ever computed by the BayesOps refit hook. One row per (workspace, driver, fit attempt). Source of truth for the spacetime view (Phase R-3) and the audit trail of how priors evolved over time. See docs/specs/23_BAYESOPS_WORLD_CUP_DEMO.md §3.6.';
COMMENT ON COLUMN bayesops_posterior_snapshots.decision IS
    'What the impact gate did with this fit: auto_accepted (params written immediately), staged (waiting on forecaster review, see bayesops_pending_fits), hard_blocked (impact too large; suspected fitting bug; nothing written but snapshot kept for diagnostics).';
COMMENT ON COLUMN bayesops_posterior_snapshots.synthetic_n IS
    'Of n_observations, how many were synthetic (from a ConditionalPosterior sample) vs real (from upstream resolutions). Always 0 for the demo''s real-data path; populated when agents use synthetic-data MCP tools to augment fits.';
COMMENT ON COLUMN bayesops_posterior_snapshots.rate_before IS
    'Forecast rate from a Monte Carlo run with the current prior. Computed by the impact gate. NULL if the workspace has no scorable rate metric.';
COMMENT ON COLUMN bayesops_posterior_snapshots.rate_after IS
    'Forecast rate from a Monte Carlo run with the proposed fitted posterior. Computed by the impact gate. NULL if the workspace has no scorable rate metric.';

COMMENT ON TABLE bayesops_pending_fits IS
    'Fits whose impact exceeded the auto-accept threshold, waiting on forecaster decision via the sparkline UX. One row per staged fit per (workspace, driver). EXCLUDE constraint prevents multiple concurrent pending fits for the same driver — newer refits auto-expire older pendings.';
