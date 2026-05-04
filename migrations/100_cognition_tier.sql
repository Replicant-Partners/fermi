-- ADR-011 Phase 1: Creature cognition tier
-- Adds the bandwidth axis to creature_conditions.
-- cognition_level (computed from activity) = knowledge (grows over time, never degrades)
-- cognition_tier  (set by owner)           = bandwidth (determines which model runs)

ALTER TABLE creature_conditions
  ADD COLUMN IF NOT EXISTS cognition_tier TEXT NOT NULL DEFAULT 'free'
    CHECK (cognition_tier IN ('free', 'standard', 'premium'));

CREATE INDEX IF NOT EXISTS idx_creature_conditions_cognition_tier
  ON creature_conditions(cognition_tier);
