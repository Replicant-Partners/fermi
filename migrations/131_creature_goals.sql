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

CREATE TABLE IF NOT EXISTS public.creature_goals (
    -- Identity
    goal_id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    creature_id    UUID NOT NULL REFERENCES public.creatures(creature_id) ON DELETE CASCADE,
    owner_id       TEXT NOT NULL,

    -- Goal definition
    title          TEXT NOT NULL,
    description    TEXT NOT NULL,           -- natural language: "Watch for edible fungi near my oak woodland"
    goal_type      TEXT NOT NULL DEFAULT 'custom'
                       CHECK (goal_type IN (
                           'species_watch',     -- alert when specific species found
                           'accumulation',      -- collect N species / observations
                           'location_scout',    -- build knowledge of a specific place
                           'condition_track',   -- track microclimate patterns
                           'bioconversion',     -- full foraging → table chain
                           'custom'             -- freeform, evaluated by goal_tracker agent
                       )),
    parameters     JSONB NOT NULL DEFAULT '{}',
    -- species_watch:   { target_species[], alert_on_first: bool, radius_km }
    -- accumulation:    { target_count, taxa_filter, season_filter }
    -- location_scout:  { h3_cells[], habitat_type, depth: "surface|deep" }
    -- condition_track: { variables[], location, baseline_period_days }
    -- bioconversion:   { target_taxa, include_flavor: bool, include_processing: bool }

    -- App workspace reference (kask_wild workspace for this goal)
    wild_workspace_id  UUID REFERENCES public.teams(id) ON DELETE SET NULL,

    -- Status and progress
    status         TEXT NOT NULL DEFAULT 'active'
                       CHECK (status IN ('active', 'achieved', 'paused', 'abandoned')),
    progress       JSONB NOT NULL DEFAULT '{}',
    -- shape varies by goal_type, e.g.:
    -- species_watch:  { species_found: [], last_checked_at }
    -- accumulation:   { count: N, species_list: [], locations_visited: [] }
    -- bioconversion:  { observations: N, flavor_profiles: [], processing_notes: [] }

    -- Scoring (Brier loop)
    forecast_accuracy  FLOAT,              -- Brier score accumulated over evaluated predictions
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

-- updated_at trigger
CREATE OR REPLACE FUNCTION public.touch_creature_goals_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_creature_goals_updated_at ON public.creature_goals;
CREATE TRIGGER trg_creature_goals_updated_at
    BEFORE UPDATE ON public.creature_goals
    FOR EACH ROW EXECUTE FUNCTION public.touch_creature_goals_updated_at();
