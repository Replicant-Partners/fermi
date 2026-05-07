-- Migration 103: Social Agent Observability — Phase 0 Foundations
--
-- Reconciles the Social Agent Observability Platform architecture
-- (see docs/architecture/social_agent_observability_architecture.html)
-- with the existing ABW/Fermi codebase.
--
-- Phase 0 establishes the data-model foundations only — no behavioural
-- changes. Subsequent phases (1: evaluator registry, 2: registry wired
-- into eval pipeline, 3: longitudinal observability, 4: HITL, 5: feedback
-- loop) build on these primitives.
--
-- This migration adds:
--   1. agents.persona_version           — monotonic counter, drift baseline
--   2. episodes.provenance              — auto/HITL/synthetic source enum
--   3. episodes.authority_weight        — 1.0 = HumanAuthority (max), <1.0 = lower-confidence
--   4. episodes.dyad_id                 — deterministic id of (agent, human) dyad (deferred wiring)
--   5. episodes.persona_version_at_write -- snapshot for drift computation
--   6. episode_corrections              — DB-enforced immutable corrections table (Q2.b)
--   7. trigger on agent_versions insert -- bumps agents.persona_version (Q6)
--
-- All ALTERs use IF NOT EXISTS for idempotency. The constraint update
-- on episode provenance values is wrapped in a DO block per PgBouncer
-- transaction-mode safety rules.

-- ═══════════════════════════════════════════════════════════════════
-- 1. agents.persona_version
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE public.agents
    ADD COLUMN IF NOT EXISTS persona_version INTEGER NOT NULL DEFAULT 1;

CREATE INDEX IF NOT EXISTS idx_agents_persona_version
    ON public.agents(agent_id, persona_version);

-- ═══════════════════════════════════════════════════════════════════
-- 2-5. episodes columns
-- ═══════════════════════════════════════════════════════════════════
--
-- Default authority_weight = 0.5 ("automated default"). HumanAuthority
-- writes will be 1.0; lower-confidence sources can sit below 0.5.
--
-- Default provenance = 'auto_pass' so existing episodes keep behaving
-- like passed automated runs. Concrete value set by the eval pipeline
-- once Phase 2 lands.

ALTER TABLE public.episodes
    ADD COLUMN IF NOT EXISTS provenance TEXT NOT NULL DEFAULT 'auto_pass';

ALTER TABLE public.episodes
    ADD COLUMN IF NOT EXISTS authority_weight DOUBLE PRECISION NOT NULL DEFAULT 0.5;

ALTER TABLE public.episodes
    ADD COLUMN IF NOT EXISTS dyad_id TEXT;

ALTER TABLE public.episodes
    ADD COLUMN IF NOT EXISTS persona_version_at_write INTEGER;

-- Provenance enum constraint (separate DO block — PgBouncer safe)
DO $$
BEGIN
    ALTER TABLE public.episodes DROP CONSTRAINT IF EXISTS episodes_provenance_check;
    ALTER TABLE public.episodes ADD CONSTRAINT episodes_provenance_check
        CHECK (provenance IN (
            'auto_pass',             -- evaluator registry passed
            'auto_fail',             -- evaluator registry failed (no human seen)
            'human_approved',        -- HITL reviewer confirmed verdict
            'human_relabeled',       -- HITL reviewer corrected dimension scores
            'human_corrected',       -- HITL reviewer ran full intervention
            'synthetic_correction'   -- second write: synthetic corrected episode
        ));
END $$;

-- Authority weight bounds
DO $$
BEGIN
    ALTER TABLE public.episodes DROP CONSTRAINT IF EXISTS episodes_authority_weight_bounds;
    ALTER TABLE public.episodes ADD CONSTRAINT episodes_authority_weight_bounds
        CHECK (authority_weight >= 0.0 AND authority_weight <= 1.0);
END $$;

-- Useful filtering indexes for downstream phases
CREATE INDEX IF NOT EXISTS idx_episodes_provenance
    ON public.episodes(agent_id, provenance);

CREATE INDEX IF NOT EXISTS idx_episodes_dyad
    ON public.episodes(dyad_id, timestamp_ref DESC)
    WHERE dyad_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_episodes_persona_version
    ON public.episodes(agent_id, persona_version_at_write, timestamp_ref DESC)
    WHERE persona_version_at_write IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- 6. episode_corrections (immutable HITL audit trail — Q2.b)
