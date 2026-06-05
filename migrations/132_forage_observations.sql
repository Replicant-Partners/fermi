-- Migration 132: Forage Observations
--
-- Structured observation records from foraging runs. These are the raw
-- data points that accumulate into the creature's knowledge graph via
-- dream cycle consolidation.
--
-- PgBouncer-safe. Idempotent.

DO $$ BEGIN

CREATE TABLE IF NOT EXISTS public.forage_observations (
    observation_id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    creature_id      UUID NOT NULL REFERENCES public.creatures(creature_id) ON DELETE CASCADE,
    goal_id          UUID REFERENCES public.creature_goals(goal_id) ON DELETE SET NULL,
    owner_id         TEXT NOT NULL,

    -- What was found
    species_name     TEXT NOT NULL,
    accepted_name    TEXT,
    mycobank_number  TEXT,
    gbif_key         INTEGER,
    taxa_group       TEXT,
    edibility        TEXT CHECK (edibility IN (
                         'edible', 'choice', 'toxic', 'unknown', 'inedible'
                     )),
    quantity         TEXT CHECK (quantity IN (
                         'trace', 'sparse', 'moderate', 'abundant'
                     )),

    -- Where and when
    h3_cell          TEXT,
    location_lat     FLOAT,
    location_lng     FLOAT,
    location_name    TEXT,
    observed_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Habitat and substrate
    habitat_type     TEXT,
    substrate        TEXT,
    associated_trees TEXT[],

    -- Microclimate at time of observation
    conditions       JSONB NOT NULL DEFAULT '{}',

    -- Harvest and processing
    harvested        BOOLEAN NOT NULL DEFAULT false,
    harvest_notes    TEXT,
    processing_path  TEXT,
    processing_notes TEXT,

    -- Flavor profile
    flavor_profile   JSONB NOT NULL DEFAULT '{}',

    -- Social / data sharing
    opted_in_shared  BOOLEAN NOT NULL DEFAULT false,
    verified         BOOLEAN NOT NULL DEFAULT false,

    -- Links
    sosa_observation_id  UUID,
    inat_observation_id  TEXT,
    photo_urls       TEXT[],

    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_forage_obs_creature
    ON public.forage_observations(creature_id);

CREATE INDEX IF NOT EXISTS idx_forage_obs_h3
    ON public.forage_observations(h3_cell)
    WHERE h3_cell IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_forage_obs_species
    ON public.forage_observations(accepted_name)
    WHERE accepted_name IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_forage_obs_observed
    ON public.forage_observations(observed_at DESC);

CREATE INDEX IF NOT EXISTS idx_forage_obs_shared
    ON public.forage_observations(opted_in_shared, h3_cell)
    WHERE opted_in_shared = true;

END $$;
