-- Migration 132: Forage Observations
--
-- Structured observation records from foraging runs. These are the raw
-- data points that accumulate into the creature's knowledge graph via
-- dream cycle consolidation.
--
-- Extends (does not replace) the SOSA observations table. A forage_observation
-- is a higher-level semantic record; the SOSA observation captures the raw
-- sensor/field data. Both are linked via sosa_observation_id.
--
-- PgBouncer-safe. Idempotent.

CREATE TABLE IF NOT EXISTS public.forage_observations (
    -- Identity
    observation_id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    creature_id      UUID NOT NULL REFERENCES public.creatures(creature_id) ON DELETE CASCADE,
    goal_id          UUID REFERENCES public.creature_goals(goal_id) ON DELETE SET NULL,
    owner_id         TEXT NOT NULL,

    -- What was found
    species_name     TEXT NOT NULL,                    -- as given by observer
    accepted_name    TEXT,                             -- MycoBank/GBIF resolved name
    mycobank_number  TEXT,                             -- MycoBank identifier if fungi
    gbif_key         INTEGER,                          -- GBIF taxon key
    taxa_group       TEXT,                             -- fungi | plant | lichen | other
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
    location_name    TEXT,                             -- named spot if annotated
    observed_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Habitat and substrate
    habitat_type     TEXT,                             -- oak_woodland, beech_forest, meadow, coastal...
    substrate        TEXT,                             -- leaf_litter, dead_wood, soil, dung...
    associated_trees TEXT[],                           -- mycorrhizal associations

    -- Microclimate at time of observation
    conditions       JSONB NOT NULL DEFAULT '{}',
    -- {
    --   temp_c: float,
    --   humidity_pct: float,
    --   rainfall_prior_3d_mm: float,
    --   rainfall_prior_7d_mm: float,
    --   soil_moisture: float (0-1),
    --   wind_direction: string,
    --   moon_phase: string
    -- }

    -- Harvest and processing
    harvested        BOOLEAN NOT NULL DEFAULT false,
    harvest_notes    TEXT,                             -- maturity, damage, yield estimate
    processing_path  TEXT,                             -- fresh | dry | ferment | preserve | discard
    processing_notes TEXT,

    -- Flavor profile (Redzepi layer)
    flavor_profile   JSONB NOT NULL DEFAULT '{}',
    -- {
    --   aroma: [descriptors],
    --   taste_dimensions: { umami, earthiness, sweetness, bitterness, acidity },
    --   texture: string,
    --   pairing_notes: string,
    --   terroir_notes: string
    -- }

    -- Social / data sharing
    opted_in_shared  BOOLEAN NOT NULL DEFAULT false,  -- contribute to shared regional model
    verified         BOOLEAN NOT NULL DEFAULT false,  -- verified by community/expert

    -- Links
    sosa_observation_id  UUID,                        -- FK to sosa_observations if ingested via SOSA
    inat_observation_id  TEXT,                        -- iNaturalist observation ID if cross-referenced
    photo_urls       TEXT[],

    -- Bookkeeping
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
