-- Migration 130: Index sosa_observations.extra for projection_id lookup
-- and source filtering.
--
-- Enables ProjectionScoringEvaluator (spec 20) to efficiently find:
--   (a) the prior synthetic observation for a given projection_id
--   (b) all synthetic observations for a (observable_property, foi) pair
--       within a time window (fallback matching when projection_id is absent)
--
-- PgBouncer-safe: CREATE INDEX CONCURRENTLY not supported in transaction mode;
-- using standard CREATE INDEX which is fine for maintenance windows.

CREATE INDEX IF NOT EXISTS idx_sosa_obs_projection_id
    ON sosa_observations ((extra->>'projection_id'))
    WHERE extra->>'projection_id' IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_sosa_obs_source_property
    ON sosa_observations (observable_property, feature_of_interest, phenomenon_time DESC)
    WHERE extra->>'source' = 'simops_simulation';
