-- Migration 101: Make cognition_tier nullable (NULL = use agent default model)
-- ADR-011 fix: migration 100 defaulted all creatures to 'free', causing OpenRouter model
-- selection for every creature. NULL means "no tier override" — agent uses its native model.

ALTER TABLE creature_conditions
  ALTER COLUMN cognition_tier DROP NOT NULL;

ALTER TABLE creature_conditions
  ALTER COLUMN cognition_tier SET DEFAULT NULL;

UPDATE creature_conditions
  SET cognition_tier = NULL
  WHERE cognition_tier = 'free';
