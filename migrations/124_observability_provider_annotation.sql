-- Migration 124: Phase 2 observability annotation — tag execution provenance.
--
-- Adds provider_used + model_used to the three hot-path observability tables
-- so the observatory can distinguish "drift on local" from "drift on cloud"
-- and per-provider calibration tracking can work correctly (Loop 5).
--
-- Also adds provider_mix to coherence_evaluations so the coherence consultant
-- can explain whether incoherence is caused by model mismatch rather than
-- composition mismatch (Loop 3).
--
-- All columns are nullable with no default — existing rows stay NULL which
-- the observatory renders as "unknown". New rows are populated by the
-- executor dispatch path.
--
-- PgBouncer-safe: each ALTER is a single statement.

ALTER TABLE public.episodes
    ADD COLUMN IF NOT EXISTS provider_used TEXT,
    ADD COLUMN IF NOT EXISTS model_used TEXT;

ALTER TABLE public.eval_signals
    ADD COLUMN IF NOT EXISTS provider_used TEXT,
    ADD COLUMN IF NOT EXISTS model_used TEXT;

ALTER TABLE public.anomaly_events
    ADD COLUMN IF NOT EXISTS provider_used TEXT,
    ADD COLUMN IF NOT EXISTS model_used TEXT;

ALTER TABLE public.coherence_evaluations
    ADD COLUMN IF NOT EXISTS provider_mix JSONB;

-- Indexes for observatory filtering and per-provider calibration queries
CREATE INDEX IF NOT EXISTS idx_episodes_provider
    ON public.episodes(provider_used)
    WHERE provider_used IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_eval_signals_provider
    ON public.eval_signals(provider_used)
    WHERE provider_used IS NOT NULL;
