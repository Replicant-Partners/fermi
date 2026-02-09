-- Migration 025: Agent lifecycle states and fork infrastructure
-- Adds status (draft/published/archived), fork tracking, and fork pricing.

BEGIN;

ALTER TABLE agents ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'draft'
    CHECK (status IN ('draft', 'published', 'archived'));

ALTER TABLE agents ADD COLUMN IF NOT EXISTS fork_pricing JSONB DEFAULT '{"base_price": 0}';

ALTER TABLE agents ADD COLUMN IF NOT EXISTS forked_from UUID REFERENCES agents(agent_id) ON DELETE SET NULL;

ALTER TABLE agents ADD COLUMN IF NOT EXISTS fork_count INTEGER NOT NULL DEFAULT 0;

-- Curated agents are published by default, community agents start as drafts
UPDATE agents SET status = 'published' WHERE tier = 'curated';

CREATE INDEX IF NOT EXISTS idx_agents_status ON agents(status);
CREATE INDEX IF NOT EXISTS idx_agents_forked_from ON agents(forked_from);

COMMIT;
