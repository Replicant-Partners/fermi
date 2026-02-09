-- Migration 019: Agent provider and embedding configuration fields
-- Stores the LLM provider and embedding model choice per agent so that
-- embeddings remain dimensionally consistent across the agent's lifetime.

ALTER TABLE public.agents
  ADD COLUMN IF NOT EXISTS llm_provider TEXT NOT NULL DEFAULT 'anthropic',
  ADD COLUMN IF NOT EXISTS embedding_provider TEXT NOT NULL DEFAULT 'anthropic',
  ADD COLUMN IF NOT EXISTS embedding_model TEXT NOT NULL DEFAULT 'voyage-2',
  ADD COLUMN IF NOT EXISTS embedding_dimension INTEGER NOT NULL DEFAULT 1024;
