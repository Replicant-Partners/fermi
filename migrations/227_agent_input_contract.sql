-- Migration 227: input_contract column on agents (A2A Phase B)
--
-- Stores the compiled input_contract from capabilities.input_contract on
-- the agent card, symmetric to the output_contract column added in
-- migration 117. Enables list_workspace_agents to serve the callee's
-- accepts_schema for all agents, not just those with an on-disk card.
--
-- NULL means "no input contract declared" — the correct default for
-- every current agent.
ALTER TABLE agents ADD COLUMN IF NOT EXISTS input_contract jsonb;