-- ═══════════════════════════════════════════════════════════════════
--
-- Append-only table for HITL corrections. Original episodes are NEVER
-- mutated; corrections are appended as rows here. Each correction may
-- carry a pointer to a synthetic_correction episode that re-injects the
-- corrected interaction at HumanAuthority weight.

CREATE TABLE IF NOT EXISTS public.episode_corrections (
    correction_id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    episode_id           UUID NOT NULL REFERENCES public.episodes(episode_id) ON DELETE CASCADE,
    agent_id             UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,

    -- Reviewer
    reviewer_id          TEXT NOT NULL,
    reviewer_action      TEXT NOT NULL
        CHECK (reviewer_action IN ('approve', 'relabel', 'intervene')),

    -- Scope of the correction (per architecture doc step 2)
    scope                TEXT NOT NULL
        CHECK (scope IN ('episode', 'dyad', 'agent_wide')),

    -- Belief vs behavioural classification (architecture doc step 3)
    classification       TEXT
        CHECK (classification IS NULL OR classification IN ('belief', 'behaviour')),

    -- The correction payload — dimension overrides, free-text, etc.
    dimension            TEXT,
    correction_text      TEXT,
    score_overrides      JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Coherence gate output (Phase 5 — null until then)
    coherence_check      JSONB,
    minimum_update_set   JSONB,
    tensions_flagged     JSONB,

    -- Pointer to the synthetic corrected episode (Phase 5 — null until then)
    synthetic_episode_id UUID REFERENCES public.episodes(episode_id) ON DELETE SET NULL,

    -- Authority + provenance for the corrective signal itself
    authority_weight     DOUBLE PRECISION NOT NULL DEFAULT 1.0
        CHECK (authority_weight >= 0.0 AND authority_weight <= 1.0),

    -- Effective persona_version after this correction (only set for agent_wide scope)
    persona_version_bump INTEGER,

    -- Free-form justification
    justification        TEXT,

    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_episode_corrections_episode
    ON public.episode_corrections(episode_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_episode_corrections_agent
    ON public.episode_corrections(agent_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_episode_corrections_reviewer
    ON public.episode_corrections(reviewer_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_episode_corrections_scope
    ON public.episode_corrections(agent_id, scope, created_at DESC);

-- Block UPDATEs and DELETEs at the application layer; we additionally
-- enforce immutability via a row-level trigger so a stray UPDATE to an
-- existing correction is rejected.

CREATE OR REPLACE FUNCTION public.episode_corrections_immutable()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'episode_corrections is append-only; row % cannot be modified', OLD.correction_id;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_episode_corrections_no_update ON public.episode_corrections;
CREATE TRIGGER trg_episode_corrections_no_update
    BEFORE UPDATE ON public.episode_corrections
    FOR EACH ROW EXECUTE FUNCTION public.episode_corrections_immutable();

-- ═══════════════════════════════════════════════════════════════════
-- 7. agent_versions → persona_version trigger
-- ═══════════════════════════════════════════════════════════════════
--
-- Per Q6: persona_version increments on
--   (a) agent-wide interventions (handled in app code: it inserts an
--       agent_versions row AND increments persona_version directly), and
--   (b) any AgentVersion row insert (system_prompt / model / temperature
--       / visibility / display_alias change).
--
-- The trigger handles case (b) automatically. Case (a) calling code may
-- still call create_agent_version; the trigger will then bump as well,
-- so app code should NOT additionally bump for the same write.

CREATE OR REPLACE FUNCTION public.bump_agent_persona_version()
RETURNS TRIGGER AS $$
BEGIN
    UPDATE public.agents
        SET persona_version = persona_version + 1
      WHERE agent_id = NEW.agent_id;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_agent_versions_bump_persona ON public.agent_versions;
CREATE TRIGGER trg_agent_versions_bump_persona
    AFTER INSERT ON public.agent_versions
    FOR EACH ROW EXECUTE FUNCTION public.bump_agent_persona_version();

-- ═══════════════════════════════════════════════════════════════════
-- End of migration 103
-- ═══════════════════════════════════════════════════════════════════
