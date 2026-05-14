-- Migration 117: output_contract column on agents table.
--
-- Generalises the domain-specific fermi_contract to an arbitrary domain
-- output contract for domain-constrained MoE orchestrators and their members.
--
-- Shape: {
--   domain: string,
--   produces: string[],
--   schema: JSONSchema,
--   calibration: { signal, observable_property, resolution_delay_hours, comparison },
--   synthesis: "aggregation"|"pipeline"|"selection"|"max_risk"|"cep_weighted"
-- }
--
-- For Fermi: domain="forecasting", calibration.signal="brier_forecast",
--            synthesis="cep_weighted". fermi_contract holds forecast-specific details.
-- For SimOps: domain="process_optimisation", calibration.signal="sosa_observation",
--             synthesis="pipeline".
-- For a legal MoE: domain="legal_review", calibration.signal="hitl_review",
--                  synthesis="max_risk".
--
-- PgBouncer-safe: ALTER TABLE wrapped in DO block.

DO $$
BEGIN
    ALTER TABLE agents
        ADD COLUMN IF NOT EXISTS output_contract JSONB;
EXCEPTION WHEN duplicate_column THEN NULL;
END $$;
