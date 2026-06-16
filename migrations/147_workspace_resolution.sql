-- ═══════════════════════════════════════════════════════════════════
-- Migration 147 — Workspace Resolution Lifecycle
--
-- Generalizes forecast resolution from `fermi_forecasts` (single-question
-- Yes/No probability) to ANY workspace. A workspace represents some
-- belief about the world that will eventually be resolved (a team's
-- tournament win probability resolves at tournament end, an H2H match
-- workspace resolves when the match is played, a generic forecast
-- workspace resolves at its target_date, etc.). Resolution is the
-- universal point where:
--
--   • The outcome is recorded (domain-specific JSON payload).
--   • The workspace transitions from `active` to `completed` (or
--     `failed` if resolved with explicit failure / cancellation).
--   • Brier scores can be computed against the workspace's last
--     published probability output.
--   • Upstream workspaces are notified so BayesOps can refit their
--     learnable parameters against the observed downstream outcome.
--
-- The single-question `resolve_forecast()` SQL function in migration 094
-- remains intact — that's the LEGACY per-forecast resolver for the
-- `fermi_forecasts` table. This migration adds the GENERIC workspace
-- resolver columns; the handler that uses them lives in Rust
-- (src/handlers/workspace/resolution.rs).
-- ═══════════════════════════════════════════════════════════════════

-- ─── Resolution columns on teams ─────────────────────────────────────
--
-- We keep these as raw columns on `teams` (not a separate
-- `workspace_resolutions` table) for three reasons:
--   1. Resolution is at most one event per workspace — no append-only
--      semantics required.
--   2. The existing `workspace_status` column lives on `teams`, so
--      keeping resolution metadata adjacent keeps the lifecycle
--      atomically queryable.
--   3. The resolution OUTCOME payload (domain-specific JSON) is ALSO
--      written to `workspace_outputs` keyed `'resolution'`, which gives
--      us the cross-workspace dependency propagation for free. The
--      teams columns are the canonical lifecycle metadata; the outputs
--      row is the consumable artefact.
ALTER TABLE teams
    ADD COLUMN IF NOT EXISTS resolved_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS resolved_by TEXT,
    ADD COLUMN IF NOT EXISTS resolution_outcome JSONB,
    ADD COLUMN IF NOT EXISTS resolution_notes TEXT,
    ADD COLUMN IF NOT EXISTS resolution_source TEXT;

-- Brier score column for the workspace's published probability vs the
-- resolved outcome. NULL when:
--   (a) the workspace has not been resolved
--   (b) the workspace was resolved but had no published probability
--       output to score against (e.g. it was closed without resolution)
--   (c) the outcome isn't a Yes/No probability question (multi-class
--       outcomes go through a separate scoring pass — Phase 6+)
ALTER TABLE teams
    ADD COLUMN IF NOT EXISTS brier_score REAL;

-- ─── Indexes ─────────────────────────────────────────────────────────

CREATE INDEX IF NOT EXISTS idx_teams_workspace_status
    ON teams(workspace_status)
    WHERE workspace_status != 'active';

CREATE INDEX IF NOT EXISTS idx_teams_resolved_at
    ON teams(resolved_at DESC)
    WHERE resolved_at IS NOT NULL;

-- ─── Comments ────────────────────────────────────────────────────────

COMMENT ON COLUMN teams.resolved_at IS
    'Timestamp at which the workspace was resolved. NULL while active. Paired with workspace_status transitions to completed/failed/archived.';
COMMENT ON COLUMN teams.resolved_by IS
    'user_id of the principal that called POST /api/workspaces/:id/resolve. Maps to users.user_id (may be a Zitadel ID, ETH address, or UUID string).';
COMMENT ON COLUMN teams.resolution_outcome IS
    'Domain-specific outcome payload. Examples: { "won_tournament": false } for a team-prior workspace; { "winner_team_id": "ARG", "home_goals": 2, "away_goals": 1 } for an H2H match; { "value": 0.0|1.0 } for a generic binary forecast.';
COMMENT ON COLUMN teams.resolution_source IS
    'Provenance tag for the resolution. Examples: "manual_user", "fifa_official_api", "polymarket_resolution", "automated_target_date". Free-form string; downstream consumers may filter on it.';
COMMENT ON COLUMN teams.brier_score IS
    'Brier score (0 = perfect, 1 = catastrophic) for the workspace`s last published probability output against the resolved outcome. Only populated when the workspace had a binary probability output AND was resolved with a binary outcome.';
