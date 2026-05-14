-- Migration 114: Add valence column to agents table.
--
-- AgentValence (primary_affect, arousal, valence, personality_traits) was
-- previously stored only in filesystem agent_card.json files. This migration
-- promotes it to a first-class DB column so it can be read, written, and
-- updated via the API without requiring a card file edit + redeploy.
--
-- The column is JSONB so the shape can evolve without schema migrations.
-- Existing agents default to NULL; the UI Edit section lets owners set it.
-- The agent_card_from_db() fallback path in api_server.rs already sets
-- valence: None — no behaviour change for agents without a card file.

ALTER TABLE agents
    ADD COLUMN IF NOT EXISTS valence JSONB;

COMMENT ON COLUMN agents.valence IS
    'Affective signature: {primary_affect, arousal, valence, personality_traits}. '
    'Used by composition planner and social matching. NULL = not yet set.';
