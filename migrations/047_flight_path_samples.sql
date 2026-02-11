-- Flight path samples: lightweight GPS breadcrumbs from swarm simulation
-- Sent as a batch when flight ends (not real-time)
ALTER TABLE creature_flights ADD COLUMN IF NOT EXISTS path_samples JSONB DEFAULT NULL;

-- Index for queries on flights that have path data
CREATE INDEX IF NOT EXISTS idx_creature_flights_has_path
  ON creature_flights ((path_samples IS NOT NULL))
  WHERE path_samples IS NOT NULL;
