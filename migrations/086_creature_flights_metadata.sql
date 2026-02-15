-- Add metadata column to creature_flights for flight plan data, narrative, etc.
ALTER TABLE creature_flights ADD COLUMN IF NOT EXISTS metadata JSONB;
