-- CEP (Calibrated Evidence Protocol): add fermi_contract JSONB to agents.
--
-- fermi_contract stores the structured probabilistic reasoning contract for
-- fermi-orchestra agents: finding labels, multiplier range, KG fact categories,
-- and seed facts for initial KG population.
--
-- Additive — existing rows get NULL, all existing queries unaffected.

ALTER TABLE agents ADD COLUMN IF NOT EXISTS fermi_contract JSONB;

CREATE INDEX IF NOT EXISTS idx_agents_fermi_contract
    ON agents (agent_id)
    WHERE fermi_contract IS NOT NULL;
