-- ─────────────────────────────────────────────────────────────────────
-- 155 — Forecast Relationship Groups (Spec 25 §4.1 + §4.2)
-- ─────────────────────────────────────────────────────────────────────
--
-- Replaces the per-relationship ID-list model from mig 150 with a
-- group tag model. Members are discovered by querying
-- fermi_forecasts.relationship_groups @> ARRAY[group_id] instead of
-- explicit forecast_ids arrays.
--
-- Two schema changes:
--   1. Add `relationship_groups TEXT[]` to fermi_forecasts
--   2. Create `forecast_relationship_groups` table
--
-- The old forecast_relationships table is NOT dropped here — it stays
-- until the WC migration script (Pass 7) ports the existing mutex
-- relationship to the new group model. Once that's done, a future
-- migration can archive it.

-- §4.1: Each forecast carries a list of group tags it belongs to.
-- Empty array = no constraints. Order doesn't matter.
ALTER TABLE public.fermi_forecasts
    ADD COLUMN IF NOT EXISTS relationship_groups TEXT[] NOT NULL DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_forecasts_relationship_groups
    ON public.fermi_forecasts USING gin (relationship_groups);

-- §4.2: A group declares the semantics of constraint that applies
-- to its members. Members are NOT listed here — they're discovered
-- by querying fermi_forecasts where
-- relationship_groups @> ARRAY[group_id].
CREATE TABLE IF NOT EXISTS public.forecast_relationship_groups (
    group_id            TEXT        PRIMARY KEY,
    kind                TEXT        NOT NULL,   -- 'mutex' | 'at_most_n' | 'implies'
    parameters          JSONB       NOT NULL DEFAULT '{}'::jsonb,
    description         TEXT,
    owner_id            TEXT        NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    archived_at         TIMESTAMPTZ,
    CHECK (kind IN ('mutex', 'at_most_n', 'implies'))
);

CREATE INDEX IF NOT EXISTS idx_relationship_groups_owner
    ON public.forecast_relationship_groups(owner_id) WHERE archived_at IS NULL;
