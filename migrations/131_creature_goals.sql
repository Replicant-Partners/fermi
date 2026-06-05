-- Migration 131: Creature Goals
--
-- Goal-tracking for Rabble creatures. A goal is a standing objective that
-- the creature's agents evaluate on each observation run, accumulating
-- progress over time.
--
-- Goals are the bridge between Rabble creatures and kask-app-wild:
-- the creature holds the goal; Wild provides the intelligence to evaluate it.
--
-- PgBouncer-safe. Idempotent.

DO $$ BEGIN

CREATE TABLE IF NOT EXISTS public.creature_goals (
    -- Identity
    goal_id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    creature_id    UUID NOT NULL REFERENCES public.creatures(creature_id) ON DELETE CASCADE,
    owner_id       TEXT NOT NULL,

    -- Goal definition
    title          TEXT NOT NULL,
    description    TEXT NOT NULL,
    goal_type      TEXT NOT NULL DEFAULT 'custom'
                       CHECK (goal_type IN (
                           'species_watch',
                           'accumulation',
                           'location_scout',
                           'condition_track',
                           'bioconversion',
                           'custom'
                       )),
    parameters     JSONB NOT NULL DEFAULT '{}',

    -- App workspace reference (kask_wild workspace for this goal)
    wild_workspace_id  UUID REFERENCES public.teams(id) ON DELETE SET NULL,

    -- Status and progress
    status         TEXT NOT NULL DEFAULT 'active'
                       CHECK (status IN ('active', 'achieved', 'paused', 'abandoned')),
    progress       JSONB NOT NULL DEFAULT '{}',

    -- Scoring (Brier loop)
    forecast_accuracy  FLOAT,
    predictions_made   INTEGER DEFAULT 0,
    predictions_scored INTEGER DEFAULT 0,

    -- Lifecycle
    achieved_at        TIMESTAMPTZ,
    last_evaluated_at  TIMESTAMPTZ,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_creature_goals_creature
    ON public.creature_goals(creature_id)
    WHERE status = 'active';

CREATE INDEX IF NOT EXISTS idx_creature_goals_owner
    ON public.creature_goals(owner_id);

END $$;

-- updated_at trigger (outside DO block — CREATE OR REPLACE FUNCTION is DDL
-- that PgBouncer handles fine at the statement level)
CREATE OR REPLACE FUNCTION public.touch_creature_goals_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$;

DO $$ BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger WHERE tgname = 'trg_creature_goals_updated_at'
    ) THEN
        CREATE TRIGGER trg_creature_goals_updated_at
            BEFORE UPDATE ON public.creature_goals
            FOR EACH ROW EXECUTE FUNCTION public.touch_creature_goals_updated_at();
    END IF;
END $$;
