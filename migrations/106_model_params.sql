-- Migration 106: Add model_params JSONB to agents
--
-- ADR-011 Phase 4: replaces the single `temperature` float with a flexible
-- provider-agnostic sampling configuration object. Keys correspond to the
-- fields in SamplingParams (agent_card.rs): temperature, max_tokens, top_p,
-- top_k, extended_thinking, thinking_budget_tokens, frequency_penalty,
-- presence_penalty, repetition_penalty, random_seed.
--
-- NULL keys fall back to the legacy `temperature` column; absent keys use
-- provider/executor defaults. Existing agents keep their original temperature
-- via the fallback in AgentCapabilities::resolve_sampling_params().

ALTER TABLE agents ADD COLUMN IF NOT EXISTS model_params JSONB NOT NULL DEFAULT '{}';
