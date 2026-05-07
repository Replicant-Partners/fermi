-- CEP (Calibrated Evidence Protocol): add structured payload columns to KG tables.
--
-- facts.data       — arbitrary JSONB metadata on any fact (n, source, year, area, etc.)
-- entities.properties — structured attributes on any entity; used by CEP seed facts
--                       (entity_type = 'cep_base_rate' | 'cep_multiplier' | 'cep_accuracy')
--                       to store numeric reference data queryable at execution time.
--
-- Both columns are additive — existing rows get NULL, all existing queries unaffected.

ALTER TABLE facts      ADD COLUMN IF NOT EXISTS data       JSONB;
ALTER TABLE entities   ADD COLUMN IF NOT EXISTS properties JSONB;

CREATE INDEX IF NOT EXISTS idx_entities_type_cep
    ON entities (agent_id, entity_type)
    WHERE entity_type LIKE 'cep_%';
