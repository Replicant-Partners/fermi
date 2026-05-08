-- Migration 104: Evaluator Signals — Phase 2 of the Social Agent
-- Observability Platform.
--
-- Adds the per-evaluator signal table that the registry (Plane B,
-- shipped in Phase 1) writes to, and extends `eval_runs` with
-- aggregated-signal fields so a run-level conflict view can be
-- rendered without re-aggregating from rows.
--
-- Reconciles with:
--   docs/architecture/social_agent_observability_architecture.html (Plane B + Plane C signal store mock)
--   docs/architecture/OBSERVABILITY_IMPL.md (Phase 2 entry)
--
-- All ALTERs are idempotent. Constraint changes wrapped in DO blocks
-- per PgBouncer transaction-mode safety rules.

-- ═══════════════════════════════════════════════════════════════════
-- 1. eval_signals — one row per (run, episode, evaluator, dimension)
-- ═══════════════════════════════════════════════════════════════════
--
-- This is the long-term store of what each evaluator said about each
-- episode on each dimension. Phase 3 trend analyser reads from here;
-- Phase 4 HITL surfaces dimension-by-dimension breakdowns.
--
-- Notes:
--   - episode_id is nullable: registry may produce signals for an
--     execution that didn't store an episode (rare, but possible).
--   - run_id is nullable: registry can be invoked outside the eval
--     pipeline once Phase 3 wires longitudinal scoring.
--   - evaluator_version captures the prompt/weights revision so trend
--     analysis can split before/after a prompt change (EVALUATOR_DESIGN.md Q-CC4).

CREATE TABLE IF NOT EXISTS public.eval_signals (
    signal_id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Linkage
    run_id               UUID REFERENCES public.eval_runs(run_id) ON DELETE CASCADE,
    episode_id           UUID REFERENCES public.episodes(episode_id) ON DELETE SET NULL,
    agent_id             UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,

    -- Evaluator identity
    evaluator_name       TEXT NOT NULL,
    evaluator_version    TEXT NOT NULL DEFAULT 'v1',
    evaluator_tier       TEXT NOT NULL DEFAULT 'dimensional'
        CHECK (evaluator_tier IN ('pre_filter', 'dimensional')),

    -- Dimension + score (stored once per dimension per evaluator)
    dimension            TEXT NOT NULL,
    score                DOUBLE PRECISION NOT NULL
        CHECK (score >= 0.0 AND score <= 1.0),

    -- Self-reported confidence in this score in [0, 1]
    confidence           DOUBLE PRECISION NOT NULL DEFAULT 1.0
        CHECK (confidence >= 0.0 AND confidence <= 1.0),

    -- Free-form flags raised by the evaluator (e.g. safety:violence,
    -- groundedness:contradicted)
    flags                JSONB NOT NULL DEFAULT '[]'::jsonb,

    -- Provenance pass-through from the originating EpisodeBundle so
    -- this row is self-contained for trend analysis
    bundle_provenance    TEXT NOT NULL DEFAULT 'auto_pass',
    persona_version      INTEGER,

    -- Provider metadata (for cost attribution / evaluator versioning)
    model_used           TEXT,
    cost_credits         INTEGER NOT NULL DEFAULT 0,
    latency_ms           BIGINT NOT NULL DEFAULT 0,

    -- Optional one-line rationale (HITL surface uses this)
    rationale            TEXT,

    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Look up all signals for a run (eval-run dashboard)
CREATE INDEX IF NOT EXISTS idx_eval_signals_run
    ON public.eval_signals(run_id, evaluator_name, dimension)
    WHERE run_id IS NOT NULL;

-- Look up signals per agent for trend analysis (Phase 3)
CREATE INDEX IF NOT EXISTS idx_eval_signals_agent_dim
    ON public.eval_signals(agent_id, dimension, created_at DESC);

-- Look up signals per episode (HITL drill-down)
CREATE INDEX IF NOT EXISTS idx_eval_signals_episode
    ON public.eval_signals(episode_id)
    WHERE episode_id IS NOT NULL;

-- Look up signals by evaluator name (per-evaluator drift / quality tracking)
CREATE INDEX IF NOT EXISTS idx_eval_signals_evaluator
    ON public.eval_signals(evaluator_name, evaluator_version, created_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- 2. eval_runs — aggregated-signal columns
-- ═══════════════════════════════════════════════════════════════════
--
-- The registry's `AggregatedSignal` is stored as JSONB so we can
-- render the eval-run dashboard (Phase 4) without re-aggregating
-- from `eval_signals`. `conflict_flags` is denormalised for cheap
-- "this run had conflicts" queries.

ALTER TABLE public.eval_runs
    ADD COLUMN IF NOT EXISTS aggregated_signal JSONB;

ALTER TABLE public.eval_runs
    ADD COLUMN IF NOT EXISTS conflict_flags JSONB NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE public.eval_runs
    ADD COLUMN IF NOT EXISTS prefilter_blocked BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX IF NOT EXISTS idx_eval_runs_with_conflicts
    ON public.eval_runs(agent_id, started_at DESC)
    WHERE jsonb_array_length(conflict_flags) > 0;

-- ═══════════════════════════════════════════════════════════════════
-- End of migration 104
-- ═══════════════════════════════════════════════════════════════════
--
-- Note: notifications.type is unconstrained TEXT (see migration 021),
-- so the new 'eval_conflict' notification type that Phase 2 emits via
-- create_notification() requires no schema change.
