-- ADR-011 Phase 2: per-agent model ladder for adaptive tier-based model selection
ALTER TABLE agents
  ADD COLUMN IF NOT EXISTS model_ladder JSONB NOT NULL DEFAULT '[]',
  ADD COLUMN IF NOT EXISTS min_tier TEXT NOT NULL DEFAULT 'free'
    CHECK (min_tier IN ('free', 'standard', 'premium')),
  ADD COLUMN IF NOT EXISTS capability_gates JSONB NOT NULL DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_agents_min_tier ON agents(min_tier);
