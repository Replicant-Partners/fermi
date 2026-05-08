-- Migration 105: Longitudinal Observability — Phase 3 of the Social
-- Agent Observability Platform.
--
-- Reconciles with:
--   docs/architecture/social_agent_observability_architecture.html (Plane C)
--   docs/architecture/OBSERVABILITY_IMPL.md (Phase 3 entry)
--
-- Adds the storage layer for the four Plane C concerns:
--   1. agent_timeline_entries  — per-episode rolled-up scoring view
--   2. dyad_state              — per-(agent, human) running rapport / trust / reciprocity
--   3. anomaly_events          — drift / conflict / rupture / safety events
--   4. agent_observability_state — per-agent worker checkpoint
--
-- All ALTERs idempotent; constraint mutations wrapped in DO blocks
-- per PgBouncer transaction-mode safety rules.

-- ═══════════════════════════════════════════════════════════════════
-- 1. agent_timeline_entries — per-episode timeline projection
-- ═══════════════════════════════════════════════════════════════════
--
-- One row per episode that completes successfully through the
-- evaluator registry. The architecture-doc's mock shows this as the
-- primary read path for the observatory dashboard's per-agent timeline
-- charts. Fields are mostly denormalized projections of (Episode,
-- AggregatedSignal, persona_version, dyad_id) so the dashboard can
-- render without joins.

