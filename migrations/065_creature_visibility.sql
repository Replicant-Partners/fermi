-- Migration 065: Creature visibility
-- Controls who can see a creature on the Flights map
-- Values: 'public' (anyone), 'contacts' (owner's contacts only), 'private' (hidden)

ALTER TABLE creatures
  ADD COLUMN IF NOT EXISTS visibility TEXT NOT NULL DEFAULT 'public';

-- Flights inherit visibility from creature at creation time
ALTER TABLE creature_flights
  ADD COLUMN IF NOT EXISTS visibility TEXT NOT NULL DEFAULT 'public';

-- Index for efficient visible-flights queries
CREATE INDEX IF NOT EXISTS idx_creature_flights_visibility
  ON creature_flights (visibility)
  WHERE ended_at IS NULL;
