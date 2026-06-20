-- ─────────────────────────────────────────────────────────────────────
-- 150 — forecast_relationships: declarable inter-forecast dependencies
-- ─────────────────────────────────────────────────────────────────────
--
-- Generalizes "when forecast A changes, forecast B should follow" beyond
-- the WC mutually-exclusive case. The relationship is its own object,
-- not portfolio-coupled, because portfolios may mix related and
-- independent forecasts.
--
-- First-implemented `kind`: 'mutually_exclusive' — the WC sims case.
-- Stubbed (server returns 400) for now: 'logical_implies', 'conjunction',
-- 'conditional', 'exhaustive_cover'.
--
-- Propagation runs via POST /api/forecast-relationships/:id/propagate
-- with {trigger_forecast_id, trigger_kind, ...}. The server dispatches
-- to a per-kind handler that writes update_probability rows on each
-- affected forecast — so every propagation appears in the
-- forecast_spacetime table and shows up on the trajectory tab as a
-- 'cascade' event (revision_trigger='cascade').
--
-- Operator-explicit by design: the operator clicks "Cascade to N
-- forecasts" after a resolve. No auto-fire on resolve. Auto-cascade
-- is one toggle away once we trust the math at scale.

CREATE TABLE IF NOT EXISTS public.forecast_relationships (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    -- 'mutually_exclusive', 'logical_implies', 'conjunction',
    -- 'conditional', 'exhaustive_cover' — see propagate registry in
    -- src/handlers/relationships.rs.
    kind            TEXT        NOT NULL,
    -- All forecast IDs participating in this relationship. For mutex this
    -- is the full member set (48 WC teams); for binary kinds (implies,
    -- conjunction) length=2.
    forecast_ids    TEXT[]      NOT NULL,
    -- Kind-specific config: weights for redistribution, antecedent/
    -- consequent role markers for implies, correlation values for
    -- conditional, etc. JSONB so new kinds can extend without migration.
    parameters      JSONB       NOT NULL DEFAULT '{}'::jsonb,
    description     TEXT,
    owner_id        TEXT        NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Soft-delete instead of CASCADE on forecast deletion: a relationship
    -- with a missing member is informative ("ARG was in this 48-team
    -- mutex group, but ARG's forecast was deleted").
    archived_at     TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_relationships_kind
    ON public.forecast_relationships(kind);
CREATE INDEX IF NOT EXISTS idx_relationships_owner
    ON public.forecast_relationships(owner_id);
-- GIN on forecast_ids so "find all relationships involving this forecast"
-- is fast — used by the console to surface the Cascade button on a
-- resolved forecast.
CREATE INDEX IF NOT EXISTS idx_relationships_forecast_ids
    ON public.forecast_relationships USING gin (forecast_ids);

-- Pre-existing migrations 094 + 149 already declared revision_trigger on
-- fermi_forecast_updates with the value set 'initial', 'evidence_update',
-- 'agent_correction', 'schedule_rerun', 'manual', 'bayesops_refit'. The
-- propagation handler writes 'cascade'. Extend the CHECK constraint.
DO $$ BEGIN
    -- Drop the old CHECK if it exists; PostgreSQL's CHECK constraints
    -- can't be ALTER'd in place, only dropped+recreated.
    IF EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fermi_forecast_updates_revision_trigger_check'
    ) THEN
        ALTER TABLE public.fermi_forecast_updates
            DROP CONSTRAINT fermi_forecast_updates_revision_trigger_check;
    END IF;
END $$;

ALTER TABLE public.fermi_forecast_updates
    ADD CONSTRAINT fermi_forecast_updates_revision_trigger_check
    CHECK (
        revision_trigger IS NULL OR revision_trigger IN (
            'initial',
            'evidence_update',
            'agent_correction',
            'schedule_rerun',
            'manual',
            'bayesops_refit',
            'cascade'
        )
    );