CREATE TABLE IF NOT EXISTS public.agent_timeline_entries (
    entry_id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Identity
    agent_id            UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    episode_id          UUID REFERENCES public.episodes(episode_id) ON DELETE SET NULL,
    run_id              UUID REFERENCES public.eval_runs(run_id)     ON DELETE SET NULL,

    -- Persona / dyad context (Phase 0 + Phase 3)
    persona_version     INTEGER NOT NULL DEFAULT 1,
    dyad_id             TEXT,                             -- null for non-dyadic invocations
    session_id          TEXT,                             -- workspace session, eval run, or null

    -- Source enum pass-through (`auto_pass` | `auto_fail` | `human_*` | `synthetic_correction`)
    provenance          TEXT NOT NULL DEFAULT 'auto_pass',

    -- Per-dimension means (denormalized from AggregatedSignal — fast
    -- for chart queries, the full signal still lives on eval_runs)
    dim_scores          JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Drift vector magnitude vs. previous persona_version baseline.
    -- Computed by PersonaDriftMonitor; null when no prior baseline
    -- exists yet.
    drift_norm          DOUBLE PRECISION,

    -- Cosine similarity vs. the rolling-mean embedding of the same
    -- persona_version (within-version cohesion). Helps separate
    -- desired drift (cross-version) from undesired drift (within-version).
    within_version_cosine DOUBLE PRECISION,

    -- Anomaly flags raised by AnomalyDetector. Array of strings.
    anomaly_flags       JSONB NOT NULL DEFAULT '[]'::jsonb,

    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for the per-agent timeline chart (most common access pattern)
CREATE INDEX IF NOT EXISTS idx_timeline_agent_time
    ON public.agent_timeline_entries(agent_id, created_at DESC);

-- Index for per-(agent, persona_version) drift baseline queries
CREATE INDEX IF NOT EXISTS idx_timeline_persona_version
    ON public.agent_timeline_entries(agent_id, persona_version, created_at DESC);

-- Index for the per-dyad social arc chart
CREATE INDEX IF NOT EXISTS idx_timeline_dyad
    ON public.agent_timeline_entries(dyad_id, created_at DESC)
    WHERE dyad_id IS NOT NULL;

-- Index for "recent anomalies across all agents" dashboard
CREATE INDEX IF NOT EXISTS idx_timeline_with_anomalies
    ON public.agent_timeline_entries(agent_id, created_at DESC)
    WHERE jsonb_array_length(anomaly_flags) > 0;

-- ═══════════════════════════════════════════════════════════════════
-- 2. dyad_state — per-(agent, human) relational running state
-- ═══════════════════════════════════════════════════════════════════
--
-- Architecture doc Plane C: "rapport · trust · reciprocity per
-- human-agent dyad." Phase 3 ships the schema and the running update
-- math; the values stay scaffolding-quality until we have multi-turn
-- workspace data flowing in.
--
-- Q1 (a): only populated for episodes with a non-null dyad_id.
-- Background / agent-to-agent / system invocations leave this table
-- empty and the social tracker silently skips them.

CREATE TABLE IF NOT EXISTS public.dyad_state (
    dyad_id             TEXT PRIMARY KEY,
    agent_id            UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    human_id            TEXT NOT NULL,

    -- Each in [0, 1]. Initial value 0.5 (neutral) until we accumulate
    -- enough episodes to compute meaningful running averages.
    rapport             DOUBLE PRECISION NOT NULL DEFAULT 0.5
        CHECK (rapport >= 0.0 AND rapport <= 1.0),
    trust               DOUBLE PRECISION NOT NULL DEFAULT 0.5
        CHECK (trust >= 0.0 AND trust <= 1.0),
    reciprocity         DOUBLE PRECISION NOT NULL DEFAULT 0.5
        CHECK (reciprocity >= 0.0 AND reciprocity <= 1.0),

    -- Episode count contributing to the running averages. Surfaces
    -- "how warm-up is the signal" to the dashboard so charts can fade
    -- in as confidence builds.
    episode_count       INTEGER NOT NULL DEFAULT 0,

    -- Rolling window of recent rapport scores for rupture detection
    -- (architecture-doc anomaly type: "sharp drop in rapport / trust").
    -- Stored as a JSONB array bounded to RUPTURE_WINDOW_LEN entries.
    recent_rapport      JSONB NOT NULL DEFAULT '[]'::jsonb,

    last_updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_dyad_agent
    ON public.dyad_state(agent_id, last_updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_dyad_human
    ON public.dyad_state(human_id, last_updated_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- 3. anomaly_events — drift / conflict / rupture / safety
-- ═══════════════════════════════════════════════════════════════════
--
-- Append-only log of anomalies detected by AnomalyDetector. Phase 4
-- HITL review queue subscribes to this table; Phase 3 just writes.

CREATE TABLE IF NOT EXISTS public.anomaly_events (
    event_id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id            UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,

    -- Linkage — at least one of (episode_id, run_id, dyad_id) is non-null.
    episode_id          UUID REFERENCES public.episodes(episode_id) ON DELETE SET NULL,
    run_id              UUID REFERENCES public.eval_runs(run_id)     ON DELETE SET NULL,
    dyad_id             TEXT,

    -- Anomaly type — see AnomalyDetector for definitions
    kind                TEXT NOT NULL
        CHECK (kind IN ('drift', 'rolling_conflict', 'rupture', 'safety')),

    severity            TEXT NOT NULL DEFAULT 'warning'
        CHECK (severity IN ('info', 'warning', 'critical')),

    -- Free-form payload — kind-specific shape
    -- - drift:            { drift_norm, threshold, prev_persona_version, curr_persona_version }
    -- - rolling_conflict: { dimension, episode_count, episodes: [...], spread }
    -- - rupture:          { dyad_id, rapport_drop, episodes_window }
    -- - safety:           { flag, evaluator_name }
    payload             JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- HITL routing flag — set true when the AnomalyDetector deems this
    -- event reviewer-actionable. Phase 4 HITL queue reads
    -- WHERE requires_review = TRUE AND resolved_at IS NULL.
    requires_review     BOOLEAN NOT NULL DEFAULT TRUE,

    -- Set when a Phase 4 reviewer addresses the event.
    resolved_at         TIMESTAMPTZ,
    resolved_by         TEXT,

    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_anomaly_agent_time
    ON public.anomaly_events(agent_id, created_at DESC);

-- HITL queue index (most common Phase 4 read path)
CREATE INDEX IF NOT EXISTS idx_anomaly_hitl_queue
    ON public.anomaly_events(created_at DESC)
    WHERE requires_review = TRUE AND resolved_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_anomaly_kind
    ON public.anomaly_events(kind, created_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- 4. agent_observability_state — per-agent worker checkpoint
-- ═══════════════════════════════════════════════════════════════════
--
-- Q4 (c) hybrid: timeline entries are written inline at episode-store
-- time; drift + anomaly scans run via ObservabilityWorker on demand.
-- This table tracks where the worker last got to so resuming is cheap.

CREATE TABLE IF NOT EXISTS public.agent_observability_state (
    agent_id                    UUID PRIMARY KEY REFERENCES public.agents(agent_id) ON DELETE CASCADE,

    -- Highest entry_id processed by the drift / anomaly scanner.
    last_scanned_entry_id       UUID,

    last_scan_started_at        TIMESTAMPTZ,
    last_scan_completed_at      TIMESTAMPTZ,
    last_scan_duration_ms       BIGINT,

    -- Counters surfaced on the dashboard
    timeline_entry_count        INTEGER NOT NULL DEFAULT 0,
    anomaly_event_count         INTEGER NOT NULL DEFAULT 0,

    created_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ═══════════════════════════════════════════════════════════════════
-- End of migration 105
-- ═══════════════════════════════════════════════════════════════════
